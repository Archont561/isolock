//! Fetch, sync/bootstrap, export, seed-manifest, and garbage collection workflows.

use crate::application::ports::{
    ArtifactTransport, BlobStore, CollectionReport, GarbageCollector, Materializer, PlanStore,
    ProfileGraph, Progress, UseCaseResult, WorkspaceSelection,
};
use crate::application::use_cases::plan::{locked_graph, require_present};
use crate::domain::{DomainError, Settings, Transport};
use isolock_cas::Sha256Digest;
use isolock_plan::profile::changed_input;
use std::path::{Path, PathBuf};

/// The reviewed context every bootstrap workflow needs.
pub struct Workspace<'a, S: PlanStore, T: ArtifactTransport> {
    /// Locked plans, manifest objects, and CAS.
    pub store: &'a S,
    /// The configured artifact source.
    pub transport: &'a T,
    /// Validated settings for the project.
    pub settings: &'a Settings,
    /// The explicit profile and platform selection.
    pub selection: &'a WorkspaceSelection,
    /// Where progress lines are reported.
    pub progress: &'a mut dyn Progress,
}

/// What a fetch decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchOutcome {
    pub profile: String,
    pub platform: String,
    /// How many payloads were transferred into CAS.
    pub transferred: usize,
}

/// What a sync decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutcome {
    pub profile: String,
    pub platform: String,
    /// The activation prefix, as the operator sees it.
    pub prefix: String,
    pub transferred: usize,
}

/// What a seed-manifest run wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedOutcome {
    pub manifest: String,
    pub bootstrap: String,
}

/// Retrieves the selected manifest graph and its missing blobs.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when no plan is locked, the graph cannot be
/// fetched, or any artifact is still missing afterwards.
pub fn fetch_profile<S: PlanStore, T: ArtifactTransport>(
    workspace: Workspace<'_, S, T>,
    manifest: Option<&str>,
) -> UseCaseResult<FetchOutcome> {
    let profile = workspace.selection.require_profile()?;
    let platform = workspace.selection.require_platform()?;
    let mut workspace = workspace;
    let (graph, transferred) = retrieve(&mut workspace, profile, platform, manifest)?;
    require_present(workspace.store, &graph, profile)?;
    Ok(FetchOutcome {
        profile: profile.into(),
        platform: platform.into(),
        transferred,
    })
}

/// Materializes the selected locked profile on this host.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the platform does not match the host, an
/// input changed, an artifact is missing, or materialization fails.
pub fn sync_profile<S: PlanStore, T: ArtifactTransport, M: Materializer>(
    workspace: Workspace<'_, S, T>,
    materializer: &M,
    offline: bool,
    manifest: Option<&str>,
) -> UseCaseResult<SyncOutcome> {
    let profile = workspace.selection.require_profile()?;
    let platform = workspace.selection.require_platform()?;
    let host = materializer.host_platform().ok_or_else(|| {
        DomainError::new("profile materialization currently requires a supported POSIX host")
    })?;
    if platform != host {
        return Err(DomainError::new(
            "selected seed platform does not match this host; host fallback is forbidden",
        ));
    }
    let mut workspace = workspace;
    let (graph, transferred) = if offline {
        let graph = locked_graph(workspace.store, workspace.settings, profile, platform)?;
        if let Some(expected) = manifest {
            let expected = Sha256Digest::parse(expected).map_err(DomainError::new)?;
            if graph
                .get(profile)
                .ok_or_else(|| DomainError::new("profile is missing"))?
                .digest()?
                != expected
            {
                return Err(DomainError::new("selected manifest digest mismatch"));
            }
        }
        (graph, 0)
    } else {
        retrieve(&mut workspace, profile, platform, manifest)?
    };
    require_present(workspace.store, &graph, profile)?;
    verify_inputs(workspace.store.root(), &graph)?;
    let prefix = materializer.install(&graph, profile)?;
    Ok(SyncOutcome {
        profile: profile.into(),
        platform: platform.into(),
        prefix: prefix.display().to_string(),
        transferred,
    })
}

/// Exports the selected locked profile to an offline directory snapshot.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the plan is not locked, an artifact is
/// missing, or the snapshot cannot be written.
pub fn export_profile<S: PlanStore, T: ArtifactTransport>(
    store: &S,
    transport: &T,
    settings: &Settings,
    selection: &WorkspaceSelection,
    output: &Path,
) -> UseCaseResult<()> {
    let profile = selection.require_profile()?;
    let platform = selection.require_platform()?;
    let graph = locked_graph(store, settings, profile, platform)?;
    let mut blobs = StoreBlobs { store };
    transport.export(&mut blobs, &graph, output)
}

/// Writes the reviewed POSIX seed manifest and bootstrap script.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when the plan is not locked or a file cannot be
/// written.
pub fn seed_manifest<S: PlanStore, M: Materializer>(
    store: &S,
    materializer: &M,
    settings: &Settings,
    selection: &WorkspaceSelection,
) -> UseCaseResult<SeedOutcome> {
    let profile = selection.require_profile()?;
    let platform = selection.require_platform()?;
    let graph = locked_graph(store, settings, profile, platform)?;
    let manifest = crate::application::use_cases::plan::selected(&graph, profile)?;
    let contents = materializer.bootstrap_manifest(manifest)?;
    let path = materializer.seed_path(profile, platform)?;
    materializer.write_seed(&path, &contents)?;
    Ok(SeedOutcome {
        manifest: path.display().to_string(),
        bootstrap: store
            .root()
            .join(".isolock/bootstrap.sh")
            .display()
            .to_string(),
    })
}

fn retrieve<S: PlanStore, T: ArtifactTransport>(
    workspace: &mut Workspace<'_, S, T>,
    profile: &str,
    platform: &str,
    manifest: Option<&str>,
) -> UseCaseResult<(ProfileGraph, usize)> {
    let store = workspace.store;
    let transport = workspace.transport;
    let settings = workspace.settings;
    let mut transferred = 0;
    let graph = if let Some(digest) = manifest {
        let digest = Sha256Digest::parse(digest).map_err(DomainError::new)?;
        let manifests = if settings.storage.source.is_none() {
            // A reviewed local bootstrap needs no remote merely because it pins its
            // already-available manifest explicitly.
            store.graph(store.load_object(&digest)?)?
        } else {
            let source = transport.snapshot(None)?;
            let mut blobs = StoreBlobs { store };
            transport.fetch_graph(&mut blobs, &source, &digest)?
        };
        let selected = manifests
            .get(profile)
            .ok_or_else(|| DomainError::new("source does not contain the selected profile"))?;
        if selected.platform != platform || selected.digest()? != digest {
            return Err(DomainError::new("selected manifest identity mismatch"));
        }
        crate::application::use_cases::plan::preflight(settings, &manifests)?;
        // Lock trust boundary before payload retrieval, even on the first restore.
        let identity = store.storage_identity(Some(&selected.storage))?;
        let actual = isolock_plan::profile::build_profiles(&isolock_plan::profile::PlanRequest {
            root: store.root(),
            profile,
            platform,
            profiles: &settings.profiles,
            storage: identity,
        })?;
        if let Some(path) = changed_input(
            selected,
            actual
                .get(profile)
                .ok_or_else(|| DomainError::new("profile is missing"))?,
        )? {
            return Err(DomainError::new(format!(
                "Profile {profile}/{platform}: locked input changed: {path}"
            )));
        }
        store.save(&manifests, false)?;
        manifests
    } else {
        locked_graph(store, settings, profile, platform)?
    };
    let selected = graph
        .get(profile)
        .ok_or_else(|| DomainError::new("selected profile is missing"))?;
    if !store.missing(&graph)?.is_empty() {
        let source = transport.snapshot(Some(&selected.storage))?;
        // A source snapshot must supply the reviewed manifest before supplying any blob.
        // Git stores may publish manifests after the pinned payload commit; use the named
        // ref for manifest lookup and the locked commit exclusively for payload reads.
        let manifest_source = if settings.storage.transport == Transport::Git {
            transport.snapshot(None)?
        } else {
            transport.snapshot(Some(&selected.storage))?
        };
        let mut blobs = StoreBlobs { store };
        let fetched = transport.fetch_graph(&mut blobs, &manifest_source, &selected.digest()?)?;
        transferred = transport.fetch_blobs(&mut blobs, &source, &fetched)?;
        workspace.progress.line(&format!(
            "Fetched {transferred} selected blob(s); unrelated profiles were not copied into CAS"
        ));
    }
    Ok((graph, transferred))
}

fn verify_inputs(root: &Path, manifests: &ProfileGraph) -> UseCaseResult<()> {
    for manifest in manifests.values() {
        for (path, digest) in &manifest.inputs {
            if isolock_cas::digest_file(&root.join(Path::new(path)))
                .map_err(|_| DomainError::new("cannot verify local locked input"))?
                != *digest
            {
                return Err(DomainError::new(format!(
                    "locked input changed before materialization: {path}"
                )));
            }
        }
    }
    Ok(())
}

/// The project store seen through the transport's blob port.
struct StoreBlobs<'a, S: PlanStore> {
    store: &'a S,
}

impl<S: PlanStore> BlobStore for StoreBlobs<'_, S> {
    fn write_manifest(
        &mut self,
        manifest: &isolock_plan::profile::ProfileManifest,
    ) -> UseCaseResult<()> {
        self.store.save_object(manifest)
    }

    fn holds(&self, digest: &Sha256Digest) -> UseCaseResult<bool> {
        self.store.verified(digest)
    }

    fn path_of(&self, digest: &Sha256Digest) -> UseCaseResult<PathBuf> {
        Ok(self.store.blob_path(digest))
    }

    fn put_file(&mut self, digest: &Sha256Digest, path: &Path) -> UseCaseResult<()> {
        self.store.put_file(digest, path)
    }

    fn put_reader(
        &mut self,
        digest: &Sha256Digest,
        reader: &mut dyn std::io::Read,
    ) -> UseCaseResult<()> {
        self.store.put_reader(digest, reader)
    }
}

/// Reports unreachable CAS blobs; deletion requires an explicit apply.
///
/// # Errors
/// Returns a [`crate::domain::DomainError`] when no root can be discovered or the shared CAS
/// registration is unsafe.
pub fn garbage_collect<G: GarbageCollector>(
    collector: &G,
    roots: &[String],
    apply: bool,
) -> UseCaseResult<CollectionReport> {
    collector.collect(roots, apply)
}
