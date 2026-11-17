---
id: doc-5
title: Lockfile plan drift and CI airlock guard
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Lockfile Plan Drift and Asynchronous CI Airlock Guard

### Problem Statement

When developers change dependency manifests (`requirements.txt`, `pixi.lock`, `package-lock.json`, `Cargo.lock`), direct downloads from upstream registries introduce supply chain risks, non-deterministic builds, and network dependencies. Conversely, blocking developers without an automated ingestion path causes friction.

### Architectural Solution

1. **`isolock plan` Verification**:
   - Parses target lockfiles across all supported package ecosystems.
   - Cross-references requested package names, versions, and digests against CAS blobs present locally and on the remote orphan branch.

2. **Read-Only CI Guard (`isolock plan --ci`)**:
   - Executes inside pull request CI workflows.
   - Fails closed with exit code `1` if any dependency is absent from the airlock.

3. **Automated Dispatch Ingestion Command**:
   When drift is detected, Isolock formats the exact GitHub CLI command:

   ```sh
   gh workflow run isolock.yml -f registry=pypi -f packages="fastapi==0.115.0 pydantic==2.9.2"
   ```

4. **Decoupled Workflow Ingestion**:
   - The GitHub Actions workflow `isolock.yml` downloads, hashes, and commits the requested packages to `isolock/v1`.
   - The developer runs `isolock sync` locally to pull the verified patch commits into their workstation CAS without touching `main`.

### Lockfile Schema Specification (`isolock.lock`)

Airlock execution plans are serialized deterministically to `.isolock/isolock.lock` in TOML format:

```toml
# isolock.lock - Hermetic airlock lockfile schema v1
version = 1

[[artifacts]]
name = "requests"
version = "2.32.3"
registry = "pypi"
filename = "requests-2.32.3-py3-none-any.whl"
sha256 = "d1a8e2..."
size = 472190

[[artifacts]]
name = "python"
version = "3.12.5"
registry = "conda"
filename = "python-3.12.5-h55ee38b_0_cpython.conda"
sha256 = "7bf2a4..."
size = 31201984
```
