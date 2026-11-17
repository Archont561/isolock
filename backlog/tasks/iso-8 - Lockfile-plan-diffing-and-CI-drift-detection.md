---
id: ISO-8
title: Lockfile plan diffing and CI drift detection
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-3
dependencies: [ISO-1]
type: task
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement `AirlockPlan`, planned artifact representations, and `check_plan_drift` diffing in `isolock-plan`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Planned artifact identifies name, version, registry, and SHA-256 digest.
- [x] #2 Plan diffing detects missing dependencies and checksum mismatches.
- [x] #3 BDD scenario tests and property tests pass in `isolock-plan`.
<!-- AC:END -->

## Notes

- 2026-11-17: Implemented in `crates/isolock-plan` with BDD and property testing suites.
