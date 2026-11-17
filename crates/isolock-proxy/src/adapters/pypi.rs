use super::{
    decoded_path, encode_segment, is_registry, normalize_pypi_name, valid_filename,
    RegistryAdapter, RegistryContext, RegistryRouteAction,
};
use crate::MIME_PYPI_SIMPLE_JSON;
use isolock_core::Registry;
use isolock_plan::{PlannedArtifact, PlannedArtifactMetadata};
use serde_json::{json, Value};

/// PEP 691 JSON Simple API adapter for PyPI wheels.
#[derive(Debug, Default, Clone, Copy)]
pub struct PypiAdapter;

impl RegistryAdapter for PypiAdapter {
    fn ecosystem(&self) -> Registry {
        Registry::Pypi
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
        if let Some(project) = simple_project(&path) {
            return self.simple_index(project, context);
        }
        if let Some(filename) = path.strip_prefix("/pypi/packages/") {
            if !valid_filename(filename) {
                return RegistryRouteAction::BadRequest("invalid wheel filename".into());
            }
            return self.download(filename, context);
        }
        RegistryRouteAction::NotFound
    }
}

impl PypiAdapter {
    fn simple_index(&self, project: &str, context: &RegistryContext<'_>) -> RegistryRouteAction {
        if project.is_empty() || project.contains('/') {
            return RegistryRouteAction::BadRequest("invalid PyPI project name".into());
        }
        let normalized = normalize_pypi_name(project);
        let mut files = Vec::new();
        let mut found_project = false;

        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Pypi)
                || normalize_pypi_name(&artifact.name) != normalized
            {
                continue;
            }
            found_project = true;
            let Some(bytes) = context.cas.get(&artifact.digest) else {
                return RegistryRouteAction::CacheMiss {
                    filename: metadata.filename.clone().unwrap_or_default(),
                };
            };
            let wheel = match wheel_metadata(artifact, metadata) {
                Ok(wheel) => wheel,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            if normalize_pypi_name(&wheel.name) != normalized || wheel.version != artifact.version {
                return RegistryRouteAction::InvalidMetadata(format!(
                    "wheel metadata {}=={} does not match planned artifact {}=={}",
                    wheel.name, wheel.version, artifact.name, artifact.version
                ));
            }
            let filename = match choose_filename(metadata.filename.as_deref(), &wheel.filename) {
                Ok(filename) => filename,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            let mut file = json!({
                "filename": filename,
                "url": format!("{}/pypi/packages/{}", context.base_url, encode_segment(&filename)),
                "hashes": {"sha256": artifact.digest.as_str()},
                "size": bytes.len(),
            });
            if let Some(requires_python) = metadata
                .requires_python
                .as_deref()
                .or(wheel.requires_python.as_deref())
            {
                file["requires-python"] = Value::String(requires_python.to_owned());
            }
            files.push(file);
        }

        if !found_project {
            return RegistryRouteAction::CacheMiss {
                filename: project.to_owned(),
            };
        }
        files.sort_by(|left, right| left["filename"].as_str().cmp(&right["filename"].as_str()));
        let body = json!({
            "meta": {"api-version": "1.1"},
            "name": project,
            "files": files,
        });
        match serde_json::to_vec(&body) {
            Ok(body) => RegistryRouteAction::ServeIndex {
                content_type: MIME_PYPI_SIMPLE_JSON,
                body,
            },
            Err(error) => RegistryRouteAction::InvalidMetadata(error.to_string()),
        }
    }

    fn download(&self, filename: &str, context: &RegistryContext<'_>) -> RegistryRouteAction {
        for (artifact, metadata) in context.plan.artifacts_with_metadata() {
            if !is_registry(artifact, Registry::Pypi) {
                continue;
            }
            let wheel = match wheel_metadata(artifact, metadata) {
                Ok(wheel) => wheel,
                Err(error) => return RegistryRouteAction::InvalidMetadata(error),
            };
            if wheel.filename != filename {
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

fn simple_project(path: &str) -> Option<&str> {
    let project = path.strip_prefix("/pypi/simple/")?;
    let project = project.strip_suffix('/').unwrap_or(project);
    (!project.is_empty() && !project.contains('/')).then_some(project)
}

struct WheelMetadata {
    name: String,
    version: String,
    requires_python: Option<String>,
    filename: String,
}

fn wheel_metadata(
    artifact: &PlannedArtifact,
    metadata: &PlannedArtifactMetadata,
) -> Result<WheelMetadata, String> {
    let details = metadata.registry_metadata.as_ref();
    let name = details
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(&artifact.name)
        .to_owned();
    let version = details
        .and_then(|value| value.get("version"))
        .and_then(Value::as_str)
        .unwrap_or(&artifact.version)
        .to_owned();
    let requires_python = metadata.requires_python.clone().or_else(|| {
        details
            .and_then(|value| value.get("requires-python"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let filename = metadata
        .filename
        .clone()
        .or_else(|| {
            details
                .and_then(|value| value.get("filename"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            format!(
                "{}-{}-py3-none-any.whl",
                name_component(&name),
                version.replace('-', "_")
            )
        });
    if !valid_filename(&filename) || !filename.ends_with(".whl") {
        return Err(format!("invalid wheel filename {filename:?}"));
    }
    Ok(WheelMetadata {
        name,
        version,
        requires_python,
        filename,
    })
}

fn name_component(name: &str) -> String {
    let mut output = String::with_capacity(name.len());
    let mut separator = false;
    for byte in name.bytes() {
        if matches!(byte, b'-' | b'_' | b'.') {
            if !separator {
                output.push('_');
            }
            separator = true;
        } else {
            output.push(char::from(byte));
            separator = false;
        }
    }
    output
}

fn choose_filename(explicit: Option<&str>, inferred: &str) -> Result<String, String> {
    let filename = explicit.unwrap_or(inferred);
    if !valid_filename(filename) || !filename.ends_with(".whl") {
        return Err(format!("invalid wheel filename {filename:?}"));
    }
    Ok(filename.to_owned())
}
