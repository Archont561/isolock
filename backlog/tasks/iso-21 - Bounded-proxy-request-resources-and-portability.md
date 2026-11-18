---
id: ISO-21
title: Bound proxy request resources and harden portability edges
status: To Do
assignee: []
created_date: '2026-11-18 11:00'
labels: []
milestone: m-6
dependencies: [ISO-20]
type: task
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace unbounded one-thread-per-connection proxy handling with an explicit bounded-concurrency policy, and resolve the Clippy-reported large stack-buffer and fallible integer-conversion edges in CAS sharding. Make overload and shutdown behavior deterministic and testable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 Proxy request concurrency has a bounded default and a documented operational configuration value.
- [ ] #2 Overload produces a deterministic fail-closed response or bounded wait; it never creates an unbounded number of threads.
- [ ] #3 Shutdown joins or otherwise accounts for accepted worker execution within a bounded policy.
- [ ] #4 CAS sharding uses checked size conversion and avoids oversized stack buffers or documents a verified bounded alternative.
- [ ] #5 Tests cover request saturation, shutdown during pending work, and shard behavior at relevant buffer and size boundaries.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/proxy-resource-bounds.feature`.
- The concurrency limit is operational configuration; it must not alter plan or artifact identity.
