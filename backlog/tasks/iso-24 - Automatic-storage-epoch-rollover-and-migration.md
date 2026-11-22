---
id: ISO-24
title: Automatic storage epoch rollover and reviewed snapshot migration
status: To Do
assignee: []
created_date: '2026-11-18 12:00'
labels: []
milestone: m-7
dependencies: [ISO-23]
type: task
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement the still-deferred epoch rollover specification. ISO-23 supports explicit
`isolock/vN` publication and rejects over-budget live trees, but does not roll generations,
rewrite configuration, bound cumulative Git history, or migrate legacy/unmarked/sharded
snapshots automatically. CAS sharding alone is not Git publication support.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Define live-tree versus cumulative-history budgets and an explicit reviewed rollover/retention policy.
- [ ] #2 Create a new orphan generation without rewriting existing storage or code history.
- [ ] #3 Preserve reachable profiles, immutable manifests, input originals, provenance, and locked payload-commit availability across migration.
- [ ] #4 Handle concurrent publishers and partial failures without force-pushing or silently changing the active generation.
- [ ] #5 Migrate supported legacy/unmarked/sharded snapshots only through a reviewed, validated format boundary, or reject them with actionable diagnostics.
- [ ] #6 Execute the deferred rollover BDD contract through the real CLI and disposable remotes, including a cold offline consumer and unchanged producer checkout/index.
<!-- AC:END -->

## Status evidence

Automatic rollover has no implementation or passing rollover scenario today.
`backlog/features/git-orphan-branch.feature` is explicitly tagged as a deferred specification.
Do not infer this capability from the completed CAS sharding tasks or older milestone text.
