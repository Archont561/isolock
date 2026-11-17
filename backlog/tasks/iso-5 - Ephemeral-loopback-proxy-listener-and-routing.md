---
id: ISO-5
title: Ephemeral loopback proxy listener and routing
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-2
dependencies: [ISO-1]
type: task
ordinal: 5000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement proxy listener configuration and routing decision evaluation (`evaluate_proxy_request`) across airlock modes (`Connected`, `Ci`, `LocalAirlock`).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Routing decision logic distinguishes cached hits from un-cached airlock violations.
- [x] #2 `LocalAirlock` and `Ci` modes fail closed on cache misses.
- [x] #3 TCP listener binds dynamically to `127.0.0.1:<port>` and streams HTTP responses.
<!-- AC:END -->

## Notes

- 2026-11-17: Dynamic loopback listener, HTTP routing, fail-closed cache misses, and wire-level tests implemented and verified.
