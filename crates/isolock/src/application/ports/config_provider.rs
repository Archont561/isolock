//! Loading layered configuration is an adapter concern; use cases receive typed values.

use super::UseCaseResult;
use crate::domain::Settings;

/// The overrides an adapter extracted from argv, env, and defaults.
///
/// It is deliberately plain data: application code never parses arguments, and the CLI
/// adapter never knows how a layer is merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayeredOverrides {
    /// The project configuration path named by `--config`.
    pub project_config: std::path::PathBuf,
    /// The user configuration path named by `--user-config`.
    pub user_config: Option<std::path::PathBuf>,
    /// The `--profile` selection.
    pub profile: Option<String>,
    /// The `--platform` selection.
    pub platform: Option<String>,
    /// The `--cas-dir` override.
    pub cas_dir: Option<std::path::PathBuf>,
    /// The `--pixi-environment` override.
    pub pixi_environment: Option<String>,
    /// The `--pixi-version` override.
    pub pixi_version: Option<String>,
    /// The `--storage-ref` override.
    pub storage_ref: Option<String>,
    /// The `--port` override.
    pub proxy_port: Option<u16>,
    /// The `--mode` override, already parsed by the adapter's value parser.
    pub proxy_mode: Option<crate::domain::ProxyMode>,
    /// The `--max-concurrency` override.
    pub proxy_max_concurrency: Option<usize>,
}

impl LayeredOverrides {
    /// The default override set: no flags, only the documented project default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            project_config: std::path::PathBuf::from("isolock.toml"),
            user_config: None,
            profile: None,
            platform: None,
            cas_dir: None,
            pixi_environment: None,
            pixi_version: None,
            storage_ref: None,
            proxy_port: None,
            proxy_mode: None,
            proxy_max_concurrency: None,
        }
    }
}

impl Default for LayeredOverrides {
    fn default() -> Self {
        Self::new()
    }
}

/// The configuration provenance the `config explain` view needs, owned by the adapter.
pub trait ConfigProvenance {
    /// Renders `path = value` plus the layer that supplied it.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the path is unknown or unsourced.
    fn explain(&self, path: &str) -> UseCaseResult<String>;

    /// Renders the effective settings as the exact document the CLI prints.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when the settings cannot be serialized.
    fn render(&self) -> UseCaseResult<String>;
}

/// The validated settings plus the provenance a source-aware view needs.
#[derive(Debug)]
pub struct LoadedConfiguration<P> {
    pub settings: Settings,
    pub provenance: P,
}

/// Loads every configuration layer for one project directory.
pub trait ConfigurationProvider {
    /// Provenance handle returned alongside the settings.
    type Provenance: ConfigProvenance;

    /// Loads layers in reviewed precedence order.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when a layer is unreadable, an explicit
    /// project file is missing, or the merged values violate the reviewed schema.
    fn load(&self, root: &std::path::Path) -> UseCaseResult<LoadedConfiguration<Self::Provenance>>;
}

/// The projection of a configuration into the paths an execution session needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSettings {
    /// Absolute plan path.
    pub plan: std::path::PathBuf,
    /// Whether configuration, not compiled defaults, supplied the plan path.
    pub require_plan: bool,
    /// Absolute CAS directory.
    pub cas_dir: std::path::PathBuf,
    /// Absolute Cargo home the session prepares.
    pub cargo_home: std::path::PathBuf,
    /// Absolute Pixi configuration file, when the environment names one.
    pub pixi_config: Option<std::path::PathBuf>,
    /// Temporary directory Pixi files are staged in.
    pub temporary_dir: std::path::PathBuf,
    /// The inherited `NO_PROXY` value the session must preserve.
    pub inherited_no_proxy: String,
    /// The resolved proxy configuration.
    pub proxy: isolock_proxy::ProxyConfig,
    /// A selected profile the session must run inside, when one was chosen.
    ///
    /// `None` keeps the legacy airlock-plan behaviour exactly: the plan file is
    /// read, the caller's `PATH` is inherited, and no prefix is required.
    pub profile: Option<ProfileSession>,
}

/// What a session needs to run a child inside a materialized profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSession {
    /// The profile name, used in messages.
    pub profile: String,
    /// The platform name, used in messages and in the prefix path.
    pub platform: String,
    /// The digest-addressed materialized prefix holding `bin/`.
    pub prefix: std::path::PathBuf,
    /// The profile's own Cargo home, so `cargo add` writes inside the profile.
    pub cargo_home: std::path::PathBuf,
    /// Registry artifacts the proxy may serve, already projected.
    pub plan: isolock_plan::AirlockPlan,
}

/// The explicit profile selection a command-line invocation asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceSelection {
    /// The `--profile` selection.
    pub profile: Option<String>,
    /// The `--platform` selection.
    pub platform: Option<String>,
}

impl WorkspaceSelection {
    /// The reviewed profile name, or the message every profile use case reports.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when `--profile` was not supplied.
    pub fn require_profile(&self) -> UseCaseResult<&str> {
        self.profile
            .as_deref()
            .ok_or_else(|| crate::domain::DomainError::new("explicit --profile is required"))
    }

    /// The reviewed platform name, or the message every profile use case reports.
    ///
    /// # Errors
    /// Returns a [`crate::domain::DomainError`] when `--platform` was not supplied.
    pub fn require_platform(&self) -> UseCaseResult<&str> {
        self.platform
            .as_deref()
            .ok_or_else(|| crate::domain::DomainError::new("explicit --platform is required"))
    }
}
