# `isolock`

Main executable binary CLI for the Isolock airlock manager.

## Responsibilities

- Handles CLI dispatch for `exec`, `sync`, `plan`, `ingest`, `push`, and `serve`.
- Injects local proxy registry URLs for Cargo, Conda, PyPI, and NPM.

## Plan drift and dispatch commands

`isolock plan` compares `.isolock/plan.json` (or the path in `ISOLOCK_PLAN`) with verified blobs in `.isolock/cas` (or `ISOLOCK_CAS_DIR`). It fails closed if no plan is available. When artifacts are missing, it prints one copyable `gh workflow run isolock.yml` command per registry; `--ci` also exits non-zero so CI can gate on the drift. Generated commands require a reviewed, committed `isolock.ingest.json` with exact source, filename, and SHA-256 pins. The PR workflow skips until a remote `isolock/v1` branch or tag exists, then verifies that published snapshot's plan/CAS in an isolated directory. It does not solve project lockfiles.

## Reviewed ingestion and publication

`isolock ingest --manifest isolock.ingest.json --artifacts originals` validates the whole
selected local batch before importing originals into the configured CAS. Explicit
`--download` acquires only reviewed HTTPS sources. Neither form publishes.
`isolock push --manifest isolock.ingest.json` appends a non-force, linear storage commit
without modifying the producer checkout or index. Profile manifests publish separately
with `push --profile NAME --platform PLATFORM`, against an earlier pinned payload commit.

`push --epoch-policy FILE` adds explicit source-pinned live/history budgets and retention.
Overflow atomically creates an orphan successor plus an unchanged-tree source seal, keeping
old refs and locked profiles available. No configuration or storage identity is rewritten.
See [reviewed ingestion](../../docs/content/docs/ingestion.mdx) and
[storage epochs](../../docs/content/docs/storage-epochs.mdx) for review, retention, recovery,
cold-consumer flows, and unsupported large-blob/format-conversion/registry-resolution boundaries.

## POSIX environment bootstrap

Copy `templates/bootstrap.sh` to `.isolock/bootstrap.sh` and provide `.isolock/environments/active.manifest` plus the archive blobs under `.isolock/blobs/`. The manifest contains a lowercase SHA-256 digest and a safe tar archive basename on each line. Run `sh .isolock/bootstrap.sh` to verify and unpack the archives into `.isolock/state/active/`, then source `.isolock/state/active/activate` to prepend its `bin/` directory to `PATH`. Use `--remote` to fetch the `blobs/` and `environments/` trees from `isolock/v1`. See [the bootstrap architecture](../../backlog/docs/doc-6%20-%20Pure-posix-shell-environment-bootstrapping.md) for the layout and manifest contract.

## Verification

```sh
cargo nextest run -p isolock
cargo test -p isolock
```
