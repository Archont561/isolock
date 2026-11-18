---
id: ISO-16
title: Offline Pixi and Conda profile materialization
status: Done
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-14, ISO-15]
type: task
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Materialize profile-selected Conda artifacts as a local channel view and invoke seed Pixi with locked offline inputs to create only the requested `.pixi/envs/<environment>` prefix. Preserve original Conda archives and metadata in CAS rather than untarring them directly into an arbitrary target prefix.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Isolock derives per-profile `repodata.json` and package paths from verified Conda CAS artifacts.
- [x] #2 Bootstrap invokes `pixi install --environment <name> --locked --offline` against the derived local source.
- [x] #3 A Rust-only profile produces `.pixi/envs/rust/bin/cargo` without installing Python or Bun packages.
- [x] #4 Conda records, package checksums, channel metadata, and Pixi lock inputs are verified before prefix materialization.
- [x] #5 A generated receipt records the selected environment and exact artifacts linked into the prefix.
<!-- AC:END -->

## Notes

- The implementation may use the existing loopback proxy or a generated local file channel, but must preserve the same locked artifact closure.
- Governed by decisions 3, 4, 6, and doc-8.

## Implementation evidence

Sealed local channels and selected local-file-only Pixi workspace inputs support a genuinely cold offline cache. Real seed Pixi links only rust; exact Conda records and derived-input digests are checked and receipted.

See `docs/content/docs/profiles.mdx` for the implemented schema, workflows, supported archive/lock inputs, and explicit fail-closed boundaries. Local validation includes the offline workspace/Nextest suites, strict Clippy, cargo-deny, repository/shell/workflow linting, and both real-tool integration cases.
