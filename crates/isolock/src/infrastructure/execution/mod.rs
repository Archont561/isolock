//! One RAII owner for a live proxy, generated Pixi file, and child environment.

mod cargo;
mod environment;
pub mod inputs;
mod pixi;
pub mod profile_session;

use crate::application::ports::{ChildCommand, SessionSettings};
use crate::application::ports::{ChildOutcome, ProcessRequest, ProcessRunner};
use crate::infrastructure::process::SystemProcesses;
use cargo::CargoConfigError;
use environment::{
    CargoEnvironment, ChildEnvironment, NpmEnvironment, PixiEnvironment, ProfileEnvironment,
    ProxyEnvironment, PypiEnvironment,
};
use isolock_proxy::{ProxyHandle, ProxyServer};
use pixi::{PixiConfig, PixiConfigError};
use std::error::Error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum SessionError {
    Inputs(String),
    Proxy(io::Error),
    Cargo(CargoConfigError),
    Pixi(PixiConfigError),
    Child { program: String, source: io::Error },
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inputs(message) => write!(f, "cannot load airlock data: {message}"),
            Self::Proxy(error) => write!(f, "cannot start local proxy: {error}"),
            Self::Cargo(error) => write!(f, "cannot configure Cargo: {error}"),
            Self::Pixi(error) => write!(f, "cannot configure Pixi: {error}"),
            Self::Child { program, source } => write!(f, "cannot start {program:?}: {source}"),
        }
    }
}
impl Error for SessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inputs(_) => None,
            Self::Proxy(error) | Self::Child { source: error, .. } => Some(error),
            Self::Cargo(error) => Some(error),
            Self::Pixi(error) => Some(error),
        }
    }
}

pub struct ExecutionSession {
    // Field drop order stops the proxy before removing the temporary Pixi file.
    processes: SystemProcesses,
    proxy: Option<ProxyHandle>,
    pixi: Option<PixiConfig>,
    environment: ChildEnvironment,
}

impl ExecutionSession {
    pub fn start(config: &SessionSettings) -> Result<Self, SessionError> {
        let (plan, cas) = profile_session::load(config).map_err(SessionError::Inputs)?;
        let proxy = ProxyServer::bind(config.proxy.clone(), plan, cas)
            .and_then(ProxyServer::spawn)
            .map_err(SessionError::Proxy)?;
        let origin = proxy.base_url();
        // Establish the session owner before any later setup can fail.
        let mut session = Self {
            processes: SystemProcesses::new(),
            proxy: Some(proxy),
            pixi: None,
            environment: ChildEnvironment::default(),
        };
        // A profile session keeps the profile's own Cargo home: it already carries the
        // vendor replacement, and rewriting it would replace the reviewed sources with a
        // live proxy registry.
        match &config.profile {
            Some(profile) => {
                std::fs::create_dir_all(&profile.cargo_home).map_err(|source| {
                    SessionError::Cargo(CargoConfigError::CreateHome {
                        path: profile.cargo_home.clone(),
                        source,
                    })
                })?;
            }
            None => cargo::prepare(&config.cargo_home, &origin).map_err(SessionError::Cargo)?,
        }
        let pixi = PixiConfig::prepare(
            &origin,
            config.pixi_config.as_deref(),
            &config.temporary_dir,
        )
        .map_err(SessionError::Pixi)?;
        session.environment.include(&CargoEnvironment {
            home: config
                .profile
                .as_ref()
                .map_or(&config.cargo_home, |profile| &profile.cargo_home),
            origin: &origin,
        });
        session.environment.include(&PixiEnvironment {
            config: pixi.path(),
            origin: &origin,
        });
        session.environment.include(&PypiEnvironment(&origin));
        session.environment.include(&NpmEnvironment(&origin));
        session.environment.include(&ProxyEnvironment {
            origin: &origin,
            inherited_no_proxy: &config.inherited_no_proxy,
        });
        if let Some(profile) = &config.profile {
            // The verified prefix comes first, so a child resolves the profile's own
            // toolchain. Nothing below ever falls back to the host's.
            session.environment.include(&ProfileEnvironment {
                bin: profile_session::prefix_bin(profile),
            });
        }
        session.pixi = Some(pixi);
        Ok(session)
    }

    pub fn run(&self, command: &ChildCommand) -> Result<ChildOutcome, SessionError> {
        // Run without capturing either stream, so the child's own output reaches the
        // operator exactly as if it had been invoked directly.
        let request = self.environment.project(
            ProcessRequest::new(&command.program).arguments(command.arguments.iter().cloned()),
        );
        let output = self
            .processes
            .run_inheriting_output(&request)
            .map_err(|failure| SessionError::Child {
                program: command.program.clone(),
                source: io::Error::other(failure.message),
            })?;
        Ok(ChildOutcome {
            code: output.code,
            signal: output.signal,
        })
    }

    pub fn close(mut self) -> io::Result<()> {
        // Taking the handle prevents duplicate shutdown; Self still drops the Pixi file on error.
        self.proxy.take().map_or(Ok(()), ProxyHandle::shutdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::time::Duration;

    #[test]
    fn dropping_or_unwinding_a_session_owns_all_live_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let config = SessionSettings {
            plan: temp.path().join("absent-default-plan"),
            require_plan: false,
            cas_dir: temp.path().join("cas"),
            cargo_home: temp.path().join("cargo-home"),
            pixi_config: None,
            temporary_dir: temp.path().into(),
            inherited_no_proxy: String::new(),
            proxy: isolock_proxy::ProxyConfig::default(),
            profile: None,
        };
        for unwind in [false, true] {
            let session = ExecutionSession::start(&config).unwrap();
            let proxy = session.proxy.as_ref().unwrap();
            let address = proxy.local_addr();
            let monitor = proxy.monitor();
            let pixi_path = session.pixi.as_ref().unwrap().path().to_owned();
            assert!(pixi_path.exists());
            if unwind {
                assert!(catch_unwind(AssertUnwindSafe(|| {
                    let _owner = session;
                    panic!("exercise RAII unwinding");
                }))
                .is_err());
            } else {
                drop(session);
            }
            assert!(!pixi_path.exists());
            assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
            assert_eq!(monitor.stats().active, 0);
            assert_eq!(monitor.stats().detached, 0);
        }
    }
}
