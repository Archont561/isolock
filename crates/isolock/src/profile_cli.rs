//! Typed CLI boundary for profile planning, transport and materialization.

use crate::{config, materialize, profile_store, transport};
use clap::{Parser, Subcommand};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::{build_profiles, changed_input, PlanRequest, ProfileManifest};
use profile_store::{StoreLease, Workspace};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "isolock", version = isolock_core::VERSION, about = "Verified profile-scoped offline environments")]
struct Cli {
    #[command(flatten)]
    overrides: config::Overrides,
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Validate and explain typed configuration without planning or fetching artifacts.
    Config {
        #[command(subcommand)]
        command: Configuration,
    },
    /// Generate an immutable profile plan, or check it without changing it.
    Plan {
        #[arg(long)]
        ci: bool,
        #[arg(long)]
        locked: bool,
        #[arg(long)]
        check: bool,
    },
    /// Retrieve the manifest graph first, then only its missing blobs.
    Fetch {
        #[arg(long)]
        manifest: Option<String>,
    },
    /// Verify a locked plan and materialize only its selected profile.
    #[command(alias = "bootstrap")]
    Sync {
        #[arg(long)]
        locked: bool,
        #[arg(long)]
        offline: bool,
        #[arg(long)]
        manifest: Option<String>,
    },
    /// Export selected immutable manifests and verified bytes to a directory snapshot.
    Export {
        #[arg(long)]
        output: PathBuf,
    },
    /// Write the reviewed POSIX seed manifest and bootstrap script.
    SeedManifest {},
    /// Report unreachable canonical CAS blobs; deletion requires --apply.
    Gc {
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        root: Vec<String>,
    },
}

#[derive(Subcommand)]
enum Configuration {
    Check,
    Show,
    Explain { path: String },
}

pub fn handles(arguments: &[String]) -> bool {
    if arguments
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "exec" | "serve" | "ingest"))
    {
        return false;
    }
    if arguments.first().is_some_and(|arg| arg == "plan")
        && (arguments
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
            || Path::new("isolock.toml").is_file())
    {
        return true;
    }
    arguments
        .iter()
        .any(|arg| matches!(arg.as_str(), "--profile" | "--config" | "--user-config"))
        || arguments.first().is_some_and(|arg| {
            matches!(
                arg.as_str(),
                "config" | "fetch" | "sync" | "bootstrap" | "export" | "seed-manifest" | "gc"
            )
        })
        || arguments
            .iter()
            .any(|arg| arg.starts_with("--profile=") || arg.starts_with("--config="))
}

pub fn run(arguments: &[String]) -> Result<(), String> {
    let cli = match Cli::try_parse_from(
        std::iter::once("isolock".to_owned()).chain(arguments.iter().cloned()),
    ) {
        Ok(cli) => cli,
        Err(error) if !error.use_stderr() => {
            print!("{error}");
            return Ok(());
        }
        Err(_) => {
            return Err("invalid profile command or flags; use isolock COMMAND --help".into())
        }
    };
    let root = std::env::current_dir().map_err(|_| "cannot resolve project directory")?;
    let resolved = config::load(&root, &cli.overrides)?;
    let settings = &resolved.settings;
    if let Operation::Config { command } = &cli.command {
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
    if let Operation::Gc { apply, root } = &cli.command {
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
    let profile = cli
        .overrides
        .profile
        .as_deref()
        .ok_or("explicit --profile is required")?;
    let platform = cli
        .overrides
        .platform
        .as_deref()
        .ok_or("explicit --platform is required")?;
    isolock_plan::profile::identifier(profile)?;
    isolock_plan::profile::identifier(platform)?;
    let _lease = StoreLease::acquire(&workspace)?;
    execute(cli.command, &workspace, settings, profile, platform)
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
                offline,
                manifest.as_deref(),
            )?;
        }
        Operation::Export { output } => {
            let manifests = locked_graph(workspace, settings, profile, platform)?;
            transport::export(
                workspace,
                &manifests,
                &config::absolute(&workspace.root, &output),
            )?;
            println!("Exported selected profile {profile}/{platform}");
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

fn sync(
    workspace: &Workspace,
    settings: &config::Settings,
    profile: &str,
    platform: &str,
    offline: bool,
    digest: Option<&str>,
) -> Result<(), String> {
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
        report.push_str(&format!(
            "- {digest} {}\n",
            labels.into_iter().collect::<Vec<_>>().join(", ")
        ));
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
