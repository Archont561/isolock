---
id: ISO-9
title: GitHub Actions workflow dispatch generator
status: Done
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
Format `gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=<reg> -f packages="<pkgs>"` when expected legacy plan artifacts are missing. Require reviewed/committed source, filename, and SHA-256 pins before dispatch; do not imply registry solving.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 CLI detects missing artifacts and exits with code 1 under `--ci` flag.
- [x] #2 Formats copy-pasteable `gh workflow run` dispatch command grouped by registry.
- [x] #3 Validated against GitHub Actions workflow dispatch triggers.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/plan-drift.feature` and architecture document `backlog/docs/doc-5 - Lockfile-plan-drift-and-ci-airlock-guard.md`.
- 2026-11-17: CLI drift checks now load the airlock plan and CAS, fail closed when no plan is available, and emit safe, deterministic registry-grouped commands; the BDD contract and workflow input contract tests pass.

- ISO-23 supplies the formerly missing real ingestion/publication path and explicit reviewed-manifest workflow input. The PR guard now verifies the published snapshot itself; profile CI separately checks selected input/closure drift.
