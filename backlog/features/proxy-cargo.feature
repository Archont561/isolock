Feature: Hermetic Cargo Sparse Registry Proxy

  Scenario: Request Cargo sparse registry configuration
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    When a client issues "GET /cargo/config.json"
    Then the response status is 200 OK
    And the response content-type is "application/json"
    And the JSON specifies download endpoint "/cargo/api/v1/crates/{crate}/{version}/download"

  Scenario: Request sharded crate index metadata
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains crate "serde-1.0.210.crate"
    When a client issues "GET /cargo/se/rd/serde"
    Then the response status is 200 OK
    And the response contains a JSON line defining version "1.0.210"
    And specifies the crate SHA-256 checksum

  Scenario: Download crate archive through proxy
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains crate "serde-1.0.210.crate"
    When a client issues "GET /cargo/api/v1/crates/serde/1.0.210/download"
    Then the response status is 200 OK
    And the downloaded bytes match the stored CAS crate bytes
