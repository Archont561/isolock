---
id: ISO-32
title: Reject sentinel digests in typed configuration
status: To Do
assignee: []
created_date: '2026-11-24 00:00'
labels: []
milestone: m-9
dependencies: []
type: task
ordinal: 32000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
An all-zeros digest satisfies the schema, so `config check` reports the configuration as
valid and `plan` writes a lockfile with exit status 0. Only `sync` fails, long after the
review step that was supposed to catch bad pins. A placeholder that survives validation
and planning is a trap, and it is the shape any scaffolding would naturally emit.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance criteria

<!-- AC:BEGIN -->
- [ ] #1 Reject an all-zeros (and any other non-content) SHA-256 digest during configuration validation with a message naming the artifact.
- [ ] #2 Assert that `config check` fails and `plan` does not write a lockfile for such a configuration.
- [ ] #3 Audit other typed fields for values that validate but cannot be satisfied (empty filenames, placeholder versions) and tighten the ones that behave the same way.
- [ ] #4 Preserve exact output and exit status for every currently valid configuration; this must not reject any real reviewed pin.
<!-- AC:END -->

## Notes

Verified 2026-11-24 on a minimal config:

```toml
[profiles.minimal.platform-tools.linux-64.seeds.isolock]
digest = "0000000000000000000000000000000000000000000000000000000000000000"
```

`config check` printed "Configuration is valid (schema 1).", `plan` exited 0 with
"1 selected artifact(s) are not yet in CAS", and `sync` failed with "no artifact source
configured". Two of three gates passed on a digest that can never resolve.
