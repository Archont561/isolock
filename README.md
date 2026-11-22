# Isolock

[![CI](https://github.com/Archont561/isolock/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Archont561/isolock/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-Fumadocs-6e56cf)](https://archont561.github.io/isolock/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![Rust MSRV](https://img.shields.io/badge/MSRV-1.88-000000?logo=rust&logoColor=white)](Cargo.toml)

**Hermetic dependency airlock and zero-daemon loopback proxy for Conda, PyPI, NPM, and Cargo.**

Build and test software hermetically without internet access or container daemons: resolve dependencies once, commit immutable artifacts to a Git orphan branch, and intercept native package managers through an ephemeral loopback proxy. Restore and activate full execution environments with a single pure POSIX shell script.

> Status: active development `0.1.0`. M5 provides locked profile planning and offline materialization. M6 adds scoped CLI sessions, extendable registry composition, bounded proxy workers, and selected strict Clippy gates. M7 adds reviewed ingestion, explicit linear publication, and source-pinned atomic epoch rollover with locked-profile retention. Registry resolution and automatic legacy/shard conversion remain outside that scope. See [ingestion](docs/content/docs/ingestion.mdx), [storage epochs](docs/content/docs/storage-epochs.mdx), the [profile guide](docs/content/docs/profiles.mdx), [proxy limits](docs/content/docs/proxy.mdx), and [quality policy](docs/content/docs/quality.mdx) for supported behavior and explicit boundaries.

| Crate / Tool | What it is | Description |
| :-- | :-- | :-- |
| [`isolock`](crates/isolock) | Main CLI binary | User-facing commands: `ingest`, `push`, `config`, `plan`, `fetch`, `sync`/`bootstrap`, `export`, `seed-manifest`, `gc`, `exec`, `serve` |
| [`isolock-core`](crates/isolock-core) | Core domain types | Operational modes (`Connected`, `Ci`, `LocalAirlock`), Registry enum, and Semver validation |
| [`isolock-cas`](crates/isolock-cas) | Content-Addressed Storage | SHA-256 digest addressability, large blob sharding (>95 MiB), and POSIX `cat` reassembly |
| [`isolock-plan`](crates/isolock-plan) | Airlock plan engine | Lockfile plan diffing, immutable artifact manifest, and CI drift detection |
| [`isolock-proxy`](crates/isolock-proxy) | Loopback proxy adapter | Ephemeral HTTP proxy serving Conda repodata, PyPI PEP 691 JSON, NPM packuments, and Cargo sparse index |

## Quickstart

Prepare reviewed `isolock.toml` profile intent and original artifacts as described in the
[profile guide](docs/content/docs/profiles.mdx). Populate the CAS with
`isolock ingest --manifest isolock.ingest.json --artifacts originals`;
[publication](docs/content/docs/ingestion.mdx) is a separate explicit step. Then:

```sh
isolock config check
isolock config explain storage.cas-dir
isolock plan --profile rust-pixi --platform linux-64
isolock seed-manifest --profile rust-pixi --platform linux-64
isolock fetch --profile rust-pixi --platform linux-64
isolock plan --profile rust-pixi --platform linux-64 --ci
isolock sync --profile rust-pixi --platform linux-64 --locked --offline
. .isolock/activate/rust-pixi-linux-64
cargo build --locked --offline

# Bootstrap uses reviewed seed manifests, not a preinstalled Pixi environment:
sh .isolock/bootstrap.sh --profile rust-pixi --platform linux-64 --offline
```

`sync` always requires a reviewed plan. It never obtains its seed Pixi from the prefix
that Pixi creates. Directory transport is selective; the Git fallback explicitly reports
its broader snapshot transfer. The legacy loopback proxy and positional environment
bootstrap remain available for existing airlock plans.

## Develop

```sh
bash scripts/restore.sh  # restore the reviewed developer tooling/Cargo vendor sandbox
export PATH="$HOME/.local/bin:$PATH"
pixi run --as-is --offline cargo nextest run --workspace --offline
pixi run --as-is --offline cargo test --workspace --offline
pixi run --as-is --offline cargo clippy --workspace --all-targets --offline -- -D warnings
pixi run --as-is --offline cargo deny --offline check bans licenses sources
pixi run --as-is --offline cargo fmt --check
pixi run --as-is --offline sh scripts/coverage.sh
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
