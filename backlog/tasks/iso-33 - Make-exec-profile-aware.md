---
id: ISO-33
title: Make exec profile-aware
status: In Progress
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-29]
type: task
ordinal: 33000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`serve` and `exec` consume the legacy `AirlockPlan` at `.isolock/plan.json`, while a profile
repo produces `.isolock/plans/PROFILE-PLATFORM.lock`. A user who follows the profile guide
and then runs `isolock exec cargo add ...` gets a child process that cannot find the tool,
because no restored runtime is on PATH. This blocks the primary adoption use case: adding a
dependency through the verified loopback proxy.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [x] #1 `exec` selects a profile and platform (explicitly or by the ambient defaults) and runs the child with that profile's activated PATH, CARGO_HOME, and proxy environment.
- [x] #2 Behavior with no profile configured is unchanged: the legacy `.isolock/plan.json` path keeps byte-identical output, errors, and exit status.
- [x] #3 A missing or unmaterialized profile fails closed with guidance to run `sync`, and never silently falls back to the host toolchain.
- [ ] #4 Prove `cargo add` and `pixi add` work through `exec` against the loopback proxy in a hermetic test, with upstream unreachable.
- [x] #5 `exec` still preserves the child's exit status and passes its argument tail through unchanged.
<!-- AC:END -->

## Notes

Verified 2026-11-24 in a profile-configured directory: `isolock exec -- cargo --version`
failed with `isolock: cannot start "cargo": No such file or directory (os error 2)`.
`CargoEnvironment` already sets `CARGO_REGISTRIES_CRATES_IO_INDEX` to the loopback origin,
so the environment contribution exists; what is missing is profile selection and activation.
This is the capability that gates ISO-34: scaffolding a setup that cannot run its own use
case would be worse than no scaffolding.
