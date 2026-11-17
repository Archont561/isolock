use super::{
    decoded_path, encode_segment, is_registry, valid_filename, RegistryAdapter, RegistryContext,
    RegistryRouteAction,
};
use crate::MIME_APPLICATION_JSON;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use isolock_core::Registry;
use isolock_plan::{PlannedArtifact, PlannedArtifactMetadata};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha512};
use tame_index::Version;

/// NPM registry adapter for package packuments and `.tgz` downloads.
#[derive(Debug, Default, Clone, Copy)]
pub struct NpmAdapter;

impl RegistryAdapter for NpmAdapter {
    fn ecosystem(&self) -> Registry {
        Registry::Npm
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
        let Some(registry_path) = path.strip_prefix("/npm/") else {
            return RegistryRouteAction::NotFound;
        };
        if let Some((package, filename)) = registry_path.split_once("/-/") {
            if !valid_npm_name(package) || !valid_filename(filename) {
                return RegistryRouteAction::BadRequest("invalid NPM tarball path".into());
            }
            return self.download(package, filename, context);
        }
        let package = registry_path.trim_end_matches('/');
        if !valid_npm_name(package) {
            return RegistryRouteAction::BadRequest("invalid NPM package name".into());
        }
        self.packument(package, context)
    }
}

impl NpmAdapter {
    fn packument(&self, package: &str, context: &RegistryContext<'_>) -> RegistryRouteAction {
        let mut versions = Map::new();
        let mut found_package = false;
        let mut available_versions = Vec::new();

        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Npm) || artifact.name != package {
                continue;
            }
            found_package = true;
            let Some(bytes) = context.cas.get(&artifact.digest) else {
                return RegistryRouteAction::CacheMiss {
                    filename: metadata.filename.clone().unwrap_or_default(),
                };
            };
            let mut package_json = match read_package_json(artifact, metadata) {
                Ok(package_json) => package_json,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            let Some(object) = package_json.as_object_mut() else {
                return RegistryRouteAction::InvalidMetadata(
                    "NPM package.json must contain a JSON object".into(),
                );
            };
            let archive_name = object
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(package);
            let archive_version = object
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or(&artifact.version);
            if archive_name != package || archive_version != artifact.version {
                return RegistryRouteAction::InvalidMetadata(format!(
                    "package.json {}@{} does not match planned artifact {}@{}",
                    archive_name, archive_version, artifact.name, artifact.version
                ));
            }
            let filename =
                match npm_filename(package, &artifact.version, metadata.filename.as_deref()) {
                    Ok(filename) => filename,
                    Err(error) => return RegistryRouteAction::InvalidMetadata(error),
                };
            let tarball_url = format!(
                "{}/npm/{}/-/{}",
                context.base_url,
                encode_npm_package(package),
                encode_segment(&filename)
            );
            let integrity = format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes)));
            object.insert("name".into(), Value::String(package.to_owned()));
            object.insert("version".into(), Value::String(artifact.version.clone()));
            object.insert(
                "dist".into(),
                json!({
                    "tarball": tarball_url,
                    "integrity": integrity,
                }),
            );
            available_versions.push(artifact.version.clone());
            versions.insert(artifact.version.clone(), package_json);
        }

        if !found_package {
            return RegistryRouteAction::CacheMiss {
                filename: package.to_owned(),
            };
        }
        let latest = latest_version(&available_versions);
        let body = json!({
            "_id": package,
            "name": package,
            "dist-tags": {"latest": latest},
            "versions": versions,
        });
        match serde_json::to_vec(&body) {
            Ok(body) => RegistryRouteAction::ServeIndex {
                content_type: MIME_APPLICATION_JSON,
                body,
            },
            Err(error) => RegistryRouteAction::InvalidMetadata(error.to_string()),
        }
    }

    fn download(
        &self,
        package: &str,
        filename: &str,
        context: &RegistryContext<'_>,
    ) -> RegistryRouteAction {
        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Npm) || artifact.name != package {
                continue;
            }
            let expected =
                match npm_filename(package, &artifact.version, metadata.filename.as_deref()) {
                    Ok(filename) => filename,
                    Err(error) => return RegistryRouteAction::InvalidMetadata(error),
                };
            if expected != filename {
                continue;
            }
            return if context.cas.contains(&artifact.digest) {
                RegistryRouteAction::DownloadArtifact {
                    digest: artifact.digest.clone(),
                    filename: filename.to_owned(),
                }
            } else {
                RegistryRouteAction::CacheMiss {
                    filename: filename.to_owned(),
                }
            };
        }
        RegistryRouteAction::CacheMiss {
            filename: filename.to_owned(),
        }
    }
}

fn valid_npm_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.ends_with('/') || name.contains('\\') {
        return false;
    }
    let parts = name.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        [plain] => !plain.is_empty() && !plain.starts_with('@'),
        [scope, package] => {
            scope.starts_with('@')
                && scope.len() > 1
                && !package.is_empty()
                && !package.starts_with('@')
        }
        _ => false,
    }
}

fn encode_npm_package(package: &str) -> String {
    package
        .split('/')
        .enumerate()
        .map(|(index, part)| {
            let encoded = encode_segment(part);
            if index == 0 && part.starts_with('@') {
                format!("@{}", encode_segment(&part[1..]))
            } else {
                encoded
            }
        })
        .collect::<Vec<_>>()
        .join("%2F")
}

fn npm_filename(package: &str, version: &str, explicit: Option<&str>) -> Result<String, String> {
    let inferred = format!(
        "{}-{version}.tgz",
        package.rsplit('/').next().unwrap_or(package)
    );
    let filename = explicit.unwrap_or(&inferred);
    if !valid_filename(filename) || !filename.ends_with(".tgz") {
        return Err(format!("invalid NPM tarball filename {filename:?}"));
    }
    Ok(filename.to_owned())
}

fn latest_version(versions: &[String]) -> String {
    versions
        .iter()
        .filter_map(|version| Version::parse(version).ok().map(|parsed| (version, parsed)))
        .max_by(|left, right| left.1.cmp(&right.1))
        .map_or_else(
            || versions.last().cloned().unwrap_or_default(),
            |(version, _)| version.clone(),
        )
}

fn read_package_json(
    artifact: &PlannedArtifact,
    metadata: &PlannedArtifactMetadata,
) -> Result<Value, String> {
    let Some(value) = metadata.registry_metadata.as_ref() else {
        return Ok(json!({"name": artifact.name, "version": artifact.version}));
    };
    let package_json = value
        .get("package_json")
        .or_else(|| value.get("package.json"))
        .unwrap_or(value);
    if package_json.is_object() {
        Ok(package_json.clone())
    } else {
        Err("saved NPM package.json metadata must be a JSON object".into())
    }
}
