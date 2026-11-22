Feature: Lockfile Plan Drift Detection and CI Ingestion Guard

  Scenario: Plan is clean when all dependencies exist in airlock
    Given a project with "requirements.txt" declaring "fastapi==0.115.0" and an airlock plan
    And CAS contains "fastapi-0.115.0-py3-none-any.whl"
    When I run "isolock plan --ci"
    Then the exit code is 0
    And the output reports "Verdict: ALL DEPENDENCIES SATISFIED"

  Scenario: Plan detects missing dependencies and formats dispatch command
    Given a project with "requirements.txt" declaring "fastapi==0.115.0 pydantic==2.9.2" and an airlock plan
    And CAS is empty
    When I run "isolock plan --ci"
    Then the exit code is 1
    And the output reports "Verdict: DRIFT DETECTED"
    And the output lists missing packages "fastapi==0.115.0" and "pydantic==2.9.2"
    And the output reports "Review and commit isolock.ingest.json with exact source, filename, and SHA-256 pins before dispatch. No registry resolution is performed."
    And the output formats command:
      """
      gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=pypi -f packages="fastapi==0.115.0 pydantic==2.9.2"
      """

  Scenario: Missing dependencies are grouped into one dispatch per registry
    Given an airlock plan with missing artifacts:
      """
      pypi fastapi==0.115.0
      cargo serde==1.0.210
      pypi pydantic==2.9.2
      """
    And CAS is empty
    When I run "isolock plan --ci"
    Then the exit code is 1
    And the output formats commands:
      """
      gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=cargo -f packages="serde==1.0.210"
      gh workflow run isolock.yml -f manifest=isolock.ingest.json -f registry=pypi -f packages="fastapi==0.115.0 pydantic==2.9.2"
      """
