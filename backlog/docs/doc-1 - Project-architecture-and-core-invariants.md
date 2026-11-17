---
id: doc-1
title: Project architecture and core invariants
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Technical Architecture Findings & Core Invariants

### Executive Summary

Isolock establishes that hermetic dependency management across multi-language repositories can be achieved with zero proprietary centralized infrastructure by combining:

1. **Local Loopback Registry Interception**: Transparent HTTP proxies serving standard protocol endpoints for Conda, PyPI, NPM, and Cargo.
2. **Git Orphan Branch as Object Store (`isolock/v1`)**: Zero pollution of primary Git branches; dependencies stored as immutable blobs with conventional commit histories.
3. **Pure POSIX Shell Bootstrapping**: Reconstituting the runtime environment using only ubiquitous UNIX utilities (`/bin/sh`, `git`, `tar`, `sha256sum`), eliminating circular native binary chicken-and-egg dependencies.
4. **Read-Only CI Enforcement with Asynchronous Ingestion**: PRs fail closed if lockfiles drift, providing deterministic `gh workflow run` dispatch commands to mirror missing artifacts.

### Architecture Matrix of System Capabilities

| Capability | Specification Invariant | Key Mechanism | Target Rust Crate |
| :--- | :--- | :--- | :--- |
| Content-Addressed Storage | SHA-256 Digest Addressability | Sharded hierarchy `blobs/sha256/<h[:2]>/<h[2:4]>/<h>/<file>` | `isolock-cas` |
| Blob Sharding | Large Archive Partitioning | >95 MiB chunks (`.part000`, etc.), POSIX `cat` reassembly | `isolock-cas` |
| Git Orphan Branch | Decentralized Object Storage | `git commit-tree` / `update-ref` without checkout | `isolock-core`, `isolock-cas` |
| Epoch Rollover | Bounded Shallow Clones | Rollover to `isolock/v2` when branch exceeds 1,500 MiB | `isolock-cas` |
| PyPI Proxy (PEP 691) | Simple Repository JSON API | In-memory PEP 691 JSON Simple Index synthesis with sha256 | `isolock-proxy` |
| Conda Channel Proxy | Isolated Channel Repodata | `repodata.json` generation from `.tar.bz2` / `.conda` metadata | `isolock-proxy` |
| NPM Registry Proxy | Packument Metadata Serving | JSON packuments with sha512 integrity hashes | `isolock-proxy` |
| Cargo Sparse Registry | RFC 2789 Sparse Index | Sharded path index hierarchy and `config.json` | `isolock-proxy` |
| Lockfile Plan Drift | CI Airlock Verification | Lockfile diffing and `gh workflow run` command output | `isolock-plan` |
| POSIX Bootstrap | Zero-Binary Restoration | Pure shell script unpacking and activating CAS blobs | `isolock` / scripts |
| Ergonomic Exec | Transparent Proxy Invocation | `isolock exec <cmd> [args...]` without `--` | `isolock` (CLI) |
