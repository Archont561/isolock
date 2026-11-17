Feature: Core Version and Registry Contract

  Scenario: Exported VERSION is non-empty and conforms to semver
    Given the core crate is referenced
    When I query the exported VERSION constant
    Then the version string is not empty
    And the version string conforms to semantic versioning
