---
id: ISO-22
title: Establish selected strict Clippy and public API quality policy
status: Done
assignee: []
created_date: '2026-11-18 11:00'
labels: []
milestone: m-6
dependencies: [ISO-19, ISO-20, ISO-21]
type: task
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Promote a selected set of high-value Clippy lints after resolving current findings. Tighten function-size thresholds gradually, prevent production unwrap/expect/debug/todo regressions, document public `Result` errors, and record intentional exceptions instead of enabling broad noisy Clippy groups.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria

<!-- AC:BEGIN -->
- [x] #1 Workspace lint policy denies production `unwrap`, `expect`, `dbg!`, `todo!`, and `unimplemented!` while retaining existing test-only allowances where justified.
- [x] #2 `too_many_lines`, `too_many_arguments`, and cognitive-complexity thresholds are tightened after M6 refactors and remain enforced by the standard `-D warnings` command.
- [x] #3 Public fallible APIs document their error conditions or carry a reviewed local lint exception.
- [x] #4 Resolved findings cover `format_push_string`, `map_unwrap_or`, `filter_map_next`, unused helper receivers, and checked size conversion where applicable.
- [x] #5 Archive extension case policy is explicit; a targeted lint allowance is documented if case-sensitive validation is intentional.
- [x] #6 Package keywords/categories and Cargo metadata policy are completed or `clippy::cargo` remains an audit-only command with documented exclusions.
<!-- AC:END -->

## Notes

- Do not enable `clippy::pedantic`, `clippy::cargo`, or `clippy::restriction` wholesale as blocking CI policy.
- `cargo deny` remains the authoritative dependency-license, advisory, source, and duplicate-version policy check.

## Implementation evidence

Selected workspace lints deny production panic/debug placeholders and enforce documented
public errors, checked narrowing, bounded stack arrays, and the requested cleanup lints.
Thresholds are 70 lines / 5 arguments / cognitive complexity 15. Test-helper and end-to-end
fixture exceptions are local and explained. `RegistryArchive::from_filename` centralizes
exact lowercase suffix recognition with one documented allowance and case-boundary tests.
Workspace keywords/categories are inherited by all five packages; broad Cargo lints remain
audit-only (existing transitive duplicates), with cargo-deny policy unchanged. See
`docs/content/docs/quality.mdx`. Standard all-target `-D warnings` validation passes.
