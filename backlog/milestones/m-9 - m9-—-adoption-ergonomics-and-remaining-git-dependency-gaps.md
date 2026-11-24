---
id: m-9
title: "M9 - Adoption ergonomics and remaining git dependency gaps"
status: To Do
---

## Purpose

Close the two things that still stand between a planned feature and a usable one: the
remaining correctness gaps in Cargo git dependencies (ISO-30 through ISO-32), and the fact
that a profile repo cannot yet run package managers through `exec` (ISO-33). Only after
those is scaffolding worth writing, because scaffolding automates a workflow that must
first exist (ISO-34, ISO-35).

Scaffolding is deliberately last. Authoring files by hand is the honest current state; a
generator built before the workflow works would template a shape that does not run.

## Exit criteria

- [ ] A `Cargo.lock` with a git dependency reachable only through another git dependency
  plans, ingests, and builds offline.
- [ ] A lockfile reference that is not already a full commit produces an actionable error
  that names the `cargo update` remedy, rather than a bare rejection.
- [x] A profile-planned repo can run `pixi add` and `cargo add` through `isolock exec`
  against the loopback proxy (session wiring landed; the end-to-end proof is ISO-36).
- [x] `isolock init` produces a repo whose generated config validates *and* whose plan is
  not dead on arrival; a placeholder digest is rejected, never silently accepted.
- [x] Adoption guidance carries the boilerplate that a generator would emit, and a
  maintainer can follow it without reading the source.

## Scope boundaries

No registry solving or version selection. No implicit resolution of unreviewed artifacts:
every artifact still arrives through an explicit pin. A generator may write a reviewed
manifest *template*, but it must never invent a digest or fabricate a pin.

## Tasks

- **ISO-30**: Transitive Cargo git dependencies - To Do.
- **ISO-31**: Actionable error for non-commit git lockfile references - To Do.
- **ISO-32**: Reject sentinel digests in typed configuration - Done.
- **ISO-33**: Profile-aware `exec` - Done.
- **ISO-34**: `isolock init` minimal-setup scaffolding - Done.
- **ISO-35**: Adoption scaffolding guidance - Done.
- **ISO-36**: Prove `cargo add`/`pixi add` through the loopback proxy - Done.

## Status

Registered 2026-11-24 from a review of open issue #30. ISO-30 and ISO-31 were carried as
"open questions" in `docs/architecture/cargo-git-dependencies.md` after the git dependency
work merged (PR #29); they are correctness gaps, not ergonomics, and are ranked first
because a build that fails is worse than a build that is tedious to set up.

ISO-33 and ISO-36 landed in PR #34. The end-to-end proof showed that Cargo's projection
is a vendored `directory` source, not a proxy round trip: the profile's
`cargo-home/config.toml` replaces `crates-io` with `isolock-vendor` before the proxy index
variable is consulted. That exit criterion is therefore met in the stronger sense, since a
directory source cannot reach the network at all. `pixi add` does resolve through the
generated channel. ISO-35 published the adoption guide as a documented template rather
than a generator. Only ISO-30 and ISO-31 (git dependency correctness, both behind the
ISO-29 release gate) remain open. ISO-32 landed after them, closing the last validation trap that
scaffolding would otherwise have emitted.
