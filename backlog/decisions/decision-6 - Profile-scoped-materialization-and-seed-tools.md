---
id: decision-6
title: Profile-scoped materialization and explicit seed tools
date: '2026-11-18 10:00'
status: accepted
---

## Context

A repository can define several independent development and CI environments. For example, Rust, Python, and TypeScript may be separate Pixi environments, while Cargo or Python may also be provisioned as standalone runtimes. Restoring every artifact in every lockfile makes a narrow Rust job unnecessarily large and obscures its supply-chain closure.

Pixi introduces a bootstrap cycle: Pixi is needed to install the Conda environment that might otherwise provide Pixi. Standalone Isolock similarly must remain usable even when it is not installed as a Conda package in a project environment.

## Decision

1. **Plan from explicit profile roots**. A profile declares its platform, lockfile inputs, runtime providers, and materializers. Isolock resolves only the transitive closure reachable from selected roots.
2. **Keep seed tools outside package-manager prefixes**. A verified standalone Isolock binary and a verified platform-specific Pixi binary are provisioned under `.isolock/bin/` when required. Neither is obtained by the Pixi environment it initializes.
3. **Treat Conda packages as CAS payloads, not executable prefixes**. Raw Conda archives and repodata enter CAS; Isolock derives a local channel view, and seed Pixi links the selected closure into `.pixi/envs/<environment>`.
4. **Keep standalone runtimes distinct from Conda runtimes**. A standalone Rust distribution and a Conda Rust package may provide the same capability but retain separate provenance and artifact identities. They can share a Cargo vendor payload only when its complete derivation inputs match.
5. **Express reachability with immutable manifests**. Adding a profile updates profile manifests and catalog metadata; it never mutates an existing CAS object. A composed profile may reference multiple provider-specific manifests.

## Consequences

- **Positive**: A Rust-only job imports neither Python nor JavaScript artifacts unless its declared profile requires them.
- **Positive**: The bootstrap dependency cycle is broken without treating Pixi as a host prerequisite.
- **Positive**: One CAS deduplicates shared payloads while preserving artifact provenance and profile policy.
- **Negative**: Isolock must implement backend-specific materializers instead of pretending that all ecosystems are Conda prefixes.
- **Negative**: Selective profile installation does not automatically imply selective Git transfer; transport needs its own implementation.
