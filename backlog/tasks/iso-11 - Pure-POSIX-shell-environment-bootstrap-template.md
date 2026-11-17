---
id: ISO-11
title: Pure POSIX shell environment bootstrap template
status: To Do
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-4
dependencies: [ISO-4]
type: task
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create portable POSIX shell bootstrap script template (`templates/bootstrap.sh`) that restores offline toolchain environments without native binaries.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 Script executes with standard `/bin/sh` without Bashisms.
- [ ] #2 Reassembles `.part*` shards and validates SHA-256 checksums with `sha256sum`.
- [ ] #3 Generates `.isolock/state/<env>/activate` script updating `PATH`.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/posix-bootstrap.feature` and architecture document `backlog/docs/doc-6 - Pure-posix-shell-environment-bootstrapping.md`.
