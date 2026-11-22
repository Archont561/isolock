Feature: Large artifact sharding and verification

  Scenario: Oversized artifacts are split, verified, and reassembled
    Given an empty artifact workspace with a 95 MiB shard ceiling
    When an artifact one byte larger than 95 MiB is ingested
    Then the artifact is split into sequential part files
    And the manifest records the total size and each part digest
    And a failed ingest preserves pre-existing parts and removes only its own files
    When the shards are reassembled using POSIX cat
    Then the reassembled payload matches the source and its SHA-256 digest
    And a modified shard is rejected by verification
