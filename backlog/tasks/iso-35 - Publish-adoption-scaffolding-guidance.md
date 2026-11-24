---
id: ISO-35
title: Publish adoption scaffolding guidance
status: Done
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
- [x] #1 Document the exact file set a consumer commits, and the order of the plan, ingest, push, and sync steps that produced it.
- [x] #2 Decide whether the publish and PR snapshot-guard workflows are generated per repo or documented as copy-and-review templates; record the rationale.
- [x] #3 Include the storage-branch setup and the offline consumer restore walkthrough with real command output.
- [x] #4 State explicitly which steps still require hand authoring so the guide does not promise automation that ISO-34 did not ship.
- [x] #5 Keep the reviewed-artifact boundary visible: no workflow or template may resolve or admit an unreviewed artifact.
<!-- AC:END -->

## Notes

The publish workflow here is four steps (`scripts/restore.sh` plus `tools test`
invocations) and carries almost no per-repo content, so a renderer would emit a
near-identical file for everyone. Documenting it may be the whole of the value; record that
decision instead of defaulting to code generation.

Published as `docs/content/docs/adoption.mdx`, linked from the getting-started and profiles
guides and added to the docs navigation.

AC #2's decision is documented, not generated. The publish and guard jobs carry almost no
per-repo content: the publish job is a restore plus `ingest`/`push` with a manifest path and
a registry name, and both varying values are already `workflow_dispatch` inputs. A renderer
would emit a near-identical file for every repository. The guide names the two properties
worth preserving when copying it: publication is dispatch-only and never pull-request
triggered, and package selections arrive as literal argument entries rather than shell
interpolation.

The walkthrough output is real. It was captured against a local bare remote, and the consumer
restore was performed after deleting that remote and the original archives, so the restored
prefix demonstrably depends on neither.

Two things the guide states that the task anticipated: the seed pin `init` writes is
machine-local (an absolute `file://` path), and an empty reviewed manifest is deliberate
rather than an oversight. Both belong in AC #4's list of what still needs hand authoring.
