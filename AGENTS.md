<!--
  Generated file — do not edit AGENTS.md by hand.
  Source template: templates/AGENTS.md.j2
-->

# Agent Operating Rules & Invariants (`v0.1.0`)

This document governs agent-assisted development in `isolock`.

## Core Invariants

1. **Airlock Separation**: All dependencies must be resolvable offline through CAS blobs on `isolock/v1`.
2. **Incremental Chores**: Ingest commits on `isolock/v1` are linear `chore(deps): add ...` commits with parent chaining.
3. **No Tracked Blobs on `main`**: Binary blobs (`.conda`, `.tar.bz2`, `.whl`, `.tgz`, `.crate`) must never be committed to `main`.
4. **Behavior Contracts**: User-facing promises must have an accompanying BDD scenario in `features/*.feature` with `rstest-bdd` step bindings.
5. **No Network in CI**: Tests must run hermetically with `--offline`.
6. **Pixi Inaccessibility Fallback**: Whenever `pixi` is inaccessible, missing from `PATH`, or fails in an unprovisioned environment, agents must run `bash scripts/restore.sh` to restore the locked developer sandbox.

## Verification Commands

- `cargo nextest run --workspace`
- `cargo test --workspace`
- `cargo llvm-cov --workspace --summary-only`
- `alint`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --check`
- `cargo deny check bans licenses sources`
