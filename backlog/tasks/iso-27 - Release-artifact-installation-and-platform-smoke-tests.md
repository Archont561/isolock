---
id: ISO-27
title: Release artifact installation and platform smoke tests
status: Done
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
- [x] #1 Build or download the same release artifact formats that the 0.1 release workflow publishes.
- [x] #2 On native runners for every claimed platform/install route, verify archive/package integrity and install without relying on workspace paths.
- [x] #3 Run version/help, configuration startup, and at least one documented representative command from the installed artifact.
- [x] #4 Verify published SHA-256 checksums and fail the acceptance job on missing, mismatched, or un-installable artifacts.
- [x] #5 Record unsupported release targets explicitly; cross-compilation success alone is not acceptance evidence.
<!-- AC:END -->

## Notes

The exact OS/architecture/install matrix is selected by ISO-25. Keep secrets and publishing
credentials out of build/test jobs; the acceptance path should consume staged artifacts.

Most of ACs #1-#3 already held. `release stage-binaries` and `verify-binary` run on native
runners per platform, the archive is extracted and smoke-tested from the extraction, and
`host-smoke` refuses a cross-compiled target:

    runner host x86_64-unknown-linux-gnu cannot natively execute aarch64-apple-darwin

Two gaps were real.

The Conda install route ran only `--version`, `--help`, and `config check`, so it proved a
package starts but never that it works. AC #3 asks for a representative documented command.
The recipe now runs `isolock plan --ci` on an empty plan and asserts the
`Verdict: ALL DEPENDENCIES SATISFIED` line, which keeps the test hermetic and offline. The
command was exercised against the real extracted artifact before the recipe was changed, so
the assertion is not a guess.

AC #4's checksum gate existed but had no regression protection. Sabotaging `verify_sums` to
compare nothing, or to skip a missing file, broke no test. Three cases now pin it: a digest
mismatch, a manifest naming an absent artifact, and a staging directory with no manifest at
all. All three were confirmed to fail when the corresponding check is removed. The published
digest was also recomputed independently and matched the manifest.

A fourth test pins the recipe itself, so a future edit cannot quietly drop the representative
command and leave AC #3 satisfied by proxy.

AC #5's matrix is stated in `support.mdx`: exactly three targets, each with a native runner,
and cross-compilation success explicitly rejected as evidence. Linux `aarch64`, Intel macOS,
and Windows GNU builds are named as unsupported rather than left implied.

The release workflow was not modified, so nothing here needs a real release to be believed.
Verification runs against staged artifacts exactly as it does today.
