---
id: ISO-25
title: Define the 0.1 product support and acceptance contract
status: In Progress
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
- [x] #1 Publish a versioned 0.1 support matrix covering host/target platforms, install routes, toolchain/runtime prerequisites, ecosystems, and product workflows.
- [x] #2 For every supported claim, identify its executable acceptance test, required runner, network model, and evidence artifact.
- [x] #3 Document exclusions and limits, including the reviewed-pin/lock-driven boundary and any platform or ecosystem combinations not tested.
- [ ] #4 Obtain an explicit release-scope review before platform CI or release claims are broadened.
<!-- AC:END -->

## Notes

Registry solving and version selection remain out of scope unless the 0.1 scope review
explicitly adds them. Prefer supported behavior already delivered by M0-M7; do not make
new feature work an implicit prerequisite for describing the product accurately.

ACs #1-#3 are satisfied by `docs/content/docs/support.mdx`, which predates this task and now
carries the full contract. The table gained explicit acceptance-test, runner-and-network, and
evidence-artifact columns so each claim names all four required attributes rather than
describing evidence in prose.

Writing that column surfaced a stale claim. `support.mdx` said package-manager transactions
had not been exercised end to end. That was true when written and stopped being true in
ISO-36, which added three real-client journeys. The stronger problem was that those journeys
were `#[ignore]`d and absent from `PROFILE_JOURNEYS`, so nothing in CI ever ran them: the
claim stayed accurate only because the evidence was disconnected. They are now release-gated,
and both profiles were run locally to confirm the gate executes them:

- `rust-standalone`: registry, git, and `real_cargo_add_adds_a_reviewed_crate_offline` pass.
- `rust-pixi`: cold restore, Git roundtrip, `real_pixi_add_resolves_from_the_generated_channel`,
  and `real_pixi_add_reports_a_miss_for_an_unreviewed_package` pass.

The corrected contract states what the clients actually prove and, just as importantly, what
they do not: pip and npm are not exercised, and `cargo add` resolves from the vendored
directory source rather than over the proxy, so no 0.1 claim covers dependency resolution for
those clients or transitive discovery for any of them.

Adding the cargo journey takes `rust-standalone` to roughly 8.6 minutes locally against a
25-minute job budget.

AC #4 stays open. A release-scope review is a human sign-off, and marked complete without one
it would be exactly the kind of unsupported claim this contract exists to prevent.
