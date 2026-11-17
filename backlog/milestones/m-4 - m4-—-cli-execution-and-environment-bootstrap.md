---
id: m-4
title: "M4 - CLI execution and environment bootstrap"
---

## Description

Deliver ergonomic CLI command execution without `--` and pure POSIX shell runtime environment bootstrapping.

Exit condition: `isolock exec <cmd> [args...]` forwards subprocess calls with injected registry environments; `.isolock/bootstrap.sh` reconstitutes runtimes offline.

## Exit criteria

- [x] Subprocess execution preserves command arguments and propagates child exit codes without requiring `--`.
- [x] Pure POSIX shell bootstrap script unpacks and activates toolchain environments using standard UNIX utilities.

## Status

**Complete**. CLI execution (`ISO-10`) and the portable shell bootstrap template (`ISO-11`) are implemented and tested.

## Tasks

- **ISO-10**: Ergonomic exec subcommand without double dash.
- **ISO-11**: Pure POSIX shell environment bootstrap template.
