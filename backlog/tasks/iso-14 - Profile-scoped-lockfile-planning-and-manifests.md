---
id: ISO-14
title: Profile-scoped lockfile planning and immutable manifests
status: Done
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
- [x] #1 `isolock plan --profile <name> --platform <platform>` produces a deterministic immutable profile plan.
- [x] #2 A Rust Pixi profile selects only the `rust` environment closure recorded in pixi.lock, not Python or TypeScript environment packages.
- [x] #3 Profile plans record selected lockfile digests, resolved configuration digest, runtime providers, materializers, and artifact provenance.
- [x] #4 A composed profile references constituent closures without duplicating identical CAS blobs.
- [x] #5 Plan drift checks operate against the selected profile manifest and report profile-specific missing artifacts.
<!-- AC:END -->

## Notes

- Uses `rattler_lock`, `cargo-lock`, and `chaste` adapters introduced for canonical lockfile parsing.
- Governed by decision-6, doc-8, and the profiled environment materialization feature.

## Implementation evidence

Canonical lockfile adapters, normalized profile/platform manifests, constituent objects, input preflights, and deterministic/deduplicated closures are exercised by profile_bdd.rs, profile_safety.rs, and profile_boundaries.rs.

See `docs/content/docs/profiles.mdx` for the implemented schema, workflows, supported archive/lock inputs, and explicit fail-closed boundaries. Local validation includes the offline workspace/Nextest suites, strict Clippy, cargo-deny, repository/shell/workflow linting, and both real-tool integration cases.
