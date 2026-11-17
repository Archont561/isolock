Feature: Pure POSIX Shell Environment Bootstrap

  Scenario: Bootstrap environment without native binary dependencies
    Given an air-gapped machine with only "/bin/sh", "git", "tar", and "sha256sum"
    And a local clone of orphan branch "isolock/v1"
    When I execute "sh .isolock/bootstrap.sh"
    Then all required toolchain blobs are verified against their SHA-256 hashes
    And any ".part*" shards are concatenated into complete archives
    And packages are unpacked into ".isolock/state/active/"
    And an activation script ".isolock/state/active/activate" is generated
    When I source ".isolock/state/active/activate"
    Then the PATH environment variable includes the unpacked toolchain binaries
