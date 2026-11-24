//! Typed, composable child environment contributions. No process-environment reads.

use crate::application::ports::ProcessRequest;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

#[derive(Default)]
pub struct ChildEnvironment(BTreeMap<OsString, Option<OsString>>);

impl ChildEnvironment {
    pub fn set(&mut self, key: &str, value: impl Into<OsString>) {
        self.0.insert(key.into(), Some(value.into()));
    }
    pub fn remove(&mut self, key: &str) {
        self.0.insert(key.into(), None);
    }
    pub fn include(&mut self, contribution: &impl EnvironmentContribution) {
        contribution.contribute(self);
    }
    /// Projects the collected variables onto a process request.
    pub fn project(&self, request: ProcessRequest) -> ProcessRequest {
        let mut request = request;
        for (key, value) in &self.0 {
            let key = key.to_string_lossy().into_owned();
            match value {
                Some(value) => {
                    request = request.with_variable(key, value.to_string_lossy().into_owned());
                }
                None => request = request.without_variable(key),
            }
        }
        request
    }
}

pub trait EnvironmentContribution {
    fn contribute(&self, environment: &mut ChildEnvironment);
}

pub struct CargoEnvironment<'a> {
    pub home: &'a Path,
    pub origin: &'a str,
}
impl EnvironmentContribution for CargoEnvironment<'_> {
    fn contribute(&self, environment: &mut ChildEnvironment) {
        environment.set("CARGO_HOME", self.home);
        environment.set(
            "CARGO_REGISTRIES_CRATES_IO_INDEX",
            format!("sparse+{}/cargo/", self.origin),
        );
        environment.set("CARGO_REGISTRIES_CRATES_IO_PROTOCOL", "sparse");
    }
}

pub struct PixiEnvironment<'a> {
    pub config: &'a Path,
    pub origin: &'a str,
}
impl EnvironmentContribution for PixiEnvironment<'_> {
    fn contribute(&self, environment: &mut ChildEnvironment) {
        environment.set("PIXI_CONFIG_FILE", self.config);
        environment.set("PIXI_OFFLINE", "false");
        environment.remove("PIXI_NO_CONFIG");
        for key in ["PIXI_CONDA_CHANNELS", "CONDA_CHANNELS"] {
            environment.set(key, format!("{}/conda/channel", self.origin));
        }
    }
}

pub struct PypiEnvironment<'a>(pub &'a str);
impl EnvironmentContribution for PypiEnvironment<'_> {
    fn contribute(&self, environment: &mut ChildEnvironment) {
        for key in ["PIP_INDEX_URL", "UV_INDEX_URL", "UV_DEFAULT_INDEX"] {
            environment.set(key, format!("{}/pypi/simple", self.0));
        }
        for key in ["PIP_EXTRA_INDEX_URL", "UV_EXTRA_INDEX_URL"] {
            environment.set(key, "");
        }
    }
}

pub struct NpmEnvironment<'a>(pub &'a str);
impl EnvironmentContribution for NpmEnvironment<'_> {
    fn contribute(&self, environment: &mut ChildEnvironment) {
        environment.set("NPM_CONFIG_REGISTRY", format!("{}/npm/", self.0));
    }
}

pub struct ProxyEnvironment<'a> {
    pub origin: &'a str,
    pub inherited_no_proxy: &'a str,
}
impl EnvironmentContribution for ProxyEnvironment<'_> {
    fn contribute(&self, environment: &mut ChildEnvironment) {
        for key in [
            "ISOLOCK_PROXY_URL",
            "HTTP_PROXY",
            "http_proxy",
            "HTTPS_PROXY",
            "https_proxy",
            "ALL_PROXY",
            "all_proxy",
        ] {
            environment.set(key, self.origin);
        }
        let no_proxy = ["127.0.0.1", "localhost", "::1", self.inherited_no_proxy]
            .into_iter()
            .filter(|entry| !entry.is_empty())
            .collect::<Vec<_>>()
            .join(",");
        environment.set("NO_PROXY", &no_proxy);
        environment.set("no_proxy", no_proxy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contributions_compose_without_spawning_or_mutating_process_state() {
        let mut environment = ChildEnvironment::default();
        let origin = "http://127.0.0.1:123";
        environment.include(&CargoEnvironment {
            home: Path::new("/cargo home"),
            origin,
        });
        environment.include(&PixiEnvironment {
            config: Path::new("/pixi.toml"),
            origin,
        });
        environment.include(&PypiEnvironment(origin));
        environment.include(&NpmEnvironment(origin));
        environment.include(&ProxyEnvironment {
            origin,
            inherited_no_proxy: "example.test",
        });
        // Projection onto a request never spawns a process and never reads the
        // ambient environment, so this stays a pure composition assertion.
        let request = environment.project(ProcessRequest::new("not-spawned"));
        let removed = request.removed.into_iter().map(|key| (key, None));
        let values = request
            .environment
            .into_iter()
            .map(|(key, value)| (key, Some(value)))
            .chain(removed)
            .collect::<BTreeMap<_, _>>();
        assert_eq!(values["CARGO_HOME"].as_deref(), Some("/cargo home"));
        assert_eq!(values["PIXI_NO_CONFIG"], None);
        assert_eq!(
            values["NO_PROXY"].as_deref(),
            Some("127.0.0.1,localhost,::1,example.test")
        );
        assert_eq!(values["PIP_EXTRA_INDEX_URL"].as_deref(), Some(""));
        assert_eq!(
            values["NPM_CONFIG_REGISTRY"].as_deref(),
            Some("http://127.0.0.1:123/npm/")
        );
    }
}
