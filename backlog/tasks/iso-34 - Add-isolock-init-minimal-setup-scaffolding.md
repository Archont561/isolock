---
id: ISO-34
title: Add isolock init minimal setup scaffolding
status: Done
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-32]
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
- [x] #1 `isolock init` writes `isolock.toml` and a reviewed manifest template, and refuses to overwrite existing files without an explicit flag.
- [x] #2 The generated configuration passes `config check` and produces a plan that fails for the honest reason (missing artifacts), never for a structural one.
- [x] #3 No digest, version, or source is fabricated; unreviewed fields are absent or clearly marked so that ISO-32's validation rejects them rather than accepting a placeholder.
- [x] #4 Choose and document the artifact template for both supported pin kinds (registry and git), since a git pin needs repository, revision, and commit rather than a registry URL.
- [x] #5 A test scaffolds a directory, fills only the reviewed fields, and completes plan, ingest, and sync without editing structural configuration.
<!-- AC:END -->

## Notes

ISO-33 landed, so the setup path now ends somewhere runnable. Note what `init` must *not*
generate: `seed-manifest` already emits `.isolock/bootstrap.sh` from
`templates/bootstrap.sh`, so bootstrap scripting is not boilerplate a generator needs to
produce.

The generated configuration describes a leaf profile, and a leaf profile structurally
requires an `isolock` seed, so AC #2 could not be satisfied by writing an empty profile.
The seed therefore pins the running executable: its bytes are on disk and its digest is
computed, so the one pin `init` states is true rather than plausible. Every other pin is
absent, and the reviewed manifest is empty, which is what AC #3 asks for.

AC #4's two templates live in `docs/content/docs/profiles.mdx`. They differ in more than
a URL: a git pin needs `repository`, `revision`, and `commit`, and its digest describes
`git archive` output rather than a published archive.

The test scaffolds a project from a stripped copy of the binary. A debug build exceeds the
95 MiB reviewed-ingest limit, so the journey proves the flow at the size a release build
has, rather than passing only because the fixture was small.
