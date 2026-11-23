---
id: doc-5
title: Lockfile plan drift and CI airlock guard
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-18 12:00'
---

## Implemented planning boundaries

Without profile selection or project configuration, `isolock plan` reads an expected
JSON `AirlockPlan` from `.isolock/plan.json` (or `ISOLOCK_PLAN`) and verifies its originals
against the local CAS. A missing plan fails closed. Missing bytes produce registry-grouped
dispatch guidance; `--ci` exits 1 for drift, while workstation inspection reports it and
exits 0. Neither mode discovers dependencies from arbitrary project manifests or resolves
upstream metadata.

Explicit profile planning/checking uses the supported lockfile adapters, reviewed catalogs,
configuration, and input digests described in [profiles](../../docs/content/docs/profiles.mdx).
Locked checks do not change the closure or fetch upstream packages. Configured
Git storage is pinned to a payload commit; explicit storage transfer is separate from
registry acquisition.

## Reviewed dispatch ingestion

Drift guidance first requires reviewing and committing `isolock.ingest.json` with exact
names, versions, sources, filenames, and SHA-256 pins. It emits commands such as:

```sh
gh workflow run isolock.yml -f manifest=isolock.ingest.json \
  -f registry=pypi -f packages="fastapi==0.115.0 pydantic==2.9.2"
```

The connected `workflow_dispatch` job restores reviewed tooling, builds the CLI offline,
requires a tracked manifest at the selected code ref, then runs explicit `ingest --download`
and `push` for those exact reviewed selections. Inputs are literal argument entries, not
shell code. It does not solve versions or transitive dependencies. Ingestion verifies the
whole selected batch before CAS import; publication appends a separate non-force storage
commit and preserves the producer checkout/index.

The read-only PR job skips when no `isolock/v1` branch/tag exists. Otherwise it checks the
published `plan.json` against that snapshot's own CAS in an isolated directory. This is
**published snapshot verification**, not an automatic project-lockfile drift solver or
a claim that an unrelated warm producer cache proves storage is complete.

## Actual serialized formats

The legacy execution plan is JSON: an `artifacts` list of name/version/registry/digest
records, with parallel optional `artifact_metadata`. The publisher writes this as
`plan.json` at the storage root, retaining existing entries and metadata. Registry values
in that existing plan type use its serde enum representation; the reviewed ingestion JSON
uses lowercase registry strings instead.

Profile aliases are `.isolock/plans/PROFILE-PLATFORM.lock` JSON with immutable
`.isolock/plans/objects/DIGEST.json` objects. The reviewed ingestion schema is separate.
The `.isolock/isolock.lock` TOML protocol is not implemented.

See [reviewed ingestion and publication](../../docs/content/docs/ingestion.mdx) for the
input schema, cold-consumer path, profile payload-before-manifest rule, storage budgets,
and the explicit deferred registry-resolution boundary.
