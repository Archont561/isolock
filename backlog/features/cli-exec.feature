Feature: Ergonomic Proxy Execution Without Double Dash

  Scenario: Execute tool directly through proxy without double dash
    Given the Isolock CLI is installed
    When I run "isolock exec python3 -c 'import os; print(os.environ[\"PIP_INDEX_URL\"])'"
    Then the process executes without requiring "--"
    And stdout contains "http://127.0.0.1:"
    And the exit code matches the child process exit code

  Scenario: Child process exit code propagation
    Given the Isolock CLI is installed
    When I run "isolock exec sh -c 'exit 42'"
    Then the command exits with status code 42
