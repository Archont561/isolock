#![allow(clippy::unwrap_used, clippy::expect_used)] // Test fixtures fail fast on invalid setup.
//! Wire-level contract tests for all registry protocols and the shared server.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use isolock_cas::{InMemoryCas, Sha256Digest};
use isolock_core::{AirlockMode, Registry};
use isolock_plan::{AirlockPlan, PlannedArtifact, PlannedArtifactMetadata};
use isolock_proxy::{decompress_brotli, ProxyConfig, ProxyServer};
use serde_json::Value;
use sha2::{Digest, Sha512};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};

struct TestProxy {
    plan: AirlockPlan,
    cas: InMemoryCas,
}

struct ArtifactFixture<'a> {
    registry: Registry,
    name: &'a str,
    version: &'a str,
    content: &'a str,
    metadata: PlannedArtifactMetadata,
}

struct HttpReply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpReply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn test_proxy() -> TestProxy {
    let mut plan = AirlockPlan::new();
    let mut cas = InMemoryCas::new();
    for fixture in [
        pypi_fixture(),
        conda_fixture(),
        npm_fixture(),
        cargo_fixture(),
    ] {
        add_artifact(&mut plan, &mut cas, fixture);
    }
    TestProxy { plan, cas }
}

fn pypi_fixture() -> ArtifactFixture<'static> {
    ArtifactFixture {
        registry: Registry::Pypi,
        name: "requests",
        version: "2.32.3",
        content: "wheel bytes for tests",
        metadata: PlannedArtifactMetadata {
            filename: Some("requests-2.32.3-py3-none-any.whl".into()),
            requires_python: Some(">=3.8".into()),
            registry_metadata: Some(serde_json::json!({"name": "requests", "version": "2.32.3"})),
            ..PlannedArtifactMetadata::default()
        },
    }
}

fn conda_fixture() -> ArtifactFixture<'static> {
    ArtifactFixture {
        registry: Registry::Conda,
        name: "python",
        version: "3.12.7",
        content: "conda package bytes",
        metadata: PlannedArtifactMetadata {
            filename: Some("python-3.12.7-h55ee38b_0.conda".into()),
            subdir: Some("linux-64".into()),
            registry_metadata: Some(serde_json::json!({
                "name": "python", "version": "3.12.7", "build": "h55ee38b_0",
                "build_number": 0, "subdir": "linux-64", "depends": ["libffi >=3.4"],
            })),
            ..PlannedArtifactMetadata::default()
        },
    }
}

fn npm_fixture() -> ArtifactFixture<'static> {
    ArtifactFixture {
        registry: Registry::Npm,
        name: "zod",
        version: "3.23.8",
        content: "npm tarball bytes",
        metadata: PlannedArtifactMetadata {
            filename: Some("zod-3.23.8.tgz".into()),
            registry_metadata: Some(serde_json::json!({
                "package_json": {
                    "name": "zod", "version": "3.23.8",
                    "dependencies": {"nested": "^1.0.0"},
                }
            })),
            ..PlannedArtifactMetadata::default()
        },
    }
}

fn cargo_fixture() -> ArtifactFixture<'static> {
    ArtifactFixture {
        registry: Registry::Cargo,
        name: "serde",
        version: "1.0.210",
        content: "crate archive bytes",
        metadata: PlannedArtifactMetadata {
            filename: Some("serde-1.0.210.crate".into()),
            registry_metadata: Some(serde_json::json!({
                "Cargo.toml": "[package]\nname = \"serde\"\nversion = \"1.0.210\"\nrust-version = \"1.56\"\n\n[dependencies]\nserde_core = { version = \"=1.0.210\", default-features = false }\n\n[features]\ndefault = [\"std\"]\nstd = []\n"
            })),
            ..PlannedArtifactMetadata::default()
        },
    }
}

fn add_artifact(plan: &mut AirlockPlan, cas: &mut InMemoryCas, fixture: ArtifactFixture<'_>) {
    let bytes = fixture.content.as_bytes().to_vec();
    let digest = Sha256Digest::from_bytes(&bytes);
    cas.insert(digest.clone(), bytes)
        .expect("CAS digest matches");
    plan.add_with_metadata(
        PlannedArtifact {
            name: fixture.name.into(),
            version: fixture.version.into(),
            registry: fixture.registry,
            digest,
        },
        fixture.metadata,
    );
}

fn start_proxy(
    plan: AirlockPlan,
    cas: InMemoryCas,
    mode: AirlockMode,
) -> isolock_proxy::ProxyHandle {
    ProxyServer::bind(ProxyConfig::new("127.0.0.1", 0, mode), plan, cas)
        .expect("loopback bind works")
        .spawn()
        .expect("proxy thread starts")
}

fn request(
    address: SocketAddr,
    method: &str,
    path: &str,
    extra_headers: &[(&str, &str)],
) -> HttpReply {
    let mut stream = TcpStream::connect(address).expect("proxy accepts a TCP connection");
    write!(stream, "{method} {path} HTTP/1.1\r\nHost: {address}\r\n")
        .expect("request headers write");
    for (name, value) in extra_headers {
        write!(stream, "{name}: {value}\r\n").expect("request header writes");
    }
    stream
        .write_all(b"Connection: close\r\n\r\n")
        .expect("request ends");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("response is readable");
    parse_reply(&response)
}

fn parse_reply(response: &[u8]) -> HttpReply {
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("HTTP response has a header terminator");
    let head = std::str::from_utf8(&response[..separator]).expect("HTTP headers are UTF-8");
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("HTTP status is present");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
        .collect();
    HttpReply {
        status,
        headers,
        body: response[separator + 4..].to_vec(),
    }
}

#[test]
fn pypi_pep691_serves_hashes_requires_python_and_brotli_metadata() {
    let proxy = test_proxy();
    let handle = start_proxy(proxy.plan, proxy.cas, AirlockMode::LocalAirlock);
    let response = request(
        handle.local_addr(),
        "GET",
        "/pypi/simple/Requests/",
        &[("Accept-Encoding", "gzip, br")],
    );
    assert_eq!(response.status, 200);
    assert_eq!(
        response.header("content-type"),
        Some("application/vnd.pypi.simple.v1+json")
    );
    assert_eq!(response.header("content-encoding"), Some("br"));
    assert_eq!(response.header("vary"), Some("Accept-Encoding"));
    let body = decompress_brotli(&response.body).expect("Brotli payload decompresses");
    let index: Value = serde_json::from_slice(&body).expect("PEP 691 body is JSON");
    assert_eq!(index["meta"]["api-version"], "1.1");
    assert_eq!(index["name"], "Requests");
    let file = &index["files"][0];
    assert_eq!(file["filename"], "requests-2.32.3-py3-none-any.whl");
    assert_eq!(file["requires-python"], ">=3.8");
    assert_eq!(file["hashes"]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        file["url"],
        format!(
            "{}/pypi/packages/requests-2.32.3-py3-none-any.whl",
            handle.base_url()
        )
    );
    let _ = handle.shutdown();
}

#[test]
#[allow(clippy::cognitive_complexity)] // One protocol fixture verifies canonical/mirrored metadata and raw downloads.
fn conda_repodata_and_download_use_canonical_records_and_raw_bytes() {
    let proxy = test_proxy();
    let expected_bytes = proxy
        .cas
        .get(&proxy.plan.artifacts()[1].digest)
        .expect("Conda archive is in CAS")
        .to_vec();
    let handle = start_proxy(proxy.plan, proxy.cas, AirlockMode::LocalAirlock);
    let response = request(
        handle.local_addr(),
        "GET",
        "/conda/channel/linux-64/repodata.json",
        &[],
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    let repodata: Value = serde_json::from_slice(&response.body).expect("repodata is JSON");
    assert_eq!(
        repodata["info"]["base_url"],
        format!("{}/conda/channel/linux-64/", handle.base_url())
    );
    let record = &repodata["packages.conda"]["python-3.12.7-h55ee38b_0.conda"];
    assert_eq!(record["name"], "python");
    assert_eq!(record["version"], "3.12.7");
    assert_eq!(record["build"], "h55ee38b_0");
    assert_eq!(record["depends"][0], "libffi >=3.4");

    let unsupported_shards = request(
        handle.local_addr(),
        "GET",
        "/conda/channel/linux-64/repodata_shards.msgpack.zst",
        &[],
    );
    assert_eq!(unsupported_shards.status, 404);

    let download = request(
        handle.local_addr(),
        "GET",
        "/conda/channel/linux-64/python-3.12.7-h55ee38b_0.conda",
        &[],
    );
    assert_eq!(download.status, 200);
    assert_eq!(download.body, expected_bytes);
    assert_eq!(download.header("content-encoding"), None);

    let mirrored = request(
        handle.local_addr(),
        "GET",
        "/conda/channel/conda-forge/linux-64/repodata.json",
        &[],
    );
    assert_eq!(mirrored.status, 200);
    let mirrored_repodata: Value =
        serde_json::from_slice(&mirrored.body).expect("mirrored repodata is JSON");
    assert_eq!(
        mirrored_repodata["info"]["base_url"],
        format!("{}/conda/channel/conda-forge/linux-64/", handle.base_url())
    );
    let mirrored_download = request(
        handle.local_addr(),
        "GET",
        "/conda/channel/conda-forge/linux-64/python-3.12.7-h55ee38b_0.conda",
        &[],
    );
    assert_eq!(mirrored_download.status, 200);
    assert_eq!(mirrored_download.body, expected_bytes);
    let _ = handle.shutdown();
}

#[test]
fn npm_packument_has_sha512_integrity_scoped_names_and_raw_tarball_downloads() {
    let proxy = test_proxy();
    let artifact_bytes = proxy
        .cas
        .get(&proxy.plan.artifacts()[2].digest)
        .unwrap()
        .to_vec();
    let expected_integrity = format!(
        "sha512-{}",
        STANDARD.encode(Sha512::digest(&artifact_bytes))
    );
    let handle = start_proxy(proxy.plan, proxy.cas, AirlockMode::LocalAirlock);
    let response = request(handle.local_addr(), "GET", "/npm/zod", &[]);
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));
    let packument: Value = serde_json::from_slice(&response.body).expect("packument is JSON");
    let version = &packument["versions"]["3.23.8"];
    assert_eq!(version["dependencies"]["nested"], "^1.0.0");
    assert_eq!(version["dist"]["integrity"], expected_integrity);
    assert_eq!(
        version["dist"]["tarball"],
        format!("{}/npm/zod/-/zod-3.23.8.tgz", handle.base_url())
    );
    let download = request(handle.local_addr(), "GET", "/npm/zod/-/zod-3.23.8.tgz", &[]);
    assert_eq!(download.status, 200);
    assert_eq!(download.body, artifact_bytes);
    let _ = handle.shutdown();
}

#[test]
fn cargo_sparse_index_config_checksum_and_download_are_consistent() {
    let proxy = test_proxy();
    let expected_bytes = proxy
        .cas
        .get(&proxy.plan.artifacts()[3].digest)
        .unwrap()
        .to_vec();
    let expected_checksum = proxy.plan.artifacts()[3].digest.as_str().to_owned();
    let handle = start_proxy(proxy.plan, proxy.cas, AirlockMode::LocalAirlock);

    let config = request(handle.local_addr(), "GET", "/cargo/config.json", &[]);
    assert_eq!(config.status, 200);
    let config: Value = serde_json::from_slice(&config.body).expect("Cargo config is JSON");
    assert_eq!(
        config["dl"],
        format!(
            "{}/cargo/api/v1/crates/{{crate}}/{{version}}/download",
            handle.base_url()
        )
    );

    let index = request(handle.local_addr(), "GET", "/cargo/se/rd/serde", &[]);
    assert_eq!(index.status, 200);
    let line = std::str::from_utf8(&index.body)
        .unwrap()
        .lines()
        .next()
        .expect("sparse index has one version line");
    let entry: Value = serde_json::from_str(line).expect("sparse index line is JSON");
    assert_eq!(entry["name"], "serde");
    assert_eq!(entry["vers"], "1.0.210");
    assert_eq!(entry["cksum"], expected_checksum);
    assert_eq!(entry["deps"][0]["name"], "serde_core");
    assert_eq!(entry["deps"][0]["default_features"], false);
    assert!(index.body.ends_with(b"\n"));

    let download = request(
        handle.local_addr(),
        "GET",
        "/cargo/api/v1/crates/serde/1.0.210/download",
        &[],
    );
    assert_eq!(download.status, 200);
    assert_eq!(download.body, expected_bytes);
    let _ = handle.shutdown();
}

#[test]
fn binary_ranges_head_and_fail_closed_cache_misses_are_http_behaviors() {
    let proxy = test_proxy();
    let wheel_digest = proxy.plan.artifacts()[0].digest.clone();
    let handle = start_proxy(proxy.plan.clone(), proxy.cas, AirlockMode::Ci);
    let range = request(
        handle.local_addr(),
        "GET",
        "/pypi/packages/requests-2.32.3-py3-none-any.whl",
        &[("Range", "bytes=2-6")],
    );
    assert_eq!(range.status, 206);
    assert_eq!(range.body, b"eel b");
    assert_eq!(range.header("content-range"), Some("bytes 2-6/21"));
    assert_eq!(range.header("accept-ranges"), Some("bytes"));

    let head = request(
        handle.local_addr(),
        "HEAD",
        "/pypi/packages/requests-2.32.3-py3-none-any.whl",
        &[],
    );
    assert_eq!(head.status, 200);
    assert_eq!(head.body.len(), 0);
    assert_eq!(head.header("content-length"), Some("21"));
    let _ = handle.shutdown();

    let mut missing_plan = AirlockPlan::new();
    missing_plan.add_with_metadata(
        PlannedArtifact {
            name: "missing".into(),
            version: "1.0.0".into(),
            registry: Registry::Pypi,
            digest: wheel_digest,
        },
        PlannedArtifactMetadata {
            filename: Some("missing-1.0.0-py3-none-any.whl".into()),
            ..PlannedArtifactMetadata::default()
        },
    );
    let empty_cas = InMemoryCas::new();
    let handle = start_proxy(missing_plan, empty_cas, AirlockMode::Ci);
    let miss = request(
        handle.local_addr(),
        "GET",
        "/pypi/packages/missing-1.0.0-py3-none-any.whl",
        &[],
    );
    assert_eq!(miss.status, 403);
    assert!(String::from_utf8_lossy(&miss.body).contains("airlock violation"));
    let _ = handle.shutdown();
}

#[test]
fn proxy_binds_only_to_loopback_and_uses_a_dynamic_port() {
    let server = ProxyServer::bind(
        ProxyConfig::default(),
        AirlockPlan::new(),
        InMemoryCas::new(),
    )
    .expect("port zero binds dynamically");
    let address = server.local_addr().unwrap();
    assert!(address.ip().is_loopback());
    assert_ne!(address.port(), 0);
    drop(server);

    let public_bind = ProxyConfig::new("0.0.0.0", 0, AirlockMode::LocalAirlock);
    assert!(ProxyServer::bind(public_bind, AirlockPlan::new(), InMemoryCas::new()).is_err());
}
