---
id: ISO-6
title: PyPI PEP 691 and Conda repodata proxy endpoints
status: Done
assignee: []
created_date: '2026-11-17 11:00'
labels: []
milestone: m-2
dependencies: [ISO-5]
type: task
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Synthesize PEP 691 JSON Simple Index (with zero HTML) for PyPI clients and dynamic `repodata.json` for Conda clients in `isolock-proxy`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 `/pypi/simple/<package>/` returns PEP 691 JSON index with `sha256` hashes and zero HTML.
- [x] #2 `/conda/channel/<subdir>/repodata.json` returns structured package index parsed from `.conda` and `.tar.bz2` metadata.
- [x] #3 Verified end-to-end with `pip install` and `pixi install`.
<!-- AC:END -->

## Notes

- Governed by `backlog/features/proxy-pypi.feature` and `backlog/features/proxy-conda.feature`.
- Implements `rattler_conda_types::RepoData` serialization as decided in `decision-4`.
- Enforces strict zero-HTML policy: all index feeds are JSON.
- 2026-11-17: Verified with real `pip install` and `pixi install` against the loopback proxy and CAS-only fixtures. Conda records are synthesized from `PlannedArtifactMetadata` captured from archive metadata; package bytes are served unchanged from CAS.
