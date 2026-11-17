---
id: decision-2
title: Pure POSIX shell bootstrapping
date: '2026-11-17 11:00'
status: accepted
---

## Context

To run compiled binaries on clean environments (CI runners, air-gapped workstations, Docker containers), a bootstrap mechanism is required. Distributing native bootstrap executables requires precompiled cross-platform binaries and raises security verification challenges.

## Decision

The environment bootstrap script (`.isolock/bootstrap.sh`) must be implemented in POSIX shell without requiring any preinstalled languages or binary runtimes.

1. Relies solely on `/bin/sh`, `git`, `tar`, `sha256sum`, and standard UNIX tools.
2. Extracts and reassembles CAS blobs directly into the target environment.
3. Emits a standard `activate` script that can be sourced by developer shells.

## Consequences

- **Positive**: Runs out-of-the-box on Linux distributions, macOS, and container images.
- **Positive**: Auditable single-file shell script that can be reviewed for compliance.
- **Negative**: Script logic must remain POSIX-compliant without Bashisms.
