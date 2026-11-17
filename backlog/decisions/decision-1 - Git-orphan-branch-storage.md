---
id: decision-1
title: Git orphan branch storage
date: '2026-11-17 11:00'
status: accepted
---

## Context

Storing binary dependencies directly inside application Git branches leads to working tree bloat, slow checkouts, and merge conflicts. Using third-party object stores requires proprietary accounts, network credentials, and external infrastructure.

## Decision

We use an unparented Git orphan branch (`isolock/v1`) as a decentralized, versioned object store:

1. Dependencies are committed directly to Git's object database using Git low-level plumbing (`git commit-tree`, `git update-ref`).
2. Main and feature branches remain lightweight and clean of binary payloads.
3. Git's native distributed protocol handles mirroring, access control, and deduplication.

## Consequences

- **Positive**: Zero external cloud storage dependencies; works with any standard Git hosting service.
- **Positive**: Complete tamper-evident SHA-1 / SHA-256 audit trail via Git commit trees.
- **Negative**: Branch size must be budgeted; resolved via automated rollover to `isolock/v2` at 1,500 MiB.
