Feature: Bounded loopback proxy resources

  Scenario: Reject overload without unbounded worker creation
    Given the local registry proxy has reached its configured request-concurrency limit
    When another client requests a registry resource
    Then the proxy returns a deterministic fail-closed overload response or uses a bounded wait
    And it does not create an additional unbounded request thread

  Scenario: Shut down predictably with pending work
    Given one or more proxy requests are active within the configured limit
    When the execution session requests proxy shutdown
    Then the listener stops accepting new connections
    And the execution session accounts for worker completion within its bounded shutdown policy

  Scenario: Preserve successful registry behavior below the limit
    Given the local registry proxy has available request capacity
    When a package manager requests planned metadata or an artifact
    Then the existing registry adapter response and cache-miss policy are preserved
