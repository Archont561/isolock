---
id: ISO-28
title: Cold offline profile and storage acceptance
status: To Do
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
- [ ] #1 Exercise reviewed ingestion/publication and selected-profile restoration using the real CLI and disposable remotes from a cold consumer.
- [ ] #2 Prove exact selected closure, offline use after removing the remote/originals, and preservation of producer working tree/index/configuration.
- [ ] #3 Cover the 0.1 epoch policy path, including retained locked consumers and reviewed successor adoption, wherever these are part of the support contract.
- [ ] #4 Verify relevant corruption, rejected publication, and retry/unknown-outcome handling have executable evidence and actionable diagnostics.
- [ ] #5 Keep all acceptance remotes local/disposable; do not publish tests or binaries to actual artifact storage.
<!-- AC:END -->

## Notes

Most of these contracts already exist under `crates/isolock/tests/`. This task is an
acceptance-gap audit and CI wiring exercise, not a rewrite of the ISO-23/24 implementation.
