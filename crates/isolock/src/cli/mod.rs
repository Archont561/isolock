//! The clifacet adapter and composition root: parsing, dispatch, rendering, exit mapping.
//!
//! Every handler does the same three things: convert parsed options into application data,
//! call exactly one use case, and render its typed outcome. No handler diffs plans, merges
//! configuration, reads CAS, runs Git, or talks to the proxy.

mod commands;
mod context;
mod error;
mod options;
mod view;

use crate::application::ports::{Ingestion, ProfileSnapshot, Progress};
use crate::application::use_cases::bootstrap::Workspace;
use crate::application::use_cases::{
    check_airlock_plan, check_locked_profile, exec_child, export_profile, fetch_profile,
    garbage_collect, plan_profile, seed_manifest, serve_proxy, sync_profile, PlanOutcome,
};
use crate::domain::Settings;
use clifacet::contract::ExitPolicy;
use clifacet::policy::Policy;
use clifacet::{App, Failure};
use commands::configuration::Config;
use commands::ingestion::{Ingest, Push};
use commands::profiles::{Export, Fetch, Gc, NoArguments, Plan, Sync};
use commands::proxy::Exec;
use context::AppContext;
use options::Overrides;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

/// A progress sink that writes one line per step to standard output.
struct StdoutProgress;

impl Progress for StdoutProgress {
    fn line(&mut self, line: &str) {
        println!("{line}");
    }
}

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
    #[clifacet(context = context_from_cli)]
    context: AppContext,
}

impl Isolock {
    /// The parsed global overrides; the context factory converts them once.
    pub(crate) fn overrides(&self) -> &Overrides {
        &self.overrides
    }

    fn settings(&self) -> &Settings {
        self.context.settings()
    }

    fn reports_drift(outcome: &PlanOutcome, ci: bool) -> Result<String, Failure> {
        let text = view::plan(outcome, ci);
        if view::plan_fails_ci(outcome, ci) {
            Err(error::execution(crate::domain::DomainError::new(text)))
        } else {
            Ok(text)
        }
    }

    /// Whether the invocation asks for a profile plan rather than the legacy airlock check.
    fn selects_profile_plan(&self, options: &Plan) -> bool {
        self.overrides.profile.is_some()
            || self.overrides.platform.is_some()
            || self.overrides.user_config.is_some()
            || self.overrides.config != Path::new("isolock.toml")
            || Path::new("isolock.toml").is_file()
            || options.locked
            || options.check
    }
}

fn context_from_cli(cli: &Isolock) -> Result<AppContext, Failure> {
    AppContext::from_cli(cli)
}

#[clifacet::commands]
impl Isolock {
    /// Validate or explain configuration without fetching artifacts.
    pub fn config(&self, #[clifacet(subcommand)] config: Config) -> Result<String, Failure> {
        // The nested controller receives only the already-wired context, never argv.
        config.init_from_parent(&*self.context);
        config.execute()
    }

    /// Generate/check a profile plan, or inspect the legacy airlock plan.
    pub fn plan(&self, #[clifacet(subcommand)] options: Plan) -> Result<String, Failure> {
        if self.selects_profile_plan(&options) {
            // `--ci` and `--locked` document CI intent and always mean "verify the lock".
            let checking = options.check || options.ci || options.locked;
            let store = self.context.store();
            let outcome = if checking {
                check_locked_profile(&store, self.settings(), self.context.selection())
            } else {
                plan_profile(&store, self.settings(), self.context.selection())
            }
            .map_err(error::execution)?;
            Self::reports_drift(&outcome, options.ci)
        } else {
            let plan = self.context.legacy_plan()?;
            let outcome = check_airlock_plan(&plan).map_err(error::execution)?;
            Self::reports_drift(&outcome, options.ci)
        }
    }

    /// Retrieve the selected manifest graph and its missing blobs.
    pub fn fetch(&self, #[clifacet(subcommand)] options: Fetch) -> Result<String, Failure> {
        let outcome = fetch_profile(
            Workspace {
                store: &self.context.store(),
                transport: &self.context.transport(),
                settings: self.settings(),
                selection: self.context.selection(),
                progress: &mut StdoutProgress,
            },
            options.manifest.as_deref(),
        )
        .map_err(error::execution)?;
        Ok(view::fetch(&outcome))
    }

    /// Materialize the selected locked profile.
    pub fn sync(&self, #[clifacet(subcommand)] options: Sync) -> Result<String, Failure> {
        let outcome = sync_profile(
            Workspace {
                store: &self.context.store(),
                transport: &self.context.transport(),
                settings: self.settings(),
                selection: self.context.selection(),
                progress: &mut StdoutProgress,
            },
            &self.context.materializer(),
            options.offline,
            options.manifest.as_deref(),
        )
        .map_err(error::execution)?;
        Ok(view::sync(&outcome))
    }

    /// Export the selected profile to an offline directory snapshot.
    pub fn export(&self, #[clifacet(subcommand)] options: Export) -> Result<String, Failure> {
        export_profile(
            &self.context.store(),
            &self.context.transport(),
            self.settings(),
            self.context.selection(),
            &options.output,
        )
        .map_err(error::execution)?;
        let selection = self.context.selection();
        let profile = selection.require_profile().map_err(error::execution)?;
        let platform = selection.require_platform().map_err(error::execution)?;
        Ok(view::export(profile, platform))
    }

    /// Write the reviewed POSIX seed manifest and bootstrap script.
    pub fn seed_manifest(
        &self,
        #[clifacet(subcommand)] _options: NoArguments,
    ) -> Result<String, Failure> {
        let outcome = seed_manifest(
            &self.context.store(),
            &self.context.materializer(),
            self.settings(),
            self.context.selection(),
        )
        .map_err(error::execution)?;
        Ok(view::seed(&outcome))
    }

    /// Report unreachable CAS blobs; deletion requires --apply.
    pub fn gc(&self, #[clifacet(subcommand)] options: Gc) -> Result<String, Failure> {
        let report = garbage_collect(&self.context.store(), &options.root, options.apply)
            .map_err(error::execution)?;
        view::collection(&report).map_err(error::execution)
    }

    /// Serve the locked registry snapshot on loopback.
    pub fn serve(&self, #[clifacet(subcommand)] _options: NoArguments) -> Result<String, Failure> {
        serve_proxy(&self.context.runtime()?, &mut StdoutProgress).map_err(error::execution)?;
        Ok(String::new())
    }

    /// Run a child command with the local registry proxy.
    pub fn exec(&self, #[clifacet(subcommand)] options: Exec) -> Result<String, Failure> {
        let outcome =
            exec_child(&self.context.runtime()?, &options.arguments).map_err(error::execution)?;
        if let Some(warning) = view::shutdown_warning(&outcome) {
            eprintln!("{warning}");
        }
        self.context.record_child_status(outcome.status);
        Ok(String::new())
    }

    /// Publish verified reviewed artifacts to an isolated storage branch, without force.
    pub fn push(&self, #[clifacet(subcommand)] options: Push) -> Result<String, Failure> {
        let snapshot = SnapshotAdapter {
            context: &self.context,
        };
        self.context
            .ingestion()
            .publish(crate::application::ports::Publication {
                snapshot: &snapshot,
                manifest: &options.manifest,
                remote: &options.remote,
                epoch_policy: options.epoch_policy.as_deref(),
                selection: crate::application::ports::Selection {
                    registry: options.registry.as_deref(),
                    packages: &options.packages,
                },
            })
            .map_err(error::execution)
    }

    /// Verify and import explicitly reviewed originals into CAS.
    pub fn ingest(&self, #[clifacet(subcommand)] options: Ingest) -> Result<String, Failure> {
        self.context
            .ingestion()
            .ingest(
                &options.manifest,
                &options.artifacts,
                crate::application::ports::Selection {
                    registry: options.registry.as_deref(),
                    packages: &options.packages,
                },
                options.download,
            )
            .map_err(error::execution)
    }
}

/// Exports the currently locked profile so publication can pin its earlier payload commit.
struct SnapshotAdapter<'a> {
    context: &'a AppContext,
}

impl ProfileSnapshot for SnapshotAdapter<'_> {
    fn selection(&self) -> (String, String) {
        let selection = self.context.selection();
        (
            selection.profile.clone().unwrap_or_default(),
            selection.platform.clone().unwrap_or_default(),
        )
    }

    fn snapshot(&self, output: &Path) -> Result<(), String> {
        export_profile(
            &self.context.store(),
            &self.context.transport(),
            self.context.settings(),
            self.context.selection(),
            output,
        )
        .map_err(String::from)
    }
}

/// Runs one non-interactive clifacet pipeline and returns the process status.
pub fn run() -> ExitCode {
    let completed: Rc<std::cell::Cell<Option<ExitCode>>> = Rc::new(std::cell::Cell::new(None));
    let recorded = Rc::clone(&completed);
    let status = App::<Isolock, Isolock, clifacet::Plain, AppContext>::for_cli()
        .policy(Policy::non_interactive())
        .exit(ExitPolicy::standard())
        .after_success(move |_command, context: &AppContext| {
            recorded.set(context.taken_child_status());
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
        completed.get().unwrap_or(status)
    } else {
        status
    }
}
