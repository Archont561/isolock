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
│   └── m-4 - m4-—-cli-execution-and-environment-bootstrap.md
├── tasks/                     # Task files tracking work items
│   ├── iso-1 - Scaffold-modular-domain-crates-and-workspace.md
│   ├── iso-2 - Package-isolock-as-conda-package-via-rattler-build.md
│   ├── iso-3 - In-memory-CAS-and-SHA256-digest-verification.md
│   ├── iso-4 - Large-blob-sharding-and-POSIX-cat-reassembly.md
│   ├── iso-5 - Ephemeral-loopback-proxy-listener-and-routing.md
│   ├── iso-6 - PyPI-PEP-691-and-Conda-repodata-proxy-endpoints.md
│   ├── iso-7 - NPM-packument-and-Cargo-sparse-registry-endpoints.md
│   ├── iso-8 - Lockfile-plan-diffing-and-CI-drift-detection.md
│   ├── iso-9 - GitHub-Actions-workflow-dispatch-generator.md
│   ├── iso-10 - Ergonomic-exec-subcommand-without-double-dash.md
│   └── iso-11 - Pure-POSIX-shell-environment-bootstrap-template.md
├── docs/                      # Architectural documentation
│   ├── doc-1 - Project-architecture-and-core-invariants.md
│   ├── doc-2 - Content-addressed-storage-and-large-blob-sharding.md
│   ├── doc-3 - Git-orphan-branch-storage-and-epoch-rollover.md
│   ├── doc-4 - Multi-ecosystem-loopback-proxy-interception.md
│   ├── doc-5 - Lockfile-plan-drift-and-ci-airlock-guard.md
│   ├── doc-6 - Pure-posix-shell-environment-bootstrapping.md
│   └── doc-7 - Ergonomic-command-execution-without-double-dash.md
├── decisions/                 # Architecture Decision Records (ADRs)
│   ├── decision-1 - Git-orphan-branch-storage.md
│   ├── decision-2 - Pure-posix-shell-bootstrapping.md
│   ├── decision-3 - Multi-ecosystem-loopback-proxy.md
│   └── decision-4 - Ecosystem-index-generation-and-canonical-crate-reuse.md
└── features/                  # Executable Gherkin specification contracts (.feature)
    ├── cas-sharding.feature
    ├── git-orphan-branch.feature
    ├── proxy-pypi.feature
    ├── proxy-conda.feature
    ├── proxy-npm.feature
    ├── proxy-cargo.feature
    ├── plan-drift.feature
    ├── posix-bootstrap.feature
    ├── cli-exec.feature
    └── README.md
```

---

## 2. Milestones & Task Allocations

| Milestone | Title | Tasks | Status |
| :--- | :--- | :--- | :--- |
| **m-0** | Repository Foundation and Governance | `ISO-1`, `ISO-2` | Complete |
| **m-1** | CAS Engine and Blob Storage | `ISO-3`, `ISO-4` | In Progress |
| **m-2** | Multi-Ecosystem Proxy Resolvers | `ISO-5`, `ISO-6`, `ISO-7` | In Progress |
| **m-3** | Lockfile Plan Drift and CI Airlock | `ISO-8`, `ISO-9` | In Progress |
| **m-4** | CLI Execution and Environment Bootstrap | `ISO-10`, `ISO-11` | In Progress |

---

## 3. Architecture Documents (`docs/`)

1. **[doc-1: Project Architecture and Invariants](docs/doc-1%20-%20Project-architecture-and-core-invariants.md)**
2. **[doc-2: CAS and Large Blob Sharding](docs/doc-2%20-%20Content-addressed-storage-and-large-blob-sharding.md)**
3. **[doc-3: Git Orphan Branch Storage and Rollover](docs/doc-3%20-%20Git-orphan-branch-storage-and-epoch-rollover.md)**
4. **[doc-4: Multi-Ecosystem Loopback Proxy](docs/doc-4%20-%20Multi-ecosystem-loopback-proxy-interception.md)**
5. **[doc-5: Lockfile Plan Drift and CI Guard](docs/doc-5%20-%20Lockfile-plan-drift-and-ci-airlock-guard.md)**
6. **[doc-6: Pure POSIX Shell Environment Bootstrapping](docs/doc-6%20-%20Pure-posix-shell-environment-bootstrapping.md)**
7. **[doc-7: Ergonomic Command Execution](docs/doc-7%20-%20Ergonomic-command-execution-without-double-dash.md)**

---

## 4. Architecture Decision Records (`decisions/`)

- **[decision-1: Git Orphan Branch Storage](decisions/decision-1%20-%20Git-orphan-branch-storage.md)**
- **[decision-2: Pure POSIX Shell Bootstrapping](decisions/decision-2%20-%20Pure-posix-shell-bootstrapping.md)**
- **[decision-3: Multi-Ecosystem Loopback Proxy](decisions/decision-3%20-%20Multi-ecosystem-loopback-proxy.md)**
- **[decision-4: Ecosystem Index Generation and Canonical Crate Reuse](decisions/decision-4%20-%20Ecosystem-index-generation-and-canonical-crate-reuse.md)**
