---
id: ISO-36
title: Prove cargo add and pixi add through the loopback proxy
status: Done
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-33]
type: task
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
ISO-33 delivered the session wiring: a profile-aware `exec` selects the locked manifest,
serves its registry artifacts through the loopback proxy, puts the verified prefix on
`PATH`, and points `CARGO_HOME` at the profile. What it did not deliver is the end-to-end
proof that a package manager can *resolve and add* a new package against that proxy with
upstream unreachable, which is the use case the adoption work exists for.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 Determine whether the projection in `execution/profile_session.rs` supplies everything a Cargo sparse index request needs, or whether index metadata must be synthesized per artifact.
- [x] #2 Run `cargo add NAME` inside a materialized profile session with upstream unreachable and assert the dependency is added from the proxy, not from a host registry.
- [x] #3 Cover the same journey for `pixi add` against the generated Pixi channel configuration.
- [x] #4 Assert the failure mode when the requested package is absent from the projected snapshot: a miss from the proxy, never an upstream fetch.
- [x] #5 Record the outcome in the profiles or proxy guide with real command output.
<!-- AC:END -->

## Notes

The Cargo projection is a vendored directory source, not the loopback proxy: the profile's
`cargo-home/config.toml` replaces `crates-io` with `isolock-vendor` before the proxy index
variable is consulted. That is stronger than a proxy round trip, because a directory source
cannot reach the network at all, so `cargo add` is offline by construction. The proof below
therefore asserts "added from the reviewed snapshot, never a host registry", which is what
AC #2 protects. The proxy still serves pixi/npm/pypi for those ecosystems.

Split out of ISO-33 because the session wiring and the resolution proof are separable, and
claiming the second on the strength of the first is what the earlier tasks were meant to
avoid. ISO-33 is complete except for its AC #4, which this task owns.
