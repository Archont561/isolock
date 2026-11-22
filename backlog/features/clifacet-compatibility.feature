Feature: Clifacet-backed airlock plan compatibility

  Scenario: A clean airlock plan succeeds in CI
    Given an isolated empty airlock plan
    When the plan is checked in CI mode
    Then the clean-plan verdict succeeds

  Scenario: Missing locked artifacts fail closed in CI
    Given an isolated plan requiring a missing Cargo archive
    When the plan is checked in CI mode
    Then the drift verdict fails with an actionable dispatch command

  Scenario: Workstation inspection reports drift without failing
    Given an isolated plan requiring a missing Cargo archive
    When the plan is inspected in workstation mode
    Then the drift verdict succeeds with an actionable dispatch command
