# Design note: Cargo git dependencies in a locked profile

**Status:** proposal, not implemented.
**Decision taken:** posture **A**—the CAS digest of the ingested commit tree is the
integrity authority. See §3.4 for what that does and does not prove.
**Scope:** `isolock-plan` (lockfile ingestion and derivation) and the `isolock` Cargo
materializer.
**Goal:** stop rejecting Cargo git dependencies at plan time, without weakening the v1
guarantee that a restored profile builds from reviewed bytes only.

## 1. The current behaviour

`crates/isolock-plan/src/profile/lockfiles.rs` refuses any non-registry package:

```rust
if !origin.is_registry() {
    return Err("Cargo git dependencies must be vendored into a reviewed registry before locked planning".into());
}
```

Registry packages become one `Artifact` each, carrying the `.crate` SHA-256 from
`Cargo.lock`. Cargo materialization unpacks that `.crate`, rebuilds
`.cargo-checksum.json`, and points a `source.crates-io → isolock-vendor` replacement at the
directory. `cargo build --locked --offline` then resolves entirely from local bytes.

Git packages have no `checksum` field at all, which is the whole problem: there is no content
hash in `Cargo.lock` to pin. A git package's only identifier is a commit, and a commit
identified by `rev =` is a *request*, not a hash of the bytes we would build.

## 2. What I verified empirically

I built a throwaway app depending on a local git crate and tested each candidate mechanism
with a fresh `CARGO_HOME`. Three plausible designs are wrong:

| Approach | Outcome |
| --- | --- |
| Recreate `.cargo-checksum.json`, keep the `[dependencies]` git entry, `[patch]` it | **Fails.** `[patch]` still resolves the original git source, so offline resolution dies before the patch applies. |
| Write a `[source."git+URL?rev=X#SHA"]` stanza by hand | **Fails** unless a `Cargo.lock` is already present: "requires a lock file to be present first." |
| `cargo vendor` with a generated lockfile, then replace the source | **Works.** Built and ran offline with the git repository deleted from disk. |

The working shape is what `cargo vendor` emits:

```toml
[source."git+https://example.test/org/lib?rev=REV"]
git = "https://example.test/org/lib"
rev = "REV"
replace-with = "isolock-vendor"

[source.isolock-vendor]
directory = ".isolock/vendor/cargo/KEY"
```

Two layout facts also matter, both observed rather than assumed:

- Vendored directory names are `<crate>`, or `<crate>-<version>` when two versions of one
  crate coexist. `payloads.rs::cargo_package` currently demands exactly one entry named
  `{name}-{version}`, so a naive lookup breaks on the single-version case and on
  renamed packages.
- `cargo vendor` writes a `.cargo-checksum.json` whose own `$comment` states it "only
  protects against accidental modifications. It is not a security mechanism and does not
  protect against malicious changes." It cannot be the integrity anchor.

## 3. Proposed design

### 3.1 Ingestion, per git dependency

A git package is ingested as a **commit-tree archive**, not a crate:

- Resolve `rev` to a full commit SHA, and apply the validation idiom that already exists in
  `infrastructure/transport.rs::git_snapshot`: reject sources or references beginning with
  `-`, containing `\0`, `\n`, `\r` or `:`, and require the resolved commit to be 40 or 64
  ASCII hex digits. Pass `--` before the source argument to `git` so a source cannot be
  parsed as a flag.
- Export that single commit's tree (`git archive <sha>`) and store it in CAS. **The digest of
  that archive is the pin.**
- Record the resolved SHA, the repository URL, and the requested `rev` in the artifact's
  existing `metadata` field.

Pinning is a property of the *archive*, not of the commit: the digest identifies bytes we
hold, and the commit SHA is recorded as provenance for a human reviewer.

### 3.2 Planning

`lockfiles.rs::cargo` branches on the origin scheme. A non-registry source is accepted only
when it parses as `git+URL?rev=X#SHA` (or `?tag=` / `?branch=`), and **only the trailing
`#SHA` fragment is authoritative**; it must be 40 or 64 hex digits. The query parameters are
advisory provenance.

`Artifact` needs no new fields. It already carries `metadata: serde_json::Value`,
`integrity: Option<String>` and `destinations`, all `#[serde(default)]`, so a git artifact
serialises as:

```json
{
  "kind": "cargo",
  "name": "lib",
  "version": "0.1.0",
  "filename": "lib-<sha>.tar",
  "source": "git+https://example.test/org/lib?rev=<sha>#<sha>",
  "metadata": {
    "origin": "git",
    "repository": "https://example.test/org/lib",
    "revision": "<sha>",
    "requested": "rev"
  }
}
```

`metadata` is already part of `configuration_digest`, so git pins are covered by the existing
derivation identity without a schema bump. `source` still passes through `public_source`,
which redacts credentials from URLs.

Failure stays closed for anything that cannot be pinned: a source with no `#SHA`, a
relative-path dependency, or a `path =` entry.

### 3.3 Materialization

The codebase has **two** Cargo config writers and only the second is affected:

- `execution/cargo.rs::prepare` writes the proxy's sparse-registry source replacement. Git
  dependencies never reach it.
- `materialize/payloads.rs::cargo_config` writes the materialised profile's vendor config.
  This is the one that must learn about git sources.

Four changes there:

1. **Branch the source loop on scheme.** Today every non-crates.io
   `ArtifactKind::Cargo` artifact does
   `artifact.source.strip_prefix("registry+").unwrap_or(&artifact.source)` and is emitted as
   `isolock-<hash> { registry = … }`. A `git+…` source falls through `unwrap_or` and produces
   a stanza Cargo rejects, because a git URL is not a registry. Git artifacts must emit
   `{ git = …, rev = … , replace-with = … }` under the key
   `git+URL?rev=REV` instead.
2. **Handle the vendored directory name.** `cargo_package` must accept the `<crate>` form as
   well as `<crate>-<version>`, and must not assume the directory matches the lockfile's
   `name`/`version`—a `package =` rename means they differ. Read the `[package]` table from
   the unpacked `Cargo.toml` rather than trusting the artifact fields.
3. **Skip the crate-specific post-processing for git trees.** A commit tree has no
   `.cargo-checksum.json` to rewrite, and rewriting one would imply a verification story
   Cargo's own comment disclaims. Unpack, place, done.
4. **Require a `Cargo.lock`.** Vendor source replacement cannot bootstrap without one, per
   §2. The profile already locks the lockfile as an input, so this is a validation with a
   clear error, not new machinery.

### 3.4 The integrity posture: A, and what it proves

Source replacement rewrites the `git+URL` source to a local directory. Cargo therefore never
contacts the remote—that is what makes offline builds work—and the practical consequence
is that **Cargo performs no verification of a vendored git source at all**. Whatever bytes
sit in that directory are the dependency.

Under posture A, the authority is the CAS digest of the commit tree produced at ingest:

- `isolock sync` verifies each artifact digest against the manifest before materialising. A
  tampered or corrupt vendor tree fails there, before Cargo is invoked.
- The manifest records the resolved commit SHA for human review.
- The digest is what the plan pins; the SHA is provenance.

**What this does not prove**, stated plainly so nobody over-reads it: that the vendor tree
corresponds to commit `SHA` under git's object model. Commit identity is not a tree hash, so
proving that would need a second verification path (re-exporting the tree, or `git
hash-object` over the staged files). Posture B would add that.

I am recommending against B for this increment, and the reason is not effort. B's
cross-check would confirm the tree matches a commit that *we* resolved at ingest, so it
bottoms out in the same trust root as the CAS digest. It buys an audit affordance, not a
stronger guarantee. If that audit trail becomes desirable, the honest place for it is a
separate, reviewable step—never a second verification path in the materialisation hot
path.

## 4. What must not change

- Registry-only profiles keep the current code path and byte-identical output. In particular
  the `cargo_config` loop must behave exactly as today when no git artifacts are present.
- `cargo build --locked --offline` remains the consumer contract.
- No new network access at sync or exec time. Acquisition stays an explicit `ingest` step.
- `StorageIdentity` is **not** the place for this. It is single-valued per manifest and
  describes the profile's own storage transport (`directory` or `git`, with one repository,
  reference and commit). N git dependencies cannot be represented in it, and overloading it
  would conflate profile provenance with dependency provenance. Git dependency identity lives
  in each artifact's `metadata`.

## 5. Schema compatibility

No schema bump is required, and that is a deliberate outcome of the design:

- `Artifact` already has `metadata`, `integrity` and `destinations`, all
  `#[serde(default)]`, so manifests written by this change remain readable by existing code
  and vice versa.
- `metadata` participates in `configuration_digest`, so a git pin changes the digest exactly
  as it should, while a registry-only profile's digest is unchanged.
- `PROFILE_SCHEMA_VERSION` stays at 1. If implementation finds it needs new *fields* rather
  than `metadata` entries, stop and revisit this note first—that would be a breaking
  change to a `deny_unknown_fields` struct and deserves its own decision.

## 6. Test plan

- **Unit, ingest:** a commit-tree archive round-trips through CAS; a malformed SHA is
  rejected; a source beginning with `-` is rejected; a `branch` that resolves to a SHA pins
  that SHA.
- **Unit, planning:** a `git+…#SHA` source yields the expected `metadata` and digest; a
  `git+…` source with no `#SHA` still fails closed; a registry-only lockfile produces
  artifacts identical to today's.
- **Unit, config generation:** `payloads::cargo_config` emits one `git = …` stanza per git
  source and no `registry = "git+…"` stanza; a registry-only artifact list yields
  byte-identical TOML to today.
- **Materialization, fake ports:** the vendored directory name matches the crate's own
  `Cargo.toml`, including the `<crate>` (single-version) and renamed-package cases.
- **Real end-to-end:** mirror
  `real_standalone_rust_builds_a_locked_registry_dependency_offline` with a git dependency,
  building offline with the origin repository removed, asserting the binary's output.
- **Negative:** mutate one vendored file and assert `sync` fails before Cargo runs.

## 7. Sequencing

Four reviewable increments, each green on its own. **Ingestion comes first**, because it is
the smaller change and it produces the real commit-tree fixtures that planning and
materialisation tests need; doing planning first would mean hand-writing archive fixtures
that get deleted.

1. **Ingestion** produces commit-tree archives, with the `transport.rs` validation idiom.
2. **Planning** accepts and validates pinnable git sources, emitting artifacts that carry git
   `metadata`. Still fails closed for anything unpinnable.
3. **Materialization** writes the vendor layout, handles both directory-naming forms and
   renamed packages, and emits the git source stanzas.
4. **The real offline end-to-end test**, plus documentation replacing the "fails closed"
   boundary note in `profiles.mdx`.

## 8. Open questions

- Should `branch = "main"` be permitted at plan time provided it resolves to a SHA, or
  rejected as unreviewable? Resolving at plan time and storing only the SHA is consistent
  with how `transport.rs` already treats a storage reference, but it does make the plan a
  function of remote state at plan time.
- Do we need to support `[patch]`-style substitutions from user lockfiles? Out of scope for
  the first increment, and §2 suggests they cannot work hermetically anyway.
- Transitive git dependencies: a git crate's own `Cargo.lock` may pull further git sources.
  `cargo vendor` vendors the whole graph, so ingestion must walk it rather than following
  only first-level dependencies.
