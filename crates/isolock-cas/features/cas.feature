Feature: Content Addressable Storage Contract

  Scenario: Artifact digest insertion and retrieval
    Given an initialized empty CAS store
    When an artifact with a valid SHA-256 digest is stored
    Then the CAS store contains the artifact digest
    And the retrieved payload matches the stored bytes
