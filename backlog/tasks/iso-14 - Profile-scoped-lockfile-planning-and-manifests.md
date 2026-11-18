---
id: ISO-14
title: Profile-scoped lockfile planning and immutable manifests
status: To Do
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-13]
type: task
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend `isolock-plan` with named profile and platform roots. Select only packages belonging to the requested Pixi environment, plus the explicit Cargo, PyPI, and JavaScript lockfile roots declared by that profile. Emit deterministic profile manifests and a canonical resolved-configuration digest.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 `isolock plan --profile <name> --platform <platform>` produces a deterministic immutable profile plan.
- [ ] #2 A Rust Pixi profile selects only the `rust` environment closure recorded in pixi.lock, not Python or TypeScript environment packages.
- [ ] #3 Profile plans record selected lockfile digests, resolved configuration digest, runtime providers, materializers, and artifact provenance.
- [ ] #4 A composed profile references constituent closures without duplicating identical CAS blobs.
- [ ] #5 Plan drift checks operate against the selected profile manifest and report profile-specific missing artifacts.
<!-- AC:END -->

## Notes

- Uses `rattler_lock`, `cargo-lock`, and `chaste` adapters introduced for canonical lockfile parsing.
- Governed by decision-6, doc-8, and the profiled environment materialization feature.
