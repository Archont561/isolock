---
id: ISO-4
title: Large blob sharding and POSIX cat reassembly
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-1
dependencies: [ISO-3]
type: task
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Extend `isolock-cas` to partition binary blobs exceeding 95 MiB into sequential `.part*` shards with `.shards.json` manifests and verify reassembly.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Artifacts exceeding 95 MiB are chunked into `.part000`, `.part001`, …
- [x] #2 Manifest records part digests and total uncompressed size.
- [x] #3 Sequential concatenation reassembles exact byte payload and passes SHA-256 check.
<!-- AC:END -->

## Notes

- Governed by behavioral specification `backlog/features/cas-sharding.feature` and architecture document `backlog/docs/doc-2 - Content-addressed-storage-and-large-blob-sharding.md`.
- Implemented in `crates/isolock-cas` with streaming SHA-256 hashing, ordered part files, a validated JSON manifest, and POSIX `cat` reassembly coverage.
