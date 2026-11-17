# Isolock

[![CI](https://github.com/Archont561/isolock/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Archont561/isolock/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-Fumadocs-6e56cf)](https://archont561.github.io/isolock/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Rust MSRV](https://img.shields.io/badge/MSRV-1.88-000000?logo=rust&logoColor=white)](Cargo.toml)

**Hermetic dependency airlock and zero-daemon loopback proxy for Conda, PyPI, NPM, and Cargo.**

Build and test software hermetically without internet access or container daemons: resolve dependencies once, commit immutable artifacts to a Git orphan branch, and intercept native package managers through an ephemeral loopback proxy. Restore and activate full execution environments with a single pure POSIX shell script.

> Status: active development `0.1.0`. Core primitives, Content-Addressed Storage engine, and multi-ecosystem proxy adapters are implemented; protocol endpoints and CLI commands are under active milestone delivery.

| Crate / Tool | What it is | Description |
| :-- | :-- | :-- |
| [`isolock`](crates/isolock) | Main CLI binary | User-facing commands: `plan`, `cas`, `proxy`, `exec`, `bootstrap` |
| [`isolock-core`](crates/isolock-core) | Core domain types | Operational modes (`Connected`, `Ci`, `LocalAirlock`), Registry enum, and Semver validation |
| [`isolock-cas`](crates/isolock-cas) | Content-Addressed Storage | SHA-256 digest addressability, large blob sharding (>95 MiB), and POSIX `cat` reassembly |
| [`isolock-plan`](crates/isolock-plan) | Airlock plan engine | Lockfile plan diffing, immutable artifact manifest, and CI drift detection |
| [`isolock-proxy`](crates/isolock-proxy) | Loopback proxy adapter | Ephemeral HTTP proxy serving Conda repodata, PyPI PEP 691 JSON, NPM packuments, and Cargo sparse index |

## Quickstart

```sh
# Install isolock (via pixi / conda-forge or cargo)
pixi global install isolock
# or: cargo install --path crates/isolock

# Synchronize locked dependencies into the hermetic CAS plan
isolock plan --sync

# Run native builds hermetically through the loopback proxy (no double-dash needed)
isolock exec cargo build --release
isolock exec pixi run test
isolock exec npm test

# Restore an initialized airlock environment with pure POSIX shell
sh .isolock/bootstrap.sh
. .isolock/state/active/activate
```

## Develop

```sh
pixi install                      # Rust tooling and the Lefthook binary
pixi run install-hooks            # install this clone's configured Git hooks
cargo nextest run --workspace     # fast unit and integration tests
cargo test --workspace            # BDD scenarios and property tests
alint                             # repository policy and schema hygiene
cargo clippy --workspace          # strict compiler and lint verification
cargo deny check bans licenses    # supply chain and license audit
```

## Documentation

**[archont561.github.io/isolock](https://archont561.github.io/isolock/)** - project overview, architecture specifications, and getting started guides rendered via Fumadocs on pushes to `main`.

- [Architecture Documents](backlog/docs/) - CAS layout, Git orphan branch storage, loopback proxy interception, and POSIX bootstrapping
- [Architecture Decisions](backlog/decisions/) - recorded ADRs for zero-infrastructure storage, pure shell bootstrapping, and canonical crate reuse
- [Backlog & Roadmap](backlog/README.md) - delivery milestones, tasks, and acceptance criteria
- [Gherkin Contracts](backlog/features/) - executable specification scenarios for CAS, proxy, and plan verification

Contributors start with [AGENTS.md](AGENTS.md).

## License

Dual-licensed under [MIT](LICENSE) or [Apache-2.0](LICENSE).
