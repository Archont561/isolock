---
id: ISO-17
title: Standalone runtime materializers and shared ecosystem payloads
status: Done
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-14, ISO-15]
type: task
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement materializers for standalone Rust, Python, and JavaScript runtime profiles. Each profile exposes a controlled `bin` directory with an Isolock launcher while consuming its ecosystem-specific verified payload: Cargo vendor sources, a PyPI wheelhouse, or a restored node_modules tree/local npm registry.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 A standalone Rust profile exposes verified `cargo`, `rustc`, and an Isolock launcher and builds through a locked vendor source without network access.
- [x] #2 A standalone Python profile exposes Python and pip with a locked wheelhouse or equivalent local index.
- [x] #3 A Bun/Node profile exposes its runtime and executes only restored locked package executables; missing `bun x` packages fail without network access.
- [x] #4 Materializers keep provider provenance distinct even when a Pixi profile exposes the same tool capability.
- [x] #5 Identical derived payloads are reused only when all derivation inputs match and are recorded in the profile receipt.
<!-- AC:END -->

## Notes

- The profile-local `isolock` entry should be a wrapper or symlink to one verified seed binary, not independently copied executable bytes.
- Governed by decision-6 and doc-8.

## Implementation evidence

Rust/Python/Bun/Node launchers consume verified runtime prefixes and keyed vendor/wheel/JavaScript payloads. BDD covers the standalone paths; real standalone Rust builds a locked registry dependency offline.

See `docs/content/docs/profiles.mdx` for the implemented schema, workflows, supported archive/lock inputs, and explicit fail-closed boundaries. Local validation includes the offline workspace/Nextest suites, strict Clippy, cargo-deny, repository/shell/workflow linting, and both real-tool integration cases.
