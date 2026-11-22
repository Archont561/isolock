//! Declarative controllers own command authoring; one clifacet application runs them.

mod configuration;
mod ingestion;
mod plan;
mod profiles;
mod proxy;

use crate::config::Overrides;
use crate::profile_cli::{self, Operation};
use crate::proxy_cli;
use clifacet::contract::ExitPolicy;
use clifacet::policy::Policy;
use clifacet::{App, Failure};
use configuration::Config;
use ingestion::{Ingest, Push};
use profiles::{Export, Fetch, Gc, NoArguments, Plan, Sync};
use proxy::Exec;
use std::cell::Cell;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

#[clifacet::app(
    name = "isolock",
    version,
    about = "Hermetic dependency airlock manager",
    commands
)]
#[command(
    propagate_version = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Isolock {
    #[command(flatten)]
    overrides: Overrides,
    // The runner initializes this shared completion state lazily, never for help/errors.
    #[clifacet(context)]
    child_status: Rc<Cell<Option<ExitCode>>>,
}

#[clifacet::commands]
impl Isolock {
    /// Validate or explain configuration without fetching artifacts.
    pub fn config(&self, #[clifacet(subcommand)] config: Config) -> Result<String, Failure> {
        config.init_from_parent(&self.overrides);
        config.execute()
    }

    /// Generate/check a profile plan, or inspect the legacy airlock plan.
    pub fn plan(&self, #[clifacet(subcommand)] options: Plan) -> Result<String, Failure> {
        let flags = &self.overrides;
        let profile = flags.profile.is_some()
            || flags.platform.is_some()
            || flags.user_config.is_some()
            || flags.config != Path::new("isolock.toml")
            || Path::new("isolock.toml").is_file()
            || options.locked
            || options.check;
        if profile {
            profile_cli::run(
                flags,
                Operation::Plan {
                    ci: options.ci,
                    locked: options.locked,
                    check: options.check,
                },
            )
            .map(|()| String::new())
            .map_err(Failure::execution)
        } else {
            plan::run(options.ci, flags)
        }
    }

    /// Retrieve the selected manifest graph and its missing blobs.
    pub fn fetch(&self, #[clifacet(subcommand)] options: Fetch) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Fetch {
                manifest: options.manifest,
            },
        )
    }

    /// Materialize the selected locked profile.
    pub fn sync(&self, #[clifacet(subcommand)] options: Sync) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Sync {
                locked: options.locked,
                offline: options.offline,
                manifest: options.manifest,
            },
        )
    }

    /// Export the selected profile to an offline directory snapshot.
    pub fn export(&self, #[clifacet(subcommand)] options: Export) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Export {
                output: options.output,
                quiet: false,
            },
        )
    }

    /// Write the reviewed POSIX seed manifest and bootstrap script.
    pub fn seed_manifest(
        &self,
        #[clifacet(subcommand)] _options: NoArguments,
    ) -> Result<(), String> {
        profile_cli::run(&self.overrides, Operation::SeedManifest {})
    }

    /// Report unreachable CAS blobs; deletion requires --apply.
    pub fn gc(&self, #[clifacet(subcommand)] options: Gc) -> Result<(), String> {
        profile_cli::run(
            &self.overrides,
            Operation::Gc {
                apply: options.apply,
                root: options.root,
            },
        )
    }

    /// Serve the locked registry snapshot on loopback.
    pub fn serve(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<(), String> {
        proxy_cli::run_proxy_server(&self.overrides)
    }

    /// Run a child command with the local registry proxy.
    pub fn exec(&self, #[clifacet(subcommand)] options: Exec) -> Result<(), Failure> {
        let (program, arguments) = options
            .arguments
            .split_first()
            .ok_or_else(|| Failure::validation("expected a child command"))?;
        let command = proxy_cli::ExecCommand { program, arguments };
        let status = proxy_cli::run_child_with_proxy(&command, &self.overrides)
            .map_err(Failure::execution)?;
        (*self.child_status).set(Some(status));
        Ok(())
    }

    /// Publish verified reviewed artifacts to an isolated storage branch, without force.
    pub fn push(&self, #[clifacet(subcommand)] options: Push) -> Result<String, String> {
        crate::ingestion::publish(
            &self.overrides,
            &options.manifest,
            &options.remote,
            options.epoch_policy.as_deref(),
            crate::ingestion::Selection {
                registry: options.registry.as_deref(),
                packages: &options.packages,
            },
        )
    }

    /// Verify and import explicitly reviewed originals into CAS.
    pub fn ingest(&self, #[clifacet(subcommand)] options: Ingest) -> Result<String, String> {
        crate::ingestion::ingest(
            &self.overrides,
            &options.manifest,
            &options.artifacts,
            crate::ingestion::Selection {
                registry: options.registry.as_deref(),
                packages: &options.packages,
            },
            options.download,
        )
    }
}

pub(crate) fn run() -> ExitCode {
    let child_status = Rc::new(Cell::new(None));
    let completed = Rc::clone(&child_status);
    let status = App::<Isolock, Isolock, clifacet::Plain, _>::for_cli()
        .policy(Policy::non_interactive())
        .exit(ExitPolicy::standard())
        .after_success(move |_, context| {
            completed.set(context.get());
            Ok(())
        })
        .render(|text: String, out: &mut dyn Write| {
            if text.is_empty() {
                Ok(())
            } else {
                writeln!(out, "{text}")
            }
        })
        .run();
    // A child owns its exact status, but cannot hide a runner or rendering failure.
    if status == ExitCode::SUCCESS {
        child_status.get().unwrap_or(status)
    } else {
        status
    }
}
