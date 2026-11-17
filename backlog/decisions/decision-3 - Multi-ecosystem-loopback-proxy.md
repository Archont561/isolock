---
id: decision-3
title: Multi-ecosystem loopback proxy
date: '2026-11-17 11:00'
status: accepted
---

## Context

Enterprise polyglot repositories use multiple package managers simultaneously (pip, pixi, npm, cargo). Overriding package manager configuration files often pollutes source control and leaks external URLs into lockfiles.

## Decision

Isolock runs an ephemeral loopback HTTP proxy that intercepts requests and translates them into local CAS lookups:

1. Dynamic port binding prevents collision with other local services.
2. Standard protocol endpoints:
   - Conda: `/conda/channel/<subdir>/repodata.json`
   - PyPI: `/pypi/simple/<package>/` (PEP 503)
   - NPM: `/npm/<package>` (Packument)
   - Cargo: `/cargo/...` (RFC 2789 sparse registry)
3. `isolock exec` injects standard environment variables (`PIP_INDEX_URL`, `NPM_CONFIG_REGISTRY`, `PIXI_CONDA_CHANNELS`, etc.) into child processes without requiring file modifications.

## Consequences

- **Positive**: Native package managers operate seamlessly without code changes.
- **Positive**: Eliminates lockfile corruption and URL drift.
- **Negative**: Requires accurate in-memory protocol synthesis for all supported ecosystems.
