---
id: ISO-15
title: Verified standalone seed-tool provisioning
status: To Do
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
- [ ] #1 A bootstrap manifest can declare seed tool archives separately from environment payloads.
- [ ] #2 Bootstrap verifies seed digests and atomically exposes `isolock` and `pixi` under `.isolock/bin`.
- [ ] #3 A Pixi seed tool is never obtained from the Pixi prefix it is required to create.
- [ ] #4 Locked mode records seed provider, version, platform, and digest and rejects an incompatible host fallback.
- [ ] #5 POSIX bootstrap remains runnable with only the utilities specified by decision-2.
<!-- AC:END -->

## Notes

- A permissive local profile may use a detected host Isolock or Pixi, but CI must record the exact accepted provider.
- Governed by decisions 2, 6, and 7.
