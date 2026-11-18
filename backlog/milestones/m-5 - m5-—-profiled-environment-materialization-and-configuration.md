---
id: m-5
title: "M5 - Profiled environment materialization and configuration"
---

## Description

Deliver profile-scoped plans that materialize only the runtime, package-manager artifacts, and registry views selected for a platform. Establish a typed project configuration boundary so standalone tools, Pixi/Conda environments, and non-Conda runtime profiles are explicit and reproducible.

## Exit criteria

- [ ] A project can declare named Pixi, standalone, and composed profiles in `isolock.toml`.
- [ ] `isolock plan --profile <name> --platform <platform>` emits a deterministic profile closure and records the resolved configuration digest.
- [ ] A Rust Pixi profile provisions only a verified seed Pixi binary, its locked Conda closure, and its Cargo vendor closure; unrelated Python and TypeScript artifacts are excluded.
- [ ] Isolock can materialize a locked Pixi environment offline from a CAS-derived local channel while preserving standalone Isolock and Pixi seed tools in `.isolock/bin`.
- [ ] A standalone Rust profile and a Pixi Rust profile can coexist, share an identical Cargo vendor payload, and retain distinct runtime provenance.
- [ ] CI selects profiles explicitly and fails if a locked profile is altered by an unrecorded configuration override.

## Status

**Planned**.

## Tasks

- **ISO-13**: Typed configuration schema and provenance-aware CLI loading.
- **ISO-14**: Profile-scoped lockfile planning and immutable manifests.
- **ISO-15**: Verified standalone seed-tool provisioning.
- **ISO-16**: Offline Pixi/Conda profile materialization.
- **ISO-17**: Standalone runtime materializers and shared payloads.
- **ISO-18**: Selective profile transport, CI integration, and garbage-collection roots.
