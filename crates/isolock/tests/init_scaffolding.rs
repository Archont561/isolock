//! `isolock init` scaffolds a valid project whose pins are real and whose gaps are honest.
//!
//! The point of these tests is that scaffolding must not fabricate a pin. A digest names
//! bytes a reviewer holds, so a generated one would be a placeholder, and ISO-32 exists so
//! that placeholders cannot pass the gates. The journey below fills only the reviewed
//! fields and never edits structural configuration.
#![allow(clippy::unwrap_used, clippy::expect_used)] // Fixtures fail fast on invalid setup.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

const PLATFORM: &str = "linux-64";

struct Project {
    temp: TempDir,
    root: PathBuf,
    /// A stripped copy of the binary. `init` pins the executable that runs it, and a
    /// debug build exceeds the reviewed-ingest size limit, so the journey needs an
    /// executable whose bytes can actually be ingested, as a release build's can.
    executable: PathBuf,
}

impl Project {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        let executable = temp.path().join("isolock");
        fs::copy(env!("CARGO_BIN_EXE_isolock"), &executable).unwrap();
        let stripped = Command::new("strip")
            .args(["--strip-unneeded"])
            .arg(&executable)
            .status()
            .unwrap();
        assert!(stripped.success(), "cannot strip the fixture binary");
        Self {
            temp,
            root,
            executable,
        }
    }

    /// Runs the binary under test from the project directory.
    fn run(&self, arguments: &[&str]) -> Output {
        Command::new(&self.executable)
            .args(arguments)
            .current_dir(&self.root)
            .env("HOME", self.temp.path().join("home"))
            .env("XDG_CONFIG_HOME", self.temp.path().join("config"))
            .output()
            .unwrap()
    }

    fn success(&self, arguments: &[&str]) -> Output {
        let output = self.run(arguments);
        assert!(
            output.status.success(),
            "command {arguments:?} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Writes a reviewed manifest pinning this binary, the bytes a project always has.
fn pin_the_seed(project: &Project) -> PathBuf {
    let originals = project.root.join("originals");
    fs::create_dir_all(&originals).unwrap();
    let executable = &project.executable;
    let name = "isolock-seed";
    let bytes = fs::read(executable).unwrap();
    fs::write(originals.join(name), &bytes).unwrap();
    let digest = isolock_cas::Sha256Digest::from_bytes(&bytes);
    let manifest = format!(
        "{{\n  \"schema-version\": 1,\n  \"artifacts\": [\n    {{\n      \"registry\": \"generic\",\n      \"name\": \"isolock\",\n      \"version\": \"0.1.0\",\n      \"sha256\": \"{}\",\n      \"filename\": \"{name}\",\n      \"source\": \"https://example.test/isolock\"\n    }}\n  ]\n}}\n",
        digest.as_str()
    );
    fs::write(project.root.join("isolock.ingest.json"), manifest).unwrap();
    originals
}

fn init(project: &Project) {
    project.success(&["init", "--profile", "dev", "--platform", PLATFORM]);
}

#[test]
fn init_writes_a_configuration_that_validates_and_refuses_to_overwrite() {
    let project = Project::new();
    init(&project);
    assert!(project.root.join("isolock.toml").is_file());
    assert!(project.root.join("isolock.ingest.json").is_file());

    // AC #2: the generated configuration validates immediately.
    let check = project.success(&["config", "check"]);
    assert!(
        text(&check).contains("Configuration is valid (schema 1)."),
        "{}",
        text(&check)
    );

    // AC #1: a second run refuses rather than replacing reviewed files.
    let again = project.run(&["init", "--profile", "dev", "--platform", PLATFORM]);
    assert!(!again.status.success(), "{again:?}");
    let refusal = text(&again);
    assert!(refusal.contains("refusing to overwrite"), "{refusal}");
    assert!(refusal.contains("isolock.toml"), "{refusal}");
    assert!(
        refusal.contains("--force"),
        "the remedy must be named: {refusal}"
    );

    // The explicit flag replaces them.
    project.success(&[
        "init",
        "--profile",
        "dev",
        "--platform",
        PLATFORM,
        "--force",
    ]);
}

/// AC #3: no digest, version, or source is fabricated.
#[test]
fn init_generates_no_placeholder_pins() {
    let project = Project::new();
    init(&project);
    let configuration = fs::read_to_string(project.root.join("isolock.toml")).unwrap();
    assert!(
        !configuration.contains("0000000000000000000000000000000000000000000000000000000000000000"),
        "a placeholder digest was generated:\n{configuration}"
    );
    // The one digest present is computed over the running binary, which really is on disk.
    // That is the `isolock` binary the child process pins, not this test harness.
    let seed = configuration
        .split("digest = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the seed digest");
    let expected = isolock_cas::Sha256Digest::from_bytes(&fs::read(&project.executable).unwrap());
    assert_eq!(
        seed,
        expected.as_str(),
        "the generated digest must describe real bytes"
    );

    // The manifest template states shape and invents no artifact.
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(project.root.join("isolock.ingest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["schema-version"], 1);
    assert_eq!(
        manifest["artifacts"].as_array().map(Vec::len),
        Some(0),
        "the template must not contain an invented artifact"
    );
}

/// AC #5: fill only the reviewed fields, then plan, ingest, and sync.
#[test]
fn a_scaffolded_project_completes_plan_ingest_and_sync() {
    let project = Project::new();
    init(&project);
    let originals = pin_the_seed(&project);

    project.success(&[
        "ingest",
        "--manifest",
        "isolock.ingest.json",
        "--artifacts",
        originals.to_str().unwrap(),
    ]);
    project.success(&["plan", "--profile", "dev", "--platform", PLATFORM]);
    // `--offline` reads the CAS directly, so a locally reviewed project needs no remote.
    let sync = project.success(&[
        "sync",
        "--profile",
        "dev",
        "--platform",
        PLATFORM,
        "--offline",
    ]);
    assert!(text(&sync).contains("ready"), "{}", text(&sync));

    // The scaffolded configuration was never hand-edited to make this work.
    let configuration = fs::read_to_string(project.root.join("isolock.toml")).unwrap();
    assert!(configuration.contains("[profiles.dev]"));
}

/// AC #2's second half: an unpopulated project fails only for the missing artifact.
#[test]
fn an_unpopulated_project_plans_and_reports_only_the_missing_artifact() {
    let project = Project::new();
    init(&project);

    let plan = project.success(&["plan", "--profile", "dev", "--platform", PLATFORM]);
    let report = text(&plan);
    assert!(
        report.contains("are not yet in CAS"),
        "plan must fail for the honest reason: {report}"
    );
    assert!(
        !report.contains("invalid") && !report.contains("requires"),
        "plan hit a structural error instead: {report}"
    );
}
