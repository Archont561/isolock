---
id: doc-3
title: Git orphan branch storage and epoch rollover
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-18 12:00'
---

## Implemented storage boundary

The reviewed ingestion/publication path is implemented by the `isolock` CLI, not by the
CAS sharding library. See the [ingestion guide](../../docs/content/docs/ingestion.mdx),
[epoch policy guide](../../docs/content/docs/storage-epochs.mdx), and
[executed rollover contracts](../features/git-orphan-branch.feature).

1. **Dedicated storage generation:** `push` targets only a positive `isolock/vN` branch.
   The first commit has no code parent; subsequent ordinary commits parent the current
   remote tip. The default is `storage.reference = "isolock/v1"`.
2. **No producer mutation:** a disposable Git repository and private index isolate objects,
   refs, fetch/shallow state, and snapshot staging. Producer HEAD, branch refs, index,
   configuration, staged/unstaged changes, and untracked files remain unchanged. Original
   object hashing bypasses producer clean/text filters.
3. **Reviewed raw snapshot:** the exact format marker, regular `cas/sha256` originals,
   immutable ingestion records/batches, and metadata-preserving `plan.json` are published.
   Profile publication separately adds verified immutable manifests and aliases. Existing
   unmarked, code-tree, unsafe-mode, or legacy-sharded trees are not automatically adopted.
4. **Linear non-force audit:** subjects use `chore(deps): add REGISTRY/NAME==VERSION` or
   `chore(deps): add profile/PROFILE-PLATFORM`. Identical trees are no-ops when budgets fit.
   Concurrent appends reject without force; ordinary retry reloads the current tip.
5. **Pinned profiles:** publish all payloads and reviewed input originals first, lock the
   profile to that commit, then publish its manifest. Publication verifies exact originals
   against the locked commit and checks the destination storage source.

Keeping storage off code branches prevents working-tree pollution. It does not imply that
an all-branch Git clone avoids storage transfer. Consumer Git fallback remains explicitly
broader-snapshot transfer; profile selection is local, not blob-selective networking.

## Budget definitions

Individual originals must be regular files no larger than 95 MiB. Encoded storage metadata
has a 4 MiB limit. Without an epoch policy, publication enforces the absolute 1,500 MiB
live-tree ceiling but does not claim a cumulative-history limit.

`push --epoch-policy FILE` accepts a strict schema-1 reviewed policy containing exact
`source-ref`/`source-commit`, immediate `next-ref`, positive `max-live-bytes` and
`max-history-bytes` (each at most 1,500 MiB), and explicit `retain-batches` and
`retain-manifests` digest lists. The CLI never invents these values or updates a stale pin.

- **Live tree:** sum sizes per snapshot path, including duplicate alias/object bytes.
- **Cumulative history:** sum distinct reachable Git blob objects across the complete
  source epoch history plus the proposed tree. Superseded/deleted blob versions still count.

Full history is fetched into the isolated publisher; shallow storage remotes are rejected.
These are not pack/disk-size, commit/tree-object, unreachable-transfer, or global archived-ref
budgets. Archived generations are retained indefinitely, so total storage is not bounded.

## Reviewed rollover and retention

When both budgets fit, append normally. Otherwise build a fresh orphan successor containing:

- the incoming batch and explicitly retained reviewed batches;
- every published profile alias, its immutable manifest/constituent graph, and extra
  immutable roots explicitly selected by digest;
- independently verified original/input bytes and matching review/provenance records;
- a regenerated legacy plan and an `epoch.json` policy/batch migration record.

Provenance batch objects can also describe non-retained originals; explicitly selecting
such a batch later requires its complete original closure. Profile payload commits must
remain reachable from their original storage refs, including references to earlier epochs.
Missing/corrupt mandatory closures or an oversized retained result fail before publication.
The incoming verified batch can explicitly supply missing originals, but never overwrite
existing different immutable bytes. No required profile is dropped to meet a budget.

Create a source seal commit whose parent is the exact reviewed tip and whose tree is
unchanged. A structured `Isolock-Epoch-Seal:` commit-message trailer identifies the policy,
incoming batch, and successor root without adding source blob bytes. Atomically non-force
push the source seal and orphan target together. A no-op source refspec would not guard a
race; the meaningful seal update does. No non-atomic fallback, force-push, archived-ref
removal, or code/configuration rewrite is permitted.

## Failure recovery and explicit adoption

Source/target races, receiver rejection, and lack of atomic support cannot publish half
the reference transaction. A connection failure after acceptance has an unknown outcome;
it must not be reported as proof that no refs changed. The same reviewed retry verifies
seal ancestry/tree, target orphan identity/record, budgets, the complete mandatory closure,
and target-root reachability. It is a no-op without producer originals/CAS, even after
valid target appends. Changed policy/batch/history and incomplete targets fail closed.

Sealed sources reject normal writes with successor guidance. Neither the publisher nor
configuration loader silently changes the active generation. Immutable profiles retain
their original storage identities and payload commits; old locked consumers continue
using the retained old refs. New-ref adoption requires an explicit profile replan and
separate publication, followed by reviewed project configuration changes.

## Unsupported conversion and remaining boundaries

Legacy/unmarked/sharded or unsafe snapshots receive actionable verified-raw conversion
instructions. Independently review/re-ingest raw originals, then explicitly publish into
an empty reviewed `isolock/vN`; keep the old ref. This is not automatic interpretation or
sealing of an unsupported format. Directory transport and FileStore APIs remain available
where the Git original-size limit is unsuitable. CAS sharding is not sharded Git publication.

Direct registry solving, selective transfer, connected proxy misses, automatic format
conversion, profile-alias retirement, and archived/shared-CAS root retirement remain
separate work. No binaries are added to `main` or actual artifact storage by the tests.
