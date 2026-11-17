---
id: doc-7
title: Ergonomic command execution without double dash
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Ergonomic Command Execution Without Double Dash

### Problem Statement

Many proxy and container tools require a double dash delimiter:
`tool exec -- python script.py`
This syntax is un-ergonomic, breaks shell script aliases, and disrupts standard tool invocations.

### Architectural Solution

1. **Direct Argv Parsing**:
   `isolock exec <cmd> [args...]` captures all arguments following `exec` without requiring `--`.

2. **Dynamic Environment Variable Injection**:
   The proxy allocates an ephemeral loopback port and sets package manager registry environment variables for the child process.

3. **Transparent Subprocess Forwarding**:
   Streams stdin, stdout, and stderr directly, capturing and returning the exact child exit code.

### Environment Variable Injection Matrix

When `isolock exec` spawns a child command, it starts an ephemeral proxy listener on `127.0.0.1:<port>` and populates the child environment:

- **PyPI / pip / uv / poetry**:
  - `PIP_INDEX_URL=http://127.0.0.1:<port>/pypi/simple/`
  - `UV_INDEX_URL=http://127.0.0.1:<port>/pypi/simple/`
- **Conda / Pixi / Mamba**:
  - `CONDA_OVERRIDE_CHANNELS=true`
  - `CONDA_CHANNELS=http://127.0.0.1:<port>/conda/channel`
  - `PIXI_CONDA_CHANNELS=http://127.0.0.1:<port>/conda/channel`
- **NPM / Bun / Yarn / PNPM**:
  - `npm_config_registry=http://127.0.0.1:<port>/npm/`
  - `NPM_CONFIG_REGISTRY=http://127.0.0.1:<port>/npm/`
  - `BUN_INSTALL_REGISTRY=http://127.0.0.1:<port>/npm/`
  - `YARN_NPM_REGISTRY_SERVER=http://127.0.0.1:<port>/npm/`
- **Cargo (Rust)**:
  - `CARGO_REGISTRIES_ISOLOCK_INDEX=sparse+http://127.0.0.1:<port>/cargo/`
  - `CARGO_REGISTRY_DEFAULT=isolock`
