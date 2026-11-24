---
id: ISO-31
title: Explain non-commit git lockfile references instead of rejecting them
status: To Do
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: [ISO-29]
type: task
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`full_commit` in the profile lockfile planner accepts only an already-resolved 40- or
64-character lowercase commit. A lockfile carrying `?tag=...` or `?branch=...` is rejected
with `Cargo git dependencies require a full lowercase commit, not a tag or branch`, which
states the rule but not the remedy. Decide between accepting tag/branch references by
resolving them to a commit and telling the user the exact command that pins them.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Cover the tag and branch branches of `full_commit` with tests that assert the exact message and exit status.
- [ ] #2 Include the crate name, the offending reference, and the resulting `cargo update` invocation needed to pin the commit (for example `cargo update -p NAME --precise <sha>`).
- [ ] #3 Decide and record whether tags are ever resolved implicitly; if not, state the reviewed-pin rationale in the message rather than only the format rule.
- [ ] #4 Keep the rejection fail-closed: no git artifact may be planned from an unresolvable reference.
- [ ] #5 Document the pinned-commit requirement and the remedy in the profiles guide.
<!-- AC:END -->

## Notes

Prefer the remedy over tag resolution. Resolving a tag moves the product toward implicit
resolution, which the reviewed-artifact model excludes; a message that names the fix keeps
the decision with the reviewer.
