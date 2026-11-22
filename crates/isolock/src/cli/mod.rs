//! One clifacet pipeline owns parsing, validation, command dispatch and diagnostics.

mod plan;
mod profiles;
mod proxy;

use crate::config::Overrides;
use clifacet::contract::ExitPolicy;
use clifacet::policy::Policy;
use clifacet::{App, Command, Failure, Handler};
use profiles::{Config, Export, Fetch, Gc, Plan, SeedManifest, Sync};
use proxy::{Exec, Ingest, Serve};
use std::cell::Cell;
use std::io::Write;
use std::process::ExitCode;

#[clifacet::command(input = Invocation)]
#[derive(Clone)]
#[command(
    name = "isolock",
    version = isolock_core::VERSION,
    about = "Hermetic dependency airlock manager",
    propagate_version = true,
    arg_required_else_help = true
)]
pub(crate) struct Isolock {
    #[command(flatten)]
    #[clifacet(skip)]
    overrides: Overrides,
    #[clifacet(subcommands)]
    command: Commands,
}

pub(crate) struct Invocation {
    command: Request,
}

// Subcommands report the application version, not a separate per-command version.
#[clifacet::subcommands(input = Request, output = Output, context = Overrides)]
#[derive(Clone)]
enum Commands {
    /// Validate or explain configuration without fetching artifacts.
    #[command(display_name = "isolock")]
    Config(Config),
    /// Generate/check a profile plan, or inspect the legacy airlock plan.
    #[command(display_name = "isolock")]
    Plan(Plan),
    /// Retrieve the selected manifest graph and its missing blobs.
    #[command(display_name = "isolock")]
    Fetch(Fetch),
    /// Materialize the selected locked profile.
    #[clifacet(visible_alias = "bootstrap")]
    #[command(display_name = "isolock")]
    Sync(Sync),
    /// Export the selected profile to an offline directory snapshot.
    #[command(display_name = "isolock")]
    Export(Export),
    /// Write the reviewed POSIX seed manifest and bootstrap script.
    #[command(display_name = "isolock")]
    SeedManifest(SeedManifest),
    /// Report unreachable CAS blobs; deletion requires --apply.
    #[command(display_name = "isolock")]
    Gc(Gc),
    /// Serve the locked registry snapshot on loopback.
    #[command(display_name = "isolock")]
    Serve(Serve),
    /// Run a child command with the local registry proxy.
    #[command(display_name = "isolock")]
    Exec(Exec),
    /// Legacy ingestion entrypoint.
    #[command(display_name = "isolock")]
    Ingest(Ingest),
}

pub(crate) enum Request {
    Config(Config),
    Plan(Plan),
    Fetch(Fetch),
    Sync(Sync),
    Export(Export),
    SeedManifest(SeedManifest),
    Gc(Gc),
    Serve(Serve),
    Exec(Exec),
    Ingest(Ingest),
}

impl Command for Isolock {
    type Input = Invocation;
    type Output = Output;

    fn execute(&self, input: Invocation) -> Result<Output, Failure> {
        self.command.handle(input.command, &self.overrides)
    }
}

/// Text, an already-reported operation, and a child status must not be conflated.
pub(crate) enum Output {
    Text(String),
    Done,
    Child(ExitCode),
}

impl From<String> for Output {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}
impl From<()> for Output {
    fn from((): ()) -> Self {
        Self::Done
    }
}
impl From<ExitCode> for Output {
    fn from(code: ExitCode) -> Self {
        Self::Child(code)
    }
}

pub(crate) fn run() -> ExitCode {
    let child_status = Cell::new(ExitCode::SUCCESS);
    let status = App::<Isolock>::new()
        .policy(Policy::non_interactive())
        .exit(ExitPolicy::standard())
        .render(|output: Output, out: &mut dyn Write| match output {
            Output::Text(text) => writeln!(out, "{text}"),
            Output::Done => Ok(()),
            Output::Child(code) => {
                child_status.set(code);
                Ok(())
            }
        })
        .run();
    // clifacet owns usage/validation/execution errors. A completed child owns its
    // exact status (including signals), but may never hide a runner/rendering failure.
    if status == ExitCode::SUCCESS {
        child_status.get()
    } else {
        status
    }
}
