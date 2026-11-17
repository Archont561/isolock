Feature: Content-Addressed Storage and Large Blob Sharding

  Scenario: Ingest and store standard artifact under 95 MiB
    Given an empty CAS store with a 95 MiB blob ceiling
    When an artifact "fastapi-0.115.0-py3-none-any.whl" with content "wheel-bytes" is ingested
    Then the artifact is stored at "blobs/sha256/e3/b0/e3b0c442.../fastapi-0.115.0-py3-none-any.whl"
    And no chunk shards are generated
    And the retrieved payload exactly matches "wheel-bytes"

  Scenario: Ingest and shard a large artifact
    Given an empty CAS store with a 95 MiB blob ceiling
    When an artifact "pytorch-2.4.0-cuda.whl" one byte larger than 95 MiB is ingested
    Then the artifact is partitioned into sequential shards ".part000" and ".part001"
    And a manifest records the original size and SHA-256 digest
    And the manifest records each part's index, size, filename, and SHA-256 digest
    When the shards are reassembled in manifest order using POSIX "cat"
    Then the reassembled file is byte-for-byte identical to the original artifact
    And the reassembled file passes SHA-256 verification
    And a modified part fails integrity verification
