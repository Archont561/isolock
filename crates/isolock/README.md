# `isolock`

Main executable binary CLI for the Isolock airlock manager.

## Responsibilities

- Handles CLI dispatch for `exec`, `sync`, `plan`, `ingest`, and `serve`.
- Injects local proxy registry URLs for Cargo, Conda, PyPI, and NPM.

## Plan drift and dispatch commands

`isolock plan` compares `.isolock/plan.json` (or the path in `ISOLOCK_PLAN`) with verified blobs in `.isolock/cas` (or `ISOLOCK_CAS_DIR`). It fails closed if no plan is available. When artifacts are missing, it prints one copyable `gh workflow run isolock.yml` command per registry; `--ci` also exits non-zero so CI can gate on the drift.

## Verification

```sh
cargo nextest run -p isolock
cargo test -p isolock
```
