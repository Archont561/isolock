---
id: ISO-15
title: Verified standalone seed-tool provisioning
status: Done
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-11, ISO-14]
type: task
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend the POSIX bootstrap flow to restore verified platform-specific standalone Isolock and Pixi seed tools under `.isolock/bin`. Seed tools are selected by profile policy and become available before any package-manager environment is materialized.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 A bootstrap manifest can declare seed tool archives separately from environment payloads.
- [x] #2 Bootstrap verifies seed digests and atomically exposes `isolock` and `pixi` under `.isolock/bin`.
- [x] #3 A Pixi seed tool is never obtained from the Pixi prefix it is required to create.
- [x] #4 Locked mode records seed provider, version, platform, and digest and rejects an incompatible host fallback.
- [x] #5 POSIX bootstrap remains runnable with only the utilities specified by decision-2.
<!-- AC:END -->

## Notes

- A permissive local profile may use a detected host Isolock or Pixi, but CI must record the exact accepted provider.
- Governed by decisions 2, 6, and 7.

## Implementation evidence

Separate reviewed seed manifests, archive/byte verification, version-owned atomic exposure, POSIX-only bootstrap, and no-host-fallback checks are covered by BDD and the real Pixi bootstrap test.

See `docs/content/docs/profiles.mdx` for the implemented schema, workflows, supported archive/lock inputs, and explicit fail-closed boundaries. Local validation includes the offline workspace/Nextest suites, strict Clippy, cargo-deny, repository/shell/workflow linting, and both real-tool integration cases.
