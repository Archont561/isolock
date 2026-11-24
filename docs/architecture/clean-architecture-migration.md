# Clean-architecture migration notes

This note records the module move that introduced `domain`, `application`, `infrastructure`,
and the `cli` adapter, and it is the place to look if a later reader wonders whether the
command contract changed.

## Outcome

No command, flag, alias, exit code, output stream, JSON document, dry-run semantic, or CI
semantic changed. The refactor moved *where a decision is written*, never *what the operator
sees*. Every existing CLI contract test still passes unmodified, which is the evidence for
that claim.

## Module mapping

| Before | After |
| --- | --- |
| `src/profile_cli.rs` (use-case flow and `println!`) | `src/application/use_cases/{plan,bootstrap,execution}.rs` with rendering in `src/cli/view.rs` |
| `src/proxy_cli.rs` | `src/application/use_cases/execution.rs` plus `src/infrastructure/proxy_runtime.rs` |
| `src/cli/plan.rs` (legacy plan diff in the adapter) | `src/infrastructure/legacy_plan.rs` behind the `LegacyPlan` port, decided by `check_airlock_plan` |
| `src/config.rs` (types, Figment loader, validation, explain) | `src/domain/configuration.rs` (types), `src/infrastructure/config.rs` (loader, validation, provenance) |
| `src/config/execution.rs` (`ExecutionConfig`, `ProxyMode`, limits) | `src/domain/configuration.rs` (`ProxyMode`, `ProxySettings`) and `src/infrastructure/config.rs` (`session`) |
| `src/profile_store.rs` | `src/infrastructure/profile_store.rs` plus the `PlanStore`/`GarbageCollector` adapters in `src/infrastructure/plan_store.rs` |
| `src/transport.rs` | `src/infrastructure/transport.rs` (now speaking `BlobStore`) plus `src/infrastructure/transport_adapter.rs` |
| `src/materialize/` | `src/infrastructure/materialize/` plus `src/infrastructure/materializer_adapter.rs` |
| `src/execution/` | `src/infrastructure/execution/` |
| `src/ingestion/` | `src/infrastructure/ingestion/` plus `src/infrastructure/ingestion_adapter.rs` |
| `src/cli/{configuration,ingestion,plan,profiles,proxy}.rs` | `src/cli/commands/{configuration,ingestion,profiles,proxy}.rs`, `src/cli/{context,error,options,view}.rs` |
| `src/main.rs` with nine `mod` declarations | `src/main.rs` calling `isolock::run()`; layers live behind `src/lib.rs` |

## Deliberate decisions

- **The library is internal.** `src/lib.rs` exposes only `run`, so the layering is testable by
  integration tests without promising the layers as public API. This keeps the blocking
  `clippy::missing_errors_doc` policy meaningful for the real public surface.
- **No new crates.** The layers are modules of `crates/isolock`, which avoids two new
  workspace members and the crate-layout, README, and template obligations they trigger.
- **Reused models instead of new ones.** `domain` re-uses `isolock_plan::Profile` and
  `isolock_proxy::ProxyLimits`; both are pure types without I/O, so copying them would have
  added duplication rather than separation.
- **`--ci` and `--locked` operate on the lock.** They were already documented as CI intent;
  the adapter maps them to `check_locked_profile`, matching the previous behaviour exactly.
- **Publication keeps its own diagnostics.** `push --profile` still reports
  `profile publication requires --profile` before the generic
  `explicit --profile is required`, because a publication is a different recovery.
- **Progress is a port.** `Progress` receives the lines that used to be `println!` calls, so a
  use case can be tested without capturing a process stream.

## Intentional behaviour changes

None.
