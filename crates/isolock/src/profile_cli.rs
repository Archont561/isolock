//! Execute typed profile operations without inspecting or reparsing process arguments.

use crate::{config, materialize, profile_store, transport};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{build_profiles, changed_input, PlanRequest, ProfileManifest};
use profile_store::{StoreLease, Workspace};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Profile operations already parsed and validated by the application command tree.
pub enum Operation {
    Config {
        command: Configuration,
    },
    Plan {
        ci: bool,
        locked: bool,
        check: bool,
    },
    Fetch {
        manifest: Option<String>,
    },
    Sync {
        locked: bool,
        offline: bool,
        manifest: Option<String>,
    },
    Export {
        output: PathBuf,
        quiet: bool,
    },
    SeedManifest {},
    Gc {
        apply: bool,
        root: Vec<String>,
    },
}

pub enum Configuration {
    Check,
    Show,
    Explain { path: String },
}

pub fn run(overrides: &config::Overrides, operation: Operation) -> Result<(), String> {
    let root = std::env::current_dir().map_err(|_| "cannot resolve project directory")?;
    let resolved = config::load(&root, overrides)?;
    let settings = &resolved.settings;
    if let Operation::Config { command } = &operation {
        match command {
            Configuration::Check => println!(
                "Configuration is valid (schema {}).",
                settings.schema_version
            ),
            Configuration::Show => println!(
                "{}",
                serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?
            ),
            Configuration::Explain { path } => println!("{}", config::explain(&resolved, path)?),
        }
        return Ok(());
    }
    let workspace = Workspace::new(&root, settings);
    if let Operation::Gc { apply, root } = &operation {
        let roots = if root.is_empty() {
            &settings.gc.roots
        } else {
            root
        };
        let report = profile_store::garbage_collect(&workspace, roots, *apply)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let profile = overrides
        .profile
        .as_deref()
        .ok_or("explicit --profile is required")?;
    let platform = overrides
        .platform
        .as_deref()
        .ok_or("explicit --platform is required")?;
    isolock_plan::profile::identifier(profile)?;
    isolock_plan::profile::identifier(platform)?;
    let _lease = StoreLease::acquire(&workspace)?;
    execute(operation, &workspace, settings, profile, platform)
}

fn execute(
    command: Operation,
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
) -> Result<(), String> {
    match command {
        Operation::Plan { ci, locked, check } => plan(
            workspace,
            settings,
            profile,
            platform,
            ci || locked || check,
        )?,
        Operation::Fetch { manifest } => {
            let manifests = retrieve(workspace, settings, profile, platform, manifest.as_deref())?;
            report_missing(workspace, &manifests, profile)?;
            println!("Profile {profile}/{platform}: selected closure is verified in CAS");
        }
        Operation::Sync {
            locked,
            offline,
            manifest,
        } => {
            let _ = locked; // Sync is always locked; this flag documents CI intent.
            sync(
                workspace,
                settings,
                profile,
                platform,
                SyncOptions {
                    offline,
                    digest: manifest.as_deref(),
                },
            )?;
        }
        Operation::Export { output, quiet } => {
            let manifests = locked_graph(workspace, settings, profile, platform)?;
            transport::export(
                workspace,
                &manifests,
                &config::absolute(&workspace.root, &output),
            )?;
            if !quiet {
                println!("Exported selected profile {profile}/{platform}");
            }
        }
        Operation::SeedManifest {} => seed_manifest(workspace, settings, profile, platform)?,
        Operation::Config { .. } | Operation::Gc { .. } => {
            unreachable!("handled before profile selection")
        }
    }
    Ok(())
}

fn plan(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
    check: bool,
) -> Result<(), String> {
    if check {
        let manifests = locked_graph(workspace, settings, profile, platform)?;
        report_missing(workspace, &manifests, profile)?;
        println!("Profile {profile}/{platform}: locked inputs match; ALL DEPENDENCIES SATISFIED");
    } else {
        let identity = transport::identity(&workspace.root, &settings.storage, None)?;
        let manifests = build_profiles(&PlanRequest {
            root: &workspace.root,
            profile,
            platform,
            profiles: &settings.profiles,
            storage: identity,
        })?;
        workspace.save(&manifests, true)?;
        let manifest = manifests
            .get(profile)
            .ok_or("selected plan was not built")?;
        println!(
            "Profile {profile}/{platform}: {}\nplan: {}",
            manifest.digest()?.as_str(),
            workspace.alias(profile, platform)?.display()
        );
        let missing = workspace.missing(&manifests)?;
        if !missing.is_empty() {
            println!("{} selected artifact(s) are not yet in CAS; use fetch or ingest the reviewed originals", missing.len());
        }
    }
    Ok(())
}

struct SyncOptions<'a> {
    offline: bool,
    digest: Option<&'a str>,
}

fn sync(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
    options: SyncOptions<'_>,
) -> Result<(), String> {
    let SyncOptions { offline, digest } = options;
    let host = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-64",
        ("linux", "aarch64") => "linux-aarch64",
        ("macos", "x86_64") => "osx-64",
        ("macos", "aarch64") => "osx-arm64",
        _ => {
            return Err("profile materialization currently requires a supported POSIX host".into())
        }
    };
    if platform != host {
        return Err(
            "selected seed platform does not match this host; host fallback is forbidden".into(),
        );
    }
    let manifests = if offline {
        let graph = locked_graph(workspace, settings, profile, platform)?;
        if let Some(expected) = digest {
            let expected = Sha256Digest::parse(expected).map_err(str::to_owned)?;
            if graph.get(profile).ok_or("profile is missing")?.digest()? != expected {
                return Err("selected manifest digest mismatch".into());
            }
        }
        graph
    } else {
        retrieve(workspace, settings, profile, platform, digest)?
    };
    report_missing(workspace, &manifests, profile)?;
    verify_inputs(workspace, &manifests)?;
    let prefix = materialize::install(workspace, &manifests, profile)?;
    println!("Profile {profile}/{platform} ready: {}\nActivate: . .isolock/activate/{profile}-{platform}", prefix.display());
    Ok(())
}

fn seed_manifest(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
) -> Result<(), String> {
    let manifests = locked_graph(workspace, settings, profile, platform)?;
    let manifest = manifests.get(profile).ok_or("selected plan is missing")?;
    let output = workspace
        .root
        .join(".isolock/seeds")
        .join(format!("{profile}-{platform}.manifest"));
    profile_store::write_file(
        &output,
        materialize::bootstrap_manifest(manifest)?.as_bytes(),
        false,
    )?;
    profile_store::write_file(
        &workspace.root.join(".isolock/bootstrap.sh"),
        include_bytes!("../../../templates/bootstrap.sh"),
        false,
    )?;
    println!("POSIX bootstrap manifest: {}", output.display());
    Ok(())
}

fn locked_graph(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
) -> Result<BTreeMap<String, ProfileManifest>, String> {
    let expected = workspace.load(profile, platform)?;
    let expected_graph = workspace.graph(expected.clone())?;
    preflight(workspace, settings, &expected_graph)?;
    let identity =
        transport::identity(&workspace.root, &settings.storage, Some(&expected.storage))?;
    let actual = build_profiles(&PlanRequest {
        root: &workspace.root,
        profile,
        platform,
        profiles: &settings.profiles,
        storage: identity,
    })?;
    let selected = actual.get(profile).ok_or("selected profile is missing")?;
    if let Some(path) = changed_input(&expected, selected)? {
        return Err(format!(
            "Profile {profile}/{platform}: locked input changed: {path}"
        ));
    }
    workspace.graph(expected)
}

fn retrieve(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
    digest: Option<&str>,
) -> Result<BTreeMap<String, ProfileManifest>, String> {
    let graph = if let Some(digest) = digest {
        let digest = Sha256Digest::parse(digest).map_err(str::to_owned)?;
        let manifests = if settings.storage.source.is_none() {
            // A reviewed local bootstrap needs no remote merely because it pins its
            // already-available manifest explicitly.
            workspace.graph(workspace.load_object(&digest)?)?
        } else {
            let source = transport::snapshot(&workspace.root, &settings.storage, None)?;
            transport::fetch_graph(workspace, &source, &digest)?
        };
        let manifest = manifests
            .get(profile)
            .ok_or("source does not contain the selected profile")?;
        if manifest.platform != platform || manifest.digest()? != digest {
            return Err("selected manifest identity mismatch".into());
        }
        preflight(workspace, settings, &manifests)?;
        // Lock trust boundary before payload retrieval, even on the first restore.
        let identity =
            transport::identity(&workspace.root, &settings.storage, Some(&manifest.storage))?;
        let actual = build_profiles(&PlanRequest {
            root: &workspace.root,
            profile,
            platform,
            profiles: &settings.profiles,
            storage: identity,
        })?;
        if let Some(path) =
            changed_input(manifest, actual.get(profile).ok_or("profile is missing")?)?
        {
            return Err(format!(
                "Profile {profile}/{platform}: locked input changed: {path}"
            ));
        }
        workspace.save(&manifests, false)?;
        manifests
    } else {
        locked_graph(workspace, settings, profile, platform)?
    };
    let selected = graph.get(profile).ok_or("selected profile is missing")?;
    if !workspace.missing(&graph)?.is_empty() {
        let source =
            transport::snapshot(&workspace.root, &settings.storage, Some(&selected.storage))?;
        // A source snapshot must supply the reviewed manifest before supplying any blob.
        // Git stores may publish manifests after the pinned payload commit; use the named
        // ref for manifest lookup and the locked commit exclusively for payload reads.
        let manifest_source = if settings.storage.transport == config::Transport::Git {
            transport::snapshot(&workspace.root, &settings.storage, None)?
        } else {
            transport::snapshot(&workspace.root, &settings.storage, Some(&selected.storage))?
        };
        let fetched = transport::fetch_graph(workspace, &manifest_source, &selected.digest()?)?;
        let count = transport::fetch_blobs(workspace, &source, &fetched)?;
        println!("Fetched {count} selected blob(s); unrelated profiles were not copied into CAS");
    }
    Ok(graph)
}

fn report_missing(
    workspace: &Workspace,
    manifests: &BTreeMap<String, ProfileManifest>,
    profile: &str,
) -> Result<(), String> {
    let missing = workspace.missing(manifests)?;
    if missing.is_empty() {
        return Ok(());
    }
    let mut report = format!("Profile {profile}: missing artifacts\n");
    for digest in missing {
        let labels = manifests
            .values()
            .flat_map(|manifest| manifest.artifacts.iter())
            .filter(|artifact| artifact.digest.as_str() == digest)
            .map(|artifact| {
                format!(
                    "{}=={} ({:?})",
                    artifact.name, artifact.version, artifact.kind
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let _ = writeln!(
            report,
            "- {digest} {}",
            labels.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    Err(report.trim_end().into())
}

fn verify_inputs(
    workspace: &Workspace,
    manifests: &BTreeMap<String, ProfileManifest>,
) -> Result<(), String> {
    for manifest in manifests.values() {
        for (path, digest) in &manifest.inputs {
            if isolock_cas::digest_file(&workspace.root.join(Path::new(path)))
                .map_err(|_| "cannot verify local locked input")?
                != *digest
            {
                return Err(format!(
                    "locked input changed before materialization: {path}"
                ));
            }
        }
    }
    Ok(())
}

fn preflight(
    workspace: &Workspace,
    settings: &config::Settings,
    manifests: &BTreeMap<String, ProfileManifest>,
) -> Result<(), String> {
    for expected in manifests.values() {
        let mut actual = expected.clone();
        actual.specification = settings
            .profiles
            .get(&expected.profile)
            .ok_or("locked profile was removed from configuration")?
            .resolve(&expected.platform)?;
        if let Some(path) = changed_input(expected, &actual)? {
            return Err(format!(
                "Profile {}/{}: locked input changed: {path}",
                expected.profile, expected.platform
            ));
        }
        for (path, digest) in &expected.inputs {
            let current = isolock_cas::digest_file(&workspace.root.join(path))
                .map_err(|_| format!("locked input is missing: inputs.{path}"))?;
            if &current != digest {
                return Err(format!(
                    "Profile {}/{}: locked input changed: inputs.{path}",
                    expected.profile, expected.platform
                ));
            }
        }
    }
    Ok(())
}
