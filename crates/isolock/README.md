# `isolock`

Main executable binary CLI for the Isolock airlock manager.

## Responsibilities

- Handles CLI dispatch for `exec`, `sync`, `plan`, `ingest`, and `serve`.
- Injects local proxy registry URLs for Cargo, Conda, PyPI, and NPM.

## Verification

```sh
cargo nextest run -p isolock
cargo test -p isolock
```
