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
- [ ] A profile-planned repo can run `pixi add` and `cargo add` through `isolock exec`
  against the loopback proxy.
- [ ] `isolock init` produces a repo whose generated config validates *and* whose plan is
  not dead on arrival; a placeholder digest is rejected, never silently accepted.
- [ ] Adoption guidance carries the boilerplate that a generator would emit, and a
  maintainer can follow it without reading the source.

## Scope boundaries

No registry solving or version selection. No implicit resolution of unreviewed artifacts:
every artifact still arrives through an explicit pin. A generator may write a reviewed
manifest *template*, but it must never invent a digest or fabricate a pin.

## Tasks

- **ISO-30**: Transitive Cargo git dependencies - To Do.
- **ISO-31**: Actionable error for non-commit git lockfile references - To Do.
- **ISO-32**: Reject sentinel digests in typed configuration - To Do.
- **ISO-33**: Profile-aware `exec` - To Do.
- **ISO-34**: `isolock init` minimal-setup scaffolding - To Do (blocked on ISO-33).
- **ISO-35**: Adoption scaffolding guidance - To Do (blocked on ISO-34).

## Status

Registered 2026-11-24 from a review of open issue #30. ISO-30 and ISO-31 were carried as
"open questions" in `docs/architecture/cargo-git-dependencies.md` after the git dependency
work merged (PR #29); they are correctness gaps, not ergonomics, and are ranked first
because a build that fails is worse than a build that is tedious to set up.
