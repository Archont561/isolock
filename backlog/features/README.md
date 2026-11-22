# Gherkin Feature Specifications

Executable Gherkin specifications (`.feature`) defining the behavioral contracts for Isolock. These contracts govern the implementation of capabilities inside the Rust workspace crates:

| Specification File | Capability | Target Crate |
| :--- | :--- | :--- |
| `cas-sharding.feature` | CAS Storage & Large Blob Sharding (>95M) | `crates/isolock-cas` |
| `git-orphan-branch.feature` | Git Orphan Branch & Rollover | `crates/isolock-cas` |
| `proxy-pypi.feature` | PyPI PEP 503 Simple Index Proxy | `crates/isolock-proxy` |
| `proxy-conda.feature` | Conda Repodata Channel Proxy | `crates/isolock-proxy` |
| `proxy-npm.feature` | NPM Registry Packument Proxy | `crates/isolock-proxy` |
| `proxy-cargo.feature` | Cargo RFC 2789 Sparse Index Proxy | `crates/isolock-proxy` |
| `plan-drift.feature` | Lockfile Plan Drift & CI Airlock Guard | `crates/isolock-plan` |
| `posix-bootstrap.feature` | Pure POSIX Shell Environment Bootstrap | `templates/bootstrap.sh` |
| `cli-exec.feature` | Ergonomic Subprocess Exec (No `--`) | `crates/isolock` (CLI) |
| `clifacet-routing.feature` | Generated help, usage errors, typed overrides, and opaque child invocations | `crates/isolock` (CLI) |
| `clifacet-compatibility.feature` | Legacy airlock plan verdicts and exit statuses | `crates/isolock` (CLI) |
| `profiled-environment-materialization.feature` | Profile-scoped Pixi and standalone runtime materialization | `isolock-plan`, `isolock-cas`, `isolock` (CLI), `templates/bootstrap.sh` |
| `profile-configuration.feature` | Typed project configuration, provenance, and locked override policy | `isolock` (CLI) |
| `proxy-resource-bounds.feature` | Bounded loopback proxy concurrency and shutdown behavior | `isolock-proxy` |

The profile contracts are executable in `crates/isolock/tests/profile_bdd.rs` and
`profile_safety.rs`. `profile-transport-and-safety.feature` additionally covers verified
standalone capabilities, manifest-first transfer, shared-CAS GC, seed archives, and
credential-free diagnostics. `real_profile.rs` is exercised separately by the explicit
CI profile/platform matrix using restored tools and no registry downloads.
