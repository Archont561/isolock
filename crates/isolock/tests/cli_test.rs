use isolock_cas::Sha256Digest;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("isolock-cli-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("creates an isolated CLI fixture directory");
        Self(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .expect("runs isolock --version");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), format!("isolock {}", isolock_core::VERSION));
}

#[test]
fn test_cli_default_usage_output() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let output = Command::new(bin)
        .output()
        .expect("runs isolock without arguments");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("isolock v{}", isolock_core::VERSION)));
    assert!(stdout.contains("dependency airlock manager"));
}

#[test]
fn test_cli_subcommands() {
    let bin = env!("CARGO_BIN_EXE_isolock");

    let plan_out = Command::new(bin)
        .args(["plan", "--ci"])
        .output()
        .expect("runs isolock plan --ci");
    assert!(plan_out.status.success());

    let fixture_dir = TemporaryDirectory::new();
    let cas_dir = fixture_dir.0.join("cas");
    let package_bytes = b"CLI fixture tarball bytes";
    let digest = Sha256Digest::from_bytes(package_bytes);
    let cas_blob = cas_dir.join(digest.sharded_path());
    fs::create_dir_all(cas_blob.parent().expect("CAS blob has a shard directory"))
        .expect("creates CAS shard directory");
    fs::write(&cas_blob, package_bytes).expect("writes verified CAS bytes");
    let plan_path = fixture_dir.0.join("plan.json");
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
    assert!(ingest_out.status.success());
}

#[cfg(unix)]
#[test]
fn exec_injects_local_registry_urls_and_propagates_child_exit_codes() {
    let bin = env!("CARGO_BIN_EXE_isolock");
    let fixture_dir = TemporaryDirectory::new();
    let original_pixi_config = fixture_dir.0.join("pixi-config.toml");
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
