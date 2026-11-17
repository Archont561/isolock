Feature: Hermetic Proxy Request Routing Contract

  Scenario: Proxy fails closed on cache miss in LocalAirlock mode
    Given a proxy running in LocalAirlock mode
    When a requested artifact is not present in CAS
    Then the proxy issues a fail closed airlock violation
