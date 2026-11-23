---
id: ISO-26
title: Clean-room CLI acceptance across supported ecosystems
status: To Do
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-25]
type: task
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Prove the 0.1 advertised proxy/lock-plan workflows as a user would exercise them, using
the real CLI and deterministic local fixture registries/services. Tests must start from a
clean project/cache, use only the inputs the 0.1 contract documents, and prove both
connected acquisition/caching and successful use after the upstream is unavailable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Bind an executable acceptance journey to each ecosystem/workflow claimed by ISO-25; drive the public CLI rather than internal functions alone.
- [ ] #2 Use hermetic local fixtures with deterministic package metadata and bytes; routine CI does not depend on public registries or external network availability.
- [ ] #3 Prove a clean connected acquisition/cache path followed by upstream shutdown, cold offline replay/use, and no unexpected network fallback.
- [ ] #4 Verify negative integrity/tampering and stale-plan/drift cases, including the documented verdict and exit status.
- [ ] #5 Capture useful failure diagnostics and keep fixture credentials/secrets out of logs.
<!-- AC:END -->

## Notes

The matrix of ecosystems is set by ISO-25. The current reviewed-manifest and lockfile-driven
interfaces remain the input contract; this task does not add registry dependency solving.
