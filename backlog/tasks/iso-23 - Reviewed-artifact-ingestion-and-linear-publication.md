---
id: ISO-23
title: Reviewed artifact ingestion and explicit linear storage publication
status: Done
assignee: []
created_date: '2026-11-18 12:00'
labels: []
milestone: m-7
dependencies: [ISO-9, ISO-18, ISO-22]
type: task
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Deliver one usable reviewed artifact → verified CAS → explicit linear Git storage commit
→ cold offline consumer path. The approved first slice uses a reviewed JSON manifest and
local originals, with explicit pinned-source acquisition in the connected workflow. It is
not direct registry metadata/dependency resolution.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 Replace the ingestion placeholder with strict reviewed-pin validation and whole-selected-batch verification before CAS import, including cache hits and duplicate digests.
- [x] #2 Provide explicit non-force `push`, immutable review/payload records, a metadata-preserving legacy snapshot, idempotent append, concurrent-update rejection/retry, and unchanged producer checkout/index.
- [x] #3 Publish locked profiles against earlier payload commits and prove cold consumer fetch, exact selected CAS closure, and offline execution after removing originals and remote.
- [x] #4 Update the connected workflow, literal-argv dispatch handling, generated guidance, help/error/version contracts, documentation, and backlog status.
- [x] #5 Keep reviewed ingestion roots safe across GC/shared workspaces, with corrupted roots failing closed.
- [x] #6 Preserve declarative clifacet commands and the unchanged full-workspace 90% line-coverage gate; execute hermetic tests and separate native profile regressions.
<!-- AC:END -->

## Implementation evidence

- `backlog/features/reviewed-ingestion.feature` binds to `crates/isolock/tests/reviewed_ingestion.rs`
  and `ingestion_profiles.rs`; these exercise the real CLI and disposable Git repositories.
- Reviewed JSON, acquisition staging, Git plumbing, publication assembly, pinned-profile
  export, and ingestion GC roots live under `crates/isolock/src/ingestion/`.
- The profile publication scenario uses fixture runtimes/archives, not a native toolchain.
  Separate opt-in real Pixi/Rust integration tests validate existing materializers.
- Local evidence is kept in ignored `output/ingestion/`; CI validates the submitted head.
  No test publishes binary artifacts to real storage or to the code branch.

## Explicit limits and follow-up

Originals are at most 95 MiB, reviewed/publisher-managed JSON is bounded, and live Git
snapshot trees are at most 1,500 MiB. ISO-24 adds opt-in, reviewed epoch rollover and
snapshot migration under explicit live/history budgets; archived generations are not
automatically retired. Direct registry resolution, selective Git/HTTP transport, connected
proxy-miss fetching, and broader native publication coverage remain later slices.
