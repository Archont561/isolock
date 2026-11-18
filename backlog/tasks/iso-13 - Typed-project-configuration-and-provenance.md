---
id: ISO-13
title: Typed project configuration and provenance-aware CLI loading
status: Done
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
- [x] #1 `isolock.toml` has a versioned typed schema with denied unknown fields and explicit profile declarations.
- [x] #2 Figment merges compiled defaults, user operational configuration, project configuration, generated project-local configuration, `ISOLOCK_*` variables, and CLI flags deterministically.
- [x] #3 `isolock config check`, `show`, and `explain <path>` validate, render redacted effective values, and identify their provider.
- [x] #4 Core planner APIs accept a resolved typed request and do not read process environment variables.
- [x] #5 Secrets are absent from plans, manifests, diagnostics, and configuration rendering.
<!-- AC:END -->

## Notes

- Governed by decision-7 and doc-9.
- Figment is intentionally a CLI-bound dependency; the static POSIX bootstrap script continues to consume generated simple manifests.

## Implementation evidence

Typed CLI-only Figment loading, platform-aware schema, sanitized rendering/provenance, and override rejection are exercised by profile_bdd.rs and profile_safety.rs.

See `docs/content/docs/profiles.mdx` for the implemented schema, workflows, supported archive/lock inputs, and explicit fail-closed boundaries. Local validation includes the offline workspace/Nextest suites, strict Clippy, cargo-deny, repository/shell/workflow linting, and both real-tool integration cases.
