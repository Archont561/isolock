---
id: decision-4
title: Ecosystem index generation and canonical crate reuse
date: '2026-11-17 12:30'
status: accepted
---

## Context

To serve hermetic dependencies via the local loopback proxy, Isolock must synthesize registry index feeds for Conda (`repodata.json`), Cargo (RFC 2789 sparse index lines), PyPI (PEP 691 JSON), and NPM (packuments). All index feeds are served as JSON with zero HTML payloads.

We evaluated whether to integrate full registry indexing engines (such as `rattler-index`) versus custom implementations or targeted canonical type crates.

## Decision

1. **Reject heavy offline indexing engines (`rattler-index`)**:
   `rattler-index` requires filesystem directory hierarchies (`<channel>/<subdir>/*.conda`) or S3 buckets via OpenDAL, pulls in extensive runtimes (OpenDAL, S3, Parquet, reqwest), and is designed for offline batch publishing rather than dynamic in-memory proxy serving.

2. **Reuse canonical typed domain crates**:
   - **Conda**: Adopt `rattler_conda_types` for strongly typed `RepoData`, `PackageRecord`, and platform constraints, avoiding reimplementation of Conda version ordering and repodata specifications.
   - **Cargo**: Adopt `tame-index` for RFC 2789 sparse index path hashing (`1/a`, `ab/cd/abcd`), metadata serialization, and config handling.
   - **PyPI & NPM**: Use lightweight `serde` and `serde_json` structures for PEP 691 JSON Simple API responses (serving zero HTML) and NPM packuments.

3. **Internal architectural reuse via `RegistryAdapter`**:
   Introduce a unified `RegistryAdapter` trait in `isolock-proxy`. All common proxy concerns—TCP listener binding, request routing, CAS streaming, Range requests, and airlock policy evaluation—are implemented once and reused across all four ecosystems.

4. **Plan-driven dynamic index synthesis**:
   Generate index feeds on demand directly from `AirlockPlan`. Because the locked plan defines the exact dependencies, indices require no external database, produce zero drift, and ensure hermetic isolation.

## Consequences

- **Positive**: High code reuse without dependency bloat or unneeded cloud/network runtime overhead.
- **Positive**: Index responses are guaranteed in-sync with the airlock plan with zero cache drift.
- **Positive**: Loopback proxy boots instantly with zero disk re-indexing passes.
- **Negative**: Transitive dependencies from canonical crates require strict license and advisory verification via `cargo deny`.
