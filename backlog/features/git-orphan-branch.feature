@deferred
Feature: Automatic Git storage epoch rollover
  # Unimplemented specification tracked by ISO-24; executed publication contracts are in reviewed-ingestion.feature.

  Scenario: Branch rollover when storage budget is exceeded
    Given an orphan branch "isolock/v1" with cumulative blob size 1,480 MiB
    And a reviewed maximum branch budget of 1,500 MiB
    When a new artifact of size 50 MiB would exceed the reviewed budget
    Then an explicit rollover operation initializes a new orphan branch "isolock/v2"
    And preserves the required reachable manifests and payload history
    And commits the new artifact to "refs/heads/isolock/v2" without force
    And reports the reviewed "storage.reference" change without silently rewriting project configuration
