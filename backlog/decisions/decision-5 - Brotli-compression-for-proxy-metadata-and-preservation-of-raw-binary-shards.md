---
id: decision-5
title: Brotli compression for proxy metadata and preservation of raw binary shards
date: '2026-11-17 14:00'
status: accepted
---

## Context

Package manager index feeds (PyPI PEP 691 JSON, Conda `repodata.json`, NPM packuments, and Cargo sparse index lines) represent large textual payloads ranging from several megabytes to tens of megabytes. Modern package managers (`pip`, `uv`, `pixi`, `npm`) advertise `Accept-Encoding: br, gzip, zstd` during index lookups.

We evaluated whether to apply Brotli compression across two distinct layers:

1. Binary dependency artifacts and large blob shards (`.part*`).
2. HTTP loopback proxy registry metadata responses.

## Decision

1. **Reject Brotli compression for binary shards (`isolock-cas`)**:
   - Preserves the **zero-dependency POSIX `cat` invariant** (`decision-2` and `doc-2`). If shards were compressed with Brotli, reconstruction on an air-gapped machine would require a preinstalled `brotli` binary, which is absent from standard minimal containers (`alpine`, `debian:slim`) and base operating systems.
   - Avoids the **double-compression penalty**: upstream packages (Python `.whl`, Conda `.conda`, NPM `.tgz`, Cargo `.crate`) are already compressed with DEFLATE, Zstandard, or gzip. Compressing high-entropy archives with Brotli yields negligible byte savings while wasting CPU cycles.
   - Uses native Git compression: Git already compresses staged objects using zlib when committed to `isolock/v1`. Binary shards remain raw byte slices reassemblable via `cat`.

2. **Adopt Brotli for loopback proxy metadata responses (`isolock-proxy`)**:
   - Implement Brotli compression (`Content-Encoding: br`) using the pure-Rust `brotli` crate (`brotli = "9.1"`).
   - Compress synthesized JSON index payloads (PyPI PEP 691 simple index, Conda repodata, NPM packuments, Cargo sparse index lines) when the client request header contains `Accept-Encoding: br`.
   - Deliver 75% to 90% reduction in index transfer size and memory footprint without altering underlying CAS storage formats.

## Consequences

- **Positive**: Loopback proxy index transit latency and memory consumption drop significantly for modern package managers (`uv`, `pip`, `pixi`).
- **Positive**: Zero-infrastructure environment bootstrapping via pure POSIX `cat` remains completely intact.
- **Positive**: Pure-Rust `brotli` library introduces zero C/C++ build dependencies and satisfies all `deny.toml` license requirements.
- **Negative**: Dynamic compression requires CPU time during index synthesis, mitigated by caching compressed representations of immutable plans.
