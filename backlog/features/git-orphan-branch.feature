Feature: Git Orphan Branch Storage and Epoch Rollover

  Scenario: Ingest artifact into orphan branch without checking out
    Given a clean Git repository on branch "main"
    When package "pydantic" version "2.9.2" is committed to orphan branch "isolock/v1"
    Then branch "main" remains completely clean of binary artifacts
    And a commit is appended to "refs/heads/isolock/v1"
    And the commit message conforms to "chore(deps): add pypi/pydantic==2.9.2"
    And the working directory is unchanged

  Scenario: Branch rollover when storage budget is exceeded
    Given an orphan branch "isolock/v1" with cumulative blob size 1,480 MiB
    And a maximum branch budget of 1,500 MiB
    When a new artifact of size 50 MiB is added
    Then the system detects the branch budget overflow
    And initializes a new orphan branch "isolock/v2"
    And commits the new artifact to "refs/heads/isolock/v2"
    And updates "isolock.toml" with "branch = \"isolock/v2\""
