---
id: ISO-12
title: Automated release workflow for prefix dev and GitHub releases
status: Done
assignee: []
created_date: '2026-11-17 14:30'
labels: []
milestone: m-0
dependencies: [ISO-2]
type: task
ordinal: 12000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Implement `.github/workflows/release.yml` split into cross-platform build matrix jobs and publishing jobs. The `build` job compiles standalone Rust binaries and `.conda` recipes via `rattler-build` across Linux, macOS, and Windows, uploading them as workflow artifacts. Subsequent jobs enforce explicit prerequisites (`needs`): `publish-conda` uploads Conda packages to prefix.dev channel `archont561/archont561` using GitHub OIDC Trusted Publishing under the `prefix` environment, while `publish-github` publishes GitHub Releases containing all standalone binaries, Conda packages, and SHA256 checksums.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Release workflow triggers on tag pushes (`v*`) and manual `workflow_dispatch`.
- [x] #2 Split release pipeline into distinct `build` and `publish` jobs with strict `needs` job dependencies.
- [x] #3 Cross-platform matrix builds standalone Rust binaries and `.conda` packages across Linux, macOS, and Windows.
- [x] #4 Standalone binaries and Conda packages uploaded as workflow artifacts.
- [x] #5 Publishes Conda packages to prefix.dev channel `archont561/archont561` via GitHub Actions OIDC Trusted Publishing under the `prefix` environment.
- [x] #6 Generates build attestations and publishes GitHub Releases attaching standalone binaries, Conda packages, and `SHA256SUMS`.
<!-- AC:END -->

## Notes

- 2026-11-17: Configured `.github/workflows/release.yml` with cross-platform build matrix, artifact pipelines, prefix.dev OIDC publishing, and actionlint/alint verification.
