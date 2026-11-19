---
id: ISO-21
title: Bound proxy request resources and harden portability edges
status: Done
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
- [x] #1 Proxy request concurrency has a bounded default and a documented operational configuration value.
- [x] #2 Overload produces a deterministic fail-closed response or bounded wait; it never creates an unbounded number of threads.
- [x] #3 Shutdown joins or otherwise accounts for accepted worker execution within a bounded policy.
- [x] #4 CAS sharding uses checked size conversion and avoids oversized stack buffers or documents a verified bounded alternative.
- [x] #5 Tests cover request saturation, shutdown during pending work, and shard behavior at relevant buffer and size boundaries.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/proxy-resource-bounds.feature`.
- The concurrency limit is operational configuration; it must not alter plan or artifact identity.

## Implementation evidence

`ProxyLimits` defaults to 32 workers with no application request queue. Overload returns
503 without another worker; absolute header/write deadlines and a bounded shutdown grace
are configurable operational values outside plan/artifact identity. Shutdown interrupts
sockets, joins completed workers, and explicitly counts/detaches uncooperative adapters
with a timeout error. All three resource BDD scenarios have real-socket bindings, plus
panic, trickle, slow-reader, grace-expiry, and invalid-limit tests. CAS buffers are on the
heap; checked lengths/counts have buffer, shard-ceiling, u32/usize/u64 boundary coverage.
See `docs/content/docs/proxy.mdx` for exact limits and cancellation limitations.
