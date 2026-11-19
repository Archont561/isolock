---
id: ISO-19
title: Decompose CLI execution sessions and environment injection
status: Done
assignee: []
created_date: '2026-11-18 11:00'
labels: []
milestone: m-6
dependencies: [ISO-13]
type: task
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Refactor the CLI execution path so command parsing, resolved configuration, proxy-session lifetime, Cargo configuration, Pixi configuration, child-process environment construction, and exit-status mapping have separate owners. Replace repeated direct environment-variable reads with the resolved configuration boundary introduced by ISO-13.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 `run_child_with_proxy` is reduced to command orchestration over typed collaborators rather than direct registry/configuration implementation.
- [x] #2 A single RAII execution-session type owns the live proxy, temporary Pixi configuration, and cleanup behavior on every error and success path.
- [x] #3 A typed child-environment contribution interface composes Cargo, Pixi, PyPI/uv, npm, and generic proxy variables without a monolithic chained `Command::env` block.
- [x] #4 Cargo home generation and Pixi mirror generation have independent unit tests and structured error types.
- [x] #5 The existing ergonomic `isolock exec <command>` behavior and exit-code propagation remain covered by CLI tests.
<!-- AC:END -->

## Notes

- Addresses clean-code findings for `crates/isolock/src/main.rs` around proxy execution, package-manager configuration, and direct environment access.
- Governed by decision-7 and doc-9.

## Implementation evidence

`proxy_cli` parses/orchestrates typed collaborators; `config/execution.rs` resolves legacy
inputs through the shared Figment boundary. `ExecutionSession` owns the live proxy and
private temporary Pixi file, with typed Cargo/Pixi errors and composable child-environment
contributions. Unit tests cover generators, environment composition, exit mapping, and
Drop/unwinding. `execution-sessions.feature` has four executable CLI lifecycle scenarios;
CLI regressions preserve optional `--`, forwarded child `--version`, exit 7, and Unix signals.
