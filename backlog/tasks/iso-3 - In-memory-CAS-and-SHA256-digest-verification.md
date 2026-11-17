---
id: ISO-3
title: In-memory CAS and SHA-256 digest verification
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-1
dependencies: [ISO-1]
type: task
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement Content-Addressed Storage types, lowercase 64-hex SHA-256 digest validation, and sharded path calculation (`sha256/xx/yy/...`).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 `Sha256Digest::parse` validates exact 64-character lowercase hex strings.
- [x] #2 `InMemoryCas` provides `insert`, `get`, `contains`, and `count` operations.
- [x] #3 BDD scenario tests and property tests pass in `isolock-cas`.
<!-- AC:END -->

## Notes

- 2026-11-17: Implemented in `crates/isolock-cas` with 98% test coverage and zero clippy warnings.
