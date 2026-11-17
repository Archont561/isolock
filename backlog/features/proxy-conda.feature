Feature: Hermetic Conda Channel Proxy

  Scenario: Request repodata.json for linux-64 subdir
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains Conda package "python-3.12.7-h55ee38b_0.conda" for subdir "linux-64"
    When a client issues "GET /conda/channel/linux-64/repodata.json"
    Then the response status is 200 OK
    And the response content-type is "application/json"
    And the JSON body defines package "python-3.12.7-h55ee38b_0.conda" in "packages.conda"
    And the package record specifies version "3.12.7" and build "h55ee38b_0"

  Scenario: Download Conda package through proxy
    Given the hermetic proxy is listening on "127.0.0.1:8080"
    And CAS contains Conda package "python-3.12.7-h55ee38b_0.conda"
    When a client issues "GET /conda/channel/linux-64/python-3.12.7-h55ee38b_0.conda"
    Then the response status is 200 OK
    And the downloaded bytes match the stored CAS Conda package bytes
