Feature: Hermetic PyPI Simple Index Proxy

  Scenario: Request package simple index from local proxy
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains wheel "requests-2.32.3-py3-none-any.whl" with digest "d1a8..."
    When a client issues "GET /pypi/simple/requests/"
    Then the response status is 200 OK
    And the response content-type is "application/vnd.pypi.simple.v1+json"
    And the JSON body defines file "requests-2.32.3-py3-none-any.whl" in "files"
    And the file entry specifies hashes with sha256 "d1a8..."
    And the file entry specifies "requires-python" as ">=3.8"

  Scenario: Download wheel artifact through proxy
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains wheel "requests-2.32.3-py3-none-any.whl"
    When a client issues "GET /pypi/packages/requests-2.32.3-py3-none-any.whl"
    Then the response status is 200 OK
    And the downloaded bytes match the stored CAS wheel bytes
