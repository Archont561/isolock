---
id: m-7
title: "M7 - Reviewed ingestion and storage publication"
---

## Description

Close the gap between reviewed artifact pins and a usable published storage snapshot,
then address the separate deferred generation-management specification.

## Exit criteria

- [x] Reviewed local/explicit-connected originals enter CAS only after batch verification.
- [x] Explicit linear non-force publication supports existing raw-CAS/plan/profile consumers.
- [x] A cold consumer restores the selected closure and executes offline without the originals or remote.
- [x] Connected dispatch, read-only snapshot verification, GC retention, and documentation reflect supported behavior.
- [ ] Automatic epoch rollover and reviewed snapshot migration have their own executed acceptance contracts.

## Status

**In progress.** ISO-23 delivers the reviewed ingestion/publication slice. ISO-24 remains
To Do; automatic rollover must not be inferred from the completed earlier milestones.

## Tasks

- **ISO-23**: Reviewed artifact ingestion and explicit linear storage publication.
- **ISO-24**: Automatic storage epoch rollover and reviewed snapshot migration.
