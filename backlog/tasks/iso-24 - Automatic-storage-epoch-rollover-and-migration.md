---
id: ISO-24
title: Automatic storage epoch rollover and reviewed snapshot migration
status: Done
assignee: []
created_date: '2026-11-18 12:00'
labels: []
milestone: m-7
dependencies: [ISO-23]
type: task
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement reviewed source-pinned budget rollover at the marked raw-artifact boundary.
Live-tree bytes and distinct reachable historical Git blobs have separate budgets. A
verified retained closure becomes an orphan successor, published atomically with an
unchanged-tree source seal, without rewriting old refs, immutable profiles, code history,
or configuration. Unsupported legacy/unmarked/sharded snapshots receive explicit verified-raw
conversion guidance rather than being reinterpreted as supported publication formats.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 Define live-tree versus cumulative-history budgets and an explicit reviewed rollover/retention policy.
- [x] #2 Create a new orphan generation without rewriting existing storage or code history.
- [x] #3 Preserve reachable profiles, immutable manifests, input originals, provenance, and locked payload-commit availability across migration.
- [x] #4 Handle concurrent publishers and partial failures without force-pushing or silently changing the active generation.
- [x] #5 Migrate supported legacy/unmarked/sharded snapshots only through a reviewed, validated format boundary, or reject them with actionable diagnostics.
- [x] #6 Execute the deferred rollover BDD contract through the real CLI and disposable remotes, including a cold offline consumer and unchanged producer checkout/index.
<!-- AC:END -->

## Implementation and evidence

- Declarative `push --epoch-policy FILE`: strict schema 1, exact source ref/commit, immediate
  successor, explicit live/history byte budgets, retained batch/immutable-manifest roots.
  Complete isolated Git history is required; shallow storage remotes fail closed.
- Retains every published profile alias/constituent closure, verified original/input bytes,
  and matching immutable reviews/provenance. Old refs and locked storage identities remain
  unchanged. Oversize mandatory closures fail rather than dropping roots.
- A meaningful unchanged-tree source seal and fresh orphan target are atomically non-force
  published. Concurrent writers, target rejection, unavailable atomic support, and stale
  reviews cannot overwrite refs or choose a new active generation.
- A real receiver disconnect after accepting both refs proves uncertain-outcome reporting
  and safe exact retries. Retried accepted publications re-verify the full required closure
  without producer CAS/originals, including after target appends.
- [Executed feature](../features/git-orphan-branch.feature) has real CLI bindings in
  `crates/isolock/tests/epoch_rollover.rs` and `ingestion_profiles.rs`: live/history/equality
  budgets, mandatory/optional roots, successive epochs, unsupported/corrupt formats,
  unchanged producer code/index/Git state, old locked cold consumers, explicit successor
  replanning, and offline fixture runtime execution.
- Local unchanged gates: 359 tests passed / 2 opt-in skips; full-workspace line coverage
  **7806/8447 = 92.41%**, without exclusions or threshold changes. Both opt-in native
  `rust-pixi` and `rust-standalone` offline materializers passed. Formatting, strict
  all-target Clippy, dependency policy, alint, actionlint, and shellcheck passed.
- Changed Markdown and offline links pass; Vale has zero errors. The static documentation
  build passes. Whole-tree rumdl has the same 263 pre-existing findings as the base commit;
  no unrelated agent-skill Markdown was rewritten. Native standalone initially exhausted
  disk space and passed after removing only rebuildable instrumented Cargo outputs.

## Explicit boundaries

The [epoch guide](../../docs/content/docs/storage-epochs.mdx) documents review, budget
metrics, retention, recovery, supported formats, and adoption. Without a policy, ordinary
publication keeps its absolute live-tree limit rather than applying a history policy.
Archived epochs remain reachable indefinitely, so total remote disk use is not bounded.
Profile-alias/archive/shared-CAS root retirement, sharded Git publication, automatic legacy
conversion, direct registry solving, selective transfer, connected proxy misses, and native
end-to-end publication on every platform remain separate work. Tests do not publish to
actual artifact storage. PR checks must pass before merge.
