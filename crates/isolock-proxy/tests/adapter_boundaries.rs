//! Registry metadata and request-target boundaries exercised over real loopback HTTP.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixture setup must fail the test.

use isolock_cas::{InMemoryCas, Sha256Digest};
use isolock_core::{AirlockMode, Registry};
use isolock_plan::{AirlockPlan, PlannedArtifact, PlannedArtifactMetadata};
use isolock_proxy::{
    AdapterRegistry, CargoAdapter, CondaAdapter, NpmAdapter, ProxyConfig, ProxyServer, PypiAdapter,
    RegistryAdapter,
};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

struct Fixture {
    registry: Registry,
    artifact: PlannedArtifact,
    metadata: PlannedArtifactMetadata,
    cached: bool,
}

struct Reply {
    status: u16,
    body: String,
}

impl Fixture {
    fn new(registry: Registry, name: &str, version: &str) -> Self {
        Self {
            registry,
            artifact: PlannedArtifact {
                registry,
                name: name.into(),
                version: version.into(),
                digest: Sha256Digest::from_bytes(b"verified archive"),
            },
            metadata: PlannedArtifactMetadata::default(),
            cached: true,
        }
    }

    fn get(&self, path: &str) -> Reply {
        self.get_versions(path, &[])
    }

    fn get_versions(&self, path: &str, extra: &[&str]) -> Reply {
        let mut plan = AirlockPlan::new();
        plan.add_with_metadata(self.artifact.clone(), self.metadata.clone());
        for version in extra {
            let mut artifact = self.artifact.clone();
            artifact.version = (*version).into();
            plan.add_with_metadata(artifact, self.metadata.clone());
        }
        let mut cas = InMemoryCas::new();
        if self.cached {
            cas.insert(self.artifact.digest.clone(), b"verified archive".to_vec())
                .unwrap();
        }
        let mut adapters = AdapterRegistry::new();
        match self.registry {
            Registry::Cargo => {
                adapters.register(CargoAdapter);
            }
            Registry::Conda => {
                adapters.register(CondaAdapter);
            }
            Registry::Pypi => {
                adapters.register(PypiAdapter);
            }
            Registry::Npm => {
                adapters.register(NpmAdapter);
            }
        }
        let handle = ProxyServer::bind_with_adapters(
            ProxyConfig::new("127.0.0.1", 0, AirlockMode::Ci),
            plan,
            cas,
            adapters,
        )
        .unwrap()
        .spawn()
        .unwrap();
        let mut stream = TcpStream::connect(handle.local_addr()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut bytes = String::new();
        stream.read_to_string(&mut bytes).unwrap();
        handle.shutdown().unwrap();
        let (headers, body) = bytes.split_once("\r\n\r\n").unwrap();
        Reply {
            status: headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
            body: body.into(),
        }
    }
}

fn index_json(reply: Reply) -> Value {
    assert_eq!(reply.status, 200, "{}", reply.body);
    serde_json::from_str(&reply.body).unwrap()
}

#[test]
fn all_adapters_reject_malformed_percent_encoding_and_unrelated_routes() {
    let adapters: [(&dyn RegistryAdapter, Registry); 4] = [
        (&CargoAdapter, Registry::Cargo),
        (&CondaAdapter, Registry::Conda),
        (&PypiAdapter, Registry::Pypi),
        (&NpmAdapter, Registry::Npm),
    ];
    for (adapter, registry) in adapters {
        assert_eq!(adapter.ecosystem(), registry);
        let fixture = Fixture::new(registry, "demo", "1.0.0");
        for path in ["/%", "/%G0", "/%0G", "/%FF"] {
            let reply = fixture.get(path);
            assert_eq!(reply.status, 400, "{path}: {}", reply.body);
        }
        assert_eq!(fixture.get("/unrelated").status, 404);
        assert_eq!(fixture.get("http://example.invalid").status, 404);
    }
    let fixture = Fixture::new(Registry::Cargo, "demo", "1.0.0");
    assert_eq!(
        fixture
            .get("http://example.invalid/cargo/config.json?x=1")
            .status,
        200
    );
    assert_eq!(fixture.get("/c%61rgo/config.json").status, 200);
}

#[test]
fn cargo_infers_filenames_and_sorts_versions_using_semver() {
    let fixture = Fixture::new(Registry::Cargo, "demo", "1.9.0");
    let reply = fixture.get_versions("/cargo/de/mo/demo", &["1.10.0", "1.2.0"]);
    assert_eq!(reply.status, 200, "{}", reply.body);
    let versions: Vec<Value> = reply
        .body
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        versions
            .iter()
            .map(|v| v["vers"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1.2.0", "1.9.0", "1.10.0"]
    );
    assert_eq!(
        fixture.get("/cargo/api/v1/crates/DEMO/1.9.0/download").body,
        "verified archive"
    );
    assert_eq!(fixture.get("/cargo/").status, 400);
    assert_eq!(fixture.get("/cargo/../secret").status, 400);
    assert_eq!(
        fixture.get("/cargo/api/v1/crates/demo/1.9.0/wrong").status,
        404
    );
    assert_eq!(
        fixture
            .get("/cargo/api/v1/crates/missing/1.0.0/download")
            .status,
        403
    );
}

#[test]
fn cargo_preserves_renamed_target_dependencies_and_namespaced_features() {
    let mut fixture = Fixture::new(Registry::Cargo, "demo", "1.0.0");
    fixture.metadata.registry_metadata = Some(json!({"Cargo.toml": {
        "package": {"name":"demo", "version":"1.0.0", "links":"native", "rust-version":"1.88"},
        "dependencies": {"alias": {"package":"actual", "version":"^2", "optional":true, "default-features":false, "features":["alloc"]}, "plain":"1"},
        "dev-dependencies": {"tester":"1"},
        "build-dependencies": {"builder":"1"},
        "target": {"cfg(unix)": {"dependencies":{"unix_dep":"1"}, "dev-dependencies":{"unix_test":"1"}, "build-dependencies":{"unix_build":"1"}}},
        "features": {"default":["std"], "std":[], "extra":["dep:alias", "alias?/alloc"]}
    }}));
    let entry: Value = serde_json::from_str(&fixture.get("/cargo/de/mo/demo").body).unwrap();
    assert_eq!(entry["links"], "native");
    assert_eq!(entry["rust_version"], "1.88");
    assert_eq!(entry["v"], 2);
    assert_eq!(
        entry["features2"]["extra"],
        json!(["dep:alias", "alias?/alloc"])
    );
    let dependencies = entry["deps"].as_array().unwrap();
    assert_eq!(dependencies.len(), 7);
    let alias = dependencies
        .iter()
        .find(|dep| dep["name"] == "alias")
        .unwrap();
    assert_eq!(alias["package"], "actual");
    assert_eq!(alias["optional"], true);
    assert_eq!(alias["default_features"], false);
    assert_eq!(alias["features"], json!(["alloc"]));
    assert!(dependencies
        .iter()
        .any(|dep| dep["kind"] == "build" && dep["target"] == "cfg(unix)"));
}

#[test]
fn cargo_rejects_inconsistent_or_unparseable_saved_manifests() {
    let mut fixture = Fixture::new(Registry::Cargo, "demo", "1.0.0");
    for (manifest, message) in [
        (json!(123), "must be a TOML string or object"),
        (json!("not [valid"), "invalid Cargo.toml"),
        (json!({}), "missing [package]"),
        (json!({"package":{"version":"1.0.0"}}), "missing name"),
        (json!({"package":{"name":"demo"}}), "missing version"),
        (
            json!({"package":{"name":"other", "version":"1.0.0"}}),
            "does not match",
        ),
        (
            json!({"package":{"name":"demo", "version":"1.0.0"}, "features":{"bad":true}}),
            "must be an array",
        ),
    ] {
        fixture.metadata.registry_metadata = Some(json!({"Cargo.toml":manifest}));
        let reply = fixture.get("/cargo/de/mo/demo");
        assert_eq!(reply.status, 500, "{}", reply.body);
        assert!(reply.body.contains(message), "{}", reply.body);
    }
    fixture.metadata.registry_metadata = Some(json!({"cargo_index":false}));
    assert!(fixture
        .get("/cargo/de/mo/demo")
        .body
        .contains("invalid saved Cargo index"));
    fixture.metadata.registry_metadata = None;
    fixture.metadata.filename = Some("demo.CRATE".into());
    assert_eq!(
        fixture
            .get("/cargo/api/v1/crates/demo/1.0.0/download")
            .status,
        500
    );
    fixture.metadata.filename = None;
    fixture.artifact.name = "not a crate".into();
    assert_eq!(fixture.get("/cargo/de/mo/demo").status, 500);
}

#[test]
fn cargo_saved_index_is_preserved_and_missing_blobs_fail_closed() {
    let mut fixture = Fixture::new(Registry::Cargo, "demo", "1.0.0");
    fixture.metadata.registry_metadata = Some(json!({"cargo_index": {
        "name":"demo", "vers":"1.0.0", "deps":[], "cksum":fixture.artifact.digest.as_str(), "features":{}, "yanked":true
    }}));
    let entry: Value = serde_json::from_str(&fixture.get("/cargo/de/mo/demo").body).unwrap();
    assert_eq!(entry["yanked"], true);
    fixture.cached = false;
    for path in [
        "/cargo/de/mo/demo",
        "/cargo/api/v1/crates/demo/1.0.0/download",
    ] {
        let reply = fixture.get(path);
        assert_eq!(reply.status, 403);
        assert!(reply.body.contains("demo-1.0.0.crate"));
    }
}

#[test]
fn pypi_infers_wheel_names_normalizes_projects_and_sorts_files() {
    let mut fixture = Fixture::new(Registry::Pypi, "My..Demo__Pkg", "1.0.0");
    fixture.metadata.registry_metadata = Some(json!({"requires-python":">=3.11"}));
    let index = index_json(fixture.get_versions("/pypi/simple/my-demo-pkg", &["2.0.0"]));
    assert_eq!(
        index["files"][0]["filename"],
        "My_Demo_Pkg-1.0.0-py3-none-any.whl"
    );
    assert_eq!(
        index["files"][1]["filename"],
        "My_Demo_Pkg-2.0.0-py3-none-any.whl"
    );
    assert_eq!(index["files"][0]["requires-python"], ">=3.11");
    assert_eq!(
        fixture
            .get("/pypi/packages/My_Demo_Pkg-1.0.0-py3-none-any.whl")
            .body,
        "verified archive"
    );
    assert_eq!(fixture.get("/pypi/packages/missing.whl").status, 403);
    assert_eq!(fixture.get("/pypi/packages/../outside.whl").status, 400);
    fixture.cached = false;
    assert_eq!(fixture.get("/pypi/simple/my-demo-pkg").status, 403);
    assert_eq!(
        fixture
            .get("/pypi/packages/My_Demo_Pkg-1.0.0-py3-none-any.whl")
            .status,
        403
    );
}

#[test]
fn pypi_rejects_unsafe_filenames_and_metadata_identity_mismatches() {
    let mut fixture = Fixture::new(Registry::Pypi, "demo", "1.0.0");
    fixture.metadata.registry_metadata = Some(json!({"filename":"demo-custom.whl"}));
    assert_eq!(
        index_json(fixture.get("/pypi/simple/demo/"))["files"][0]["filename"],
        "demo-custom.whl"
    );
    for details in [
        json!({"name":"other"}),
        json!({"version":"2.0.0"}),
        json!({"filename":"../escape.whl"}),
        json!({"filename":"demo.WHL"}),
    ] {
        fixture.metadata.registry_metadata = Some(details);
        assert_eq!(fixture.get("/pypi/simple/demo/").status, 500);
    }
    assert_eq!(fixture.get("/pypi/packages/demo.WHL").status, 500);
}

#[test]
fn npm_supports_scoped_names_fallback_metadata_and_semver_latest() {
    let fixture = Fixture::new(Registry::Npm, "@scope/demo", "1.9.0");
    let index = index_json(fixture.get_versions("/npm/@scope%2Fdemo/", &["1.10.0", "1.2.0"]));
    assert_eq!(index["dist-tags"]["latest"], "1.10.0");
    assert!(index["versions"]["1.9.0"]["dist"]["tarball"]
        .as_str()
        .unwrap()
        .ends_with("/npm/@scope%2Fdemo/-/demo-1.9.0.tgz"));
    assert_eq!(
        fixture.get("/npm/@scope%2Fdemo/-/demo-1.9.0.tgz").body,
        "verified archive"
    );
    assert_eq!(fixture.get("/npm/@scope%2Fdemo/-/absent.tgz").status, 403);
    for path in [
        "/npm/",
        "/npm/@scope",
        "/npm/@/demo",
        "/npm/a/b/c",
        "/npm/@scope/demo/-/../escape.tgz",
    ] {
        assert_eq!(fixture.get(path).status, 400, "{path}");
    }
}

#[test]
fn npm_saved_metadata_must_match_and_archives_must_have_canonical_suffixes() {
    let mut fixture = Fixture::new(Registry::Npm, "demo", "1.0.0");
    fixture.metadata.registry_metadata =
        Some(json!({"package.json":{"name":"demo", "version":"1.0.0", "bin":{"demo":"cli.js"}}}));
    assert_eq!(
        index_json(fixture.get("/npm/demo"))["versions"]["1.0.0"]["bin"]["demo"],
        "cli.js"
    );
    for details in [
        json!([]),
        json!({"name":"other"}),
        json!({"version":"2.0.0"}),
    ] {
        fixture.metadata.registry_metadata = Some(details);
        assert_eq!(fixture.get("/npm/demo").status, 500);
    }
    fixture.metadata.registry_metadata = None;
    fixture.metadata.filename = Some("demo.TGZ".into());
    assert_eq!(fixture.get("/npm/demo").status, 500);
    assert_eq!(fixture.get("/npm/demo/-/demo.TGZ").status, 500);
    fixture.metadata.filename = None;
    fixture.cached = false;
    assert_eq!(fixture.get("/npm/demo").status, 403);
    assert_eq!(fixture.get("/npm/demo/-/demo-1.0.0.tgz").status, 403);
}

#[test]
fn conda_infers_records_for_both_archive_formats_and_encodes_channel_names() {
    let mut fixture = Fixture::new(Registry::Conda, "demo", "1.0.0");
    for filename in [None, Some("demo-1.0.0-h0_7.tar.bz2")] {
        fixture.metadata.filename = filename.map(str::to_owned);
        let repodata =
            index_json(fixture.get("/conda/channel/channel%20name/noarch/repodata.json"));
        let key = if filename.is_some_and(|name| name.ends_with(".tar.bz2")) {
            "packages"
        } else {
            "packages.conda"
        };
        let name = filename.unwrap_or("demo-1.0.0-0.conda");
        assert_eq!(repodata[key][name]["name"], "demo");
        assert_eq!(repodata[key][name]["size"], b"verified archive".len());
        assert!(repodata["info"]["base_url"]
            .as_str()
            .unwrap()
            .ends_with("/conda/channel/channel%20name/noarch/"));
    }
    for path in [
        "/conda/channel/../noarch/repodata.json",
        "/conda/channel/%2E%2E/repodata.json",
    ] {
        assert_eq!(fixture.get(path).status, 400);
    }
    assert_eq!(fixture.get("/conda/channel/incomplete").status, 404);
    fixture.metadata.filename = Some("unusual.conda".into());
    let reply = fixture.get("/conda/channel/noarch/repodata.json");
    assert_eq!(reply.status, 500);
    assert!(reply.body.contains("invalid archive identifier"));
}

#[test]
fn conda_saved_package_records_are_rehashed_and_invalid_metadata_fails_closed() {
    let mut fixture = Fixture::new(Registry::Conda, "demo", "1.0.0");
    fixture.metadata.registry_metadata = Some(json!({"package_record": {
        "name":"demo", "version":"1.0.0", "build":"h0_0", "build_number":0,
        "subdir":"linux-64", "depends":["libc >=2"], "sha256":"0".repeat(64), "size":999
    }}));
    let index = index_json(fixture.get("/conda/channel/linux-64/repodata.json"));
    let record = &index["packages.conda"]["demo-1.0.0-0.conda"];
    assert_eq!(record["sha256"], fixture.artifact.digest.as_str());
    assert_eq!(record["size"], b"verified archive".len());
    assert_eq!(record["depends"], json!(["libc >=2"]));
    fixture.metadata.registry_metadata = Some(
        json!({"info/index.json":{"name":"demo", "version":"1.0.0", "subdir":"noarch", "build":"0", "build_number":"bad"}}),
    );
    assert_eq!(
        fixture.get("/conda/channel/noarch/repodata.json").status,
        500
    );
    fixture.metadata.filename = Some("demo.CONDA".into());
    assert_eq!(
        fixture.get("/conda/channel/noarch/repodata.json").status,
        500
    );
    fixture.metadata.filename = None;
    fixture.metadata.registry_metadata = None;
    fixture.cached = false;
    assert_eq!(
        fixture.get("/conda/channel/noarch/repodata.json").status,
        403
    );
    assert_eq!(
        fixture
            .get("/conda/channel/noarch/demo-1.0.0-0.conda")
            .status,
        403
    );
    assert_eq!(
        fixture.get("/conda/channel/noarch/missing.tar.bz2").status,
        403
    );
}
