Feature: Content-Addressed Storage and Large Blob Sharding

  Scenario: Ingest and store standard artifact under 95 MiB
    Given an empty CAS store with a 95 MiB blob ceiling
    When an artifact "fastapi-0.115.0-py3-none-any.whl" with content "wheel-bytes" is ingested
    Then the artifact is stored at "blobs/sha256/e3/e3b0c442.../fastapi-0.115.0-py3-none-any.whl"
    And no chunk shards are generated
    And the retrieved payload exactly matches "wheel-bytes"

  Scenario: Ingest and shard large artifact exceeding 95 MiB
    Given an empty CAS store with a 95 MiB blob ceiling
    When an artifact "pytorch-2.4.0-cuda.whl" of size 190 MiB is ingested
    Then the artifact is partitioned into 2 shards ".part000" and ".part001"
    And a manifest "pytorch-2.4.0-cuda.whl.shards.json" is created
    When the shards are reassembled using POSIX "cat"
    Then the reassembled file has the expected SHA-256 checksum
    And matches the original 190 MiB artifact
