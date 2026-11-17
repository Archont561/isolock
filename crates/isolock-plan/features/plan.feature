Feature: Airlock Plan and Drift Guard Contract

  Scenario: Plan drift detection detects missing artifacts
    Given an expected airlock plan with a locked package
    When evaluated against an empty current plan
    Then plan drift is detected
    And the package is reported as missing
