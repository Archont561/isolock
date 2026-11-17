Feature: Lockfile Plan Drift Detection and CI Ingestion Guard

  Scenario: Plan is clean when all dependencies exist in airlock
    Given a project with "requirements.txt" declaring "fastapi==0.115.0"
    And CAS contains "fastapi-0.115.0-py3-none-any.whl"
    When I run "isolock plan --ci"
    Then the exit code is 0
    And the output reports "Verdict: ALL DEPENDENCIES SATISFIED"

  Scenario: Plan detects missing dependencies and formats dispatch command
    Given a project with "requirements.txt" declaring "fastapi==0.115.0" and "pydantic==2.9.2"
    And CAS is empty
    When I run "isolock plan --ci"
    Then the exit code is 1
    And the output reports "Verdict: DRIFT DETECTED"
    And the output lists missing packages "fastapi==0.115.0" and "pydantic==2.9.2"
    And the output formats command:
      """
      gh workflow run isolock.yml -f registry=pypi -f packages="fastapi==0.115.0 pydantic==2.9.2"
      """
