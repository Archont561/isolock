//! The loopback interceptor and child process adapter behind the [`ProxyRuntime`] port.

use crate::application::ports::{
    ChildCommand, ChildOutcome, Listener, ProxyRuntime, ProxySession, SessionSettings,
    UseCaseResult,
};
use crate::domain::DomainError;
use crate::infrastructure::execution::{inputs, ExecutionSession};
use isolock_proxy::ProxyServer;

/// Projects already-loaded configuration into the proxy runtime.
pub struct LoopbackProxy {
    session: SessionSettings,
}

impl LoopbackProxy {
    /// Binds the runtime to one resolved configuration projection.
    #[must_use]
    pub fn new(session: SessionSettings) -> Self {
        Self { session }
    }
}

impl ProxyRuntime for LoopbackProxy {
    type Session = ChildSession;
    type Listener = BoundListener;

    fn start(&self) -> UseCaseResult<ChildSession> {
        ExecutionSession::start(&self.session)
            .map(ChildSession)
            .map_err(|error| DomainError::new(error.to_string()))
    }

    fn bind(&self) -> UseCaseResult<BoundListener> {
        let (plan, cas) = inputs::load(&self.session).map_err(DomainError::new)?;
        let server = ProxyServer::bind(self.session.proxy.clone(), plan, cas)
            .map_err(|error| DomainError::new(format!("cannot bind loopback listener: {error}")))?;
        let mode = format!("{:?}", self.session.proxy.mode);
        Ok(BoundListener { server, mode })
    }
}

/// A listener bound on loopback and not yet serving.
pub struct BoundListener {
    server: ProxyServer,
    mode: String,
}

impl Listener for BoundListener {
    fn base_url(&self) -> String {
        self.server.base_url().to_string()
    }

    fn mode_label(&self) -> String {
        self.mode.clone()
    }

    fn serve(self) -> UseCaseResult<()> {
        self.server
            .serve()
            .map_err(|error| DomainError::new(format!("listener failed: {error}")))
    }
}

/// One live child session under the interceptor.
pub struct ChildSession(ExecutionSession);

impl ProxySession for ChildSession {
    fn run(&self, command: &ChildCommand) -> UseCaseResult<ChildOutcome> {
        self.0
            .run(command)
            .map_err(|error| DomainError::new(error.to_string()))
    }

    fn close(self) -> UseCaseResult<()> {
        self.0
            .close()
            .map_err(|error| DomainError::new(error.to_string()))
    }
}
