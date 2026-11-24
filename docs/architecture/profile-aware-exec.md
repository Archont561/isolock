# Design note: Profile-aware `exec`

**Status:** proposal, not implemented.
**Decision taken:** `exec` (and `serve`) load the selected profile's locked manifest and
project it into the existing `AirlockPlan`+CAS shape, rather than teaching the proxy a
second plan format. The materialized profile prefix supplies `PATH`.
**Scope:** `crates/isolock` configuration/session projection and the `exec`/`serve` CLI path.
**Task:** ISO-33.

## 1. The current behaviour

`exec` and `serve` consume the legacy airlock plan at `.isolock/plan.json`
(`infrastructure/execution/inputs.rs`), while a profile repo produces
`.isolock/plans/${profile}-${platform}.lock`. A repo that follows the profile guide and
then runs `isolock exec cargo add serde` gets:

```text
isolock: cannot start "cargo": No such file or directory (os error 2)
```

Two separate causes, and both must be fixed for the use case to work:

1. **No plan.** The proxy serves an empty snapshot, so no artifact is available to
   `cargo add` even if cargo resolved.
2. **No toolchain.** The child inherits the caller's `PATH`. Nothing puts the restored
   profile prefix on it, so `cargo` is not found.

Verified 2026-11-24 on a profile-configured directory.

## 2. Why this is not only a `PATH` tweak

Putting the prefix on `PATH` alone would let `cargo add` run and then fail on a registry
the proxy cannot serve, which is a worse failure than "command not found" because it looks
like a network problem. The proxy snapshot and the toolchain must come from the same
selection, or the session is inconsistent.

## 3. Design

### 3.1 Selection

`exec` reuses the same explicit `--profile`/`--platform` selection `sync` uses. When
neither selects a profile, behaviour is exactly today's: legacy `.isolock/plan.json`, host
`PATH`, unchanged output and exit status. This keeps AC #2 satisfiable by construction
rather than by careful cloning of the old path.

### 3.2 Plan projection, not a second format

The proxy already understands `AirlockPlan` plus a CAS. A `ProfileManifest` contains
`Vec<Artifact>`, each with kind, name, version, digest, filename, source and metadata, and
`crate::infrastructure::ingestion::publish::merge_artifact` already projects only
*registry* artifacts into a plan.

For `exec` the snapshot must answer `cargo add`/`pixi add`, so the projection includes
every artifact that has a registry identity (Conda, Cargo registry, wheel, npm), and
excludes seeds and runtimes: those are toolchain prefixes, not packages a
package manager asks the proxy for. A git artifact is excluded too, because its
`[source]` stanza is local and `cargo add` cannot request it by name.

Because the plan is derived, `exec` must not read or write `.isolock/plan.json`. Writing
one would turn a session into a state mutation and could disagree with planning.

### 3.3 CAS

The session fills its in-memory CAS from the same `storage.cas_dir` the profile used, for
every digest in the projected plan. A digest missing from CAS stays missing, exactly as the
legacy path already tolerates: the proxy answers a miss with a miss.

### 3.4 Environment

`PATH` gains the materialized prefix for the selected profile/platform, resolved through
the same digest-addressed path `sync` writes
(`.isolock/profiles/{profile}/{platform}/{digest}/bin`).

Two rules:

- The prefix must already exist. If it does not, fail closed with guidance to run `sync`
  (AC #3) rather than falling back to the host toolchain.
- `CARGO_HOME` for a profile session must be the profile's own `cargo-home`, not the
  ambient one, so `cargo add` writes Cargo config inside the profile rather than into a
  user's home.

## 4. Alternatives rejected

**Teach the proxy to read `ProfileManifest` directly.** Duplicates the artifact model in a
third crate and forces every proxy code path to know about profile structure. The
projection is a small, tested adapter at one boundary.

**Synthesize and write `.isolock/plan.json` at `sync` time.** Makes `sync` mutate
legacy-plan state, which `plan --ci` and drift detection also read. Two writers to one
file with different identities is how drift detection starts lying.

**Require the user to `exec` inside an activated shell.** Pushes the same work onto every
caller and still leaves the proxy serving an empty snapshot.

## 5. Risks

- **Projection drift.** A new `ArtifactKind` could be added without a decision about
  whether the proxy serves it. Mitigation: an exhaustive match on kind, so a new variant
  fails to compile.
- **Two Cargo homes.** A profile with a Cargo derivation has a profile-scoped
  `.cargo/config.toml`. `exec` must not point `CARGO_HOME` at the wrong one. Mitigation: a
  test that runs `cargo add` in a profile session and asserts the config that was written.
- **Silent host fallback.** The most dangerous outcome is a session that "works" because
  the child found a host cargo. Mitigation: fail closed when the prefix is absent, and a
  test that asserts the exact error.

## 6. Acceptance mapping

| AC | How the tests prove it |
| :--- | :--- |
| #1 profile-aware PATH/CARGO_HOME/proxy | `exec` in a materialized profile finds the profile's `cargo` and the generated Cargo home |
| #2 legacy unchanged | existing session suite passes byte-for-byte; no selection means the old path |
| #3 fail closed | `exec` with a planned but unresolvable profile prints guidance and exits non-zero |
| #4 `cargo add`/`pixi add` offline | hermetic test with upstream unreachable resolves a package from the proxy |
| #5 child status preserved | existing tests plus a `cargo add` failure that returns cargo's own status |
