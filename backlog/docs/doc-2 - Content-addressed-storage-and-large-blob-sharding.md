---
id: doc-2
title: Content addressed storage and large blob sharding
type: architecture
created_date: '2026-11-17 11:00'
updated_date: '2026-11-17 11:00'
---

## Content-Addressed Storage and Large Blob Sharding

### Problem Statement

Git repositories and standard Git hosting services enforce file size limits (typically 100 MiB). Machine learning libraries, CUDA binaries, and fat wheels frequently exceed 100 MiB (for example PyTorch, TensorFlow, LLVM toolchains).

### Architectural Solution

1. **Deterministic Sharded Path Hierarchy**:
   Every artifact is stored at:

   ```text
   blobs/sha256/<hash[:2]>/<full_hash>/<filename>
   ```

2. **Size Ceiling (95 MiB)**:
   If an incoming archive exceeds 95 MiB (99,614,720 bytes):
   - The file is chunked into sequential 95 MiB slices:
     `<filename>.part000`, `<filename>.part001`, `<filename>.part002`, …
   - An index manifest `<filename>.shards.json` is generated listing chunk digests and ordering.

3. **Zero-Dependency Reassembly via POSIX `cat`**:
   During environment bootstrap or proxy delivery, chunks are concatenated into the full file:

   ```sh
   cat "$blob_dir/$filename.part"* > "$dest_dir/$filename"
   ```

   The reconstructed file is then validated against its top-level SHA-256 digest.

4. **Deduplication**:
   Files with identical SHA-256 digests coalesce onto the same directory structure, preventing redundant storage across environments.

### Shard Manifest Specification (`<filename>.shards.json`)

When an artifact exceeds 95 MiB, an index manifest is stored alongside the part files:

```json
{
  "schema_version": 1,
  "original_filename": "pytorch-2.4.0-cuda.whl",
  "original_size_bytes": 199229440,
  "original_sha256": "4a7c1b82e2c8...",
  "chunk_size_bytes": 99614720,
  "chunks": [
    {
      "index": 0,
      "part_filename": "pytorch-2.4.0-cuda.whl.part000",
      "size_bytes": 99614720,
      "sha256": "9f8c12a4b1..."
    },
    {
      "index": 1,
      "part_filename": "pytorch-2.4.0-cuda.whl.part001",
      "size_bytes": 99614720,
      "sha256": "3e2a01d5c7..."
    }
  ]
}
```
