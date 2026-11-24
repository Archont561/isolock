//! Deterministic profile/platform closures, immutable manifests, and lockfile adapters.

mod lockfiles;
mod model;

pub use lockfiles::verify_integrity;
pub use model::*;

use isolock_cas::Sha256Digest;
use std::collections::{BTreeMap, BTreeSet};

/// Build the selected manifest and its constituent objects, without reading environment variables.
///
/// # Errors
/// Returns errors for invalid profile/platform/composition, unsafe or unreadable lock inputs,
/// unsupported lockfile entries, inconsistent catalog metadata, or manifest serialization.
pub fn build_profiles(
    request: &PlanRequest<'_>,
) -> Result<BTreeMap<String, ProfileManifest>, String> {
    let mut manifests = BTreeMap::new();
    build(
        request,
        request.profile,
        &mut BTreeSet::new(),
        &mut manifests,
    )?;
    Ok(manifests)
}

fn build(
    request: &PlanRequest<'_>,
    name: &str,
    active: &mut BTreeSet<String>,
    manifests: &mut BTreeMap<String, ProfileManifest>,
) -> Result<(), String> {
    if manifests.contains_key(name) {
        return Ok(());
    }
    if !active.insert(name.into()) {
        return Err(format!("profile composition cycle at {name}"));
    }
    let profile = request
        .profiles
        .get(name)
        .ok_or_else(|| format!("unknown profile {name}"))?
        .resolve(request.platform)?;
    profile.validate(name, request.platform)?;
    let mut manifest = ProfileManifest {
        schema_version: PROFILE_SCHEMA_VERSION,
        profile: name.into(),
        platform: request.platform.into(),
        configuration_digest: Sha256Digest::from_bytes(b""),
        specification: profile.clone(),
        storage: request.storage.clone(),
        inputs: BTreeMap::new(),
        artifacts: Vec::new(),
        cargo_payloads: BTreeMap::new(),
        constituents: BTreeMap::new(),
        materializers: BTreeMap::new(),
    };
    if profile.provider == Provider::Composed {
        compose(request, &profile, active, manifests, &mut manifest)?;
    } else {
        leaf(request, &profile, &mut manifest)?;
    }
    let mut unique = BTreeMap::new();
    for artifact in manifest.artifacts {
        let key = serde_json::to_string(&artifact).map_err(|error| error.to_string())?;
        unique.insert(key, artifact);
    }
    manifest.artifacts = unique.into_values().collect();
    manifest.materializers.insert("seeds".into(), 1);
    manifest.materializers.insert(
        format!("{:?}", manifest.specification.provider).to_ascii_lowercase(),
        1,
    );
    for artifact in &manifest.artifacts {
        manifest
            .materializers
            .insert(format!("{:?}", artifact.kind).to_ascii_lowercase(), 1);
    }
    manifest.configuration_digest = model::configuration_digest(&manifest)?;
    manifest.validate()?;
    active.remove(name);
    manifests.insert(name.into(), manifest);
    Ok(())
}

fn compose(
    request: &PlanRequest<'_>,
    profile: &Profile,
    active: &mut BTreeSet<String>,
    manifests: &mut BTreeMap<String, ProfileManifest>,
    manifest: &mut ProfileManifest,
) -> Result<(), String> {
    for child in &profile.compose {
        build(request, child, active, manifests)?;
        let child_manifest = manifests
            .get(child)
            .ok_or("constituent manifest was not built")?;
        manifest
            .constituents
            .insert(child.clone(), child_manifest.digest()?);
        for (path, digest) in &child_manifest.inputs {
            if manifest
                .inputs
                .insert(path.clone(), digest.clone())
                .is_some_and(|previous| previous != *digest)
            {
                return Err("composed profiles disagree on a locked input".into());
            }
        }
        for tool in child_manifest
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == ArtifactKind::Seed)
        {
            if manifest.artifacts.iter().any(|previous| {
                previous.kind == ArtifactKind::Seed
                    && previous.name == tool.name
                    && previous != tool
            }) {
                return Err("composed profiles must agree on shared seed tool policy".into());
            }
        }
        manifest.artifacts.extend(child_manifest.artifacts.clone());
        manifest
            .cargo_payloads
            .extend(child_manifest.cargo_payloads.clone());
    }
    Ok(())
}

fn leaf(
    request: &PlanRequest<'_>,
    profile: &Profile,
    manifest: &mut ProfileManifest,
) -> Result<(), String> {
    append_tools(&profile.seeds, ArtifactKind::Seed, &mut manifest.artifacts)?;
    append_tools(
        &profile.runtimes,
        ArtifactKind::Runtime,
        &mut manifest.artifacts,
    )?;
    if profile.provider == Provider::Pixi {
        manifest.artifacts.extend(lockfiles::pixi(
            request.root,
            profile,
            request.platform,
            &mut manifest.inputs,
        )?);
    }
    if let Some(path) = &profile.cargo_lock {
        let source = lockfiles::input(request.root, path, &mut manifest.inputs)?;
        manifest.artifacts.extend(lockfiles::cargo(
            &source,
            request.platform,
            &profile.cargo_git,
        )?);
        let derivation = CargoDerivation {
            schema_version: PROFILE_SCHEMA_VERSION,
            lock_digest: manifest
                .inputs
                .get(path)
                .ok_or("Cargo lock input was not captured")?
                .clone(),
            platform: request.platform.into(),
            policy: profile.cargo.clone(),
            // Two commits of one crate must never share a vendor tree, so the
            // reviewed git pins are part of the derivation identity.
            git: profile.cargo_git.clone(),
        };
        manifest
            .cargo_payloads
            .insert(canonical_digest(&derivation)?.as_str().into(), derivation);
    }
    if profile.pypi_lock.is_some() {
        manifest.artifacts.extend(lockfiles::pypi(
            request.root,
            profile,
            request.platform,
            &mut manifest.inputs,
        )?);
    }
    if profile.javascript_lock.is_some() {
        manifest.artifacts.extend(lockfiles::javascript(
            request.root,
            profile,
            request.platform,
            &mut manifest.inputs,
        )?);
    }
    Ok(())
}

fn append_tools(
    tools: &BTreeMap<String, ToolArtifact>,
    kind: ArtifactKind,
    artifacts: &mut Vec<Artifact>,
) -> Result<(), String> {
    for (name, tool) in tools {
        artifacts.push(Artifact {
            kind,
            name: name.clone(),
            version: tool.version.clone(),
            platform: tool.platform.clone(),
            digest: tool.digest.clone(),
            filename: tool.filename.clone(),
            source: tool.source.clone(),
            metadata: serde_json::to_value(tool).map_err(|error| error.to_string())?,
            integrity: None,
            destinations: Vec::new(),
        });
    }
    Ok(())
}

/// Identify a changed locked field without printing potentially sensitive values.
///
/// # Errors
/// Returns an error if either manifest cannot be represented as canonical JSON.
pub fn changed_input(
    expected: &ProfileManifest,
    actual: &ProfileManifest,
) -> Result<Option<String>, String> {
    let expected = serde_json::to_value(expected).map_err(|error| error.to_string())?;
    let actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    // Prefer a useful source field over its consequential digest changes.
    for field in [
        "specification",
        "inputs",
        "storage",
        "constituents",
        "artifacts",
        "cargo-payloads",
        "materializers",
        "profile",
        "platform",
        "schema-version",
    ] {
        if let Some(path) = different(&expected[field], &actual[field], field) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn different(
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    path: &str,
) -> Option<String> {
    if expected == actual {
        return None;
    }
    if let (Some(expected), Some(actual)) = (expected.as_object(), actual.as_object()) {
        let keys = expected
            .keys()
            .chain(actual.keys())
            .collect::<BTreeSet<_>>();
        for key in keys {
            let expected = expected.get(key).unwrap_or(&serde_json::Value::Null);
            let actual = actual.get(key).unwrap_or(&serde_json::Value::Null);
            if let Some(path) = different(expected, actual, &format!("{path}.{key}")) {
                return Some(path);
            }
        }
    }
    Some(path.into())
}
