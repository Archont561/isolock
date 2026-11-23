---
id: ISO-29
title: Gate and rehearse the 0.1 release
status: To Do
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-26, ISO-27, ISO-28]
type: task
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make the ISO-25 through ISO-28 evidence a required 0.1 release gate. Rehearse the existing
release workflow with a release candidate, verify the artifacts as a consumer, and leave
a concise, reproducible checklist and go/no-go record tied to the exact candidate commit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Run the required clean-room CLI, artifact-install, profile, and storage acceptance jobs on the ISO-25 matrix; failed or skipped required cases block release.
- [ ] #2 Rehearse the release workflow end to end on a candidate without publishing unintended packages or binaries.
- [ ] #3 Independently verify candidate artifacts and SHA-256 checksums from the consumer side.
- [ ] #4 Publish the 0.1 support matrix, known limitations, installation/quickstart guidance, and a repeatable release checklist.
- [ ] #5 Record a go/no-go review tied to the exact source commit and preserve its CI/test evidence.
<!-- AC:END -->

## Notes

Any external registry canary must be isolated, opt-in, and non-blocking; routine release
acceptance is hermetic and reproducible.
