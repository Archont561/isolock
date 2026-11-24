---
id: ISO-30
title: Support transitive Cargo git dependencies
status: To Do
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-29]
type: task
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
A git dependency whose own dependencies include another git dependency currently plans
(cargo_lock reports both) but cannot be completed from one reviewed pin per top-level
crate. Cargo.lock lists the nested git package with its own `git+...?rev=...#...` source,
so it needs its own reviewed pin, its own CAS object, and its own vendored tree. Confirm
whether ingest walks that graph, and make the failure appear at plan time rather than as a
broken vendor tree at build time.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Reproduce a nested case in a test: a git crate that depends on a second git crate at a different repository, resolved by cargo_lock with two distinct git sources.
- [ ] #2 Require and validate a reviewed pin for every git source in the lockfile, not only those reachable through a first-level dependency.
- [ ] #3 Materialize each nested commit tree into its own commit-addressed vendor directory and emit one `[source."git+...?rev=..."]` stanza per repository.
- [ ] #4 Prove an offline `cargo build --locked` with every origin repository deleted.
- [ ] #5 Fail at plan time with an explicit message when a git source in the lockfile has no reviewed pin, naming the crate and repository.
<!-- AC:END -->

## Notes

Verified 2026-11-24: `app -> outer(git) -> inner(git)` produces two git sources in
Cargo.lock (`git+file:///tmp/transdep/outer#...` and
`git+file:///tmp/transdep/inner?rev=...#...`). The nested source does not inherit the
outer crate's pin. `docs/architecture/cargo-git-dependencies.md` lists this as an open
question; it is ranked above the ergonomics tasks because the failure is a build error
rather than tedium.
