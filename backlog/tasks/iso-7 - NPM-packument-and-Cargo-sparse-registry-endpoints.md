---
id: ISO-7
title: NPM packument and Cargo sparse registry endpoints
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-2
dependencies: [ISO-5]
type: task
ordinal: 7000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Synthesize NPM packument JSON with SHA-512 integrity hashes and RFC 2789 sparse index paths for Cargo in `isolock-proxy`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 `/npm/<package>` returns valid packuments and serves `.tgz` tarballs.
- [x] #2 `/cargo/...` implements sharded sparse registry index and serves `.crate` archives.
- [x] #3 Verified end-to-end with `npm install` and `cargo fetch`.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/proxy-npm.feature` and `backlog/features/proxy-cargo.feature`.
- Uses `tame-index` for RFC 2789 sparse registry metadata representation as decided in `decision-4`.
- 2026-11-17: Verified with real `npm install` and `cargo fetch` against the loopback proxy and CAS-only fixtures.
