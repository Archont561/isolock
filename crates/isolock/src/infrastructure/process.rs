//! The real process boundary: the only place in the crate that spawns a program.
//!
//! Every earlier `Command::new` call site now builds a [`ProcessRequest`] and
//! hands it here. Mapping staying in this module means the diagnostics each
//! caller already produced are unchanged: this layer reports only start/exit
//! facts, and the caller keeps its own user-facing message.

use crate::application::ports::{ProcessFailure, ProcessOutput, ProcessRequest, ProcessRunner};
use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};

/// The terminating signal, when the platform exposes one.
fn signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

/// Spawns real child processes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcesses;

impl SystemProcesses {
    /// Builds the standard process runner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Builds the child command, applying the file-backed stdin when requested.
    fn command(request: &ProcessRequest) -> Result<Command, ProcessFailure> {
        let mut command = Command::new(&request.program);
        if let Some(directory) = &request.directory {
            command.current_dir(directory);
        }
        command.args(&request.arguments);
        for name in &request.removed {
            command.env_remove(name);
        }
        for (name, value) in &request.environment {
            command.env(name, value);
        }
        if let Some(path) = &request.stdin {
            let file = fs::File::open(path).map_err(|error| Self::failure(request, error))?;
            command.stdin(Stdio::from(file));
        }
        Ok(command)
    }

    fn failure(request: &ProcessRequest, error: std::io::Error) -> ProcessFailure {
        ProcessFailure {
            program: request.program.clone(),
            message: error.to_string(),
        }
    }
}

impl ProcessRunner for SystemProcesses {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessFailure> {
        match &request.stdout {
            Some(path) => {
                let file = fs::File::create(path).map_err(|error| Self::failure(request, error))?;
                let status = Self::command(request)?
                    .stdout(Stdio::from(file))
                    .status()
                    .map_err(|error| Self::failure(request, error))?;
                Ok(ProcessOutput {
                    success: status.success(),
                    code: status.code(),
                    signal: signal(&status),
                    stdout: Vec::new(),
                })
            }
            None => {
                let output = Self::command(request)?
                    .output()
                    .map_err(|error| Self::failure(request, error))?;
                Ok(ProcessOutput {
                    success: output.status.success(),
                    code: output.status.code(),
                    signal: signal(&output.status),
                    stdout: output.stdout,
                })
            }
        }
    }

    fn run_discarding_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        let status = Self::command(request)?
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| Self::failure(request, error))?;
        Ok(ProcessOutput {
            success: status.success(),
            code: status.code(),
            signal: signal(&status),
            stdout: Vec::new(),
        })
    }

    fn run_inheriting_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        let status = Self::command(request)?
            .status()
            .map_err(|error| Self::failure(request, error))?;
        Ok(ProcessOutput {
            success: status.success(),
            code: status.code(),
            signal: signal(&status),
            stdout: Vec::new(),
        })
    }

    fn run_streaming(
        &self,
        request: &ProcessRequest,
        consume: &mut dyn FnMut(Option<&mut dyn Read>) -> Result<(), String>,
    ) -> Result<ProcessOutput, ProcessFailure> {
        let mut child = Self::command(request)?
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| Self::failure(request, error))?;
        let result = match child.stdout.take() {
            Some(mut stream) => consume(Some(&mut stream)),
            None => consume(None),
        };
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child
            .wait()
            .map_err(|error| Self::failure(request, error))?;
        Ok(ProcessOutput {
            success: status.success(),
            code: status.code(),
            signal: signal(&status),
            stdout: Vec::new(),
        })
    }
}
