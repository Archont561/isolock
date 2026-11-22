Feature: Complete Rust workspace coverage reporting

  Scenario: Publish fresh coverage for the entire Rust workspace
    Given the declared Rust workspace crates
    When the CI coverage publishing policy is inspected
    Then the rust coverage flag includes every workspace crate source directory
    And every configured coverage scope directory exists
    And CI generates and uploads the same full-workspace LCOV report
    And previous reports cannot replace missing current coverage
    And coverage below the unchanged ninety percent target fails CI
