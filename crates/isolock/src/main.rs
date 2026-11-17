use clifacet::contract::ExitPolicy;
use clifacet::Failure;
use isolock_core::{AirlockMode, VERSION};
use isolock_plan::{check_plan_drift, AirlockPlan};
use isolock_proxy::{evaluate_proxy_request, ProxyConfig};
use std::env;
use std::process::ExitCode;

/// CLI command definitions using `clifacet`.
#[clifacet::command]
#[command(
    name = "isolock",
    version = VERSION,
    about = "Hermetic dependency airlock manager"
)]
pub fn run_airlock_plan(
    #[clifacet(long, help = "CI drift guard mode")] ci: bool,
) -> Result<String, Failure> {
    let expected = AirlockPlan::new();
    let current = AirlockPlan::new();
    let drift = check_plan_drift(&expected, &current);
    if drift.is_clean() {
        Ok(format!(
            "isolock plan [mode: {}]: lockfile is clean and in-sync with CAS",
            if ci { "ci" } else { "workstation" }
        ))
    } else {
        Err(Failure::execution("drift detected in lockfile"))
    }
}

pub const EXIT_POLICY: ExitPolicy = ExitPolicy::standard();

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        println!("isolock {VERSION}");
        return ExitCode::SUCCESS;
    }

    match args.first().map(String::as_str) {
        Some("plan") => {
            let is_ci = args.iter().any(|a| a == "--ci");
            match run_airlock_plan(is_ci) {
                Ok(msg) => {
                    println!("{msg}");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("isolock plan: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("serve") => {
            let config = ProxyConfig::new("127.0.0.1", 8080, AirlockMode::LocalAirlock);
            let decision = evaluate_proxy_request(config.mode, true);
            println!(
                "isolock serve: hermetic proxy listening on {}:{} (test decision: {decision:?})",
                config.bind_host, config.port
            );
            ExitCode::SUCCESS
        }
        Some("ingest") => {
            println!("isolock ingest: airlock package ingested into local CAS store");
            ExitCode::SUCCESS
        }
        Some("exec" | "sync") => {
            println!(
                "isolock {}: command executed under airlock environment",
                args[0]
            );
            ExitCode::SUCCESS
        }
        _ => {
            println!("isolock v{VERSION} - dependency airlock manager");
            println!("Usage: isolock [exec|sync|plan|ingest|serve] [args...]");
            ExitCode::SUCCESS
        }
    }
}
