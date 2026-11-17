---
id: doc-6
title: Pure POSIX shell environment bootstrapping
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Pure POSIX Shell Environment Bootstrapping

### Problem Statement

Precompiled native bootstrapping binaries (for example Go, Rust, or Python wheels) create a chicken-and-egg problem: how do you run the bootstrap tool before having a verified toolchain installed on an air-gapped machine or minimalist container?

### Architectural Solution

1. **Zero-Dependency Invariant**:
   `.isolock/bootstrap.sh` is written in standard POSIX shell (`#!/bin/sh`) and requires only:
   - `/bin/sh`
   - `git`
   - `tar`
   - `sha256sum` (with fallback to `shasum -a 256` on macOS)
   - Standard core utilities: `mkdir`, `cp`, `cat`, `rm`, `chmod`

2. **Operations Flow**:
   - Local CAS mode: extracts blobs directly from `.isolock/blobs/`.
   - Remote shallow fetch mode (`--remote`): fetches shallow commit of the orphan branch `isolock/v1`.
   - Iterates over stored blobs matching manifest specifications.
   - Reassembles any `.part*` shards with POSIX `cat`.
   - Verifies SHA-256 integrity using `sha256sum -c`.
   - Unpacks toolchain archives into `.isolock/state/<env>/`.
   - Generates an environment activation script `.isolock/state/<env>/activate`.

3. **Reproducibility**:
   Guarantees that fresh environments can be reconstituted offline without network access or preinstalled runtimes.
