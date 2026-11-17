---
id: m-1
title: "M1 - CAS engine and blob storage"
---

## Description

Implement Content-Addressed Storage, deterministic SHA-256 digest verification, and large archive chunk sharding.

Exit condition: CAS stores and verifies blobs by SHA-256; files exceeding 95 MiB are chunked and reassembled deterministically.

## Exit criteria

- [x] In-memory CAS store validates 64-hex SHA-256 digests and computes sharded directory paths.
- [x] Blobs over 95 MiB are chunked into `.part*` sequences with `.shards.json` manifests.
- [ ] POSIX `cat` reassembly reconstructs exact byte payload and passes SHA-256 verification.

## Status

**In Progress**. ISO-4 implementation and behavioral coverage are authored; final verification remains pending.

## Tasks

- **ISO-3**: In-memory CAS and SHA-256 digest verification.
- **ISO-4**: Large blob sharding and POSIX cat reassembly.
