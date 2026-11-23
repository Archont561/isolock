---
id: ISO-25
title: Define the 0.1 product support and acceptance contract
status: To Do
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-18, ISO-23, ISO-24]
type: task
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Decide exactly what Isolock 0.1 promises before expanding CI. Define the supported host
platforms, installation artifacts, ecosystems, user workflows, prerequisites, and known
exclusions. Turn each public claim into a named acceptance case and assign it to the
appropriate test layer. Unclaimed or untested combinations must remain explicitly
unsupported rather than being implied by a successful build.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Publish a versioned 0.1 support matrix covering host/target platforms, install routes, toolchain/runtime prerequisites, ecosystems, and product workflows.
- [ ] #2 For every supported claim, identify its executable acceptance test, required runner, network model, and evidence artifact.
- [ ] #3 Document exclusions and limits, including the reviewed-pin/lock-driven boundary and any platform or ecosystem combinations not tested.
- [ ] #4 Obtain an explicit release-scope review before platform CI or release claims are broadened.
<!-- AC:END -->

## Notes

Registry solving and version selection remain out of scope unless the 0.1 scope review
explicitly adds them. Prefer supported behavior already delivered by M0-M7; do not make
new feature work an implicit prerequisite for describing the product accurately.
