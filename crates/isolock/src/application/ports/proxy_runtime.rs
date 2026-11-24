//! Starting an interceptor and running a child under it.

use super::UseCaseResult;

/// A child command and its opaque argument tail, exactly as the operator typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildCommand {
    pub program: String,
    pub arguments: Vec<String>,
}

impl ChildCommand {
    /// Builds a command from the parsed trailing arguments.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when no program was supplied.
    pub fn from_arguments(arguments: &[String]) -> UseCaseResult<Self> {
        let (program, arguments) = arguments
            .split_first()
            .ok_or_else(|| crate::domain::DomainError::new("expected a child command"))?;
        Ok(Self {
            program: program.clone(),
            arguments: arguments.to_vec(),
        })
    }
}

/// Owning a live proxy, its generated Pixi file, and the child environment.
pub trait ProxyRuntime {
    /// The RAII child session the use case holds for the child's lifetime.
    type Session: Session;
    /// A bound, not yet serving listener.
    type Listener: Listener;

    /// Binds the listener and prepares the child environment.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the plan cannot be read, the listener
    /// cannot bind, or the child environment cannot be prepared.
    fn start(&self) -> UseCaseResult<Self::Session>;

    /// Binds a listener that the caller then serves.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the plan cannot be read or the listener
    /// cannot bind.
    fn bind(&self) -> UseCaseResult<Self::Listener>;
}

/// A listener bound and ready to accept, described for the operator-facing banner.
pub trait Listener {
    /// The URL this listener answers on.
    fn base_url(&self) -> String;

    /// The reviewed mode label the banner reports.
    fn mode_label(&self) -> String;

    /// Accepts requests until the process is interrupted.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the listener fails.
    fn serve(self) -> UseCaseResult<()>;
}

/// How a child finished, described without a platform-specific status type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChildOutcome {
    /// The exit code when the child exited normally.
    pub code: Option<i32>,
    /// The terminating signal on Unix, when the child did not exit normally.
    pub signal: Option<i32>,
}

/// One live execution session.
pub trait Session {
    /// Runs the child and returns how it finished.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the child cannot be spawned or waited on.
    fn run(&self, command: &ChildCommand) -> UseCaseResult<ChildOutcome>;

    /// Shuts the proxy down, reporting any failure for the caller to surface on stderr.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when shutdown fails.
    fn close(self) -> UseCaseResult<()>;
}
