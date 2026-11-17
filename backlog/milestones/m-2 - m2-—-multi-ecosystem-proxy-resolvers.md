---
id: m-2
title: "M2 - Multi-ecosystem proxy resolvers"
---

## Description

Construct the ephemeral loopback HTTP proxy serving standard package manager protocols for Conda, PyPI, NPM, and Cargo.

Exit condition: Native tools resolve and install dependencies hermetically from the local proxy without internet connectivity.

## Exit criteria

- [x] Proxy listener binds to loopback with configurable fail-closed airlock policies.
- [ ] PyPI PEP 691 JSON Simple Index (zero HTML) and wheel delivery endpoints are functional.
- [ ] Conda `repodata.json` generation and archive downloads succeed.
- [ ] NPM packuments with SHA-512 integrity and Cargo RFC 2789 sparse registry are served.

## Status

**In Progress**. Routing decision core (`ISO-5`) is complete; protocol endpoint synthesizers (`ISO-6`, `ISO-7`) are queued.

## Tasks

- **ISO-5**: Ephemeral loopback proxy listener and routing.
- **ISO-6**: PyPI PEP 691 and Conda repodata proxy endpoints.
- **ISO-7**: NPM packument and Cargo sparse registry endpoints.
