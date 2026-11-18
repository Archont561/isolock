---
id: doc-8
title: Profiled environment materialization and runtime providers
type: architecture
created_date: '2026-11-18 10:00'
updated_date: '2026-11-18 10:00'
---

## Profiled Environment Materialization and Runtime Providers

### Problem Statement

A project can contain several independent environment needs: a Rust build, a Python test suite, a TypeScript toolchain, and CI-only quality tooling. A single union of every lockfile is expensive, obscures the dependency boundary of each job, and prevents small offline restores. The installation mechanism also differs by provider: Pixi links a Conda prefix, while Cargo, pip, Bun, npm, pnpm, and Yarn consume ecosystem-specific sources, caches, or materialized trees.

### Profile Roots

An Isolock profile is the explicit root set used to build a plan. It declares a platform, lockfile inputs, selected Pixi environment when applicable, runtime providers, and materializers. Example profiles are `rust-pixi`, `rust-standalone`, `python`, `ts`, and a composed `ci` profile.

A selected profile produces an immutable manifest containing only its reachable artifacts. A Rust Pixi profile includes its seed Pixi binary, the packages selected for the `rust` environment in `pixi.lock`, and its Cargo dependency closure. It excludes Python wheels and npm artifacts unless those are reachable from the profile's declared roots.

### Seed Tools and CAS Payloads

Seed tools break package-manager bootstrap cycles. Isolock and Pixi are platform-specific verified executables under `.isolock/bin/`; their source may be a host installation under a permissive local policy or a verified bootstrap archive under a locked policy. Seed Pixi is used to construct a Conda prefix and is therefore never obtained from that prefix.

CAS retains original bytes and provenance:

```text
.isolock/
├── bin/                         # standalone verified seed tools
├── cas/sha256/                  # immutable source artifacts
├── channels/conda/              # derived local channel and repodata view
├── vendor/cargo/<input-digest>/ # derived Cargo vendor materialization
└── plans/<profile>-<platform>.lock
```

Conda archives remain CAS objects. Isolock derives a local Conda channel from their package records and `repodata.json`; seed Pixi performs the normal package linking operation into `.pixi/envs/<environment>`. This preserves Conda package metadata and activation semantics instead of directly untarring package archives into an arbitrary prefix.

### Provider-specific Materialization

| Provider | Runtime location | Locked payload materialization |
| --- | --- | --- |
| Pixi/Conda | `.pixi/envs/<environment>/bin` | local Conda channel plus `pixi install --locked --offline` |
| Standalone Rust | `.isolock/state/<profile>/bin` | Rust distribution plus Cargo vendor/local registry |
| Standalone Python | `.isolock/state/<profile>/bin` | Python runtime/venv plus PyPI wheelhouse |
| Bun/Node | `.isolock/state/<profile>/bin` or Pixi prefix | runtime plus restored `node_modules` or local npm registry |

An executable capability is not enough to make artifacts interchangeable. A Conda Rust package and a standalone Rust distribution retain their own registry provenance, byte digest, and materializer even if both expose `cargo` and `rustc`.

### Shared Payloads and Composition

The CAS deduplicates bytes globally. Derived payloads such as a Cargo vendor tree can be shared only when all derivation inputs match: Cargo.lock digest, Cargo version/vendor policy, target platform and triple, and feature/target selection policy. Profiles reference the common digest; they do not copy or mutate it.

A composed profile references constituent manifests. For example, `rust-all` may contain both the Pixi and standalone Rust runtime closures while sharing one Cargo vendor payload. Adding a new profile changes catalog and manifest reachability metadata only; existing CAS objects remain immutable.

### Transport

Profile-selective planning and profile-selective transfer are separate guarantees. A storage branch may contain a union of artifacts for many profiles. To avoid downloading unrelated blobs, bootstrap must use a profile manifest plus a selective transport mechanism such as partial Git clone with lazy blob retrieval, profile-scoped storage refs, or a CAS HTTP endpoint. Until that mechanism exists, Isolock may select a narrow local installation while still fetching a broader storage snapshot.
