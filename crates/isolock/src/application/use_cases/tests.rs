//! Fake-port tests for every extracted use case, run with no CLI, filesystem, or process.

use super::bootstrap::{export_profile, fetch_profile, garbage_collect, seed_manifest, Workspace};
use super::configuration::{check_configuration, explain_configuration, show_configuration};
use super::execution::{exec_child, serve_proxy};
use super::plan::{check_airlock_plan, check_locked_profile, plan_profile, PlanOutcome};
use crate::application::ports::{
    ArtifactTransport, BlobStore, CollectionReport, ConfigProvenance, ConfigurationProvider,
    DriftState, GarbageCollector, LayeredOverrides, LegacyPlan, Listener, LoadedConfiguration,
    Materializer, PlanStore, ProfileGraph, ProxyRuntime, ProxySession as ChildSession, Recorded,
    SessionSettings, SourceSnapshot, UseCaseResult, WorkspaceSelection,
};
use crate::domain::{DomainError, Settings};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{ProfileManifest, StorageIdentity, PROFILE_SCHEMA_VERSION};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// ---------------------------------------------------------------- fixtures

fn profile_json() -> isolock_plan::profile::Profile {
    serde_json::from_str(
        r#"{"platforms":["linux-64"],"compose":[],"seeds":{"isolock":{"version":"0.1.0","platform":"linux-64","digest":"abababababababababababababababababababababababababababababababab","filename":"isolock-0.1.0.tar.gz","source":"https://example.invalid/isolock"}},"cargo":{},"environment":null}"#,
    )
    .expect("reviewed profile")
}

fn manifest(seed: &str) -> ProfileManifest {
    ProfileManifest {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: "dev".into(),
        platform: "linux-64".into(),
        configuration_digest: Sha256Digest::from_bytes(seed.as_bytes()),
        specification: profile_json(),
        storage: StorageIdentity::default(),
        inputs: BTreeMap::new(),
        artifacts: Vec::new(),
        cargo_payloads: BTreeMap::new(),
        constituents: BTreeMap::new(),
        materializers: BTreeMap::new(),
    }
}

fn settings() -> Settings {
    let mut settings = Settings::default();
    settings.profiles.insert("dev".into(), profile_json());
    settings.storage.source = Some("storage".into());
    settings
}

fn selection() -> WorkspaceSelection {
    WorkspaceSelection {
        profile: Some("dev".into()),
        platform: Some("linux-64".into()),
    }
}

/// A plan store with no filesystem: it answers from memory and records mutations.
#[derive(Default)]
struct FakeStore {
    graph: ProfileGraph,
    missing: usize,
    locked: bool,
    saved: RefCell<Vec<bool>>,
    leases: RefCell<usize>,
}

impl FakeStore {
    /// A store with a locked plan whose graph the fakes answer directly.
    fn locked(missing: usize) -> Self {
        let mut graph = ProfileGraph::new();
        graph.insert("dev".into(), manifest("a"));
        Self {
            graph,
            missing,
            locked: true,
            saved: RefCell::new(Vec::new()),
            leases: RefCell::new(0),
        }
    }

    /// A store with no locked plan, which is the reviewed first-run state.
    fn unlocked() -> Self {
        Self {
            missing: 0,
            ..Self::locked(0)
        }
        .without_lock()
    }

    fn without_lock(mut self) -> Self {
        self.locked = false;
        self
    }
}

struct FakeGuard;

impl crate::application::ports::LockGuard for FakeGuard {}

impl PlanStore for FakeStore {
    type Guard = FakeGuard;

    fn root(&self) -> &Path {
        Path::new("/project")
    }
    fn alias(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf> {
        Ok(PathBuf::from(format!(
            ".isolock/plans/{profile}-{platform}.lock"
        )))
    }
    fn load(&self, _profile: &str, _platform: &str) -> UseCaseResult<ProfileManifest> {
        if !self.locked {
            return Err(DomainError::new(
                "no locked profile plan; run isolock plan --profile NAME --platform PLATFORM first",
            ));
        }
        self.graph
            .get("dev")
            .cloned()
            .ok_or_else(|| DomainError::new("no locked profile plan"))
    }
    fn load_object(&self, _digest: &Sha256Digest) -> UseCaseResult<ProfileManifest> {
        self.load("dev", "linux-64")
    }
    fn graph(&self, _manifest: ProfileManifest) -> UseCaseResult<ProfileGraph> {
        Ok(self.graph.clone())
    }
    fn save(&self, _manifests: &ProfileGraph, capture_inputs: bool) -> UseCaseResult<()> {
        self.saved.borrow_mut().push(capture_inputs);
        Ok(())
    }
    fn missing(&self, _manifests: &ProfileGraph) -> UseCaseResult<Vec<String>> {
        Ok((0..self.missing)
            .map(|index| format!("missing-{index}"))
            .collect())
    }
    fn verified(&self, _digest: &Sha256Digest) -> UseCaseResult<bool> {
        Ok(true)
    }
    fn put_file(&self, _digest: &Sha256Digest, _path: &Path) -> UseCaseResult<()> {
        Ok(())
    }
    fn put_reader(&self, _digest: &Sha256Digest, _reader: &mut dyn Read) -> UseCaseResult<()> {
        Ok(())
    }
    fn blob_path(&self, _digest: &Sha256Digest) -> PathBuf {
        PathBuf::from("cas/blob")
    }
    fn save_object(&self, _manifest: &ProfileManifest) -> UseCaseResult<()> {
        Ok(())
    }
    fn lease(&self) -> UseCaseResult<FakeGuard> {
        *self.leases.borrow_mut() += 1;
        Ok(FakeGuard)
    }
    fn storage_identity(
        &self,
        _locked: Option<&StorageIdentity>,
    ) -> UseCaseResult<StorageIdentity> {
        Ok(StorageIdentity::default())
    }
}

impl GarbageCollector for FakeStore {
    fn collect(&self, roots: &[String], apply: bool) -> UseCaseResult<CollectionReport> {
        Ok(CollectionReport {
            roots: roots.to_vec(),
            retained: Vec::new(),
            reclaimable: Vec::new(),
            bytes: 0,
            applied: apply,
        })
    }
}

struct FakeSnapshot;

impl SourceSnapshot for FakeSnapshot {}

#[derive(Default)]
struct FakeTransport {
    exported: RefCell<Option<PathBuf>>,
}

impl ArtifactTransport for FakeTransport {
    type Snapshot = FakeSnapshot;

    fn snapshot(&self, _locked: Option<&StorageIdentity>) -> UseCaseResult<FakeSnapshot> {
        Ok(FakeSnapshot)
    }
    fn fetch_graph(
        &self,
        _store: &mut dyn BlobStore,
        _source: &FakeSnapshot,
        _digest: &Sha256Digest,
    ) -> UseCaseResult<ProfileGraph> {
        let mut graph = ProfileGraph::new();
        graph.insert("dev".into(), manifest("a"));
        Ok(graph)
    }
    fn fetch_blobs(
        &self,
        _store: &mut dyn BlobStore,
        _source: &FakeSnapshot,
        _manifests: &ProfileGraph,
    ) -> UseCaseResult<usize> {
        Ok(3)
    }
    fn export(
        &self,
        _store: &mut dyn BlobStore,
        _manifests: &ProfileGraph,
        output: &Path,
    ) -> UseCaseResult<()> {
        *self.exported.borrow_mut() = Some(output.into());
        Ok(())
    }
}

struct FakeMaterializer;

impl Materializer for FakeMaterializer {
    fn install(&self, _manifests: &ProfileGraph, _selected: &str) -> UseCaseResult<PathBuf> {
        Ok(PathBuf::from("/project/.isolock/prefix"))
    }
    fn bootstrap_manifest(&self, _manifest: &ProfileManifest) -> UseCaseResult<String> {
        Ok("seed".into())
    }
    fn write_seed(&self, _manifest_path: &Path, _contents: &str) -> UseCaseResult<()> {
        Ok(())
    }
    fn seed_path(&self, profile: &str, platform: &str) -> UseCaseResult<PathBuf> {
        Ok(PathBuf::from(format!(
            ".isolock/seeds/{profile}-{platform}.manifest"
        )))
    }
    fn host_platform(&self) -> Option<String> {
        Some("linux-64".into())
    }
}

struct FakeProvenance;

impl ConfigProvenance for FakeProvenance {
    fn explain(&self, path: &str) -> UseCaseResult<String> {
        if path == "proxy.port" {
            Ok("proxy.port = 0\nsource: compiled defaults".into())
        } else {
            Err(DomainError::new("unknown configuration path"))
        }
    }
    fn render(&self) -> UseCaseResult<String> {
        Ok("{ \"schema-version\": 1 }".into())
    }
}

struct FakeConfiguration;

impl ConfigurationProvider for FakeConfiguration {
    type Provenance = FakeProvenance;

    fn load(&self, _root: &Path) -> UseCaseResult<LoadedConfiguration<FakeProvenance>> {
        Ok(LoadedConfiguration {
            settings: Settings::default(),
            provenance: FakeProvenance,
        })
    }
}

struct FakePlan {
    exists: bool,
    drift: DriftState,
}

impl LegacyPlan for FakePlan {
    fn exists(&self) -> bool {
        self.exists
    }
    fn path(&self) -> String {
        ".isolock/plan.json".into()
    }
    fn drift(&self) -> UseCaseResult<DriftState> {
        Ok(self.drift.clone())
    }
}

struct FakeRuntime {
    start_error: Option<&'static str>,
    shutdown_error: Option<&'static str>,
    code: i32,
}

struct FakeChild {
    shutdown_error: Option<&'static str>,
    code: i32,
}

impl ChildSession for FakeChild {
    fn run(
        &self,
        _command: &crate::application::ports::ChildCommand,
    ) -> UseCaseResult<crate::application::ports::ChildOutcome> {
        Ok(crate::application::ports::ChildOutcome {
            code: Some(self.code),
            signal: None,
        })
    }
    fn close(self) -> UseCaseResult<()> {
        self.shutdown_error
            .map_or(Ok(()), |message| Err(DomainError::new(message)))
    }
}

struct FakeListener;

impl Listener for FakeListener {
    fn base_url(&self) -> String {
        "http://127.0.0.1:4321".into()
    }
    fn mode_label(&self) -> String {
        "LocalAirlock".into()
    }
    fn serve(self) -> UseCaseResult<()> {
        Ok(())
    }
}

impl ProxyRuntime for FakeRuntime {
    type Session = FakeChild;
    type Listener = FakeListener;

    fn start(&self) -> UseCaseResult<FakeChild> {
        if let Some(message) = self.start_error {
            return Err(DomainError::new(message));
        }
        Ok(FakeChild {
            shutdown_error: self.shutdown_error,
            code: self.code,
        })
    }
    fn bind(&self) -> UseCaseResult<FakeListener> {
        Ok(FakeListener)
    }
}

fn runtime() -> FakeRuntime {
    FakeRuntime {
        start_error: None,
        shutdown_error: None,
        code: 0,
    }
}

// ---------------------------------------------------------------- config

#[test]
fn config_check_reports_the_validated_schema() {
    let outcome = check_configuration(&FakeConfiguration, Path::new("/project")).expect("valid");
    assert_eq!(outcome.text, "Configuration is valid (schema 1).");
}

#[test]
fn config_show_renders_the_effective_document_verbatim() {
    let outcome = show_configuration(&FakeConfiguration, Path::new("/project")).expect("rendered");
    assert_eq!(outcome.text, "{ \"schema-version\": 1 }");
}

#[test]
fn config_explain_names_the_winning_layer() {
    let outcome = explain_configuration(&FakeConfiguration, Path::new("/project"), "proxy.port")
        .expect("found");
    assert!(outcome.text.contains("source: compiled defaults"));
}

#[test]
fn config_explain_rejects_unknown_paths() {
    let error = explain_configuration(&FakeConfiguration, Path::new("/project"), "proxy.plan")
        .expect_err("unknown");
    assert_eq!(error.to_string(), "unknown configuration path");
}

#[test]
fn layered_overrides_default_to_the_reviewed_project_file() {
    assert_eq!(
        LayeredOverrides::new().project_config,
        PathBuf::from("isolock.toml")
    );
}

// ---------------------------------------------------------------- plan

#[test]
fn planning_publishes_once_and_reports_the_missing_count() {
    let store = FakeStore::locked(2);
    let outcome = plan_profile(&store, &settings(), &selection()).expect("planned");
    let PlanOutcome::Planned {
        digest,
        alias,
        missing,
        ..
    } = outcome
    else {
        panic!("expected a planned outcome");
    };
    assert_eq!(missing, 2);
    assert_eq!(digest.len(), 64);
    assert!(alias.ends_with("dev-linux-64.lock"));
    assert_eq!(*store.saved.borrow(), vec![true]);
    assert_eq!(*store.leases.borrow(), 1);
}

#[test]
fn planning_requires_both_identifiers() {
    let store = FakeStore::locked(0);
    let error = plan_profile(&store, &settings(), &WorkspaceSelection::default())
        .expect_err("profile required");
    assert_eq!(error.to_string(), "explicit --profile is required");
}

#[test]
fn checking_a_profile_reports_the_first_run_guidance_when_nothing_is_locked() {
    let error = check_locked_profile(&FakeStore::unlocked(), &settings(), &selection())
        .expect_err("nothing locked");
    assert!(error.to_string().contains("run isolock plan --profile"));
}

#[test]
fn checking_the_legacy_plan_requires_an_explicit_selection() {
    let error = check_locked_profile(
        &FakeStore::locked(0),
        &settings(),
        &WorkspaceSelection::default(),
    )
    .expect_err("selection required");
    assert_eq!(error.to_string(), "explicit --profile is required");
}

#[test]
fn the_legacy_plan_reports_drift_with_dispatch_commands() {
    let plan = FakePlan {
        exists: true,
        drift: DriftState::Drift {
            missing: vec!["demo==1.0.0 (cargo)".into()],
            mismatched_digests: vec!["ab".into()],
            commands: vec!["isolock ingest --manifest isolock.ingest.json".into()],
        },
    };
    let PlanOutcome::AirlockDrift { text } = check_airlock_plan(&plan).expect("drift") else {
        panic!("expected drift");
    };
    assert!(text.contains("Missing packages:\n- demo==1.0.0 (cargo)"));
    assert!(text.contains("Mismatched digests:\n- ab"));
    assert!(text.contains("Dispatch commands:"));
}

#[test]
fn the_legacy_plan_reports_a_missing_lockfile() {
    let plan = FakePlan {
        exists: false,
        drift: DriftState::InSync,
    };
    let error = check_airlock_plan(&plan).expect_err("absent");
    assert_eq!(
        error.to_string(),
        "cannot evaluate lockfile drift: no airlock plan found at .isolock/plan.json"
    );
}

// ---------------------------------------------------------------- bootstrap

#[test]
fn fetch_reports_the_first_run_guidance_when_nothing_is_locked() {
    let store = FakeStore::unlocked();
    let transport = FakeTransport::default();
    let mut progress = Recorded::default();
    let error = fetch_profile(
        Workspace {
            store: &store,
            transport: &transport,
            settings: &settings(),
            selection: &selection(),
            progress: &mut progress,
        },
        None,
    )
    .expect_err("nothing locked");
    assert!(error.to_string().contains("run isolock plan --profile"));
    assert!(progress.0.is_empty());
}

#[test]
fn fetch_refuses_a_stale_lock_before_transferring() {
    let store = FakeStore::locked(1);
    let transport = FakeTransport::default();
    let mut progress = Recorded::default();
    let error = fetch_profile(
        Workspace {
            store: &store,
            transport: &transport,
            settings: &settings(),
            selection: &selection(),
            progress: &mut progress,
        },
        None,
    )
    .expect_err("stale lock");
    assert_eq!(
        error.to_string(),
        "Profile dev/linux-64: locked input changed: artifacts"
    );
    assert!(progress.0.is_empty());
}

#[test]
fn sync_refuses_before_transfer_when_nothing_is_locked() {
    let store = FakeStore::unlocked();
    let transport = FakeTransport::default();
    let mut progress = Recorded::default();
    let error = super::bootstrap::sync_profile(
        Workspace {
            store: &store,
            transport: &transport,
            settings: &settings(),
            selection: &selection(),
            progress: &mut progress,
        },
        &FakeMaterializer,
        true,
        None,
    )
    .expect_err("nothing locked");
    assert!(error.to_string().contains("run isolock plan --profile"));
}

#[test]
fn sync_refuses_a_platform_that_is_not_the_host() {
    let store = FakeStore::locked(0);
    let transport = FakeTransport::default();
    let mut progress = Recorded::default();
    let selection = WorkspaceSelection {
        profile: Some("dev".into()),
        platform: Some("osx-arm64".into()),
    };
    let error = super::bootstrap::sync_profile(
        Workspace {
            store: &store,
            transport: &transport,
            settings: &settings(),
            selection: &selection,
            progress: &mut progress,
        },
        &FakeMaterializer,
        true,
        None,
    )
    .expect_err("host mismatch");
    assert!(error.to_string().contains("host fallback is forbidden"));
}

#[test]
fn export_reports_the_first_run_guidance_when_nothing_is_locked() {
    let store = FakeStore::unlocked();
    let transport = FakeTransport::default();
    let error = export_profile(
        &store,
        &transport,
        &settings(),
        &selection(),
        Path::new("snapshot"),
    )
    .expect_err("nothing locked");
    assert!(error.to_string().contains("run isolock plan --profile"));
    assert_eq!(*transport.exported.borrow(), None);
}

#[test]
fn seed_manifest_reports_the_first_run_guidance_when_nothing_is_locked() {
    let error = seed_manifest(
        &FakeStore::unlocked(),
        &FakeMaterializer,
        &settings(),
        &selection(),
    )
    .expect_err("nothing locked");
    assert!(error.to_string().contains("run isolock plan --profile"));
}

#[test]
fn collection_reports_what_it_would_reclaim_before_deleting() {
    let store = FakeStore::locked(0);
    let dry = garbage_collect(&store, &["dev".into()], false).expect("collect");
    assert!(!dry.applied);
    assert_eq!(dry.roots, vec!["dev".to_owned()]);
    let applied = garbage_collect(&store, &[], true).expect("collect");
    assert!(applied.applied);
}

// ---------------------------------------------------------------- execution

#[test]
fn exec_preserves_the_child_status_exactly() {
    let runtime = FakeRuntime {
        code: 7,
        ..runtime()
    };
    let outcome =
        exec_child(&runtime, &["sh".into(), "-c".into(), "exit 7".into()]).expect("child ran");
    assert_eq!(outcome.status, ExitCode::from(7));
    assert_eq!(outcome.shutdown_warning, None);
}

#[test]
fn exec_keeps_the_child_status_when_the_proxy_fails_to_shut_down() {
    let runtime = FakeRuntime {
        code: 3,
        shutdown_error: Some("listener failed"),
        ..runtime()
    };
    let outcome = exec_child(&runtime, &["true".into()]).expect("child ran");
    assert_eq!(outcome.status, ExitCode::from(3));
    assert_eq!(
        outcome.shutdown_warning.expect("warning").line(),
        "isolock exec: proxy shutdown failed: listener failed"
    );
}

#[test]
fn exec_requires_a_child_command() {
    let error = exec_child(&runtime(), &[]).expect_err("no command");
    assert_eq!(error.to_string(), "expected a child command");
}

#[test]
fn exec_reports_a_failed_start_as_a_typed_error() {
    let runtime = FakeRuntime {
        start_error: Some("cannot start local proxy"),
        ..runtime()
    };
    let error = exec_child(&runtime, &["true".into()]).expect_err("start failed");
    assert_eq!(error.to_string(), "cannot start local proxy");
}

#[test]
fn serve_announces_the_url_and_mode_then_accepts() {
    let mut progress = Recorded::default();
    serve_proxy(&runtime(), &mut progress).expect("served");
    assert_eq!(
        progress.joined(),
        "isolock serve: listening on http://127.0.0.1:4321 (LocalAirlock)"
    );
}

#[test]
fn the_session_projection_carries_the_reviewed_defaults() {
    let session = SessionSettings {
        plan: PathBuf::from("plan.json"),
        require_plan: false,
        cas_dir: PathBuf::from("cas"),
        cargo_home: PathBuf::from("cargo-home"),
        pixi_config: None,
        temporary_dir: PathBuf::from("tmp"),
        inherited_no_proxy: String::new(),
        proxy: isolock_proxy::ProxyConfig::default(),
        profile: None,
    };
    assert!(!session.require_plan);
}
