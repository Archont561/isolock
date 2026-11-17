---
id: doc-3
title: Git orphan branch storage and epoch rollover
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Git Orphan Branch Storage and Epoch Rollover

### Problem Statement

Storing multi-gigabyte binary dependency artifacts directly on `main` or development branches bloats working tree checkouts, causes merge conflicts on binaries, and pollutes developer commit logs.

### Architectural Solution

1. **Dedicated Orphan Branch (`isolock/v1`)**:
   - The orphan branch shares no commit ancestry with code branches (`main`, `develop`).
   - Normal `git clone` or `git checkout main` does not download or check out dependency blobs unless requested.

2. **Git Plumbing Without Checkout**:
   - Isolock invokes system `git` CLI plumbing via `std::process::Command` without working tree pollution.
   - Packages are staged into Git object database using `git hash-object -w` and `git mktree`.
   - Commits are appended using `git commit-tree` with the current remote branch tip as parent.
   - Ref pointer updated via `git update-ref refs/heads/isolock/v1 <commit_sha>`.
   - This leaves developer working trees, index, and `main` untouched.

3. **Conventional Commit Audit Trail**:
   Each ingestion generates a message in the format:

   ```text
   chore(deps): add <registry>/<package>==<version>
   ```

4. **Epoch Rollover Budget**:
   - To keep shallow clone latency bounded, the branch is monitored for cumulative size.
   - When branch size approaches `1,500 MiB`, a new orphan branch `isolock/v2` is initialized.
   - `isolock.toml` records the active branch generation (`branch = "isolock/v2"`).
