---
id: ISO-18
title: Selective profile transport, CI integration, and garbage-collection roots
status: To Do
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-14, ISO-16, ISO-17]
type: task
ordinal: 18000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make profile selection effective across storage transfer, CI, and CAS retention. A selected profile should fetch and materialize only the digests reachable from its immutable manifest where transport supports it. CI jobs select profiles explicitly and retain their manifests as garbage-collection roots.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 Bootstrap can fetch a selected profile manifest before fetching its artifact payload.
- [ ] #2 The transport implementation supports selective retrieval or clearly reports when a fallback fetches a broader storage snapshot.
- [ ] #3 CI jobs declare the profile and platform they require and execute only its materialization path.
- [ ] #4 CAS garbage collection retains artifacts reachable from configured profile manifests and reports reclaimable blobs deterministically.
- [ ] #5 Locked CI fails when a configuration override changes the selected profile closure or seed-tool trust boundary.
<!-- AC:END -->

## Notes

- Git shallow history does not by itself guarantee blob-selective transfer from a cumulative storage tree.
- Governed by decisions 1, 6, and 7 and doc-8.
