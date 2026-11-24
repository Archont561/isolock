---
id: ISO-34
title: Add isolock init minimal setup scaffolding
status: To Do
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-32, ISO-33]
type: task
ordinal: 34000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Provide a command that writes a minimal, valid starting point: a typed `isolock.toml` with one
profile, and a reviewed ingest manifest template with the fields a reviewer must fill. The
generated configuration must validate immediately, and the generated manifest must make the
unfilled pins obvious instead of inventing values.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 `isolock init` writes `isolock.toml` and a reviewed manifest template, and refuses to overwrite existing files without an explicit flag.
- [ ] #2 The generated configuration passes `config check` and produces a plan that fails for the honest reason (missing artifacts), never for a structural one.
- [ ] #3 No digest, version, or source is fabricated; unreviewed fields are absent or clearly marked so that ISO-32's validation rejects them rather than accepting a placeholder.
- [ ] #4 Choose and document the artifact template for both supported pin kinds (registry and git), since a git pin needs repository, revision, and commit rather than a registry URL.
- [ ] #5 A test scaffolds a directory, fills only the reviewed fields, and completes plan, ingest, and sync without editing structural configuration.
<!-- AC:END -->

## Notes

Depends on ISO-33: the setup path must end somewhere runnable, or `init` teaches a workflow
that dead-ends. Note what `init` must *not* generate — `seed-manifest` already emits
`.isolock/bootstrap.sh` from `templates/bootstrap.sh`, so bootstrap scripting is not
boilerplate a generator needs to produce.
