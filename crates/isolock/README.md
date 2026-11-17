# `isolock`

Main executable binary CLI for the Isolock airlock manager.

## Responsibilities

- Handles CLI dispatch for `exec`, `sync`, `plan`, `ingest`, and `serve`.
- Injects local proxy registry URLs for Cargo, Conda, PyPI, and NPM.

## Plan drift and dispatch commands

`isolock plan` compares `.isolock/plan.json` (or the path in `ISOLOCK_PLAN`) with verified blobs in `.isolock/cas` (or `ISOLOCK_CAS_DIR`). It fails closed if no plan is available. When artifacts are missing, it prints one copyable `gh workflow run isolock.yml` command per registry; `--ci` also exits non-zero so CI can gate on the drift. The PR workflow skips this audit until the remote `isolock/v1` branch or tag exists; once initialized, the plan and CAS must also be provisioned for the audit to run successfully.

## POSIX environment bootstrap

Copy `templates/bootstrap.sh` to `.isolock/bootstrap.sh` and provide `.isolock/environments/active.manifest` plus the archive blobs under `.isolock/blobs/`. The manifest contains a lowercase SHA-256 digest and a safe tar archive basename on each line. Run `sh .isolock/bootstrap.sh` to verify and unpack the archives into `.isolock/state/active/`, then source `.isolock/state/active/activate` to prepend its `bin/` directory to `PATH`. Use `--remote` to fetch the `blobs/` and `environments/` trees from `isolock/v1`. See [the bootstrap architecture](../../backlog/docs/doc-6%20-%20Pure-posix-shell-environment-bootstrapping.md) for the layout and manifest contract.

## Verification

```sh
cargo nextest run -p isolock
cargo test -p isolock
```
