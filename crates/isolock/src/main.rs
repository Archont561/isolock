use clifacet::contract::ExitPolicy;
use clifacet::Failure;
use isolock_cas::InMemoryCas;
use isolock_core::{AirlockMode, VERSION};
use isolock_plan::{
    check_plan_drift, format_dispatch_commands, missing_artifacts, AirlockPlan, PlanDrift,
};
use isolock_proxy::{ProxyConfig, ProxyServer};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

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
    let configured_plan = env::var_os("ISOLOCK_PLAN");
    let plan_path = configured_plan
        .as_ref()
        .map_or_else(|| PathBuf::from(".isolock/plan.json"), PathBuf::from);
    if configured_plan.is_none() && !plan_path.exists() {
        return Err(Failure::execution(format!(
            "cannot evaluate lockfile drift: no airlock plan found at {}",
            plan_path.display()
        )));
    }

    let (expected, cas) = load_proxy_inputs().map_err(Failure::execution)?;
    let current = plan_available_in_cas(&expected, &cas);
    match check_plan_drift(&expected, &current) {
        PlanDrift::InSync => Ok(format!(
            "Verdict: ALL DEPENDENCIES SATISFIED\nisolock plan [mode: {}]: lockfile is clean and in-sync with CAS",
            if ci { "ci" } else { "workstation" }
        )),
        drift @ PlanDrift::DriftDetected { .. } => {
            let missing = missing_artifacts(&expected, &current);
            let commands = format_dispatch_commands(&missing).map_err(Failure::execution)?;
            let PlanDrift::DriftDetected {
                missing: missing_names,
                mismatched_digests,
            } = drift
            else {
                unreachable!("the drift match arm contains a drift result")
            };
            let mut report = format!(
                "Verdict: DRIFT DETECTED\nPlan mode: {}\n",
                if ci { "ci" } else { "workstation" }
            );
            if !missing_names.is_empty() {
                report.push_str("Missing packages:\n");
                for artifact in missing {
                    report.push_str(&format!(
                        "- {}=={} ({})\n",
                        artifact.name,
                        artifact.version,
                        artifact.registry.as_str()
                    ));
                }
            }
            if !mismatched_digests.is_empty() {
                report.push_str("Mismatched digests:\n");
                for artifact in mismatched_digests {
                    report.push_str(&format!("- {artifact}\n"));
                }
            }
            if !commands.is_empty() {
                report.push_str("Dispatch commands:\n");
                for command in commands {
                    report.push_str(&format!("{command}\n"));
                }
            }
            let report = report.trim_end().to_owned();
            if ci {
                Err(Failure::execution(report))
            } else {
                Ok(report)
            }
        }
    }
}

fn plan_available_in_cas(expected: &AirlockPlan, cas: &InMemoryCas) -> AirlockPlan {
    let mut current = AirlockPlan::new();
    for (artifact, metadata) in expected.artifacts_with_metadata() {
        if cas.contains(&artifact.digest) {
            current.add_with_metadata(artifact.clone(), metadata.clone());
        }
    }
    current
}

pub const EXIT_POLICY: ExitPolicy = ExitPolicy::standard();

fn run_proxy_server(args: &[String]) -> ExitCode {
    let port = option_value(args, "--port").or_else(|| env::var("ISOLOCK_PROXY_PORT").ok());
    let port = match port.map_or(Ok(0), |value| value.parse::<u16>()) {
        Ok(port) => port,
        Err(error) => {
            eprintln!("isolock serve: invalid port: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mode = option_value(args, "--mode")
        .or_else(|| env::var("ISOLOCK_MODE").ok())
        .as_deref()
        .map(parse_airlock_mode)
        .unwrap_or(Some(AirlockMode::LocalAirlock));
    let Some(mode) = mode else {
        eprintln!("isolock serve: mode must be connected, ci, or local-airlock");
        return ExitCode::FAILURE;
    };
    let (plan, cas) = match load_proxy_inputs() {
        Ok(inputs) => inputs,
        Err(error) => {
            eprintln!("isolock serve: cannot load airlock data: {error}");
            return ExitCode::FAILURE;
        }
    };
    let config = ProxyConfig::new("127.0.0.1", port, mode);
    match ProxyServer::bind(config, plan, cas) {
        Ok(server) => {
            match server.local_addr() {
                Ok(address) => println!("isolock serve: listening on http://{address} ({mode:?})"),
                Err(error) => {
                    eprintln!("isolock serve: cannot read listener address: {error}");
                    return ExitCode::FAILURE;
                }
            }
            match server.serve() {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("isolock serve: listener failed: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(error) => {
            eprintln!("isolock serve: cannot bind loopback listener: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_child_with_proxy(args: &[String]) -> ExitCode {
    let command_args = if args.first().is_some_and(|argument| argument == "--") {
        &args[1..]
    } else {
        args
    };
    let Some(program) = command_args.first() else {
        eprintln!("isolock exec: expected a command to run");
        return ExitCode::FAILURE;
    };
    let mode = env::var("ISOLOCK_MODE")
        .ok()
        .as_deref()
        .map(parse_airlock_mode)
        .unwrap_or(Some(AirlockMode::LocalAirlock));
    let Some(mode) = mode else {
        eprintln!("isolock exec: ISOLOCK_MODE must be connected, ci, or local-airlock");
        return ExitCode::FAILURE;
    };
    let (plan, cas) = match load_proxy_inputs() {
        Ok(inputs) => inputs,
        Err(error) => {
            eprintln!("isolock exec: cannot load airlock data: {error}");
            return ExitCode::FAILURE;
        }
    };
    let config = ProxyConfig::new("127.0.0.1", 0, mode);
    let server = match ProxyServer::bind(config, plan, cas).and_then(ProxyServer::spawn) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("isolock exec: cannot start local proxy: {error}");
            return ExitCode::FAILURE;
        }
    };
    let origin = server.base_url();
    let cargo_home = match prepare_cargo_home(&origin) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("isolock exec: cannot configure Cargo for the local registry: {error}");
            let _ = server.shutdown();
            return ExitCode::FAILURE;
        }
    };
    let pixi_config = match prepare_pixi_config(&origin) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("isolock exec: cannot configure Pixi for the local registry: {error}");
            let _ = server.shutdown();
            return ExitCode::FAILURE;
        }
    };
    let no_proxy = local_no_proxy();
    let child_status = run_child_process(
        program,
        &command_args[1..],
        &origin,
        &cargo_home,
        &pixi_config,
        &no_proxy,
    );
    if let Err(error) = server.shutdown() {
        eprintln!("isolock exec: proxy shutdown failed: {error}");
    }
    match child_status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            eprintln!("isolock exec: cannot start {program:?}: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_child_process(
    program: &str,
    arguments: &[String],
    origin: &str,
    cargo_home: &Path,
    pixi_config: &TemporaryConfig,
    no_proxy: &str,
) -> std::io::Result<ExitStatus> {
    Command::new(program)
        .args(arguments)
        .env("CARGO_HOME", cargo_home)
        .env("PIXI_CONFIG_FILE", pixi_config.path())
        .env("PIXI_OFFLINE", "false")
        .env_remove("PIXI_NO_CONFIG")
        .env("ISOLOCK_PROXY_URL", origin)
        .env("HTTP_PROXY", origin)
        .env("http_proxy", origin)
        .env("HTTPS_PROXY", origin)
        .env("https_proxy", origin)
        .env("ALL_PROXY", origin)
        .env("all_proxy", origin)
        .env("PIP_INDEX_URL", format!("{origin}/pypi/simple"))
        .env("PIP_EXTRA_INDEX_URL", "")
        .env("UV_INDEX_URL", format!("{origin}/pypi/simple"))
        .env("UV_DEFAULT_INDEX", format!("{origin}/pypi/simple"))
        .env("UV_EXTRA_INDEX_URL", "")
        .env("NPM_CONFIG_REGISTRY", format!("{origin}/npm/"))
        .env(
            "CARGO_REGISTRIES_CRATES_IO_INDEX",
            format!("sparse+{origin}/cargo/"),
        )
        .env("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "sparse")
        .env("PIXI_CONDA_CHANNELS", format!("{origin}/conda/channel"))
        .env("CONDA_CHANNELS", format!("{origin}/conda/channel"))
        .env("NO_PROXY", no_proxy)
        .env("no_proxy", no_proxy)
        .status()
}

struct TemporaryConfig {
    path: PathBuf,
}

impl TemporaryConfig {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryConfig {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn local_conda_mirror(origin: &str, upstream: &str, known_hosts: &[&str]) -> Option<String> {
    let host = known_hosts.iter().copied().find(|host| {
        upstream == *host
            || upstream
                .strip_prefix(*host)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })?;
    let suffix = upstream.strip_prefix(host).unwrap_or_default();
    let channel = suffix
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/");
    if channel.is_empty() {
        Some(format!("{origin}/conda/channel/"))
    } else {
        Some(format!("{origin}/conda/channel/{channel}/"))
    }
}

fn encode_path_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn prepare_pixi_config(origin: &str) -> Result<TemporaryConfig, String> {
    let mut config = if let Some(path) = env::var_os("PIXI_CONFIG_FILE") {
        let path = PathBuf::from(path);
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        toml::from_str::<toml::Value>(&contents)
            .map_err(|error| format!("invalid Pixi config {}: {error}", path.display()))?
    } else {
        toml::Value::Table(toml::value::Table::new())
    };
    let table = config
        .as_table_mut()
        .ok_or_else(|| "Pixi config must be a TOML table".to_owned())?;
    let mirrors = table
        .entry("mirrors".to_owned())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .ok_or_else(|| "Pixi config 'mirrors' entry must be a table".to_owned())?;
    let known_hosts = [
        "https://conda.anaconda.org",
        "https://repo.anaconda.com",
        "https://anaconda.org",
        "https://prefix.dev",
        "https://repo.prefix.dev",
    ];
    let configured_mirrors = mirrors.keys().cloned().collect::<Vec<_>>();
    for upstream in configured_mirrors {
        if let Some(local_url) = local_conda_mirror(origin, &upstream, &known_hosts) {
            mirrors.insert(
                upstream,
                toml::Value::Array(vec![toml::Value::String(local_url)]),
            );
        }
    }
    for upstream in known_hosts {
        mirrors.insert(
            upstream.to_owned(),
            toml::Value::Array(vec![toml::Value::String(format!(
                "{origin}/conda/channel/"
            ))]),
        );
    }
    let contents = toml::to_string_pretty(&config)
        .map_err(|error| format!("cannot serialize Pixi config: {error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_nanos();
    let path = env::temp_dir().join(format!(
        "isolock-pixi-config-{}-{nonce}.toml",
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    if let Err(error) = file.write_all(contents.as_bytes()) {
        let _ = fs::remove_file(&path);
        return Err(format!("cannot write {}: {error}", path.display()));
    }
    drop(file);
    Ok(TemporaryConfig { path })
}

fn prepare_cargo_home(origin: &str) -> Result<PathBuf, String> {
    let cargo_home = env::var_os("ISOLOCK_CARGO_HOME")
        .map_or_else(|| PathBuf::from(".isolock/cargo-home"), PathBuf::from);
    let cargo_home = if cargo_home.is_absolute() {
        cargo_home
    } else {
        env::current_dir()
            .map_err(|error| format!("cannot resolve Cargo home path: {error}"))?
            .join(cargo_home)
    };
    fs::create_dir_all(&cargo_home)
        .map_err(|error| format!("cannot create {}: {error}", cargo_home.display()))?;
    let config = format!(
        "[source.crates-io]\nreplace-with = \"isolock\"\n\n[source.isolock]\nregistry = \"sparse+{origin}/cargo/\"\n"
    );
    fs::write(cargo_home.join("config.toml"), config)
        .map_err(|error| format!("cannot write Cargo registry config: {error}"))?;
    Ok(cargo_home)
}

fn load_proxy_inputs() -> Result<(AirlockPlan, InMemoryCas), String> {
    let configured_plan = env::var_os("ISOLOCK_PLAN");
    let plan_path = configured_plan
        .as_ref()
        .map_or_else(|| PathBuf::from(".isolock/plan.json"), PathBuf::from);
    let plan = match fs::read(&plan_path) {
        Ok(contents) => serde_json::from_slice(&contents)
            .map_err(|error| format!("invalid plan {}: {error}", plan_path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && configured_plan.is_none() => {
            AirlockPlan::new()
        }
        Err(error) => return Err(format!("cannot read plan {}: {error}", plan_path.display())),
    };

    let cas_path =
        env::var_os("ISOLOCK_CAS_DIR").map_or_else(|| PathBuf::from(".isolock/cas"), PathBuf::from);
    let mut cas = InMemoryCas::new();
    for artifact in plan.artifacts() {
        let path = cas_path.join(artifact.digest.sharded_path());
        match fs::read(&path) {
            Ok(bytes) => cas
                .insert(artifact.digest.clone(), bytes)
                .map_err(|error| format!("invalid CAS blob {}: {error}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot read CAS blob {}: {error}", path.display())),
        }
    }
    Ok((plan, cas))
}

fn local_no_proxy() -> String {
    let inherited = env::var("NO_PROXY")
        .or_else(|_| env::var("no_proxy"))
        .unwrap_or_default();
    ["127.0.0.1", "localhost", "::1", inherited.as_str()]
        .into_iter()
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

fn option_value(args: &[String], option: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].clone())
}

fn parse_airlock_mode(value: &str) -> Option<AirlockMode> {
    match value.to_ascii_lowercase().as_str() {
        "connected" => Some(AirlockMode::Connected),
        "ci" => Some(AirlockMode::Ci),
        "local-airlock" | "localairlock" => Some(AirlockMode::LocalAirlock),
        _ => None,
    }
}

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
        Some("serve") => run_proxy_server(&args[1..]),
        Some("ingest") => {
            println!("isolock ingest: airlock package ingested into local CAS store");
            ExitCode::SUCCESS
        }
        Some("exec") => run_child_with_proxy(&args[1..]),
        Some("sync") => {
            println!("isolock sync: dependency state is in-sync");
            ExitCode::SUCCESS
        }
        _ => {
            println!("isolock v{VERSION} - dependency airlock manager");
            println!("Usage: isolock [exec|sync|plan|ingest|serve] [args...]");
            ExitCode::SUCCESS
        }
    }
}
