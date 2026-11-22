//! Proxy orchestration over already-parsed arguments and resolved collaborators.

use crate::config::{ExecutionConfig, Overrides};
use crate::execution::{exit, inputs, ExecutionSession};
use isolock_proxy::ProxyServer;
use std::process::ExitCode;

pub struct ExecCommand<'a> {
    pub program: &'a str,
    pub arguments: &'a [String],
}

pub fn run_child_with_proxy(
    command: &ExecCommand<'_>,
    overrides: &Overrides,
) -> Result<ExitCode, String> {
    let config = ExecutionConfig::load(overrides)?;
    let session = ExecutionSession::start(&config).map_err(|error| error.to_string())?;
    let status = session.run(command);
    if let Err(error) = session.close() {
        eprintln!("isolock exec: proxy shutdown failed: {error}");
    }
    status
        .map(exit::map_status)
        .map_err(|error| error.to_string())
}

pub fn run_proxy_server(overrides: &Overrides) -> Result<(), String> {
    let config = ExecutionConfig::load(overrides)?;
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
        .map_err(|error| format!("listener failed: {error}"))
}
