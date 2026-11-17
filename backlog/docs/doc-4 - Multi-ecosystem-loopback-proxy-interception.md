---
id: doc-4
title: Multi-ecosystem loopback proxy interception
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 14:00'
---

## Multi-Ecosystem Loopback Proxy Interception

### Problem Statement

Different package managers (pip, uv, pixi, conda, npm, bun, pnpm, yarn, cargo) use distinct metadata formats and wire protocols. Forcing developer tools to change or using non-standard file URLs breaks lockfiles and checksum verification.

### Architectural Solution

Isolock provisions an ephemeral, loopback HTTP proxy listening on `127.0.0.1:<port>`. It serves four distinct ecosystem endpoints concurrently:

#### 1. PyPI Simple Index (PEP 691 JSON)

- Route: `/pypi/simple/<package>/`
- Serves standard PEP 691 JSON (`application/vnd.pypi.simple.v1+json` or `application/json`) with files, `sha256` hashes, and `requires-python`.
- HTML serving is explicitly omitted across all proxy endpoints; modern tools (such as uv and pip >= 22.2) consume PEP 691 JSON directly.
- Downloads routed to local CAS blobs.

#### 2. Conda Channel Repodata

- Route: `/conda/channel/<subdir>/repodata.json` (for example `linux-64`, `noarch`, `osx-arm64`)
- Reads metadata directly from `.conda` and `.tar.bz2` (`info/index.json`) or `<file>.meta.json`.
- Synthesizes dynamic `repodata.json` using `rattler_conda_types::RepoData` populated directly from the hermetic `AirlockPlan`.
- Avoids heavy batch indexers like `rattler-index` in favor of in-memory model serialization.

#### 3. NPM Registry Packuments

- Route: `/npm/<package>` (including scoped packages `/npm/@scope%2Fpkg`)
- Extracts `package/package.json` from `.tgz` archives.
- Synthesizes NPM packument JSON specifying `dist.tarball` URL, `dist.shasum` (SHA-1), and `dist.integrity` (`sha512-...`).

#### 4. Cargo Sparse Index (RFC 2789)

- Route: `/cargo/config.json` defining download endpoint `http://127.0.0.1:<port>/cargo/api/v1/crates/{crate}/{version}/download`.
- Sharded metadata paths:
  - 1 char: `/cargo/1/a`
  - 2 chars: `/cargo/2/ab`
  - 3 chars: `/cargo/3/a/abc`
  - 4+ chars: `/cargo/ab/cd/abcd`
- Emits RFC 2789 JSON lines using `tame-index` crate primitives extracted from `.crate` metadata in the plan.

### Brotli Content Encoding for Registry Metadata

As established in `decision-5`, `isolock-proxy` provides transparent HTTP compression for textual index endpoints:

- **Negotiation**: When client requests supply `Accept-Encoding: br` (standard in `pip`, `uv`, `pixi`), the proxy compresses synthesized JSON feeds using the pure-Rust `brotli` engine.
- **Header**: Emits `Content-Encoding: br` alongside canonical ecosystem MIME types.
- **Compression Ratio**: Yields 75% to 90% reduction in index transfer volume.
- **Binary Boundary**: Binary packages (`.whl`, `.conda`, `.tgz`, `.crate`) are streamed directly from CAS without re-compression, preventing double-compression latency and preserving upstream SHA-256 byte parity.

### Architecture and Maximal Code Reuse

As established in `decision-4`, `isolock-proxy` decouples network handling from ecosystem metadata via the `RegistryAdapter` trait:

- Loopback TCP listener, Range request parsing, CAS blob streaming, Brotli compression, and fail-closed airlock checks are implemented once.
- Individual registry adapters (`CondaAdapter`, `PypiAdapter`, `NpmAdapter`, `CargoAdapter`) only implement path matching and index synthesis.
