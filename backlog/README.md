# Isolock Backlog & Architectural Specification Blueprint

This directory contains the formal project backlog, capability specifications, architectural documents, decision records, milestones, and development tasks for the `isolock` workspace.

---

## 1. Directory Structure

```text
backlog/
├── config.yml                 # Backlog project configuration (task prefix: ISO)
├── milestones/                # Delivery milestones grouping tasks
│   ├── m-0 - m0-—-repository-foundation-and-governance.md
│   ├── m-1 - m1-—-cas-engine-and-blob-storage.md
│   ├── m-2 - m2-—-multi-ecosystem-proxy-resolvers.md
│   ├── m-3 - m3-—-lockfile-plan-drift-and-ci-airlock.md
│   ├── m-4 - m4-—-cli-execution-and-environment-bootstrap.md
│   ├── m-5 - m5-—-profiled-environment-materialization-and-configuration.md
│   ├── m-6 - m6-—-code-quality-and-proxy-resilience.md
│   ├── m-7 - m7-—-reviewed-ingestion-and-storage-publication.md
│   └── m-8 - m8-—-0.1-release-validation-and-product-acceptance.md
├── tasks/                     # Task files tracking work items
│   └── iso-1 through iso-29
├── docs/                      # Architectural documentation
│   └── doc-1 through doc-9
├── decisions/                 # Architecture Decision Records (ADRs)
│   └── decision-1 through decision-7
└── features/                  # Executed contracts and tagged deferred specifications
```

---

## 2. Milestones & Task Allocations

| Milestone | Title | Tasks | Status |
| :--- | :--- | :--- | :--- |
| **m-0** | Repository Foundation and Governance | `ISO-1`, `ISO-2`, `ISO-12` | Complete |
| **m-1** | CAS Engine and Blob Storage | `ISO-3`, `ISO-4` | Complete |
| **m-2** | Multi-Ecosystem Proxy Resolvers | `ISO-5`, `ISO-6`, `ISO-7` | Complete |
| **m-3** | Lockfile Plan Drift and CI Airlock | `ISO-8`, `ISO-9` | Complete |
| **m-4** | CLI Execution and Environment Bootstrap | `ISO-10`, `ISO-11` | Complete |
| **m-5** | Profiled Environment Materialization and Configuration | `ISO-13` through `ISO-18` | Complete |
| **m-6** | Code Quality and Proxy Resilience | `ISO-19` through `ISO-22` | Complete |
| **m-7** | Reviewed Ingestion and Storage Publication | `ISO-23`, `ISO-24` | Complete |
| **m-8** | 0.1 Release Validation and Product Acceptance | `ISO-25` through `ISO-29` | In Progress |

Milestone completion refers to its scoped tasks, not every architectural aspiration.
ISO-23 implements reviewed local/explicit-connected ingestion, linear publication,
conservative GC roots, and a cold offline consumer. ISO-24 adds reviewed, source-pinned
atomic rollover, verified batch/profile retention, safe retries, and explicit successor
adoption. Unsupported legacy/sharded formats receive conversion guidance. Direct registry
solving, selective Git/HTTP transport, connected proxy-miss fetching, and broader native
publication coverage remain deferred.

**Next:** [M8](milestones/m-8%20-%20m8-%E2%80%94-0.1-release-validation-and-product-acceptance.md)
has passed pre-merge acceptance and a non-publishing, three-platform release rehearsal on
candidate `0a48e5c82963f4eb5687ae56928668ccb7a585c9`. Final completion is held until the
change is integrated, the workflow is rerun on the exact merged `main` SHA, and a maintainer
records GO/NO-GO. Registry solving remains outside 0.1 scope. The PR guard verifies
published snapshot completeness rather than discovering dependencies from arbitrary
project lockfiles.

---

## 3. Architecture Documents (`docs/`)

1. **[doc-1: Project Architecture and Invariants](docs/doc-1%20-%20Project-architecture-and-core-invariants.md)**
2. **[doc-2: CAS and Large Blob Sharding](docs/doc-2%20-%20Content-addressed-storage-and-large-blob-sharding.md)**
3. **[doc-3: Git Orphan Branch Storage and Rollover](docs/doc-3%20-%20Git-orphan-branch-storage-and-epoch-rollover.md)**
4. **[doc-4: Multi-Ecosystem Loopback Proxy](docs/doc-4%20-%20Multi-ecosystem-loopback-proxy-interception.md)**
5. **[doc-5: Lockfile Plan Drift and CI Guard](docs/doc-5%20-%20Lockfile-plan-drift-and-ci-airlock-guard.md)**
6. **[doc-6: Pure POSIX Shell Environment Bootstrapping](docs/doc-6%20-%20Pure-posix-shell-environment-bootstrapping.md)**
7. **[doc-7: Ergonomic Command Execution](docs/doc-7%20-%20Ergonomic-command-execution-without-double-dash.md)**
8. **[doc-8: Profiled Environment Materialization and Runtime Providers](docs/doc-8%20-%20Profiled-environment-materialization-and-runtime-providers.md)**
9. **[doc-9: Typed Configuration and Plan Locking](docs/doc-9%20-%20Typed-configuration-and-plan-locking.md)**

---

## 4. Architecture Decision Records (`decisions/`)

- **[decision-1: Git Orphan Branch Storage](decisions/decision-1%20-%20Git-orphan-branch-storage.md)**
- **[decision-2: Pure POSIX Shell Bootstrapping](decisions/decision-2%20-%20Pure-posix-shell-bootstrapping.md)**
- **[decision-3: Multi-Ecosystem Loopback Proxy](decisions/decision-3%20-%20Multi-ecosystem-loopback-proxy.md)**
- **[decision-4: Ecosystem Index Generation and Canonical Crate Reuse](decisions/decision-4%20-%20Ecosystem-index-generation-and-canonical-crate-reuse.md)**
- **[decision-5: Brotli Compression for Proxy Metadata and Raw Binary Shards](decisions/decision-5%20-%20Brotli-compression-for-proxy-metadata-and-preservation-of-raw-binary-shards.md)**
- **[decision-6: Profile-Scoped Materialization and Seed Tools](decisions/decision-6%20-%20Profile-scoped-materialization-and-seed-tools.md)**
- **[decision-7: Typed Layered Configuration with Figment](decisions/decision-7%20-%20Typed-layered-configuration-with-figment.md)**

---

## 5. Feature Specifications (`features/`)

- **[CAS Sharding](features/cas-sharding.feature)**
- **[Reviewed Ingestion and Publication](features/reviewed-ingestion.feature)**
- **[Reviewed Epoch Rollover Contracts](features/git-orphan-branch.feature)**
- **[Proxy: PyPI](features/proxy-pypi.feature)**
- **[Proxy: Conda](features/proxy-conda.feature)**
- **[Proxy: NPM](features/proxy-npm.feature)**
- **[Proxy: Cargo](features/proxy-cargo.feature)**
- **[Plan Drift](features/plan-drift.feature)**
- **[POSIX Bootstrap](features/posix-bootstrap.feature)**
- **[CLI Execution](features/cli-exec.feature)**
- **[Profiled Environment Materialization](features/profiled-environment-materialization.feature)**
- **[Profile Configuration](features/profile-configuration.feature)**
- **[Proxy Resource Bounds](features/proxy-resource-bounds.feature)**
- **[Execution Sessions](features/execution-sessions.feature)**
