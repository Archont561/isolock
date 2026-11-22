mod cli;
mod config;
mod execution;
mod ingestion;
mod materialize;
mod profile_cli;
mod profile_store;
mod proxy_cli;
mod transport;

fn main() -> std::process::ExitCode {
    cli::run()
}
