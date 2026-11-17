---
id: doc-6
title: Pure POSIX shell environment bootstrapping
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Pure POSIX Shell Environment Bootstrapping

### Problem Statement

Precompiled native bootstrapping binaries (for example Go, Rust, or Python wheels) create a chicken-and-egg problem: how do you run the bootstrap tool before having a verified toolchain installed on an air-gapped machine or minimalist container?

### Layout and Manifest

The bootstrap template expects an environment manifest at `.isolock/environments/<environment>.manifest`. Each non-comment line contains a lowercase SHA-256 digest and a safe archive basename, separated by whitespace:

```text
# SHA-256 digest  archive filename
<64-character-lowercase-sha256> toolchain.tar
```

The corresponding archive is stored under `.isolock/blobs/sha256/<first-two-digest-characters>/<next-two-characters>/<full-digest>/<filename>`. The same tree can live at the root of a checked-out storage branch as `blobs/` and `environments/`. Archive basenames use ASCII letters, digits, periods, underscores, plus signs, and hyphens; they cannot contain whitespace or path separators. Each tar archive must unpack a top-level `bin/` directory; the host `tar` must support the archive's compression format, if compressed.

### Operations Flow

1. **Local restore**: `sh .isolock/bootstrap.sh [environment]` reads the local manifest and CAS. The environment name defaults to `active`.
2. **Remote restore**: `sh .isolock/bootstrap.sh --remote [environment]` shallow-fetches `refs/heads/isolock/v1`, then materializes its top-level `blobs/` and `environments/` trees beneath `.isolock/` without checking out the storage branch.
3. **Integrity and unpacking**: Existing archives are verified directly. Shards named `<filename>.part000`, `<filename>.part001`, and subsequent numbered files are concatenated in lexical order with POSIX `cat`; the reconstructed archive is checked against its manifest digest using `sha256sum` (or `shasum -a 256` on systems that provide the macOS utility instead). Only verified archives are unpacked into a staging environment.
4. **Activation**: On success, the script installs the environment under `.isolock/state/<environment>/` and writes an `activate` file that prepends its `bin/` directory to `PATH`. Source it with `. .isolock/state/<environment>/activate`.

The script uses `/bin/sh`, `git` for `--remote`, `tar`, a SHA-256 utility, and standard POSIX utilities. Local restore does not access the network. A digest verifies that bytes match the manifest; the manifest itself must come from a trusted source.

### Reproducibility

The bootstrap flow uses no language runtime or Isolock executable. It restores declared toolchain archives deterministically, rejects missing or corrupt artifacts, and preserves the project's ordinary working tree when fetching the orphan storage branch.
