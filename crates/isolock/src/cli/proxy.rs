//! Typed proxy commands retain the existing session owner and child-exit mapping.

use crate::config::Overrides;
use crate::proxy_cli;
use clifacet::{Command, Failure, Handler};
use std::process::ExitCode;

#[clifacet::command(input = Serve, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Serve {}

impl Handler<Overrides> for Serve {
    type Input = Serve;
    type Output = ();

    fn handle(&self, _: Serve, flags: &Overrides) -> Result<(), Failure> {
        proxy_cli::run_proxy_server(flags).map_err(Failure::execution)
    }
}

#[clifacet::command(input = Exec, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Exec {
    /// The child command and its arguments; everything after COMMAND is passed through.
    #[arg(
        required = true,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "COMMAND"
    )]
    arguments: Vec<String>,
}

impl Handler<Overrides> for Exec {
    type Input = Exec;
    type Output = ExitCode;

    fn handle(&self, input: Exec, flags: &Overrides) -> Result<ExitCode, Failure> {
        // clap owns the optional separator and the opaque tail. This only splits
        // the already-parsed invocation into the executable and its arguments.
        let (program, arguments) = input
            .arguments
            .split_first()
            .ok_or_else(|| Failure::validation("expected a child command"))?;
        let command = proxy_cli::ExecCommand { program, arguments };
        proxy_cli::run_child_with_proxy(&command, flags).map_err(Failure::execution)
    }
}

#[clifacet::command(input = Ingest, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Ingest {
    #[arg(long, value_parser = ["cargo", "pypi", "npm", "conda"])]
    registry: Option<String>,
    packages: Vec<String>,
}

impl Command for Ingest {
    type Input = Ingest;
    type Output = String;

    fn execute(&self, input: Ingest) -> Result<String, Failure> {
        // Preserve the existing ingestion placeholder; implementing transport is not
        // part of changing the application's parser and runner.
        let _ = (input.registry, input.packages);
        Ok("isolock ingest: airlock package ingested into local CAS store".into())
    }
}
