---
id: ISO-28
title: Cold offline profile and storage acceptance
status: Done
assignee: []
created_date: '2026-11-23 00:00'
labels: []
milestone: m-8
dependencies: [ISO-23, ISO-24, ISO-25]
type: task
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit the M7 storage/profile flows against the 0.1 user journey and ensure release-gate
evidence covers clean producers and cold consumers, not only component tests. Reuse the
existing real-CLI disposable-remote contracts; add only missing coverage needed for
workflows and platforms selected by ISO-25.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 Exercise reviewed ingestion/publication and selected-profile restoration using the real CLI and disposable remotes from a cold consumer.
- [x] #2 Prove exact selected closure, offline use after removing the remote/originals, and preservation of producer working tree/index/configuration.
- [x] #3 Cover the 0.1 epoch policy path, including retained locked consumers and reviewed successor adoption, wherever these are part of the support contract.
- [x] #4 Verify relevant corruption, rejected publication, and retry/unknown-outcome handling have executable evidence and actionable diagnostics.
- [x] #5 Keep all acceptance remotes local/disposable; do not publish tests or binaries to actual artifact storage.
<!-- AC:END -->

## Notes

Most of these contracts already exist under `crates/isolock/tests/`. This task is an
acceptance-gap audit and CI wiring exercise, not a rewrite of the ISO-23/24 implementation.

The task is an audit plus CI wiring, and the audit found the contracts already sound. Every
acceptance criterion had executable evidence before this task started:

- #1 and #2: `ingestion_profiles` publishes through the real CLI to a disposable bare remote,
  deletes the remote and the producer's `originals`, then runs the restored binary offline.
  `reviewed_ingestion` asserts the producer branch, index, and working files are byte-identical
  after publication, including `.git/index`.
- #3: `epoch_rollover` covers budget rollover, retained locked consumers across two epochs,
  idempotent accepted retries, and explicit successor adoption. Epoch retention is in the ISO-25
  contract, so this path is in scope rather than aspirational.
- #4: corruption, rejected publication, and uncertain outcomes have scenario outlines. A
  diagnostic was sabotaged to confirm the tests assert message content, not merely a non-zero
  exit: replacing `retained closure metadata is missing` failed the scenario and surfaced the
  substituted string.
- #5: every remote is a bare repository under a tempdir. No test names real artifact storage.

The gap was wiring. `reviewed_ingestion` held the ingestion and producer-preservation evidence
but was absent from `tools test acceptance`, so it ran only in the general workspace suite. A
regression there would still have failed CI, but the acceptance gate would not have been the
thing that caught it. It is now part of the gate, which runs 160 tests across four binaries in
about 20 seconds.

One assertion was checked rather than assumed. The support table claims a consumer fetches
only its selected closure; `ingestion_profiles` verifies the consumer's stored blobs equal the
manifests' reachable blobs, and `profile_bdd` asserts a Rust profile's manifest carries no
wheel or JavaScript artifact. Neither test was changed.
