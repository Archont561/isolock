//! Legacy command parsing and orchestration over resolved execution collaborators.

use crate::config::{ExecutionConfig, Overrides};
use crate::execution::{exit, inputs, ExecutionSession};
use clap::Parser;
use isolock_proxy::ProxyServer;
use std::process::ExitCode;

pub struct ExecCommand<'a> {
    pub program: &'a str,
    pub arguments: &'a [String],
}
impl<'a> ExecCommand<'a> {
    pub fn parse(arguments: &'a [String]) -> Result<Self, &'static str> {
        let arguments = if arguments.first().is_some_and(|argument| argument == "--") {
            &arguments[1..]
        } else {
            arguments
        };
        let (program, arguments) = arguments.split_first().ok_or("expected a command to run")?;
        Ok(Self { program, arguments })
    }
}

#[derive(Parser)]
#[command(
    name = "isolock serve",
    about = "Serve the locked registry snapshot on loopback"
)]
struct ServeCommand {
    #[command(flatten)]
    overrides: Overrides,
}

pub fn run_child_with_proxy(arguments: &[String]) -> ExitCode {
    let result = (|| {
        let command = ExecCommand::parse(arguments).map_err(str::to_owned)?;
        let config = ExecutionConfig::load(&Overrides::default())?;
        let session = ExecutionSession::start(&config).map_err(|error| error.to_string())?;
        let status = session.run(&command);
        if let Err(error) = session.close() {
            eprintln!("isolock exec: proxy shutdown failed: {error}");
        }
        status
            .map(exit::map_status)
            .map_err(|error| error.to_string())
    })();
    report("exec", result)
}

pub fn run_proxy_server(arguments: &[String]) -> ExitCode {
    let command = match ServeCommand::try_parse_from(
        std::iter::once("isolock serve".to_owned()).chain(arguments.iter().cloned()),
    ) {
        Ok(command) => command,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(1));
        }
    };
    let result = (|| {
        let config = ExecutionConfig::load(&command.overrides)?;
        let (plan, cas) = inputs::load(&config)?;
        let server = ProxyServer::bind(config.proxy.clone(), plan, cas)
            .map_err(|error| format!("cannot bind loopback listener: {error}"))?;
        println!(
            "isolock serve: listening on {} ({:?})",
            server.base_url(),
            config.proxy.mode
        );
        server
            .serve()
            .map(|()| ExitCode::SUCCESS)
            .map_err(|error| format!("listener failed: {error}"))
    })();
    report("serve", result)
}

fn report(command: &str, result: Result<ExitCode, String>) -> ExitCode {
    result.unwrap_or_else(|error| {
        eprintln!("isolock {command}: {error}");
        ExitCode::FAILURE
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_parser_preserves_program_arguments_and_optional_separator() {
        for arguments in [vec!["tool", "--version"], vec!["--", "tool", "--version"]] {
            let arguments = arguments.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let command = ExecCommand::parse(&arguments).unwrap();
            assert_eq!(command.program, "tool");
            assert_eq!(command.arguments, ["--version"]);
        }
        assert!(ExecCommand::parse(&[]).is_err());
        assert!(ExecCommand::parse(&["--".into()]).is_err());
    }
}
