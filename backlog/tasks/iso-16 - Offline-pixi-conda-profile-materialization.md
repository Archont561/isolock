---
id: ISO-16
title: Offline Pixi and Conda profile materialization
status: To Do
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
- [ ] #1 Isolock derives per-profile `repodata.json` and package paths from verified Conda CAS artifacts.
- [ ] #2 Bootstrap invokes `pixi install --environment <name> --locked --offline` against the derived local source.
- [ ] #3 A Rust-only profile produces `.pixi/envs/rust/bin/cargo` without installing Python or Bun packages.
- [ ] #4 Conda records, package checksums, channel metadata, and Pixi lock inputs are verified before prefix materialization.
- [ ] #5 A generated receipt records the selected environment and exact artifacts linked into the prefix.
<!-- AC:END -->

## Notes

- The implementation may use the existing loopback proxy or a generated local file channel, but must preserve the same locked artifact closure.
- Governed by decisions 3, 4, 6, and doc-8.
