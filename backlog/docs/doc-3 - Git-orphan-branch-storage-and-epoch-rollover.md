---
id: doc-3
title: Git orphan branch storage and epoch rollover
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-18 12:00'
---

## Git orphan storage: Implemented scope

The reviewed ingestion/publication path is implemented by the `isolock` CLI, not by the
CAS library. See [the user guide](../../docs/content/docs/ingestion.mdx) and
[executable contracts](../features/reviewed-ingestion.feature).

1. **Dedicated storage generation:** `push` targets only a positive `isolock/vN` branch.
   Its first commit has no code parent; later commits parent the current remote storage
   tip. The default is `storage.reference = "isolock/v1"`, not a `branch` configuration key.
2. **No storage checkout:** a private Git index stages only recognized regular snapshot
   paths. Producer HEAD, branch, real index, staged/unstaged changes, and untracked files
   remain unchanged. Git object/fetch metadata is operational state, not a code commit.
3. **Reviewed raw snapshot:** a format marker, `cas/sha256`, immutable ingestion records
   and batches, and a metadata-preserving legacy `plan.json` are published. Explicit
   profile publication adds existing export-compatible manifest objects and aliases.
   Existing unmarked or legacy-sharded trees are not migrated/adopted.
4. **Linear audit:** subjects use `chore(deps): add REGISTRY/NAME==VERSION`; profile
   publication uses `chore(deps): add profile/PROFILE-PLATFORM`. Identical trees are no-ops.
   Pushes are never forced. A concurrent update rejects publication; retry reloads its tip.
5. **Pinned profiles:** publish all payloads and reviewed input-file originals first,
   lock the profile to that commit, then publish its manifest. Publication verifies that
   the locked commit contains the exact exported originals and matches the destination.

Keeping storage off the code branch prevents binary working-tree pollution. It does **not**
mean a normal all-branch Git clone avoids downloading storage objects. Use deliberate
single-branch/shallow fetches as appropriate; the current Git profile fallback explicitly
reports broader snapshot transfer rather than claiming blob-selective transport.

## Publication limits

The publisher rejects individual originals over 95 MiB and live snapshot trees over
1,500 MiB. Reviewed ingestion JSON, its encoded audit objects, and legacy plan JSON have
4 MiB bounds. The live-tree budget does not cap cumulative historical objects. No binary
archives are added to `main` by this implementation or its disposable-remote tests.

CAS large-blob sharding exists independently, but it is **not** integrated with reviewed
Git publication. Directory transport and the existing FileStore API support other raw
snapshot provisioning workflows; there is no automatic conversion of sharded layouts.

## Deferred epoch rollover

**Automatic rollover is not implemented.** The publisher fails at its budget; it does not
create `isolock/v2`, prune history, or rewrite `isolock.toml`. A reviewed, explicitly chosen
new generation is accepted, but migration/retention across generations is not automated.

[ISO-24](../tasks/iso-24%20-%20Automatic-storage-epoch-rollover-and-migration.md) tracks the
remaining design and implementation. The tagged [rollover feature](../features/git-orphan-branch.feature)
is a deferred specification, not an executed implementation test. It needs a bounded
history policy, reachable-profile migration, atomic/ref-race handling, and cold-consumer
validation before it can be claimed as supported.
