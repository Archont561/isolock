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
- **Negative**: Live paths and cumulative reachable blobs require different budgets.
  Rollover needs reviewed retention and atomic-push support; retained archived refs mean
  total remote disk usage is not globally bounded.

## Implementation status

ISO-23 implements reviewed explicit publication and a linear non-force storage audit.
ISO-24 adds source-pinned `push --epoch-policy FILE`: full isolated history accounting,
verified batch/profile retention, an orphan successor, and an unchanged-tree source seal
published in one atomic non-force transaction. Producer code refs/index/checkout and
configuration remain unchanged. Old refs and immutable profile identities/payload commits
remain available; successor adoption requires explicit replanning.

Originals over 95 MiB, over-budget retained closures, shallow storage histories, and
unsupported unmarked/legacy-sharded/unsafe snapshots fail closed with conversion guidance.
No non-atomic fallback or automatic format conversion is provided. See
[doc-3](../docs/doc-3%20-%20Git-orphan-branch-storage-and-epoch-rollover.md) for budget
semantics, recovery from uncertain acceptance, and the remaining storage boundaries.
