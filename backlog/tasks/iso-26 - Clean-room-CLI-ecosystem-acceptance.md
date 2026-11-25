---
id: ISO-26
title: Clean-room CLI acceptance across supported ecosystems
status: Done
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-25]
type: task
ordinal: 26000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Prove the 0.1 advertised proxy/lock-plan workflows as a user would exercise them, using
the real CLI and deterministic local fixture registries/services. Tests must start from a
clean project/cache, use only the inputs the 0.1 contract documents, and prove both
connected acquisition/caching and successful use after the upstream is unavailable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 Bind an executable acceptance journey to each ecosystem/workflow claimed by ISO-25; drive the public CLI rather than internal functions alone.
- [x] #2 Use hermetic local fixtures with deterministic package metadata and bytes; routine CI does not depend on public registries or external network availability.
- [x] #3 Prove a clean connected acquisition/cache path followed by upstream shutdown, cold offline replay/use, and no unexpected network fallback.
- [x] #4 Verify negative integrity/tampering and stale-plan/drift cases, including the documented verdict and exit status.
- [x] #5 Capture useful failure diagnostics and keep fixture credentials/secrets out of logs.
<!-- AC:END -->

## Notes

The matrix of ecosystems is set by ISO-25. The current reviewed-manifest and lockfile-driven
interfaces remain the input contract; this task does not add registry dependency solving.

An audit against ISO-25's claims found most of the matrix already gated; the gap was AC #3's
second half. Everything that existed proved a cold replay worked. Nothing proved the *absence
of a fallback* once the upstream was gone, which is the part of the claim a successful replay
by itself cannot make.

`proxy_serves_cold_snapshot_after_the_reviewed_upstream_is_gone` closes it by driving the
public `serve` command: it resolves index and artifact, deletes the reviewed upstream
directory entirely, then replays the artifact from the same running process and asserts the
identical bytes. It then requests an unreviewed package and requires a non-200.

That last assertion was checked for teeth rather than assumed. Capturing the actual response
shows `HTTP/1.1 403 Forbidden`, so the fail-closed path is genuinely exercised and the test
does not pass because the proxy answers everything.

A second test, `corrupt_cas_blob_fails_closed_before_any_bytes_are_served`, covers tampering.
Writing it corrected a wrong assumption of mine: CAS bytes are read into memory when the
listener binds, so corrupting a blob of a *running* server is not observable and asserting on
it would have tested nothing. `serve` refuses to bind at all with exit 1 and a
`SHA-256 mismatch` diagnostic naming the expected digest. The test asserts that stronger
guarantee instead.

Evidence per ISO-25 claim:

- Standalone CLI archive and Conda package: release-artifact smoke matrix.
- Locked-profile materialization: `tools test profile-materialize`, both profiles.
- Reviewed storage publication: `ingestion_profiles` and `reviewed_ingestion`.
- Registry proxy protocols: `cli_test` four-protocol journey, 200 for each endpoint.
- Real clients: the three ISO-36 journeys, gated in ISO-25.
- Both new journeys run in `tools test acceptance`, which is a required CI job.

AC #5 already held: `reviewed_ingestion` asserts a credential-bearing source is rejected and
that `do-not-echo-this-secret` never reaches stderr.

Fixture sources stay hermetic. Connected acquisition shells out to `curl --proto =https`, which
a loopback plain-HTTP fixture cannot satisfy by design, so that boundary remains covered by
the shimmed-curl journey asserting the reviewed pin and HTTPS constraints.
