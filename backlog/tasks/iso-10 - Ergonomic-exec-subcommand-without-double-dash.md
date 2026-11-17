---
id: ISO-10
title: Ergonomic exec subcommand without double dash
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-4
dependencies: [ISO-1, ISO-5]
type: task
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Parse `isolock exec <cmd> [args...]` arguments directly without requiring `--` and forward execution to child subprocess.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Command-line arguments following `exec` are captured without `--`.
- [x] #2 Child process exit codes are propagated directly.
- [x] #3 CLI integration tests prove successful command execution.
<!-- AC:END -->

## Notes

- 2026-11-17: Implemented in `crates/isolock/src/main.rs` and verified in `crates/isolock/tests/cli_test.rs`.
