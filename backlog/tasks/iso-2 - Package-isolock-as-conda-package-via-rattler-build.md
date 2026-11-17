---
id: ISO-2
title: Package isolock as conda package via rattler-build
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-0
dependencies: [ISO-1]
type: task
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Author modern Conda recipe in `crates/isolock/recipe.yaml` and verify that `rattler-build` and `pixi build` compile the package and pass test verification.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 `recipe.yaml` and `recipe.yml` specify schema version 1 with isolated build requirements.
- [x] #2 `rattler-build build --recipe crates/isolock/recipe.yaml` produces `.conda` package.
- [x] #3 Automated verification test executes `isolock --version` inside the isolated test environment.
<!-- AC:END -->

## Notes

- 2026-11-17: Verified with rattler-build 0.80.2 and pixi build; `isolock-0.1.0-hb0f4dca_0.conda` built and passed post-build test verification.
