Feature: Scoped CLI execution sessions

  Scenario: Compose local registry configuration and preserve the child exit code
    Given an isolated execution project with inherited Pixi configuration
    When I run a child with ergonomic isolock exec
    Then the child receives the composed local registry environment
    And its exit code 7 is propagated unchanged
    And the local proxy and temporary Pixi configuration are cleaned up

  Scenario: Clean up after a child fails to start
    Given an isolated execution project with inherited Pixi configuration
    When the requested child executable does not exist
    Then execution reports a child startup failure
    And the local proxy and temporary Pixi configuration are cleaned up

  Scenario: Clean up after Pixi configuration fails
    Given an isolated execution project with inherited Pixi configuration
    When execution encounters invalid inherited Pixi mirrors
    Then execution fails before starting the child
    And the local proxy and temporary Pixi configuration are cleaned up

  Scenario: Clean up after Cargo configuration fails
    Given an isolated execution project with inherited Pixi configuration
    When execution cannot create the resolved Cargo home
    Then execution fails before starting the child
    And the local proxy and temporary Pixi configuration are cleaned up
