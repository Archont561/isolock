---
id: ISO-1
title: Scaffold modular domain crates and workspace
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-0
dependencies: []
type: task
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Scaffold modular `isolock-*` domain crates with strict workspace lints, BDD specifications, and property testing.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Four domain crates (`isolock-core`, `isolock-cas`, `isolock-plan`, `isolock-proxy`) and CLI binary (`isolock`) are created.
- [x] #2 BDD Gherkin scenarios (`rstest-bdd`) and property tests (`proptest`) pass across all crates.
- [x] #3 All governance rules in `alint` pass cleanly.
<!-- AC:END -->

## Notes

- 2026-11-17: All crates scaffolded, 21 tests pass across 14 binaries, line coverage is 92.39%, and all 48 alint rules pass.
