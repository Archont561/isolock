---
id: ISO-13
title: Typed project configuration and provenance-aware CLI loading
status: To Do
assignee: []
created_date: '2026-11-18 10:00'
labels: []
milestone: m-5
dependencies: [ISO-10]
type: task
ordinal: 13000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define versioned `isolock.toml` configuration types and load them in the CLI through Figment's TOML and environment providers. Keep configuration loading outside core planning, CAS, proxy, and POSIX shell code. Expose commands that validate, render, and explain the resolved configuration and its value provenance.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [ ] #1 `isolock.toml` has a versioned typed schema with denied unknown fields and explicit profile declarations.
- [ ] #2 Figment merges compiled defaults, user operational configuration, project configuration, generated project-local configuration, `ISOLOCK_*` variables, and CLI flags deterministically.
- [ ] #3 `isolock config check`, `show`, and `explain <path>` validate, render redacted effective values, and identify their provider.
- [ ] #4 Core planner APIs accept a resolved typed request and do not read process environment variables.
- [ ] #5 Secrets are absent from plans, manifests, diagnostics, and configuration rendering.
<!-- AC:END -->

## Notes

- Governed by decision-7 and doc-9.
- Figment is intentionally a CLI-bound dependency; the static POSIX bootstrap script continues to consume generated simple manifests.
