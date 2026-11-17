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
