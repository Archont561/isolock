---
id: m-3
title: "M3 - Lockfile plan drift and CI airlock"
---

## Description

Provide expected-plan/CAS drift auditing, read-only fail-closed checks, and reviewed-manifest dispatch command generation. Profile-scoped lockfile parsing/checking is delivered separately by M5.

Exit condition: legacy `isolock plan --ci` rejects missing expected plan/CAS artifacts and emits actionable dispatch guidance. It does not discover or solve arbitrary project dependencies.

## Exit criteria

- [x] Lockfile diffing evaluates expected against current dependency artifacts.
- [x] Unmirrored packages trigger non-zero CI exit codes.
- [x] Automated generation of `gh workflow run isolock.yml` dispatch commands.

## Status

**Complete within this scope.** Drift evaluation (`ISO-8`) and dispatch command synthesis (`ISO-9`) are complete. ISO-23 supplies reviewed acquisition/publication; the PR workflow checks a published snapshot, not general lockfile discovery. Automatic epoch rollover remains ISO-24.

## Tasks

- **ISO-8**: Lockfile plan diffing and CI drift detection.
- **ISO-9**: GitHub Actions workflow dispatch generator.
