---
id: m-7
title: "M7 - Reviewed ingestion and storage publication"
---

## Description

Close the gap between reviewed artifact pins and a usable published storage snapshot,
including explicitly reviewed, budget-triggered generation management.

## Exit criteria

- [x] Reviewed local/explicit-connected originals enter CAS only after batch verification.
- [x] Explicit linear non-force publication supports existing raw-CAS/plan/profile consumers.
- [x] A cold consumer restores the selected closure and executes offline without the originals or remote.
- [x] Connected dispatch, read-only snapshot verification, GC retention, and documentation reflect supported behavior.
- [x] Automatic epoch rollover and reviewed snapshot migration have their own executed acceptance contracts.

## Status

**Complete within this scope.** ISO-23 delivers reviewed ingestion/publication. ISO-24 adds
source-pinned live/history budgets, verified profile/batch retention, atomic source sealing
and orphan succession, safe failure/retry handling, and explicit generation adoption.
Real CLI/disposable-remotes contracts include successive epochs, old locked consumers,
explicitly replanned successor consumers, and cold offline execution.

Unsupported legacy/unmarked/sharded formats are rejected with verified-raw conversion
guidance. No automatic format reinterpretation, ref/configuration rewrite, global remote
storage cap, registry solving, selective transfer, or archived/shared-CAS root retirement
is claimed. Native materializer tests do not imply native publication coverage everywhere.

## Tasks

- **ISO-23**: Reviewed artifact ingestion and explicit linear storage publication (Done).
- **ISO-24**: Automatic storage epoch rollover and reviewed snapshot migration (Done).
