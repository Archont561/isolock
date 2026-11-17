---
id: ISO-9
title: GitHub Actions workflow dispatch generator
status: In Progress
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-3
dependencies: [ISO-8]
type: task
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Format `gh workflow run isolock.yml -f registry=<reg> -f packages="<pkgs>"` dispatch command when plan drift is detected.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 CLI detects missing artifacts and exits with code 1 under `--ci` flag.
- [ ] #2 Formats copy-pasteable `gh workflow run` dispatch command grouped by registry.
- [ ] #3 Validated against GitHub Actions workflow dispatch triggers.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/plan-drift.feature` and architecture document `backlog/docs/doc-5 - Lockfile-plan-drift-and-ci-airlock-guard.md`.
