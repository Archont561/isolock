Feature: Hermetic NPM Registry Packument Proxy

  Scenario: Request package packument metadata
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains NPM tarball "zod-3.23.8.tgz"
    When a client issues "GET /npm/zod"
    Then the response status is 200 OK
    And the response content-type is "application/json"
    And the packument defines version "3.23.8"
    And the dist object specifies tarball URL "http://127.0.0.1:8080/npm/zod/-/zod-3.23.8.tgz"
    And the dist object specifies a valid "sha512" integrity string

  Scenario: Download NPM tarball through proxy
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains NPM tarball "zod-3.23.8.tgz"
    When a client issues "GET /npm/zod/-/zod-3.23.8.tgz"
    Then the response status is 200 OK
    And the downloaded bytes match the stored CAS tarball bytes
