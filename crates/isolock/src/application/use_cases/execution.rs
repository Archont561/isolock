//! `exec` and `serve`: the two workflows that own a live loopback interceptor.

use crate::application::ports::{
    ChildCommand, Listener, Progress, ProxyRuntime, ProxySession, UseCaseResult,
};
use std::process::ExitCode;

/// What an `exec` run left behind.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecOutcome {
    /// The child's exact status, which becomes the process status.
    pub status: ExitCode,
    /// A shutdown failure the caller must surface on stderr without changing the status.
    pub shutdown_warning: Option<ShutdownWarning>,
}

/// A proxy shutdown failure reported as `isolock exec: proxy shutdown failed: …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownWarning {
    /// The underlying adapter message.
    pub message: String,
}

impl ShutdownWarning {
    /// The exact line the CLI writes to standard error.
    #[must_use]
    pub fn line(&self) -> String {
        format!("isolock exec: proxy shutdown failed: {}", self.message)
    }
}

/// Runs a child command under a freshly started interceptor.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the command is empty or the session cannot
/// start or run the child.
pub fn exec_child<R: ProxyRuntime>(
    runtime: &R,
    arguments: &[String],
) -> UseCaseResult<ExecOutcome> {
    let command = ChildCommand::from_arguments(arguments)?;
    let session = runtime.start()?;
    let status = session.run(&command)?;
    let shutdown_warning = session.close().err().map(|error| ShutdownWarning {
        message: error.to_string(),
    });
    Ok(ExecOutcome {
        status,
        shutdown_warning,
    })
}

/// Serves the locked registry snapshot on loopback until interrupted.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the listener cannot bind or fails.
pub fn serve_proxy<R: ProxyRuntime>(runtime: &R, progress: &mut dyn Progress) -> UseCaseResult<()> {
    let listener = runtime.bind()?;
    progress.line(&format!(
        "isolock serve: listening on {} ({})",
        listener.base_url(),
        listener.mode_label()
    ));
    listener.serve()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::ProxySession as Session;
    use crate::domain::DomainError;

    struct FakeRuntime {
        status: ExitCode,
        shutdown: Option<&'static str>,
        fail_start: bool,
    }

    struct FakeSession {
        status: ExitCode,
        shutdown: Option<&'static str>,
    }

    impl Session for FakeSession {
        fn run(&self, _command: &ChildCommand) -> UseCaseResult<ExitCode> {
            Ok(self.status)
        }

        fn close(self) -> UseCaseResult<()> {
            match self.shutdown {
                Some(message) => Err(DomainError::new(message)),
                None => Ok(()),
            }
        }
    }

    struct FakeListener;

    impl Listener for FakeListener {
        fn base_url(&self) -> String {
            "http://127.0.0.1:4321".into()
        }
        fn mode_label(&self) -> String {
            "LocalAirlock".into()
        }
        fn serve(self) -> UseCaseResult<()> {
            Ok(())
        }
    }

    impl ProxyRuntime for FakeRuntime {
        type Session = FakeSession;
        type Listener = FakeListener;

        fn start(&self) -> UseCaseResult<FakeSession> {
            if self.fail_start {
                return Err(DomainError::new("cannot start local proxy"));
            }
            Ok(FakeSession {
                status: self.status,
                shutdown: self.shutdown,
            })
        }

        fn bind(&self) -> UseCaseResult<FakeListener> {
            Ok(FakeListener)
        }
    }

    #[test]
    fn exec_returns_the_child_status_untouched() {
        let runtime = FakeRuntime {
            status: ExitCode::from(7),
            shutdown: None,
            fail_start: false,
        };
        let outcome = exec_child(&runtime, &["sh".into(), "-c".into(), "exit 7".into()])
            .expect("session runs");
        assert_eq!(outcome.status, ExitCode::from(7));
        assert_eq!(outcome.shutdown_warning, None);
    }

    #[test]
    fn exec_reports_the_argument_tail_verbatim() {
        let command = ChildCommand::from_arguments(&["cargo".into(), "--".into(), "-v".into()])
            .expect("tail");
        assert_eq!(command.program, "cargo");
        assert_eq!(command.arguments, ["--", "-v"]);
    }

    #[test]
    fn an_empty_tail_is_a_validation_failure() {
        let runtime = FakeRuntime {
            status: ExitCode::SUCCESS,
            shutdown: None,
            fail_start: false,
        };
        let error = exec_child(&runtime, &[]).expect_err("no command");
        assert_eq!(error.to_string(), "expected a child command");
    }

    #[test]
    fn a_shutdown_failure_becomes_a_warning_and_keeps_the_child_status() {
        let runtime = FakeRuntime {
            status: ExitCode::from(3),
            shutdown: Some("listener failed"),
            fail_start: false,
        };
        let outcome = exec_child(&runtime, &["true".into()]).expect("session runs");
        assert_eq!(outcome.status, ExitCode::from(3));
        let warning = outcome.shutdown_warning.expect("warning recorded");
        assert_eq!(
            warning.line(),
            "isolock exec: proxy shutdown failed: listener failed"
        );
    }

    #[test]
    fn serve_announces_the_bound_url_and_mode_before_accepting() {
        let runtime = FakeRuntime {
            status: ExitCode::SUCCESS,
            shutdown: None,
            fail_start: false,
        };
        let mut progress = crate::application::ports::Recorded::default();
        serve_proxy(&runtime, &mut progress).expect("serves");
        assert_eq!(
            progress.joined(),
            "isolock serve: listening on http://127.0.0.1:4321 (LocalAirlock)"
        );
    }

    #[test]
    fn a_failed_start_is_a_typed_error_not_a_rendered_string() {
        let runtime = FakeRuntime {
            status: ExitCode::SUCCESS,
            shutdown: None,
            fail_start: true,
        };
        assert!(exec_child(&runtime, &["true".into()]).is_err());
    }
}
