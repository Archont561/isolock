---
id: ISO-27
title: Release artifact installation and platform smoke tests
status: To Do
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-25]
type: task
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Validate the artifacts users will install for 0.1, not only the Cargo workspace build.
On every platform and installation route claimed by the accepted ISO-25 support matrix,
install the release-shaped standalone binary or package and run a small representative
smoke journey using the installed artifact.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Build or download the same release artifact formats that the 0.1 release workflow publishes.
- [ ] #2 On native runners for every claimed platform/install route, verify archive/package integrity and install without relying on workspace paths.
- [ ] #3 Run version/help, configuration startup, and at least one documented representative command from the installed artifact.
- [ ] #4 Verify published SHA-256 checksums and fail the acceptance job on missing, mismatched, or un-installable artifacts.
- [ ] #5 Record unsupported release targets explicitly; cross-compilation success alone is not acceptance evidence.
<!-- AC:END -->

## Notes

The exact OS/architecture/install matrix is selected by ISO-25. Keep secrets and publishing
credentials out of build/test jobs; the acceptance path should consume staged artifacts.
