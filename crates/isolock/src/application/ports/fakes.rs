//! Fake ports for infrastructure adapter tests that must not touch the system.
//!
//! Nothing here spawns a process, opens a socket, or writes outside a caller
//! supplied temporary directory. Every observation a test needs is recorded so
//! assertions can be made about *what would have run*, not just about outcomes.

use crate::application::ports::{ProcessFailure, ProcessOutput, ProcessRequest, ProcessRunner};
use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

/// One recorded invocation and the stdout it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub request: ProcessRequest,
    pub stdout: Vec<u8>,
    pub success: bool,
}

impl Invocation {
    /// The program that was requested, for concise assertions.
    #[must_use]
    pub fn program(&self) -> String {
        self.request.program.to_string_lossy().into_owned()
    }

    /// The arguments that were requested, joined with single spaces.
    #[must_use]
    pub fn arguments(&self) -> String {
        self.request.arguments.join(" ")
    }

    /// Whether the request carried `name` in its environment.
    #[must_use]
    pub fn environment(&self, name: &str) -> Option<String> {
        self.request
            .environment
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    }

    /// Whether the request removed `name` from the inherited environment.
    #[must_use]
    pub fn removed(&self, name: &str) -> bool {
        self.request.removed.iter().any(|key| key == name)
    }
}

/// A scripted outcome for one invocation.
#[derive(Debug, Clone)]
enum Step {
    /// Exit with this status and write these bytes to stdout.
    Exit {
        success: bool,
        code: Option<i32>,
        stdout: Vec<u8>,
    },
    /// Fail to start.
    Fail(&'static str),
    /// Exit successfully after streaming `bytes` to the consumer.
    Stream { bytes: Vec<u8>, success: bool },
}

/// Records requests and answers them from a script, so no process is spawned.
#[derive(Default)]
pub struct FakeProcesses {
    script: Mutex<VecDeque<Step>>,
    invocations: Mutex<Vec<Invocation>>,
}

impl FakeProcesses {
    /// Builds a fake that answers every request with a successful, silent exit.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues one successful exit with no output.
    #[must_use]
    pub fn succeeding(self) -> Self {
        self.script
            .lock()
            .expect("fake script lock")
            .push_back(Step::Exit {
                success: true,
                code: Some(0),
                stdout: Vec::new(),
            });
        self
    }

    /// Queues one successful exit that writes `stdout`.
    #[must_use]
    pub fn printing(self, stdout: impl Into<Vec<u8>>) -> Self {
        self.script
            .lock()
            .expect("fake script lock")
            .push_back(Step::Exit {
                success: true,
                code: Some(0),
                stdout: stdout.into(),
            });
        self
    }

    /// Queues one failure that still produced `stdout`.
    #[must_use]
    pub fn failing_with(self, code: i32, stdout: impl Into<Vec<u8>>) -> Self {
        self.script
            .lock()
            .expect("fake script lock")
            .push_back(Step::Exit {
                success: false,
                code: Some(code),
                stdout: stdout.into(),
            });
        self
    }

    /// Queues one failure to start.
    #[must_use]
    pub fn unavailable(self, message: &'static str) -> Self {
        self.script
            .lock()
            .expect("fake script lock")
            .push_back(Step::Fail(message));
        self
    }

    /// Queues one invocation whose stdout is streamed to the consumer.
    #[must_use]
    pub fn streaming(self, bytes: impl Into<Vec<u8>>, success: bool) -> Self {
        self.script
            .lock()
            .expect("fake script lock")
            .push_back(Step::Stream {
                bytes: bytes.into(),
                success,
            });
        self
    }

    /// Every request this fake has answered, in order.
    #[must_use]
    pub fn invocations(&self) -> Vec<Invocation> {
        self.invocations.lock().expect("fake script lock").clone()
    }

    /// The programs requested, in order.
    #[must_use]
    pub fn programs(&self) -> Vec<String> {
        self.invocations
            .lock()
            .expect("fake script lock")
            .iter()
            .map(Invocation::program)
            .collect()
    }

    /// The single recorded invocation, panicking when the count differs.
    #[must_use]
    pub fn single(&self) -> Invocation {
        let invocations = self.invocations.lock().expect("fake script lock");
        assert_eq!(invocations.len(), 1, "expected exactly one invocation");
        invocations[0].clone()
    }

    fn next(&self) -> Step {
        self.script
            .lock()
            .expect("fake script lock")
            .pop_front()
            .unwrap_or(Step::Exit {
                success: true,
                code: Some(0),
                stdout: Vec::new(),
            })
    }

    fn record(&self, request: &ProcessRequest, stdout: Vec<u8>, success: bool) -> Invocation {
        let invocation = Invocation {
            request: request.clone(),
            stdout,
            success,
        };
        self.invocations
            .lock()
            .expect("fake script lock")
            .push(invocation.clone());
        invocation
    }
}

impl ProcessRunner for FakeProcesses {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessFailure> {
        match self.next() {
            Step::Exit {
                success,
                code,
                stdout,
            } => {
                self.record(request, stdout.clone(), success);
                Ok(ProcessOutput {
                    success,
                    code,
                    signal: None,
                    stdout,
                })
            }
            Step::Fail(message) => {
                self.record(request, Vec::new(), false);
                Err(ProcessFailure {
                    program: request.program.clone(),
                    message: message.into(),
                })
            }
            Step::Stream { .. } => unreachable!("a streaming step cannot answer a plain run"),
        }
    }

    fn run_discarding_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        self.run(request)
    }

    fn run_inheriting_output(
        &self,
        request: &ProcessRequest,
    ) -> Result<ProcessOutput, ProcessFailure> {
        self.run(request)
    }

    fn run_streaming(
        &self,
        request: &ProcessRequest,
        consume: &mut dyn FnMut(Option<&mut dyn Read>) -> Result<(), String>,
    ) -> Result<ProcessOutput, ProcessFailure> {
        match self.next() {
            Step::Stream { bytes, success } => {
                let mut cursor = std::io::Cursor::new(bytes.clone());
                let result = consume(Some(&mut cursor));
                self.record(request, Vec::new(), success && result.is_ok());
                Ok(ProcessOutput {
                    success,
                    code: if success { Some(0) } else { Some(1) },
                    signal: None,
                    stdout: Vec::new(),
                })
            }
            step => {
                let _ = step;
                self.record(request, Vec::new(), false);
                Ok(ProcessOutput {
                    success: false,
                    code: Some(1),
                    signal: None,
                    stdout: Vec::new(),
                })
            }
        }
    }
}

/// Writes `bytes` to a new file inside `directory` and returns its path.
#[must_use]
pub fn write_file(directory: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, bytes).expect("fake fixture file");
    path
}
