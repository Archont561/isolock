---
id: ISO-32
title: Reject sentinel digests in typed configuration
status: Done
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
- [x] #1 Reject an all-zeros (and any other non-content) SHA-256 digest during configuration validation with a message naming the artifact.
- [x] #2 Assert that `config check` fails and `plan` does not write a lockfile for such a configuration.
- [x] #3 Audit other typed fields for values that validate but cannot be satisfied (empty filenames, placeholder versions) and tighten the ones that behave the same way.
- [x] #4 Preserve exact output and exit status for every currently valid configuration; this must not reject any real reviewed pin.
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

Resolved by validating that every seed and runtime digest is content-derived
(`Sha256Digest::is_content_derived`), so an all-zeros digest is rejected during
configuration validation with a message naming the tool. `config check` and `plan` both
fail and no lockfile is written.

The AC #3 audit found the neighbouring fields already sound: an empty filename is
rejected by `filename`, an empty or malformed version by `validate_tools`, and an unsafe
entry path by `relative_path`. The digest was the only field that validated without being
satisfiable, because it is the one field whose legality cannot be judged from its shape
alone.

AC #4 is covered by the existing suite, which builds every profile from digests computed
over real bytes, plus an end-to-end offline Rust build against a restored prefix.
