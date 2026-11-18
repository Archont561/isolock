Feature: Verified profile transport and standalone capabilities

  Scenario: Expose standalone Rust through its locked vendor derivation
    Given verified standalone profile "rust-standalone"
    When I materialize the standalone profile "rust-standalone"
    Then its "cargo" launcher builds with the locked offline vendor source
    And its Isolock launcher refers to a version-owned verified seed

  Scenario: Expose Python with an offline hashed wheelhouse
    Given verified standalone profile "python-standalone"
    When I materialize the standalone profile "python-standalone"
    Then Python and pip expose only the locked wheelhouse installation flow

  Scenario: Execute a restored JavaScript executable without package resolution
    Given verified standalone profile "bun-standalone"
    When I materialize the standalone profile "bun-standalone"
    Then bun x executes the restored executable "hello"
    And bun x rejects the unrecorded executable "not-locked" without calling the runtime

  Scenario: Copy only the selected closure from a directory snapshot
    Given a directory snapshot containing Rust and unrelated profile blobs
    When I fetch the selected Rust manifest into an empty CAS
    Then only blobs reachable from the Rust manifest are copied
    And a missing source manifest prevents any payload copy

  Scenario: Retain independent workspaces sharing a CAS
    Given two workspaces register different profile roots in a shared CAS
    When I collect garbage for one workspace
    Then both workspaces retain their reachable artifacts
    And the reclaimable blob report is deterministic

  Scenario: Reject a corrupt seed before prefix materialization
    Given the locked Pixi seed is corrupt and a host Pixi is available
    When I attempt locked profile synchronization
    Then synchronization fails before executing Pixi or creating a prefix

  Scenario: Seed bootstrap verifies archives separately from payloads
    Given a reviewed bootstrap manifest with an archived Isolock seed
    When I run the POSIX profile bootstrap without a language runtime
    Then only the verified version-owned seed is executed

  Scenario: Never render inline credentials
    Given project configuration contains an inline registry credential
    When I validate render and explain the configuration
    Then every diagnostic omits the credential
