# Design note: Cargo git dependencies in a locked profile

**Status:** proposal, not implemented.
**Scope:** `isolock-plan` (lockfile ingestion and derivation) and the `isolock` Cargo
materializer.
**Goal:** stop rejecting Cargo git dependencies at plan time, without weakening the v1
guarantees that a restored profile builds from reviewed bytes only.

## 1. The current behaviour

`crates/isolock-plan/src/profile/lockfiles.rs` refuses any non-registry package:

```rust
if !origin.is_registry() {
    return Err("Cargo git dependencies must be vendored into a reviewed registry before locked planning".into());
}
```

Registry packages become one `Artifact` each, carrying the `.crate` SHA-256 from
`Cargo.lock`. Cargo materialization unpacks that `.crate`, rebuilds
`.cargo-checksum.json`, and points a `source.crates-io → isolate-vendor` replacement at the
directory. `cargo build --locked --offline` then resolves entirely from local bytes.

Git dependencies have no `checksum` field at all, which is the whole problem: there is no
content hash in `Cargo.lock` to pin.

## 2. What I verified empirically

I built a throwaway app depending on a local git crate and tested each candidate mechanism
with a fresh `CARGO_HOME`. Results, because three plausible designs are wrong:

| Approach | Outcome |
| --- | --- |
| Recreate `.cargo-checksum.json`, keep the `[dependencies]` git entry, `[patch]` it | **Fails.** `[patch]` still requires Cargo to resolve the original git source, so offline resolution dies before the patch applies. |
| Write a `[source."git+URL?rev=X#SHA"]` stanza by hand | **Fails**, unless a `Cargo.lock` is already present: "requires a lock file to be present first." |
| `cargo vendor` with a generated lockfile, then replace the source | **Works.** Built and ran offline with the git repository deleted from disk. |

The working shape is exactly what `cargo vendor` emits:

```toml
[source."git+https://example.test/org/lib?rev=REV"]
git = "https://example.test/org/lib"
rev = "REV"
replace-with = "isolock-vendor"

[source.isolock-vendor]
directory = ".isolock/vendor/cargo/KEY"
```

Two layout facts also matter:

- Vendored directory names are `<crate>`, or `<crate>-<version>` when two versions coexist.
  The current Cargo materializer asserts "archive must contain exactly its `name-version`
  directory", so a naive `format!("{name}-{version}")` lookup breaks.
- `cargo vendor` writes a `.cargo-checksum.json` whose own `$comment` says it "is not a
  security mechanism". It cannot be the integrity anchor. Integrity must come from the CAS
  digest of the exact commit tree.

## 3. Proposed design

### 3.1 Ingestion, per git dependency

A git package is ingested as a **commit-tree archive**, not a crate:

- Resolve `rev` to a full commit SHA. Reject anything that is not 40 hex characters after
  resolution; a branch or tag name is never stored.
- Export that single commit's tree (`git archive <sha>`) and store it in CAS. The digest of
  that archive is the pin.
- Record the resolved SHA, the repository URL, and the requested `rev` alongside it.

This keeps one artifact per dependency and reuses the existing blob, lease, and GC paths.

### 3.2 Planning

`lockfiles.rs` accepts a non-registry source only when it matches
`git+URL?rev=X#SHA` (or `?tag=` / `?branch=`, each of which must still resolve to a pinned
`#SHA`). It then emits an `Artifact` of kind `Cargo` whose metadata carries:

```json
{
  "origin": "git",
  "repository": "https://example.test/org/lib",
  "revision": "<40-hex sha>",
  "filename": "lib-<sha>.tar"
}
```

Failure stays closed for anything that cannot be pinned: a source with no `#SHA`, a
relative-path dependency, or a `path =` entry.

### 3.3 Materialization

1. Unpack the commit-tree archive to a stage.
2. Place it under the vendored directory name Cargo will look for. Because the name is
   `<crate>` or `<crate>-<version>`, read the crate's own `Cargo.toml` for its version
   rather than trusting the lockfile's field.
3. Emit `[source."git+…"]` stanzas plus one `[source.isolock-vendor]` stanza in the profile's
   generated Cargo config, alongside the existing `crates-io` replacement.
4. Require a `Cargo.lock` to exist in the profile inputs. This is not optional: the empirical
   result above shows vendor source replacement cannot bootstrap without one. The profile
   already locks `cargo-lock` as an input, so this is a validation, not new machinery.

### 3.4 The one trust decision that needs your input

Source replacement rewrites the `git+URL` source to a local directory. Cargo therefore does
**not** contact the remote, which is what makes offline builds work, and also means Cargo's
own view is "the vendored directory is that source."

Two postures are available:

- **A. Digest-only (simplest).** Treat the CAS digest of the commit-tree archive as the
  authority. `isolock sync` verifies the archive digest, so a tampered vendor tree fails
  before Cargo runs. The manifest records the resolved SHA for review.
- **B. Digest plus SHA cross-check.** Additionally verify that the tree Cargo will consume
  really is commit `SHA`. Commit identity is not a tree hash, so this requires either
  re-exporting and comparing, or shelling out to `git hash-object` over the staged tree.

I recommend **A** for the first increment, and B only if you want an audit trail that ties
the vendored bytes back to the commit rather than to the ingest artifact. A is honest about
what it proves; B is stronger but adds a second verification path. The note should not claim
B's guarantee while implementing A.

## 4. What must not change

- Registry-only profiles keep the current code path and output.
- `cargo build --locked --offline` remains the consumer contract.
- The derivation key must gain the git pins (repository, resolved SHA) so two different
  commits never share a vendor tree. Today the key covers lock digest, Cargo version,
  target, features, all-features, and schema version.
- No new network access at sync or exec time. Acquisition stays an explicit `ingest` step.

## 5. Test plan

- **Unit, ingest:** a commit-tree archive round-trips; a bad SHA is rejected; a branch name
  that does not resolve is rejected.
- **Unit, planning:** a `git+…#SHA` source yields the expected artifact metadata; a
  `git+…` source with no `#SHA` still fails closed.
- **Materialization, fake ports:** the generated `.cargo/config.toml` contains one stanza
  per git source plus the vendor stanza, and the vendored directory name matches the crate's
  own version.
- **Real end-to-end:** mirror `real_standalone_rust_builds_a_locked_registry_dependency_offline`
  with a git dependency, building offline with the origin repository removed and asserting
  the binary's output.
- **Negative:** mutate one vendored file and assert sync fails before Cargo runs.

## 6. Effort and sequencing

Four reviewable increments, each green on its own:

1. Planning accepts and validates pinnable git sources, emitting artifacts. Still fails
   closed for anything unpinnable.
2. Ingestion produces commit-tree archives.
3. Materialization writes the vendor layout and source stanzas.
4. The real offline end-to-end test, plus docs replacing the "fails closed" boundary note in
   `profiles.mdx`.

Rough size: increments 1 and 3 are the substantial ones; 2 and 4 are small.

## 7. Open questions

- Should `branch = "main"` be permitted at plan time provided it resolves to a SHA, or
  rejected as unreviewable? Current v1 posture rejects unpinnable inputs, so resolving at
  plan time and storing only the SHA is consistent, but it does mean the plan is a function
  of remote state at plan time.
- Do we need to support `[patch]`-style substitutions from user lockfiles? I would scope them
  out for the first increment.
