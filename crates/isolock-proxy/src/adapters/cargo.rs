use super::{decoded_path, is_registry, RegistryAdapter, RegistryContext, RegistryRouteAction};
use crate::MIME_APPLICATION_JSON;
use isolock_core::{Registry, RegistryArchive};
use isolock_plan::{PlannedArtifact, PlannedArtifactMetadata};
use serde_json::{json, Map, Value};
use tame_index::{IndexKrate, IndexVersion, KrateName, Version};

/// Cargo RFC 2789 sparse registry adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoAdapter;

impl RegistryAdapter for CargoAdapter {
    fn ecosystem(&self) -> Registry {
        Registry::Cargo
    }

    fn handle_route(
        &self,
        _method: &str,
        path: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        let path = match decoded_path(path) {
            Ok(path) => path,
            Err(error) => return RegistryRouteAction::BadRequest(error),
        };
        if path == "/cargo/config.json" {
            let body = json!({
                "dl": format!(
                    "{}/cargo/api/v1/crates/{{crate}}/{{version}}/download",
                    context.base_url
                ),
            });
            return match serde_json::to_vec(&body) {
                Ok(body) => RegistryRouteAction::ServeIndex {
                    content_type: MIME_APPLICATION_JSON,
                    body,
                },
                Err(error) => RegistryRouteAction::InvalidMetadata(error.to_string()),
            };
        }
        if let Some(download_path) = path.strip_prefix("/cargo/api/v1/crates/") {
            return Self::download(download_path, context);
        }
        if let Some(index_path) = path.strip_prefix("/cargo/") {
            return Self::index(index_path, context);
        }
        RegistryRouteAction::NotFound
    }
}

impl CargoAdapter {
    fn index(requested_path: &str, context: &RegistryContext<'_>) -> RegistryRouteAction {
        if requested_path.is_empty() || requested_path.contains("..") {
            return RegistryRouteAction::BadRequest("invalid Cargo sparse index path".into());
        }
        let mut versions = Vec::new();
        let mut found_crate = false;

        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Cargo) {
                continue;
            }
            let expected_path = match sparse_index_path(&artifact.name) {
                Ok(path) => path,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            if !expected_path.eq_ignore_ascii_case(requested_path) {
                continue;
            }
            found_crate = true;
            if !context.cas.contains(&artifact.digest) {
                return RegistryRouteAction::CacheMiss {
                    filename: metadata
                        .filename
                        .clone()
                        .unwrap_or_else(|| cargo_filename(artifact)),
                };
            }
            let version = match cargo_index_version(artifact, metadata) {
                Ok(version) => version,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            versions.push(version);
        }

        if !found_crate {
            return RegistryRouteAction::CacheMiss {
                filename: requested_path.to_owned(),
            };
        }
        versions.sort_by(|left, right| {
            Version::parse(&left.version)
                .ok()
                .cmp(&Version::parse(&right.version).ok())
        });
        let krate = IndexKrate { versions };
        let mut body = Vec::new();
        if let Err(error) = krate.write_json_lines(&mut body) {
            return RegistryRouteAction::InvalidMetadata(format!(
                "cannot serialize Cargo sparse index entry: {error}"
            ));
        }
        RegistryRouteAction::ServeIndex {
            content_type: MIME_APPLICATION_JSON,
            body,
        }
    }

    fn download(download_path: &str, context: &RegistryContext<'_>) -> RegistryRouteAction {
        let parts = download_path.split('/').collect::<Vec<_>>();
        if parts.len() != 3 || parts[2] != "download" || parts[0].is_empty() || parts[1].is_empty()
        {
            return RegistryRouteAction::NotFound;
        }
        let (name, version) = (parts[0], parts[1]);
        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Cargo)
                || !artifact.name.eq_ignore_ascii_case(name)
                || artifact.version != version
            {
                continue;
            }
            let filename = metadata
                .filename
                .clone()
                .unwrap_or_else(|| cargo_filename(artifact));
            if RegistryArchive::from_filename(&filename) != Some(RegistryArchive::Cargo)
                || filename.contains('/')
                || filename.contains('\\')
            {
                return RegistryRouteAction::InvalidMetadata(format!(
                    "invalid Cargo archive filename {filename:?}"
                ));
            }
            return if context.cas.contains(&artifact.digest) {
                RegistryRouteAction::DownloadArtifact {
                    digest: artifact.digest.clone(),
                    filename,
                }
            } else {
                RegistryRouteAction::CacheMiss { filename }
            };
        }
        RegistryRouteAction::CacheMiss {
            filename: format!("{name}-{version}.crate"),
        }
    }
}

fn sparse_index_path(name: &str) -> Result<String, String> {
    let krate = KrateName::try_from(name)
        .map_err(|error| format!("invalid Cargo crate name {name:?}: {error}"))?;
    Ok(krate.relative_path(Some('/')))
}

fn cargo_filename(artifact: &PlannedArtifact) -> String {
    format!("{}-{}.crate", artifact.name, artifact.version)
}

fn cargo_index_version(
    artifact: &PlannedArtifact,
    metadata: &PlannedArtifactMetadata,
) -> Result<IndexVersion, String> {
    if let Some(value) = metadata
        .registry_metadata
        .as_ref()
        .and_then(|value| value.get("cargo_index"))
    {
        return serde_json::from_value(value.clone())
            .map_err(|error| format!("invalid saved Cargo index metadata: {error}"));
    }
    let source = metadata
        .registry_metadata
        .as_ref()
        .and_then(|value| value.get("Cargo.toml"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "package": {
                    "name": artifact.name.as_str(),
                    "version": artifact.version.as_str(),
                }
            })
        });
    let manifest = if let Some(source) = source.as_str() {
        toml::from_str::<toml::Value>(source)
            .map_err(|error| format!("invalid Cargo.toml: {error}"))?
    } else if source.is_object() {
        toml::Value::try_from(source)
            .map_err(|error| format!("invalid saved Cargo.toml metadata: {error}"))?
    } else {
        return Err("saved Cargo.toml metadata must be a TOML string or object".into());
    };
    let package = manifest
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| "Cargo.toml is missing [package]".to_owned())?;
    let name = string_field(package, "name")
        .ok_or_else(|| "Cargo.toml [package] is missing name".to_owned())?;
    let version = string_field(package, "version")
        .ok_or_else(|| "Cargo.toml [package] is missing version".to_owned())?;
    if !name.eq_ignore_ascii_case(&artifact.name) || version != artifact.version {
        return Err(format!(
            "Cargo.toml package {name}@{version} does not match planned artifact {}@{}",
            artifact.name, artifact.version
        ));
    }

    let dependencies = cargo_dependencies(&manifest)?;
    let features = cargo_features(&manifest)?;
    let mut entry = json!({
        "name": name,
        "vers": version,
        "deps": dependencies,
        "cksum": artifact.digest.as_str(),
        "features": features.legacy,
        "yanked": false,
    });
    let object = entry
        .as_object_mut()
        .ok_or_else(|| "Cargo index metadata must be an object".to_owned())?;
    if !features.namespaced.is_empty() {
        object.insert("features2".into(), Value::Object(features.namespaced));
        object.insert("v".into(), json!(features.version));
    }
    if let Some(links) = string_field(package, "links") {
        object.insert("links".into(), Value::String(links));
    }
    if let Some(rust_version) = string_field(package, "rust-version") {
        object.insert("rust_version".into(), Value::String(rust_version));
    }
    serde_json::from_value(entry)
        .map_err(|error| format!("invalid generated Cargo index metadata: {error}"))
}

fn string_field(table: &toml::value::Table, key: &str) -> Option<String> {
    table.get(key)?.as_str().map(str::to_owned)
}

fn cargo_dependencies(manifest: &toml::Value) -> Result<Vec<Value>, String> {
    let mut dependencies = Vec::new();
    collect_dependency_table(
        manifest.get("dependencies"),
        "normal",
        None,
        &mut dependencies,
    )?;
    collect_dependency_table(
        manifest.get("dev-dependencies"),
        "dev",
        None,
        &mut dependencies,
    )?;
    collect_dependency_table(
        manifest.get("build-dependencies"),
        "build",
        None,
        &mut dependencies,
    )?;
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, sections) in targets {
            collect_dependency_table(
                sections.get("dependencies"),
                "normal",
                Some(target),
                &mut dependencies,
            )?;
            collect_dependency_table(
                sections.get("dev-dependencies"),
                "dev",
                Some(target),
                &mut dependencies,
            )?;
            collect_dependency_table(
                sections.get("build-dependencies"),
                "build",
                Some(target),
                &mut dependencies,
            )?;
        }
    }
    Ok(dependencies)
}

fn collect_dependency_table(
    section: Option<&toml::Value>,
    kind: &str,
    target: Option<&str>,
    output: &mut Vec<Value>,
) -> Result<(), String> {
    let Some(table) = section.and_then(toml::Value::as_table) else {
        return Ok(());
    };
    for (alias, specification) in table {
        let (package, requirement, features, optional, default_features) =
            dependency_fields(specification);
        let mut dependency = json!({
            "name": alias,
            "req": requirement,
            "features": features,
            "optional": optional,
            "default_features": default_features,
            "kind": kind,
        });
        let object = dependency
            .as_object_mut()
            .ok_or_else(|| "Cargo dependency entry must be an object".to_owned())?;
        if let Some(package) = package.filter(|package| package != alias) {
            object.insert("package".into(), Value::String(package));
        }
        if let Some(target) = target {
            object.insert("target".into(), Value::String(target.to_owned()));
        }
        output.push(dependency);
    }
    Ok(())
}

fn dependency_fields(
    specification: &toml::Value,
) -> (Option<String>, String, Vec<String>, bool, bool) {
    let Some(table) = specification.as_table() else {
        return (
            None,
            specification.as_str().unwrap_or("*").to_owned(),
            Vec::new(),
            false,
            true,
        );
    };
    let package = table
        .get("package")
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    let requirement = table
        .get("version")
        .and_then(toml::Value::as_str)
        .unwrap_or("*")
        .to_owned();
    let features = table
        .get("features")
        .and_then(toml::Value::as_array)
        .map(|features| {
            features
                .iter()
                .filter_map(toml::Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let optional = table
        .get("optional")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    let default_features = table
        .get("default-features")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    (package, requirement, features, optional, default_features)
}

struct CargoFeatures {
    legacy: Map<String, Value>,
    namespaced: Map<String, Value>,
    version: u32,
}

fn cargo_features(manifest: &toml::Value) -> Result<CargoFeatures, String> {
    let mut legacy = Map::new();
    let mut namespaced = Map::new();
    let Some(features) = manifest.get("features").and_then(toml::Value::as_table) else {
        return Ok(CargoFeatures {
            legacy,
            namespaced,
            version: 1,
        });
    };
    for (name, values) in features {
        let values = values
            .as_array()
            .ok_or_else(|| format!("Cargo feature {name:?} must be an array"))?
            .iter()
            .filter_map(toml::Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let needs_v2 = name.contains('/')
            || values
                .iter()
                .any(|feature| feature.starts_with("dep:") || feature.contains("?/"));
        if needs_v2 {
            namespaced.insert(name.clone(), json!(values));
        } else {
            legacy.insert(name.clone(), json!(values));
        }
    }
    let version = if namespaced.is_empty() { 1 } else { 2 };
    Ok(CargoFeatures {
        legacy,
        namespaced,
        version,
    })
}
