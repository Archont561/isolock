---
id: m-0
title: "M0 - Repository foundation and governance"
---

## Description

Establish the modular Cargo workspace, Conda packaging recipe via rattler-build, and strict repository governance.

Exit condition: Workspace compiles cleanly, rattler-build produces verified Conda packages, and all alint governance rules pass.

## Exit criteria

- [x] Four domain crates (`isolock-core`, `isolock-cas`, `isolock-plan`, `isolock-proxy`) and CLI binary (`isolock`) are scaffolded.
- [x] Conda build recipe (`crates/isolock/recipe.yaml`) compiles and passes verification tests with rattler-build and pixi build.
- [x] All 48 repository governance rules in `alint` pass cleanly with zero warnings.

## Status

**Complete**. All M0 tasks (`ISO-1`, `ISO-2`) are completed and passing in CI.

## Tasks

- **ISO-1**: Scaffold modular domain crates and workspace.
- **ISO-2**: Package isolock as conda package via rattler-build.
