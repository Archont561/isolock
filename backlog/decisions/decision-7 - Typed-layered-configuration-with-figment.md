---
id: decision-7
title: Typed layered configuration with Figment at the CLI boundary
date: '2026-11-18 10:00'
status: accepted
---

## Context

Profile selection, storage locations, runtime-provider policy, and transport controls require project configuration. Existing CLI behavior reads a small set of environment variables directly, which does not provide a typed project schema, a consistent precedence order, or value provenance for diagnostics.

At the same time, artifact identities, lockfile inputs, registry origins, and bootstrap digests are security-sensitive. Allowing arbitrary environment or user configuration to alter them after plan generation would defeat fail-closed airlock behavior.

## Decision

1. **Adopt Figment for CLI configuration loading only**. The CLI crate uses Figment with its TOML and environment providers to merge typed configuration and report value provenance. Core planning, CAS, proxy, and POSIX bootstrap crates receive already-resolved typed values and do not read process configuration directly.
2. **Use a project-owned `isolock.toml` schema**. It declares profile roots, supported platforms, runtime-provider policy, materializers, storage references, and operational defaults. `isolock config check`, `show`, and `explain` expose the resolved result and provenance.
3. **Use deterministic precedence**. Compiled defaults are overridden by user operational defaults, project configuration, project-local generated configuration, `ISOLOCK_*` variables, and explicit CLI flags in that order.
4. **Lock security-sensitive resolution inputs**. A generated profile plan records its normalized configuration digest, resolved seed-tool digest, selected lockfile digests, storage commit, and registry/channel metadata. Locked CI rejects overrides that would change these values.
5. **Keep secrets out of project configuration and plans**. Credentials remain external references or CI secrets and never appear in CAS manifests, generated plans, or `config show` output.

## Consequences

- **Positive**: Users can customize paths, profile selection, and materialization policy without source changes.
- **Positive**: Diagnostics can identify the configuration source that supplied an effective value.
- **Positive**: Planning remains reproducible because the resolved configuration is part of the plan identity.
- **Negative**: Figment's TOML provider introduces a newer TOML dependency line in the CLI until the workspace's existing TOML usage is migrated.
