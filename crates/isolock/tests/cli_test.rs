#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
use isolock_cas::Sha256Digest;
use isolock_core::Registry;
use isolock_plan::{AirlockPlan, PlannedArtifact, PlannedArtifactMetadata};
use rstest::fixture;
use rstest_bdd_macros::{given, scenario, then, when};
use std::cell::RefCell;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn temporary_directory() -> TempDir {
    tempfile::Builder::new()
        .prefix("isolock-cli-test-")
        .tempdir()
        .expect("creates an isolated CLI fixture directory")
}

fn http_get(address: &str, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).expect("the listener accepts requests");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .expect("request writes");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("response is readable");
    response
}

#[test]
fn test_cli_version_flag() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    for arguments in [
        &["--version"][..],
        &["plan", "--version"],
        &["serve", "-V"],
        &["fetch", "--version"],
        &["sync", "--version"],
        &["bootstrap", "--version"],
        &["export", "--version"],
        &["seed-manifest", "--version"],
        &["gc", "--version"],
        &["ingest", "--version"],
        &["push", "--version"],
        &["config", "--version"],
        &["config", "check", "--version"],
        &["config", "show", "--version"],
        &["config", "explain", "--version"],
    ] {
        let output = Command::new(bin)
            .args(arguments)
            .output()
            .expect("runs a non-exec version flag");
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(stdout.trim(), format!("isolock {}", isolock_core::VERSION));
    }
}

#[test]
fn plan_fails_closed_when_no_expected_plan_is_available() {
    let fixture_dir = temporary_directory();
    let output = Command::new(env!("CARGO_BIN_EXE_isolock"))
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env_remove("ISOLOCK_PLAN")
        .env_remove("ISOLOCK_CAS_DIR")
        .output()
        .expect("runs isolock plan without an airlock plan");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no airlock plan found"));
}

#[test]
#[allow(clippy::too_many_lines)] // End-to-end fixture setup and lifecycle assertions stay together.
fn test_cli_subcommands() {
    let bin = env!("CARGO_BIN_EXE_isolock");

    let fixture_dir = temporary_directory();
    let cas_dir = fixture_dir.path().join("cas");
    let empty_plan_path = fixture_dir.path().join("empty-plan.json");
    fs::write(&empty_plan_path, r#"{"artifacts":[]}"#)
        .expect("writes an explicit empty plan for the clean-plan check");
    let plan_out = Command::new(bin)
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &empty_plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("runs isolock plan --ci");
    assert!(plan_out.status.success());
    assert!(
        String::from_utf8_lossy(&plan_out.stdout).contains("Verdict: ALL DEPENDENCIES SATISFIED")
    );

    let package_bytes = b"CLI fixture tarball bytes";
    let digest = Sha256Digest::from_bytes(package_bytes);
    let cas_blob = cas_dir.join(digest.sharded_path());
    fs::create_dir_all(cas_blob.parent().expect("CAS blob has a shard directory"))
        .expect("creates CAS shard directory");
    fs::write(&cas_blob, package_bytes).expect("writes verified CAS bytes");
    let plan_path = fixture_dir.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&serde_json::json!({
            "artifacts": [{
                "name": "fixture",
                "version": "1.0.0",
                "registry": "Npm",
                "digest": digest.as_str(),
            }],
            "artifact_metadata": [{
                "filename": "fixture-1.0.0.tgz",
                "registry_metadata": {
                    "package_json": {"name": "fixture", "version": "1.0.0"}
                },
            }],
        }))
        .expect("serializes fixture plan"),
    )
    .expect("writes fixture plan");
    let mut serve = Command::new(bin)
        .args(["serve", "--port", "0"])
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .stdout(Stdio::piped())
        .spawn()
        .expect("starts isolock serve");
    let stdout = serve.stdout.take().expect("serve has captured stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("serve prints its bound URL");
    assert!(line.contains("isolock serve: listening on http://"));
    let address = line
        .split_whitespace()
        .find_map(|word| word.strip_prefix("http://"))
        .expect("serve output contains the listener address");
    let cargo_response = http_get(address, "/cargo/config.json");
    assert!(cargo_response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(String::from_utf8_lossy(&cargo_response).contains("/cargo/api/v1/crates/"));

    let npm_response = http_get(address, "/npm/fixture");
    assert!(npm_response.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(String::from_utf8_lossy(&npm_response).contains("fixture-1.0.0.tgz"));
    let npm_download = http_get(address, "/npm/fixture/-/fixture-1.0.0.tgz");
    assert!(npm_download.starts_with(b"HTTP/1.1 200 OK\r\n"));
    assert!(npm_download.ends_with(package_bytes));

    serve.kill().expect("serve process stops");
    assert!(!serve.wait().expect("serve process exits").success());

    let ingest_out = Command::new(bin)
        .arg("ingest")
        .output()
        .expect("runs isolock ingest");
    assert_eq!(ingest_out.status.code(), Some(1));
    assert!(ingest_out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&ingest_out.stderr)
        .contains("cannot read reviewed ingestion manifest"));
}

#[test]
#[allow(clippy::too_many_lines)] // One public-CLI journey checks all supported registry wire formats.
fn cli_serves_all_four_registry_protocols_from_a_cold_locked_snapshot() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let fixture_dir = temporary_directory();
    let cas_dir = fixture_dir.path().join("cas");
    fs::create_dir_all(&cas_dir).expect("creates an isolated CAS directory");

    let artifacts = [
        (
            "Pypi",
            "requests",
            "2.32.3",
            "requests-2.32.3-py3-none-any.whl",
            b"fixture wheel bytes".as_slice(),
            serde_json::json!({"name":"requests","version":"2.32.3"}),
        ),
        (
            "Conda",
            "python",
            "3.12.7",
            "python-3.12.7-h55ee38b_0.conda",
            b"fixture conda bytes".as_slice(),
            serde_json::json!({
                "name":"python", "version":"3.12.7", "build":"h55ee38b_0",
                "build_number":0, "subdir":"linux-64", "depends":[]
            }),
        ),
        (
            "Npm",
            "zod",
            "3.23.8",
            "zod-3.23.8.tgz",
            b"fixture npm bytes".as_slice(),
            serde_json::json!({"package_json":{"name":"zod","version":"3.23.8"}}),
        ),
        (
            "Cargo",
            "serde",
            "1.0.210",
            "serde-1.0.210.crate",
            b"fixture cargo bytes".as_slice(),
            serde_json::json!({
                "Cargo.toml": r#"[package]
name = "serde"
version = "1.0.210"
"#
            }),
        ),
    ];
    let mut planned = Vec::new();
    let mut metadata = Vec::new();
    let mut pypi_blob = None;
    for (registry, name, version, filename, bytes, registry_metadata) in artifacts {
        let digest = Sha256Digest::from_bytes(bytes);
        let blob = cas_dir.join(digest.sharded_path());
        fs::create_dir_all(blob.parent().expect("CAS digest has a parent shard"))
            .expect("creates CAS shard directory");
        fs::write(&blob, bytes).expect("writes a reviewed fixture blob");
        if registry == "Pypi" {
            pypi_blob = Some((blob, bytes.to_vec()));
        }
        planned.push(serde_json::json!({
            "name": name, "version": version, "registry": registry, "digest": digest.as_str()
        }));
        metadata.push(serde_json::json!({
            "filename": filename, "registry_metadata": registry_metadata
        }));
    }
    let plan_path = fixture_dir.path().join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&serde_json::json!({
            "artifacts": planned,
            "artifact_metadata": metadata,
        }))
        .expect("serializes the reviewed plan"),
    )
    .expect("writes the reviewed plan");

    let plan = Command::new(bin)
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("checks the real CLI plan");
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    assert!(String::from_utf8_lossy(&plan.stdout).contains("Verdict: ALL DEPENDENCIES SATISFIED"));

    let (pypi_blob, pypi_bytes) = pypi_blob.expect("fixture includes a PyPI wheel");
    fs::write(&pypi_blob, b"tampered bytes").expect("corrupts the planned wheel");
    let drift = Command::new(bin)
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("rechecks the tampered CAS through the CLI");
    assert_eq!(drift.status.code(), Some(1));
    let drift_output = format!(
        "{}{}",
        String::from_utf8_lossy(&drift.stdout),
        String::from_utf8_lossy(&drift.stderr)
    );
    assert!(
        drift_output.contains("SHA-256 mismatch"),
        "tampering fails with an integrity diagnostic: {drift_output}"
    );
    fs::write(&pypi_blob, pypi_bytes).expect("restores the reviewed wheel for proxy use");

    let child_test = std::env::current_exe().expect("locates this integration-test executable");
    let output = Command::new(bin)
        .arg("exec")
        .arg(child_test)
        .args([
            "--exact",
            "exec_child_probes_four_registry_protocols",
            "--nocapture",
        ])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .env("ISOLOCK_M8_PROXY_PROBE", "1")
        .output()
        .expect("runs an independent client through the public exec command");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn exec_child_probes_four_registry_protocols() {
    if std::env::var_os("ISOLOCK_M8_PROXY_PROBE").is_none() {
        return;
    }
    let origin = std::env::var("ISOLOCK_PROXY_URL").expect("exec sets the local proxy URL");
    let address = origin
        .strip_prefix("http://")
        .expect("local proxy is plain HTTP")
        .to_owned();
    assert!(std::env::var("PIP_INDEX_URL")
        .expect("exec configures PyPI")
        .starts_with(&format!("{origin}/pypi/simple")));
    assert!(std::env::var("NPM_CONFIG_REGISTRY")
        .expect("exec configures npm")
        .starts_with(&format!("{origin}/npm/")));
    assert!(std::env::var("CARGO_REGISTRIES_CRATES_IO_INDEX")
        .expect("exec configures Cargo")
        .starts_with(&format!("sparse+{origin}/cargo/")));
    assert!(std::env::var("PIXI_CONDA_CHANNELS")
        .expect("exec configures Conda")
        .starts_with(&format!("{origin}/conda/channel")));

    for path in [
        "/pypi/simple/requests/",
        "/pypi/packages/requests-2.32.3-py3-none-any.whl",
        "/conda/channel/conda-forge/linux-64/repodata.json",
        "/conda/channel/conda-forge/linux-64/python-3.12.7-h55ee38b_0.conda",
        "/npm/zod",
        "/npm/zod/-/zod-3.23.8.tgz",
        "/cargo/config.json",
        "/cargo/se/rd/serde",
        "/cargo/api/v1/crates/serde/1.0.210/download",
    ] {
        let response = http_get(&address, path);
        assert!(
            response.starts_with(b"HTTP/1.1 200 OK\r\n"),
            "{path}: {response:?}"
        );
    }
    let miss = http_get(&address, "/npm/not-in-the-reviewed-snapshot");
    assert!(
        miss.starts_with(b"HTTP/1.1 403 Forbidden\r\n"),
        "unreviewed package fails closed: {miss:?}"
    );
}

#[test]
fn pull_request_workflow_skips_drift_audit_without_storage_ref() {
    let workflow: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../.github/workflows/isolock.yml"))
            .expect("workflow yaml parses");
    let pull_request_paths = workflow["on"]["pull_request"]["paths"]
        .as_sequence()
        .expect("pull request workflow has a path filter");
    assert!(pull_request_paths
        .iter()
        .any(|path| { path.as_str() == Some(".github/workflows/isolock.yml") }));

    let steps = workflow["jobs"]["plan-guard"]["steps"]
        .as_sequence()
        .expect("plan guard has workflow steps");
    let storage_step = steps
        .iter()
        .find(|step| step["id"].as_str() == Some("airlock-storage"))
        .expect("plan guard checks whether storage is initialized");
    let storage_command = storage_step["run"]
        .as_str()
        .expect("storage check calls the repository tool task");
    assert_eq!(
        storage_command,
        ".pixi/tools/linux-64/pixi run --as-is --offline tools airlock storage-ref"
    );

    let audit_step = steps
        .iter()
        .find(|step| step["name"].as_str() == Some("Run Isolock Hermeticity Audit"))
        .expect("plan guard has an audit step");
    assert_eq!(
        audit_step["if"].as_str(),
        Some("steps.airlock-storage.outputs.available == 'true'")
    );
}

fn assert_dispatch_command_matches_inputs<'a>(command: &'a str, options: &[&str]) -> &'a str {
    let fields = command
        .strip_prefix("gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=")
        .expect("command begins with the workflow and registry field");
    let (registry, packages_field) = fields
        .split_once(" -f packages=\"")
        .expect("command includes the string packages field");
    assert!(
        options.contains(&registry),
        "unknown workflow registry: {registry}"
    );
    assert!(packages_field.ends_with('"'), "packages input is quoted");
    let packages = packages_field
        .strip_suffix('"')
        .expect("packages value has a closing quote");
    assert!(!packages.is_empty());
    assert!(packages.split_whitespace().all(|package| package
        .split_once("==")
        .is_some_and(|(name, version)| { !name.is_empty() && !version.is_empty() })));
    registry
}

fn assert_reviewed_workflow_inputs(workflow: &serde_yaml::Value) -> Vec<&str> {
    let inputs = &workflow["on"]["workflow_dispatch"]["inputs"];
    assert_eq!(inputs["manifest"]["type"].as_str(), Some("string"));
    assert_eq!(inputs["manifest"]["required"].as_bool(), Some(true));
    assert_eq!(
        inputs["manifest"]["default"].as_str(),
        Some("isolock.ingest.json")
    );
    let registry_input = &inputs["registry"];
    assert_eq!(registry_input["type"].as_str(), Some("choice"));
    assert_eq!(registry_input["required"].as_bool(), Some(true));
    let options = registry_input["options"]
        .as_sequence()
        .expect("registry is a choice input")
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(options, ["cargo", "conda", "pypi", "npm"]);

    let packages_input = &inputs["packages"];
    assert_eq!(packages_input["type"].as_str(), Some("string"));
    assert_eq!(packages_input["required"].as_bool(), Some(true));
    options
}

#[test]
fn generated_dispatch_arguments_match_workflow_dispatch_inputs() {
    let fixture_dir = temporary_directory();
    let plan_path = fixture_dir.path().join("plan.json");
    let cas_dir = fixture_dir.path().join("cas");
    fs::create_dir_all(&cas_dir).expect("creates an empty CAS fixture");

    let cases = [
        ("serde", "1.0.210", Registry::Cargo),
        ("numpy", "2.1.0", Registry::Conda),
        ("fastapi", "0.115.0", Registry::Pypi),
        ("fixture", "1.0.0", Registry::Npm),
    ];
    let mut plan = AirlockPlan::new();
    for (name, version, registry) in cases {
        let bytes = format!("{name}=={version}").into_bytes();
        plan.add(PlannedArtifact {
            name: name.into(),
            version: version.into(),
            registry,
            digest: Sha256Digest::from_bytes(&bytes),
        });
    }
    fs::write(
        &plan_path,
        serde_json::to_vec(&plan).expect("serializes the registry plan"),
    )
    .expect("writes plan fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_isolock"))
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("runs the plan drift command");
    assert_eq!(output.status.code(), Some(1));
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow: serde_yaml::Value =
        serde_yaml::from_str(include_str!("../../../.github/workflows/isolock.yml"))
            .expect("workflow yaml parses");
    let options = assert_reviewed_workflow_inputs(&workflow);
    assert!(report.contains("Review and commit isolock.ingest.json"));

    let commands = report
        .lines()
        .filter(|line| line.starts_with("gh workflow run isolock.yml "))
        .collect::<Vec<_>>();
    assert_eq!(commands.len(), options.len(), "{report}");
    let mut generated_registries = std::collections::BTreeSet::new();
    for command in commands {
        let registry = assert_dispatch_command_matches_inputs(command, &options);
        assert!(
            generated_registries.insert(registry),
            "registry repeated: {registry}"
        );
    }
    assert_eq!(generated_registries.len(), options.len());
}

#[cfg(unix)]
#[test]
fn exec_injects_local_registry_urls_and_propagates_child_exit_codes() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let fixture_dir = temporary_directory();
    let original_pixi_config = fixture_dir.path().join("pixi-config.toml");
    fs::write(
        &original_pixi_config,
        "offline = true\n[mirrors]\n\"https://example.invalid\" = [\"https://mirror.example.invalid\"]\n\"https://conda.anaconda.org/conda-forge\" = [\"https://prefix.dev/conda-forge\"]\n",
    )
    .expect("writes an explicit Pixi config");
    let output = Command::new(bin)
        .args([
            "exec",
            "sh",
            "-c",
            "test -n \"$PIP_INDEX_URL\" && test -n \"$NPM_CONFIG_REGISTRY\" && test -n \"$CARGO_REGISTRIES_CRATES_IO_INDEX\" && test -n \"$PIXI_CONDA_CHANNELS\" && test \"$NO_PROXY\" != \"\" && test \"$HTTPS_PROXY\" = \"$ISOLOCK_PROXY_URL\" && test -f \"$PIXI_CONFIG_FILE\" && grep -q 'https://conda.anaconda.org' \"$PIXI_CONFIG_FILE\" && grep -q 'http://127.0.0.1:' \"$PIXI_CONFIG_FILE\" && grep -q 'https://example.invalid' \"$PIXI_CONFIG_FILE\" && grep -q 'conda/channel/conda-forge/' \"$PIXI_CONFIG_FILE\"",
        ])
        .env("PIXI_CONFIG_FILE", &original_pixi_config)
        .output()
        .expect("runs a child through the local proxy");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let failure = Command::new(bin)
        .args(["exec", "sh", "-c", "exit 7"])
        .status()
        .expect("runs the child process");
    assert_eq!(failure.code(), Some(7));
}

#[derive(Clone)]
struct PlanPackageFixture {
    filename: String,
    artifact: PlannedArtifact,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct PlanDispatchWorld {
    fixture_dir: RefCell<Option<TempDir>>,
    packages: RefCell<Vec<PlanPackageFixture>>,
    output: RefCell<Option<Output>>,
}

#[fixture]
fn world() -> PlanDispatchWorld {
    PlanDispatchWorld::default()
}

fn plan_project_dir(world: &PlanDispatchWorld) -> PathBuf {
    if let Some(path) = world
        .fixture_dir
        .borrow()
        .as_ref()
        .map(|fixture| fixture.path().to_path_buf())
    {
        return path;
    }

    let fixture = temporary_directory();
    let path = fixture.path().to_path_buf();
    fs::create_dir_all(path.join(".isolock/cas"))
        .expect("creates the plan and CAS fixture directories");
    *world.fixture_dir.borrow_mut() = Some(fixture);
    path
}

fn write_plan_fixture(world: &PlanDispatchWorld, definitions: Vec<(String, String, Registry)>) {
    let project = plan_project_dir(world);
    let mut plan = AirlockPlan::new();
    let mut packages = Vec::with_capacity(definitions.len());
    for (name, version, registry) in definitions {
        let bytes = format!("fixture {} {name}=={version}", registry.as_str()).into_bytes();
        let digest = Sha256Digest::from_bytes(&bytes);
        let filename = match registry {
            Registry::Pypi => format!("{name}-{version}-py3-none-any.whl"),
            Registry::Conda => format!("{name}-{version}-h123_0.conda"),
            Registry::Npm => format!("{name}-{version}.tgz"),
            Registry::Cargo => format!("{name}-{version}.crate"),
        };
        let artifact = PlannedArtifact {
            name,
            version,
            registry,
            digest,
        };
        plan.add_with_metadata(
            artifact.clone(),
            PlannedArtifactMetadata {
                filename: Some(filename.clone()),
                ..PlannedArtifactMetadata::default()
            },
        );
        packages.push(PlanPackageFixture {
            filename,
            artifact,
            bytes,
        });
    }

    fs::write(
        project.join(".isolock/plan.json"),
        serde_json::to_vec(&plan).expect("serializes fixture airlock plan"),
    )
    .expect("writes fixture airlock plan");
    *world.packages.borrow_mut() = packages;
}

fn registry_from_name(name: &str) -> Registry {
    match name {
        "cargo" => Registry::Cargo,
        "conda" => Registry::Conda,
        "pypi" => Registry::Pypi,
        "npm" => Registry::Npm,
        _ => panic!("unsupported fixture registry {name:?}"),
    }
}

fn package_definitions(requirements: &str) -> Vec<(String, String, Registry)> {
    requirements
        .split_whitespace()
        .map(|specification| {
            let (name, version) = specification
                .split_once("==")
                .unwrap_or_else(|| panic!("requirement is not pinned: {specification:?}"));
            (name.to_owned(), version.to_owned(), Registry::Pypi)
        })
        .collect()
}

#[given("a project with {filename:string} declaring {requirements:string} and an airlock plan")]
fn given_project_and_airlock_plan(
    world: &PlanDispatchWorld,
    filename: String,
    requirements: String,
) {
    let project = plan_project_dir(world);
    fs::write(project.join(filename), &requirements).expect("writes requirements fixture");
    write_plan_fixture(world, package_definitions(&requirements));
}

#[given("CAS contains {filename:string}")]
fn given_cas_contains(world: &PlanDispatchWorld, filename: String) {
    let package = world
        .packages
        .borrow()
        .iter()
        .find(|package| package.filename == filename)
        .cloned()
        .unwrap_or_else(|| panic!("fixture plan does not include {filename:?}"));
    let blob = plan_project_dir(world)
        .join(".isolock/cas")
        .join(package.artifact.digest.sharded_path());
    fs::create_dir_all(blob.parent().expect("CAS blob has a parent directory"))
        .expect("creates CAS digest directory");
    fs::write(blob, package.bytes).expect("writes CAS artifact bytes");
}

#[given("CAS is empty")]
fn given_empty_cas(world: &PlanDispatchWorld) {
    let cas_dir = plan_project_dir(world).join(".isolock/cas");
    if cas_dir.exists() {
        fs::remove_dir_all(&cas_dir).expect("removes any fixture CAS contents");
    }
    fs::create_dir_all(cas_dir).expect("creates an empty CAS fixture");
}

#[given("an airlock plan with missing artifacts:")]
fn given_plan_with_missing_artifacts(world: &PlanDispatchWorld, docstring: String) {
    let definitions = docstring
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            let registry = fields.next().expect("fixture line has a registry");
            let specification = fields.next().expect("fixture line has a package spec");
            assert!(fields.next().is_none(), "fixture line has two fields");
            let (name, version) = specification
                .split_once("==")
                .unwrap_or_else(|| panic!("package spec is not pinned: {specification:?}"));
            (
                name.to_owned(),
                version.to_owned(),
                registry_from_name(registry),
            )
        })
        .collect();
    write_plan_fixture(world, definitions);
}

#[when("I run {command:string}")]
fn when_run_plan(world: &PlanDispatchWorld, command: String) {
    assert_eq!(command, "isolock plan --ci");
    let project = plan_project_dir(world);
    let output = Command::new(env!("CARGO_BIN_EXE_isolock"))
        .args(["plan", "--ci"])
        .current_dir(&project)
        .env("ISOLOCK_PLAN", project.join(".isolock/plan.json"))
        .env("ISOLOCK_CAS_DIR", project.join(".isolock/cas"))
        .output()
        .expect("runs isolock plan --ci");
    *world.output.borrow_mut() = Some(output);
}

fn plan_output(world: &PlanDispatchWorld) -> String {
    let output = world.output.borrow();
    let output = output.as_ref().expect("plan command has run");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[then("the exit code is {code:u8}")]
fn then_exit_code_is(world: &PlanDispatchWorld, code: u8) {
    let status = world
        .output
        .borrow()
        .as_ref()
        .expect("plan command has run")
        .status
        .code();
    assert_eq!(status, Some(i32::from(code)), "{}", plan_output(world));
}

#[then("the output reports {expected:string}")]
fn then_output_reports(world: &PlanDispatchWorld, expected: String) {
    let output = plan_output(world);
    assert!(
        output.contains(&expected),
        "expected {expected:?} in:\n{output}"
    );
}

#[then("the output lists missing packages {first:string} and {second:string}")]
fn then_output_lists_missing_packages(world: &PlanDispatchWorld, first: String, second: String) {
    let output = plan_output(world);
    assert!(output.contains(&first), "expected {first:?} in:\n{output}");
    assert!(
        output.contains(&second),
        "expected {second:?} in:\n{output}"
    );
}

#[then("the output formats command:")]
fn then_output_formats_command(world: &PlanDispatchWorld, docstring: String) {
    let output = plan_output(world);
    assert!(
        output.contains(docstring.trim()),
        "expected dispatch command {:?} in:\n{output}",
        docstring.trim()
    );
}

#[then("the output formats commands:")]
fn then_output_formats_commands(world: &PlanDispatchWorld, docstring: String) {
    let output = plan_output(world);
    assert!(
        output.contains(docstring.trim()),
        "expected dispatch commands {:?} in:\n{output}",
        docstring.trim()
    );
}

#[test]
fn plan_fixture_survives_another_scenarios_cleanup() {
    let surviving_world = world();
    given_project_and_airlock_plan(
        &surviving_world,
        "requirements.txt".into(),
        "fastapi==0.115.0".into(),
    );
    {
        let other_world = world();
        given_project_and_airlock_plan(
            &other_world,
            "requirements.txt".into(),
            "pydantic==2.9.2".into(),
        );
    }

    given_empty_cas(&surviving_world);
    when_run_plan(&surviving_world, "isolock plan --ci".into());
    then_exit_code_is(&surviving_world, 1);
    then_output_reports(&surviving_world, "Verdict: DRIFT DETECTED".into());
    then_output_reports(&surviving_world, "fastapi==0.115.0".into());
    assert!(!plan_output(&surviving_world).contains("pydantic==2.9.2"));
}

#[scenario(path = "../../backlog/features/plan-drift.feature", index = 0)]
fn bdd_plan_drift_is_clean_when_all_dependencies_exist(world: PlanDispatchWorld) {
    let _ = world;
}

#[scenario(path = "../../backlog/features/plan-drift.feature", index = 1)]
fn bdd_plan_drift_formats_grouped_pypi_command(world: PlanDispatchWorld) {
    let _ = world;
}

#[scenario(path = "../../backlog/features/plan-drift.feature", index = 2)]
fn bdd_plan_drift_groups_commands_by_registry(world: PlanDispatchWorld) {
    let _ = world;
}
/// Owns a spawned `serve` process so a failing assertion cannot leave it listening.
struct ServeProcess(std::process::Child);

impl Drop for ServeProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Starts `isolock serve` on an ephemeral port and returns its base address.
fn start_serve(plan: &std::path::Path, cas: &std::path::Path) -> (ServeProcess, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_isolock"))
        .args(["serve", "--port", "0"])
        .env("ISOLOCK_PLAN", plan)
        .env("ISOLOCK_CAS_DIR", cas)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("starts isolock serve");
    let stdout = child.stdout.take().expect("serve has captured stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("serve prints its bound URL");
    let address = line
        .split_whitespace()
        .find_map(|word| word.strip_prefix("http://"))
        .expect("serve output contains the listener address")
        .to_owned();
    (ServeProcess(child), address)
}

/// Writes a one-artifact reviewed plan and returns the plan path and artifact digest.
fn write_single_pypi_plan(
    directory: &std::path::Path,
    package: &[u8],
    filename: &str,
) -> (PathBuf, Sha256Digest) {
    let digest = Sha256Digest::from_bytes(package);
    let plan_path = directory.join("plan.json");
    fs::write(
        &plan_path,
        serde_json::to_vec(&serde_json::json!({
            "artifacts": [{
                "name": "fixture", "version": "2.0.0", "registry": "Pypi",
                "digest": digest.as_str()
            }],
            "artifact_metadata": [{
                "filename": filename,
                "registry_metadata": {"name": "fixture", "version": "2.0.0"}
            }],
        }))
        .expect("serializes the reviewed plan"),
    )
    .expect("writes the reviewed plan");
    (plan_path, digest)
}

#[test]
fn proxy_serves_cold_snapshot_after_the_reviewed_upstream_is_gone() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let fixture_dir = temporary_directory();
    let cas_dir = fixture_dir.path().join("cas");
    let upstream = fixture_dir.path().join("upstream");
    fs::create_dir_all(&cas_dir).expect("creates an isolated CAS directory");
    fs::create_dir_all(&upstream).expect("creates the reviewed upstream directory");

    // The upstream holds the bytes a reviewer signed. Acquisition copies them into CAS,
    // which is the step that makes the later offline replay possible.
    let package = b"reviewed wheel bytes from the local upstream".as_slice();
    let filename = "fixture-2.0.0-py3-none-any.whl";
    fs::write(upstream.join(filename), package).expect("writes the reviewed upstream artifact");
    let (plan_path, digest) = write_single_pypi_plan(fixture_dir.path(), package, filename);

    let blob = cas_dir.join(digest.sharded_path());
    fs::create_dir_all(blob.parent().expect("CAS digest has a parent shard"))
        .expect("creates CAS shard directory");
    fs::copy(upstream.join(filename), &blob).expect("acquires the reviewed artifact into CAS");

    let plan = Command::new(bin)
        .args(["plan", "--ci"])
        .current_dir(fixture_dir.path())
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("checks the real CLI plan");
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );

    let (_serve, address) = start_serve(&plan_path, &cas_dir);

    // Connected path: index and artifact both resolve while the upstream exists.
    let index = http_get(&address, "/pypi/simple/fixture/");
    assert!(
        index.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "connects: {index:?}"
    );
    assert!(String::from_utf8_lossy(&index).contains(filename));
    let connected = http_get(&address, &format!("/pypi/packages/{filename}"));
    assert!(
        connected.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "connects: {connected:?}"
    );
    assert!(
        connected.ends_with(package),
        "a connected fetch returns the reviewed bytes"
    );

    // Shut the reviewed upstream down completely. Nothing remains to fall back to.
    fs::remove_dir_all(&upstream).expect("deletes the reviewed upstream");

    // Cold replay: the same process still answers, so the consumer never noticed.
    let replay = http_get(&address, &format!("/pypi/packages/{filename}"));
    assert!(
        replay.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "offline replay serves from CAS: {replay:?}"
    );
    assert!(
        replay.ends_with(package),
        "replay returns the same reviewed bytes after upstream shutdown"
    );

    // No fallback: anything outside the reviewed snapshot still fails closed. This is the
    // half of the claim a successful replay alone cannot make.
    let miss = http_get(&address, "/pypi/simple/fixture-unreviewed/");
    assert!(
        !miss.starts_with(b"HTTP/1.1 200 OK\r\n"),
        "an unreviewed package must not resolve, even as a fallback: {miss:?}"
    );
}

#[test]
fn corrupt_cas_blob_fails_closed_before_any_bytes_are_served() {
    // Kept separate from the replay journey above because the CAS is read into memory when
    // the listener binds. Corruption is therefore refused at startup rather than mid-serve,
    // which is the stronger guarantee and is what this asserts.
    let fixture_dir = temporary_directory();
    let cas_dir = fixture_dir.path().join("cas");
    fs::create_dir_all(&cas_dir).expect("creates an isolated CAS directory");

    let package = b"reviewed wheel bytes".as_slice();
    let filename = "fixture-2.0.0-py3-none-any.whl";
    let (plan_path, digest) = write_single_pypi_plan(fixture_dir.path(), package, filename);

    let blob = cas_dir.join(digest.sharded_path());
    fs::create_dir_all(blob.parent().expect("CAS digest has a parent shard"))
        .expect("creates CAS shard directory");
    fs::write(&blob, b"tampered before the listener binds").expect("writes a corrupt blob");

    let output = Command::new(env!("CARGO_BIN_EXE_isolock"))
        .args(["serve", "--port", "0"])
        .env("ISOLOCK_PLAN", &plan_path)
        .env("ISOLOCK_CAS_DIR", &cas_dir)
        .output()
        .expect("runs isolock serve against a corrupt CAS");

    assert_eq!(
        output.status.code(),
        Some(1),
        "a corrupt CAS must fail closed before listening"
    );
    assert!(
        output.stdout.is_empty(),
        "no listener URL is announced: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("SHA-256 mismatch"),
        "the diagnostic names the integrity failure: {stderr}"
    );
    assert!(
        stderr.contains(digest.as_str()),
        "the diagnostic names the expected digest so a reviewer can trace it: {stderr}"
    );
}
