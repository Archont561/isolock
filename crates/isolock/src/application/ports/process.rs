//! Running an external program: the one boundary every adapter shares.
//!
//! The port models exactly what the adapter needs and no more: a program, its
//! arguments, an optional working directory, environment removals/additions, an
//! optional stdin file, and three ways the result is consumed. Anything richer
//! (streaming to a caller-supplied writer, piping two children together) is not
//! required by any current call site, so it is deliberately absent.
//!
//! Use cases never depend on this port. It exists so infrastructure call sites
//! can be driven by a fake in tests instead of spawning `git`, `tar`, `pixi`, or
//! `curl`.

use std::path::PathBuf;

/// One external program invocation, described before it runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessRequest {
    /// The program name or absolute path to execute.
    pub program: PathBuf,
    /// Arguments passed in order, never re-split or shell-expanded.
    pub arguments: Vec<String>,
    /// Working directory, when the program must run somewhere other than the caller's.
    pub directory: Option<PathBuf>,
    /// Variables removed from the inherited environment before running.
    pub removed: Vec<String>,
    /// Variables set on the child environment, in insertion order.
    pub environment: Vec<(String, String)>,
    /// File connected to the child's stdin.
    pub stdin: Option<PathBuf>,
    /// File the child's stdout is written to, instead of being captured.
    pub stdout: Option<PathBuf>,
}

impl ProcessRequest {
    /// Starts a request for `program` with no arguments.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    /// Appends one argument.
    #[must_use]
    pub fn argument(mut self, argument: impl Into<String>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    /// Appends several arguments in order.
    #[must_use]
    pub fn arguments<I, S>(mut self, arguments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Removes one variable from the inherited environment.
    #[must_use]
    pub fn without_variable(mut self, name: impl Into<String>) -> Self {
        self.removed.push(name.into());
        self
    }

    /// Removes several variables from the inherited environment.
    #[must_use]
    pub fn without_variables<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.removed.extend(names.into_iter().map(Into::into));
        self
    }

    /// Sets one child environment variable.
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.push((name.into(), value.into()));
        self
    }

    /// Connects `path` to the child's stdin.
    #[must_use]
    pub fn reading_stdin_from(mut self, path: impl Into<PathBuf>) -> Self {
        self.stdin = Some(path.into());
        self
    }

    /// Writes the child's stdout to `path` instead of capturing it.
    #[must_use]
    pub fn writing_stdout_to(mut self, path: impl Into<PathBuf>) -> Self {
        self.stdout = Some(path.into());
        self
    }
}

/// The outcome of a completed program that did not fail to start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessOutput {
    /// Whether the program exited successfully.
    pub success: bool,
    /// The exit code when the program exited normally, or `None` when it was signalled.
    pub code: Option<i32>,
    /// The terminating signal on Unix, when the program did not exit normally.
    pub signal: Option<i32>,
    /// Everything the program wrote to stdout.
    pub stdout: Vec<u8>,
}

/// Why a program could not be started or waited on.
///
/// This is intentionally a description, not an [`std::io::Error`]: callers map
/// it to the message their own operation already reports, so no user-visible
/// diagnostic changes when a process moves behind this port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessFailure {
    /// The program that could not be run.
    pub program: PathBuf,
    /// The operating system's description of the failure.
    pub message: String,
}

impl std::fmt::Display for ProcessFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot run {}: {}",
            self.program.display(),
            self.message
        )
    }
}

/// Receives a child's stdout stream, or `None` when no stream was available.
pub type StreamConsumer<'a> =
    &'a mut dyn FnMut(Option<&mut dyn std::io::Read>) -> Result<(), String>;

/// Runs external programs.
///
/// `Send + Sync` is required so a runner can be shared by adapters that are
/// moved between threads, which the proxy already does.
pub trait ProcessRunner: Send + Sync {
    /// Runs the request to completion, capturing stdout.
    ///
    /// # Errors
    /// Returns a [`ProcessFailure`] when the program cannot be started or waited on.
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessFailure>;

    /// Runs the request to completion without capturing stdout or stderr.
    ///
    /// Only use this for programs whose output is never shown to the operator.
    /// Streams are discarded, so this is not a substitute for inheritance.
    ///
    /// # Errors
    /// Returns a [`ProcessFailure`] when the program cannot be started or waited on.
    fn run_discarding_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure>;

    /// Runs the request with stdout and stderr inherited from this process.
    ///
    /// This is the `isolock exec` path: the child's own output must reach the
    /// operator exactly as if they had invoked it directly.
    ///
    /// # Errors
    /// Returns a [`ProcessFailure`] when the program cannot be started or waited on.
    fn run_inheriting_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure>;

    /// Runs the request, handing its stdout to `consume` as a stream.
    ///
    /// The program is killed if `consume` fails, so a broken consumer never
    /// leaves an orphan behind. `consume` receives `None` when the child could
    /// not expose a stdout stream.
    ///
    /// # Errors
    /// Returns a [`ProcessFailure`] when the program cannot be started or waited on.
    fn run_streaming(
        &self,
        request: &ProcessRequest,
        consume: StreamConsumer<'_>,
    ) -> Result<ProcessOutput, ProcessFailure>;
}

/// Convenience blanket implementation so `&T` and `Arc<T>` can be used directly.
impl<T: ProcessRunner + ?Sized> ProcessRunner for std::sync::Arc<T> {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessFailure> {
        (**self).run(request)
    }

    fn run_discarding_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        (**self).run_discarding_output(request)
    }

    fn run_inheriting_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        (**self).run_inheriting_output(request)
    }

    fn run_streaming(
        &self,
        request: &ProcessRequest,
        consume: StreamConsumer<'_>,
    ) -> Result<ProcessOutput, ProcessFailure> {
        (**self).run_streaming(request, consume)
    }
}
