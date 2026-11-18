//! Canonical lockfile adapters. No solver or network access occurs here.

use super::model::*;
use isolock_cas::Sha256Digest;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) fn input(
    root: &Path,
    path: &str,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<String, String> {
    relative_path(path)?;
    let resolved = root
        .join(path)
        .canonicalize()
        .map_err(|_| format!("cannot read locked input {path}"))?;
    let project = root
        .canonicalize()
        .map_err(|_| "cannot resolve project root")?;
    if !resolved.starts_with(project) {
        return Err("locked input symlink escapes project root".into());
    }
    let bytes = fs::read(&resolved)
        .map_err(|error| format!("cannot read locked input {path}: {}", error.kind()))?;
    inputs.insert(path.into(), Sha256Digest::from_bytes(&bytes));
    String::from_utf8(bytes).map_err(|_| format!("locked input {path} is not UTF-8"))
}

pub(super) fn pixi(
    root: &Path,
    profile: &Profile,
    platform: &str,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<Vec<Artifact>, String> {
    let path = profile.pixi_lock.as_deref().ok_or("missing pixi-lock")?;
    let source = input(root, path, inputs)?;
    let manifest = profile
        .pixi_manifest
        .as_deref()
        .ok_or("missing pixi-manifest")?;
    input(root, manifest, inputs)?;
    let lock = rattler_lock::LockFile::from_str_with_base_directory(&source, Some(root))
        .map_err(|_| "invalid Pixi lockfile".to_owned())?;
    let selected_platform = lock
        .platform(platform)
        .ok_or("Pixi lockfile does not contain selected platform")?;
    let environment = lock
        .environment(
            profile
                .environment
                .as_deref()
                .ok_or("missing Pixi environment")?,
        )
        .ok_or("Pixi lockfile does not contain selected environment")?;
    let packages = environment
        .packages(selected_platform)
        .ok_or("Pixi environment does not contain selected platform")?;
    packages
        .map(|package| pixi_artifact(package, platform))
        .collect()
}

fn pixi_artifact(
    package: &rattler_lock::LockedPackage,
    platform: &str,
) -> Result<Artifact, String> {
    let source = package.location().to_string();
    public_source(&source)?;
    if let Some(binary) = package.as_binary_conda() {
        let record = &binary.package_record;
        let sha = record
            .sha256
            .as_ref()
            .ok_or("locked Conda packages require SHA-256")?;
        return Ok(Artifact {
            kind: ArtifactKind::Conda,
            name: record.name.as_normalized().into(),
            version: record.version.to_string(),
            platform: platform.into(),
            digest: parse_hex(&hex::encode(sha))?,
            filename: binary.file_name.to_string(),
            source,
            metadata: serde_json::to_value(record).map_err(|error| error.to_string())?,
            integrity: None,
            destinations: Vec::new(),
        });
    }
    let wheel = package
        .as_pypi()
        .and_then(|package| package.as_wheel())
        .ok_or("source packages are not permitted in a locked Pixi profile")?;
    let sha = wheel
        .hash
        .as_ref()
        .and_then(|hash| hash.sha256())
        .ok_or("locked wheels require SHA-256")?;
    let filename = source
        .rsplit('/')
        .next()
        .ok_or("wheel source has no filename")?
        .to_owned();
    Ok(Artifact {
        kind: ArtifactKind::Wheel,
        name: wheel.name.to_string(),
        version: wheel.version.to_string(),
        platform: platform.into(),
        digest: parse_hex(&hex::encode(sha))?,
        filename,
        source,
        metadata: serde_json::json!({"requires-python": wheel.requires_python.as_ref().map(ToString::to_string)}),
        integrity: None,
        destinations: Vec::new(),
    })
}

pub(super) fn cargo(source: &str, platform: &str) -> Result<Vec<Artifact>, String> {
    let lock: cargo_lock::Lockfile = source
        .parse()
        .map_err(|_| "invalid Cargo.lock".to_owned())?;
    let mut artifacts = Vec::new();
    for package in lock.packages {
        let Some(origin) = package.source else {
            continue;
        };
        if !origin.is_registry() {
            return Err("Cargo git dependencies must be vendored into a reviewed registry before locked planning".into());
        }
        let checksum = package
            .checksum
            .ok_or("registry Cargo packages require checksums")?;
        let name = package.name.to_string();
        let version = package.version.to_string();
        // Registry identity stays in provenance; transport retrieves by digest, never this URL.
        let source = origin.to_string();
        public_source(&source)?;
        artifacts.push(Artifact {
            kind: ArtifactKind::Cargo,
            filename: format!("{name}-{version}.crate"),
            name,
            version,
            platform: platform.into(),
            digest: parse_hex(&checksum.to_string())?,
            source,
            metadata: serde_json::Value::Null,
            integrity: None,
            destinations: Vec::new(),
        });
    }
    Ok(artifacts)
}

fn catalog(
    root: &Path,
    profile: &Profile,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<Vec<CatalogEntry>, String> {
    let path = profile
        .catalog
        .as_deref()
        .ok_or("this lockfile requires an explicit artifact catalog")?;
    let source = input(root, path, inputs)?;
    let entries: Vec<CatalogEntry> =
        serde_json::from_str(&source).map_err(|_| "invalid artifact catalog")?;
    for entry in &entries {
        filename(&entry.filename)?;
        public_source(&entry.source)?;
    }
    Ok(entries)
}

fn catalog_artifact(entry: &CatalogEntry, kind: ArtifactKind) -> Result<Artifact, String> {
    filename(&entry.filename)?;
    public_source(&entry.source)?;
    Ok(Artifact {
        kind,
        name: entry.name.clone(),
        version: entry.version.clone(),
        platform: entry.platform.clone(),
        digest: entry.digest.clone(),
        filename: entry.filename.clone(),
        source: entry.source.clone(),
        metadata: serde_json::Value::Null,
        integrity: entry.integrity.clone(),
        destinations: Vec::new(),
    })
}

pub(super) fn pypi(
    root: &Path,
    profile: &Profile,
    platform: &str,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<Vec<Artifact>, String> {
    let source = input(
        root,
        profile.pypi_lock.as_deref().ok_or("missing pypi-lock")?,
        inputs,
    )?;
    let entries = catalog(root, profile, inputs)?;
    let source = source.replace("\\\n", " ");
    let mut artifacts = Vec::new();
    for line in source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        artifacts.push(pinned_wheel(line, &entries, platform)?);
    }
    Ok(artifacts)
}

fn pinned_wheel(line: &str, entries: &[CatalogEntry], platform: &str) -> Result<Artifact, String> {
    let mut fields = line.split_whitespace();
    let requirement = fields.next().ok_or("empty locked requirement")?;
    let (name, version) = requirement
        .split_once("==")
        .ok_or("PyPI locks require exact name==version pins")?;
    identifier(name)?;
    if version.is_empty() || version.contains([';', '*', '@']) {
        return Err("PyPI locks must be platform-resolved, exact pins without markers".into());
    }
    let hashes = fields
        .map(|field| {
            field
                .strip_prefix("--hash=sha256:")
                .ok_or_else(|| "PyPI locks accept only SHA-256 hashes after a pin".to_owned())
                .and_then(parse_hex)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if hashes.is_empty() {
        return Err("PyPI locks require SHA-256 hashes".into());
    }
    let matches = entries
        .iter()
        .filter(|entry| {
            entry.name == name
                && entry.version == version
                && entry.platform == platform
                && hashes.contains(&entry.digest)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one catalog wheel for {name}=={version} on {platform}"
        ));
    }
    let artifact = catalog_artifact(matches[0], ArtifactKind::Wheel)?;
    if !artifact.filename.ends_with(".whl") {
        return Err("offline PyPI profiles require built wheels, not source distributions".into());
    }
    Ok(artifact)
}

pub(super) fn javascript(
    root: &Path,
    profile: &Profile,
    platform: &str,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<Vec<Artifact>, String> {
    let path = profile
        .javascript_lock
        .as_deref()
        .ok_or("missing javascript-lock")?;
    input(root, path, inputs)?;
    let lock_path = root.join(path);
    let directory = lock_path
        .parent()
        .ok_or("JavaScript lockfile has no parent")?;
    let package_json = directory.join("package.json");
    if package_json.exists() {
        input(
            root,
            package_json
                .strip_prefix(root)
                .map_err(|_| "package manifest escapes project")?
                .to_str()
                .ok_or("package manifest path is not UTF-8")?,
            inputs,
        )?;
    }
    let name = lock_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("invalid JavaScript lockfile name")?;
    let lock = javascript_lock(directory, name)?;
    let entries = catalog(root, profile, inputs)?;
    let mut artifacts = Vec::new();
    let mut selected = std::collections::HashSet::new();
    selected.insert(lock.root_package_id());
    for dependency in lock.recursive_package_dependencies(lock.root_package_id()) {
        selected.insert(dependency.on);
    }
    for id in selected {
        let package = lock.package(id);
        if package.source().is_none() {
            capture_workspace(root, directory, &lock, id, inputs)?;
            continue;
        }
        artifacts.push(javascript_artifact(&lock, id, &entries, platform)?);
    }
    Ok(artifacts)
}

fn javascript_lock(directory: &Path, name: &str) -> Result<chaste::Chastefile, String> {
    match name {
        "bun.lock" => chaste::bun::parse(directory).map_err(|_| "invalid Bun lockfile".into()),
        "package-lock.json" | "npm-shrinkwrap.json" => {
            if name == "package-lock.json" && directory.join("npm-shrinkwrap.json").exists() {
                return Err("npm-shrinkwrap.json shadows the selected package-lock.json".into());
            }
            chaste::npm::parse(directory).map_err(|_| "invalid npm lockfile".into())
        }
        "pnpm-lock.yaml" => {
            chaste::pnpm::parse(directory).map_err(|_| "invalid pnpm lockfile".into())
        }
        "yarn.lock" => chaste::yarn::parse(directory).map_err(|_| "invalid Yarn lockfile".into()),
        _ => Err("unsupported JavaScript lockfile name".into()),
    }
}

fn capture_workspace(
    root: &Path,
    directory: &Path,
    lock: &chaste::Chastefile,
    id: chaste::PackageID,
    inputs: &mut BTreeMap<String, Sha256Digest>,
) -> Result<(), String> {
    for installation in lock.package_installations(id) {
        let relative = directory
            .join(installation.path().as_ref())
            .join("package.json");
        let relative = relative
            .strip_prefix(root)
            .map_err(|_| "workspace escapes project root")?;
        if relative.exists() || root.join(relative).exists() {
            input(
                root,
                relative.to_str().ok_or("workspace path is not UTF-8")?,
                inputs,
            )?;
        }
    }
    Ok(())
}

fn javascript_artifact(
    lock: &chaste::Chastefile,
    id: chaste::PackageID,
    entries: &[CatalogEntry],
    platform: &str,
) -> Result<Artifact, String> {
    let package = lock.package(id);
    let name = package
        .name()
        .ok_or("JavaScript package has no name")?
        .to_string();
    let version = package
        .version()
        .ok_or("JavaScript package has no version")?
        .to_string();
    let checksums = package
        .checksums()
        .ok_or("JavaScript dependencies require locked archive integrity")?;
    if !matches!(checksums, chaste::types::Checksums::Tarball(_)) {
        return Err(
            "Yarn repacked zip checksums require a reviewed tarball lock before planning".into(),
        );
    }
    let integrity = checksums.integrity().to_string();
    let matches = entries
        .iter()
        .filter(|entry| {
            entry.name == name
                && entry.version == version
                && entry.platform == platform
                && entry.integrity.as_deref() == Some(&integrity)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "expected one integrity-matching catalog artifact for {name}@{version}"
        ));
    }
    let mut artifact = catalog_artifact(matches[0], ArtifactKind::Javascript)?;
    artifact.destinations = lock
        .package_installations(id)
        .iter()
        .map(|installation| installation.path().as_ref().to_owned())
        .collect();
    artifact.destinations.sort();
    artifact.destinations.dedup();
    if artifact.destinations.is_empty() {
        return Err("JavaScript package has no locked installation path".into());
    }
    Ok(artifact)
}

pub fn verify_integrity(artifact: &Artifact, bytes: &[u8]) -> Result<(), String> {
    if Sha256Digest::from_bytes(bytes) != artifact.digest {
        return Err("artifact SHA-256 mismatch".into());
    }
    if let Some(integrity) = &artifact.integrity {
        let integrity: chaste::types::Integrity = integrity
            .parse()
            .map_err(|_| "invalid locked SRI integrity")?;
        integrity
            .check(bytes)
            .map_err(|_| "JavaScript lockfile integrity mismatch")?;
    }
    Ok(())
}

fn parse_hex(value: &str) -> Result<Sha256Digest, String> {
    Sha256Digest::parse(value).map_err(str::to_owned)
}
