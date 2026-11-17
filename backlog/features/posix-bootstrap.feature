Feature: Pure POSIX Shell Environment Bootstrap

  Scenario: Bootstrap an environment from local CAS in an air-gapped checkout
    Given an air-gapped machine with "/bin/sh", "tar", "sha256sum", and standard POSIX utilities
    And environment "active" has a digest-and-filename manifest
    And verified toolchain archives are stored in the local CAS
    And one archive may be represented by ordered ".part*" shards
    When I execute "sh .isolock/bootstrap.sh"
    Then all required toolchain blobs are verified against their SHA-256 hashes
    And any ".part*" shards are concatenated into complete archives
    And packages are unpacked into ".isolock/state/active/"
    And an activation script ".isolock/state/active/activate" is generated
    When I source ".isolock/state/active/activate"
    Then the PATH environment variable includes the unpacked toolchain binaries

  Scenario: Bootstrap directly from the remote storage branch
    Given the project origin has an orphan branch "isolock/v1" with "blobs/" and "environments/"
    When I execute "sh .isolock/bootstrap.sh --remote"
    Then the storage tree is fetched without checking out the orphan branch
    And the active environment is verified, unpacked, and activated
