Feature: Reviewed artifact ingestion and explicit storage publication

  Scenario: Reviewed local bytes are verified before entering CAS
    Given a reviewed artifact manifest and its original archive
    When the reviewed manifest is ingested through the CLI
    Then the original bytes are verified in the configured CAS
    And no storage reference is published

  Scenario: Explicit publication preserves the code checkout and restores offline
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    When the reviewed manifest is ingested and explicitly published
    Then the storage remote contains the reviewed bytes and audit commit
    And the producer branch, index, and working files are unchanged
    And a cold consumer verifies the snapshot without the originals

  Scenario: A published profile restores a cold offline runtime
    Given reviewed originals for a locked standalone Rust profile
    When its payloads and pinned profile manifest are explicitly published
    Then a cold consumer fetches only its selected verified closure
    And the selected runtime runs offline after the storage remote disappears

  Scenario Outline: Rejected reviews never publish a partial batch
    Given a reviewed artifact manifest and its original archive
    And the ingestion input has "<problem>"
    When the reviewed manifest is ingested through the CLI
    Then ingestion fails with "<diagnostic>" without accepting the batch
    And no storage reference is published

    Examples:
      | problem                   | diagnostic                     |
      | unsupported schema        | schema-version                 |
      | unknown field             | manifest schema                |
      | empty batch               | contains no artifacts          |
      | duplicate identity        | duplicate artifact             |
      | duplicate filename        | duplicate artifact             |
      | invalid registry          | unsupported ingestion registry |
      | traversal filename        | artifact filename              |
      | invalid package name      | package name                   |
      | invalid version           | package version                |
      | malformed digest          | manifest schema                |
      | conflicting filename      | conflicts with registry        |
      | credential source         | credentials                    |
      | non-HTTPS source          | HTTPS                          |
      | whitespace source         | HTTPS                          |
      | missing original          | original artifact              |
      | directory original        | regular file                   |
      | symlink original          | regular file                   |
      | symlink directory         | regular directory              |
      | mismatched original       | SHA-256 mismatch               |
      | later mismatched original | SHA-256 mismatch               |
      | corrupt existing CAS      | SHA-256 mismatch               |
      | oversized original        | 95 MiB                         |
      | missing review            | cannot read reviewed           |
      | oversized review          | exceeds 4 MiB                  |
      | oversized encoded review  | exceeds 4 MiB                  |

  Scenario: Repeated publication is idempotent and later batches append
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    When the reviewed manifest is ingested and explicitly published
    And the same review is published again before another reviewed batch
    Then the storage history is a two-commit linear audit trail
    And the first artifact remains byte-identical

  Scenario Outline: Rejected publication never advances storage references
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    And publication has "<problem>"
    When explicit publication is attempted
    Then publication fails with "<diagnostic>" without changing remote references
    And the producer branch, index, and working files are unchanged

    Examples:
      | problem                   | diagnostic         |
      | missing CAS artifact      | run isolock ingest |
      | corrupt CAS artifact      | SHA-256 mismatch   |
      | code branch destination   | storage reference  |
      | malformed epoch           | storage reference  |
      | unknown remote            | remote lookup      |
      | credential remote         | credentials        |
      | remote helper injection   | publication remote |
      | code tree on storage ref  | non-snapshot paths |
      | altered storage marker    | initialized        |
      | executable storage mode   | unsafe modes       |
      | malformed published plan  | airlock plan       |
      | oversized remote metadata | exceeds 4 MiB      |
      | changed review provenance | immutable          |
      | changed reviewed bytes    | immutable          |
      | changed package variant   | package identity   |

  Scenario: Concurrent publication is never force-pushed
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    And another publisher advances storage during publication
    When explicit publication is attempted
    Then the concurrent storage tip is preserved and retry succeeds linearly
    And the producer branch, index, and working files are unchanged

  Scenario Outline: Connected acquisition still verifies reviewed pins
    Given a reviewed artifact manifest and its original archive
    And a connected acquisition boundary returning "<response>"
    When explicitly connected ingestion is requested
    Then acquisition has the reviewed outcome "<outcome>"
    And no storage reference is published

    Examples:
      | response          | outcome          |
      | reviewed bytes    | verified         |
      | reserved basename | verified         |
      | changed bytes     | SHA-256 mismatch |
      | download fails    | download failed  |
      | missing curl      | cannot start     |

  Scenario Outline: Workflow dispatch treats package input as literal reviewed selections
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    And a connected acquisition boundary returning "reviewed bytes"
    When the dispatch workflow receives "<packages>"
    Then the workflow outcome is "<outcome>" without executing package text

    Examples:
      | packages                 | outcome   |
      | demo==1.0.0              | published |
      | missing==1.0.0           | rejected  |
      | demo==1.0.0;touch marker | rejected  |
      | --storage-ref main       | rejected  |

  Scenario Outline: Profile publication cannot repair or change a locked payload history
    Given reviewed originals for a locked standalone Rust profile
    And profile publication has "<problem>"
    When the selected profile is published
    Then profile publication fails with "<diagnostic>" without changing storage

    Examples:
      | problem               | diagnostic                      |
      | missing profile       | requires --profile              |
      | missing platform      | requires --platform             |
      | mixed package flags   | cannot select registry          |
      | mixed epoch policy    | requires a reviewed artifact batch |
      | directory plan        | locked to Git                   |
      | different destination | another storage source          |
      | unpublished input     | locked payload lookup           |
      | corrupt pinned blob   | selected verified payload bytes |
      | input drift           | locked input changed            |

  Scenario: Ingested reviews are durable shared CAS roots before publication
    Given a reviewed artifact manifest and its original archive
    When the reviewed manifest is ingested through the CLI
    And garbage collection runs here and in another ingesting workspace
    Then both workspaces retain their reviewed originals without publication

  Scenario Outline: Damaged ingestion roots stop garbage collection
    Given a reviewed artifact manifest and its original archive
    And an ingested review object has "<problem>"
    When garbage collection is explicitly applied
    Then collection rejects the damaged ingestion root and retains the bytes

    Examples:
      | problem             |
      | corrupt JSON        |
      | misnamed object     |
      | noncanonical object |
      | symlink object      |
      | unexpected filename |

  Scenario: The PR guard verifies the published snapshot rather than producer caches
    Given a reviewed artifact manifest and its original archive
    And a publisher checkout with staged and unstaged code changes
    When the reviewed manifest is ingested and explicitly published
    And the workflow audits storage after producer originals and caches are removed
    Then the workflow guard verifies the published airlock snapshot
