//! Typed profile commands feed operations directly to the existing execution boundary.

use super::{plan, Output};
use crate::config::Overrides;
use crate::profile_cli::{self, Configuration, Operation};
use clifacet::{Failure, Handler};
use std::path::{Path, PathBuf};

// No field normalization is needed here: clap supplies typed values, and the existing
// profile boundary validates configuration and locked inputs before any transfer.
#[clifacet::command(input = Config, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Config {
    #[command(subcommand)]
    command: Configuration,
}

impl Handler<Overrides> for Config {
    type Input = Config;
    type Output = ();

    fn handle(&self, input: Config, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(
            flags,
            Operation::Config {
                command: input.command,
            },
        )
        .map_err(Failure::execution)
    }
}

#[clifacet::command(input = Plan, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Plan {
    /// Check locked inputs and fail on drift.
    #[arg(long)]
    ci: bool,
    #[arg(long)]
    locked: bool,
    #[arg(long)]
    check: bool,
}

impl Handler<Overrides> for Plan {
    type Input = Plan;
    type Output = Output;

    fn handle(&self, input: Plan, flags: &Overrides) -> Result<Output, Failure> {
        let profile = flags.profile.is_some()
            || flags.platform.is_some()
            || flags.user_config.is_some()
            || flags.config != Path::new("isolock.toml")
            || Path::new("isolock.toml").is_file()
            || input.locked
            || input.check;
        if profile {
            profile_cli::run(
                flags,
                Operation::Plan {
                    ci: input.ci,
                    locked: input.locked,
                    check: input.check,
                },
            )
            .map(Output::from)
            .map_err(Failure::execution)
        } else {
            plan::run(input.ci, flags).map(Output::from)
        }
    }
}

#[clifacet::command(input = Fetch, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Fetch {
    #[arg(long)]
    manifest: Option<String>,
}

impl Handler<Overrides> for Fetch {
    type Input = Fetch;
    type Output = ();

    fn handle(&self, input: Fetch, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(
            flags,
            Operation::Fetch {
                manifest: input.manifest,
            },
        )
        .map_err(Failure::execution)
    }
}

#[clifacet::command(input = Sync, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Sync {
    #[arg(long)]
    locked: bool,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    manifest: Option<String>,
}

impl Handler<Overrides> for Sync {
    type Input = Sync;
    type Output = ();

    fn handle(&self, input: Sync, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(
            flags,
            Operation::Sync {
                locked: input.locked,
                offline: input.offline,
                manifest: input.manifest,
            },
        )
        .map_err(Failure::execution)
    }
}

#[clifacet::command(input = Export, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Export {
    #[arg(long)]
    output: PathBuf,
}

impl Handler<Overrides> for Export {
    type Input = Export;
    type Output = ();

    fn handle(&self, input: Export, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(
            flags,
            Operation::Export {
                output: input.output,
            },
        )
        .map_err(Failure::execution)
    }
}

#[clifacet::command(input = SeedManifest, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct SeedManifest {}

impl Handler<Overrides> for SeedManifest {
    type Input = SeedManifest;
    type Output = ();

    fn handle(&self, _: SeedManifest, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(flags, Operation::SeedManifest {}).map_err(Failure::execution)
    }
}

#[clifacet::command(input = Gc, bridge = Ok)]
#[derive(Clone)]
pub(crate) struct Gc {
    #[arg(long)]
    apply: bool,
    #[arg(long)]
    root: Vec<String>,
}

impl Handler<Overrides> for Gc {
    type Input = Gc;
    type Output = ();

    fn handle(&self, input: Gc, flags: &Overrides) -> Result<(), Failure> {
        profile_cli::run(
            flags,
            Operation::Gc {
                apply: input.apply,
                root: input.root,
            },
        )
        .map_err(Failure::execution)
    }
}
