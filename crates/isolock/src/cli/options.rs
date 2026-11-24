//! The single clap declaration of every global override, flattened by each controller.
//!
//! It is the only place argv becomes application data: the handler converts it into
//! [`LayeredOverrides`] before any use case sees it.

use crate::application::ports::LayeredOverrides;
use crate::domain::ProxyMode;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, clap::Args)]
pub struct Overrides {
    /// Project configuration (relative to the working directory).
    #[arg(long, global = true, default_value = "isolock.toml")]
    pub config: PathBuf,
    #[arg(long, global = true)]
    pub user_config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub platform: Option<String>,
    #[arg(long, global = true)]
    pub cas_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub pixi_environment: Option<String>,
    #[arg(long, global = true)]
    pub pixi_version: Option<String>,
    #[arg(long, global = true)]
    pub storage_ref: Option<String>,
    #[command(flatten)]
    pub proxy: ProxyOverrides,
}

#[derive(Debug, Clone, Default, clap::Args)]
pub struct ProxyOverrides {
    /// Local loopback listener port (zero chooses a free port).
    #[arg(long = "port", global = true)]
    pub port: Option<u16>,
    #[arg(long = "mode", global = true, value_enum, ignore_case = true)]
    pub mode: Option<ModeArgument>,
    /// Maximum concurrent proxy requests (1–256); extra requests receive 503.
    #[arg(long = "max-concurrency", global = true)]
    pub max_concurrency: Option<usize>,
}

/// The clap-facing spelling of the reviewed proxy mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ModeArgument {
    Connected,
    Ci,
    LocalAirlock,
}

impl From<ModeArgument> for ProxyMode {
    fn from(mode: ModeArgument) -> Self {
        match mode {
            ModeArgument::Connected => Self::Connected,
            ModeArgument::Ci => Self::Ci,
            ModeArgument::LocalAirlock => Self::LocalAirlock,
        }
    }
}

impl From<&Overrides> for LayeredOverrides {
    fn from(overrides: &Overrides) -> Self {
        Self {
            project_config: overrides.config.clone(),
            user_config: overrides.user_config.clone(),
            profile: overrides.profile.clone(),
            platform: overrides.platform.clone(),
            cas_dir: overrides.cas_dir.clone(),
            pixi_environment: overrides.pixi_environment.clone(),
            pixi_version: overrides.pixi_version.clone(),
            storage_ref: overrides.storage_ref.clone(),
            proxy_port: overrides.proxy.port,
            proxy_mode: overrides.proxy.mode.map(ProxyMode::from),
            proxy_max_concurrency: overrides.proxy.max_concurrency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Wrapper {
        #[command(flatten)]
        overrides: Overrides,
    }

    fn parse(arguments: &[&str]) -> LayeredOverrides {
        let parsed =
            Wrapper::parse_from(std::iter::once("isolock").chain(arguments.iter().copied()));
        LayeredOverrides::from(&parsed.overrides)
    }

    #[test]
    fn defaults_match_the_reviewed_document() {
        let overrides = parse(&[]);
        assert_eq!(overrides.project_config, PathBuf::from("isolock.toml"));
        assert_eq!(overrides.proxy_mode, None);
        assert_eq!(overrides.proxy_port, None);
    }

    #[test]
    fn every_override_reaches_application_data_unchanged() {
        let overrides = parse(&[
            "--config",
            "custom.toml",
            "--profile",
            "dev",
            "--platform",
            "linux-64",
            "--cas-dir",
            "cas",
            "--mode",
            "ci",
            "--port",
            "8080",
            "--max-concurrency",
            "4",
        ]);
        assert_eq!(overrides.project_config, PathBuf::from("custom.toml"));
        assert_eq!(overrides.profile.as_deref(), Some("dev"));
        assert_eq!(overrides.platform.as_deref(), Some("linux-64"));
        assert_eq!(overrides.cas_dir, Some(PathBuf::from("cas")));
        assert_eq!(overrides.proxy_mode, Some(ProxyMode::Ci));
        assert_eq!(overrides.proxy_port, Some(8080));
        assert_eq!(overrides.proxy_max_concurrency, Some(4));
    }

    #[test]
    fn the_mode_flag_keeps_its_case_insensitive_and_kebab_spellings() {
        assert_eq!(
            parse(&["--mode", "LOCAL-AIRLOCK"]).proxy_mode,
            Some(ProxyMode::LocalAirlock)
        );
        assert_eq!(
            parse(&["--mode", "connected"]).proxy_mode,
            Some(ProxyMode::Connected)
        );
    }
}
