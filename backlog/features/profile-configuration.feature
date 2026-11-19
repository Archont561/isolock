Feature: Typed profile configuration and locked planning

  Scenario: Explain a resolved operational setting
    Given project configuration selects a CAS directory
    And ISOLOCK_CAS_DIR supplies a higher-precedence directory
    When I run "isolock config explain storage.cas-dir"
    Then the effective value is the environment-provided directory
    And the output identifies ISOLOCK_CAS_DIR as its source

  Scenario: Reject an unrecorded locked CI override
    Given profile "rust-pixi" has a generated locked plan
    And the plan records the verified seed Pixi digest and selected Pixi environment
    When CI overrides the seed Pixi version or the selected environment
    Then "isolock plan --ci" fails before materialization
    And the failure identifies the locked input that changed

  Scenario: Permit a local operational path override
    Given profile "rust-pixi" has a generated locked plan
    When a developer sets ISOLOCK_CAS_DIR to a writable local cache directory
    Then Isolock uses the local cache directory
    And the resolved artifact closure remains unchanged

  Scenario: Proxy resource limits are operational rather than locked inputs
    Given profile "rust-pixi" has a generated locked plan
    When a developer overrides proxy concurrency and deadlines
    Then the resolved artifact closure remains unchanged
