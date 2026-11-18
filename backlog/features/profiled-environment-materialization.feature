Feature: Profile-scoped environment materialization

  Scenario: Restore only the selected Rust Pixi profile
    Given a project declares Pixi environments named "rust", "python", and "ts"
    And profile "rust-pixi" selects the "rust" Pixi environment and Cargo.lock
    And the storage catalog contains artifacts for all three profiles
    When I plan and synchronize profile "rust-pixi" for "linux-64"
    Then its manifest contains a verified standalone Pixi seed artifact
    And its Conda closure contains only packages reachable from the locked "rust" environment
    And its Cargo closure contains the selected Cargo vendor payload
    And its manifest contains no Python wheel or JavaScript package artifact

  Scenario: Materialize a Rust Pixi profile offline
    Given profile "rust-pixi" has a verified seed Pixi binary under ".isolock/bin"
    And its locked Conda packages and repodata are available from the local Isolock channel
    And its Cargo vendor payload is available from CAS
    When I bootstrap profile "rust-pixi"
    Then Pixi installs only environment "rust" with locked offline inputs
    And ".pixi/envs/rust/bin/cargo" is available
    And Cargo uses the verified vendor payload without contacting crates.io

  Scenario: Keep Pixi and standalone Rust provenance distinct
    Given profiles "rust-pixi" and "rust-standalone" select the same Cargo.lock vendor derivation
    And "rust-pixi" uses Conda Rust packages
    And "rust-standalone" uses a standalone Rust distribution
    When I synchronize both profiles
    Then both manifests may reference the same Cargo vendor digest
    And each manifest records its own runtime-provider artifacts
    And no CAS blob is mutated to represent either profile
