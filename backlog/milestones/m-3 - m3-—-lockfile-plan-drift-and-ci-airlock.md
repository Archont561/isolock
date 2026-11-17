---
id: m-3
title: "M3 - Lockfile plan drift and CI airlock"
---

## Description

Provide multi-lockfile drift auditing, read-only CI fail-closed gates, and automated dispatch ingestion command generation.

Exit condition: `isolock plan --ci` detects lockfile tampering or missing dependencies and emits actionable ingestion commands.

## Exit criteria

- [x] Lockfile diffing evaluates expected against current dependency artifacts.
- [x] Unmirrored packages trigger non-zero CI exit codes.
- [ ] Automated generation of `gh workflow run isolock.yml` dispatch commands.

## Status

**In Progress**. Drift evaluation (`ISO-8`) is complete; dispatch command synthesis (`ISO-9`) is in progress.

## Tasks

- **ISO-8**: Lockfile plan diffing and CI drift detection.
- **ISO-9**: GitHub Actions workflow dispatch generator.
