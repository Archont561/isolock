---
id: ISO-35
title: Publish adoption scaffolding guidance
status: To Do
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-34]
type: task
ordinal: 35000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The getting-started and profile guides describe commands but not the surrounding repository
setup: which files to commit, how to publish to a storage branch, and how a consumer
restores. Decide which boilerplate belongs in prose and which belongs in a generated
workflow, and publish the result so a maintainer can adopt isolock without reading source.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Document the exact file set a consumer commits, and the order of the plan, ingest, push, and sync steps that produced it.
- [ ] #2 Decide whether the publish and PR snapshot-guard workflows are generated per repo or documented as copy-and-review templates; record the rationale.
- [ ] #3 Include the storage-branch setup and the offline consumer restore walkthrough with real command output.
- [ ] #4 State explicitly which steps still require hand authoring so the guide does not promise automation that ISO-34 did not ship.
- [ ] #5 Keep the reviewed-artifact boundary visible: no workflow or template may resolve or admit an unreviewed artifact.
<!-- AC:END -->

## Notes

The publish workflow here is four steps (`scripts/restore.sh` plus `tools test`
invocations) and carries almost no per-repo content, so a renderer would emit a
near-identical file for everyone. Documenting it may be the whole of the value; record that
decision instead of defaulting to code generation.
