Feature: Reviewed Git storage epoch rollover

  Scenario: Budget overflow rolls into a reviewed orphan epoch without changing code
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    When another reviewed batch is published with that policy
    Then a fresh orphan epoch contains the new reviewed batch within budget
    And the old epoch is sealed without changing its snapshot bytes
    And the producer checkout and project configuration remain unchanged
    And a cold consumer verifies the new epoch without producer originals or caches

  Scenario: Rollover preserves locked profiles for cold offline consumers
    Given reviewed originals for a locked standalone Rust profile
    When its payloads and pinned profile manifest are explicitly published
    And reviewed history pressure rolls the profile into a new epoch
    Then the successor preserves immutable profiles input originals and provenance
    And a cold consumer fetches only its selected verified closure
    And the selected runtime runs offline after the storage remote disappears

  Scenario: Accepted rollover retries are idempotent after successor appends
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    When another reviewed batch is published with that policy
    And an explicit publisher appends to the successor
    Then an exact rollover retry leaves all remote references unchanged

  Scenario Outline: Competing publishers and partial failures never publish half an epoch
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And the epoch boundary encounters "<problem>"
    When another reviewed batch is published with that policy
    Then epoch publication fails with "atomic non-force epoch publication" and preserves concurrent references
    And the producer checkout and project configuration remain unchanged

    Examples:
      | problem             |
      | a source race       |
      | a successor race    |
      | a rejected target   |
      | no atomic support   |

  Scenario Outline: Invalid review and migration boundaries fail closed
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And epoch migration has "<problem>"
    When another reviewed batch is published with that policy
    Then epoch publication fails with "<diagnostic>" and preserves concurrent references
    And the producer checkout and project configuration remain unchanged

    Examples:
      | problem                    | diagnostic                        |
      | missing policy             | cannot read reviewed epoch policy |
      | unknown policy field       | invalid reviewed epoch policy     |
      | unsupported policy schema  | unsupported epoch policy          |
      | oversized policy           | exceeds 4 MiB                     |
      | invalid source commit      | invalid Git object ID             |
      | stale source commit        | does not match                    |
      | different source ref       | does not match                    |
      | code source ref            | isolock/vN                        |
      | leading zero generation    | isolock/vN                        |
      | generation overflow        | no supported successor            |
      | skipped successor          | immediate next                    |
      | zero live budget           | budgets must be positive          |
      | zero history budget        | budgets must be positive          |
      | oversized live budget      | budgets must be positive          |
      | oversized history budget   | budgets must be positive          |
      | duplicate batch root       | duplicate epoch retention root    |
      | duplicate manifest root    | duplicate epoch retention root    |
      | malformed retention root   | invalid reviewed epoch policy     |
      | missing retained batch     | retained batch is missing         |
      | missing retained manifest  | closure metadata is missing       |
      | oversized retained closure | retained closure exceeds          |
      | oversized incoming closure | retained closure exceeds          |
      | existing successor         | successor already exists          |
      | unmarked snapshot          | re-ingest original files          |
      | legacy shards              | directory transport for sharded   |
      | unsafe symlink             | unsafe modes                      |
      | unsafe executable          | unsafe modes                      |
      | shallow storage history    | requires a complete remote history|
      | invalid epoch record       | invalid epoch migration record    |
      | corrupt retained original  | SHA-256 mismatch                  |
      | missing retained original  | missing an original/input         |
      | missing provenance record  | no reviewed provenance            |
      | altered provenance         | no immutable reviewed batch       |
      | altered review             | retained review digest mismatch   |

  Scenario: Composed and immutable profile roots survive successive epochs
    Given reviewed originals for a locked standalone Rust profile
    When its payloads and pinned profile manifest are explicitly published
    And a composed profile and an unaliased immutable root are retained
    And reviewed history pressure rolls the profile into a new epoch
    Then the successor preserves immutable profiles input originals and provenance
    And another rollover retains the original locked payload history
    And a cold consumer fetches only its selected verified closure
    And the selected runtime runs offline after the storage remote disappears

  Scenario: Batches append linearly while both reviewed budgets fit
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And the reviewed budgets allow the complete candidate
    When another reviewed batch is published with that policy
    Then publication appends to the source without creating or sealing an epoch
    And the producer checkout and project configuration remain unchanged

  Scenario Outline: Cumulative history triggers rollover even when the live tree fits
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And removed historical blobs exceed the history budget with "<retention>"
    When another reviewed batch is published with that policy
    Then the successor respects "<retention>" while the archived history remains available
    And the producer checkout and project configuration remain unchanged

    Examples:
      | retention            |
      | retire the old batch |
      | retain the old batch |

  Scenario: Exact budgets count repeated historical blobs only once
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    When an existing batch is retried at exact live and history budgets
    Then the original references are unchanged without an orphan successor
    And the producer checkout and project configuration remain unchanged

  Scenario: An accepted rollover retry needs no producer cache
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    When another reviewed batch is published with that policy
    And producer originals and CAS disappear
    Then an exact rollover retry leaves all remote references unchanged

  Scenario Outline: An accepted rollover cannot hide changed or incomplete remote history
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And an accepted rollover has "<problem>"
    When another reviewed batch is published with that policy
    Then epoch publication fails with "<diagnostic>" and preserves concurrent references
    And the producer checkout and project configuration remain unchanged

    Examples:
      | problem                        | diagnostic                          |
      | a different reviewed policy    | sealed under a different            |
      | a different reviewed batch     | sealed under a different            |
      | a missing successor            | successor is missing                |
      | a replaced successor           | ancestry                            |
      | an append after the source seal| sealed but its history was changed  |
      | a malformed source seal        | invalid epoch seal metadata         |
      | an unsupported source seal     | unsupported epoch seal schema       |
      | two source seal trailers       | invalid epoch seal metadata         |
      | a non-orphan accepted target   | not an orphan root                  |
      | an invalid accepted record     | invalid accepted epoch migration    |
      | a changed accepted record      | record does not match               |
      | a changed accepted marker      | invalid storage marker              |
      | missing accepted originals     | accepted epoch is missing           |
      | a missing accepted review      | accepted epoch is missing           |
      | an over-budget accepted target | accepted epoch exceeds              |

  Scenario: Lost acknowledgement of an atomic publication is safely retryable
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And the receiver disconnects after accepting the atomic references
    When another reviewed batch is published with that policy
    Then publication reports an uncertain outcome and the same reviewed retry verifies accepted references
    And the producer checkout and project configuration remain unchanged

  Scenario Outline: Damaged retained profiles cannot be migrated
    Given reviewed originals for a locked standalone Rust profile
    When its payloads and pinned profile manifest are explicitly published
    And retained profile migration has "<problem>"
    And a reviewed rollover of that retained profile is attempted
    Then retained profile migration fails with "<diagnostic>" without changing storage

    Examples:
      | problem                        | diagnostic                         |
      | missing immutable root         | closure metadata is missing        |
      | changed immutable root         | differs from its immutable         |
      | wrong alias identity           | alias identity mismatch            |
      | unsupported profile schema     | unsupported profile manifest       |
      | missing profile input          | missing original/input bytes       |
      | corrupt profile payload        | selected verified payload bytes    |
      | missing profile provenance     | no reviewed provenance record      |
      | detached locked payload history| payload commit                     |
      | missing archived payload ref   | payload reference is missing       |

  Scenario: Adopting a successor reference requires an explicit profile replan
    Given reviewed originals for a locked standalone Rust profile
    When its payloads and pinned profile manifest are explicitly published
    And reviewed history pressure rolls the profile into a new epoch
    Then the old lock cannot silently adopt the successor reference
    When the profile is explicitly replanned and published on the successor
    Then a cold successor consumer fetches the selected verified closure
    And the selected runtime runs offline after the storage remote disappears

  Scenario: Producer Git filters never transform reviewed originals
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And producer Git attributes would normalize the incoming original
    When another reviewed batch is published with that policy
    Then the successor contains the exact reviewed CRLF original
    And the producer checkout and project configuration remain unchanged

  Scenario: An unaccepted atomic attempt can retry the same reviewed boundary
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And the epoch boundary encounters "no atomic support"
    When another reviewed batch is published with that policy
    Then epoch publication fails with "atomic non-force epoch publication" and preserves concurrent references
    When atomic support is restored and the same review is retried
    Then a fresh orphan epoch contains the new reviewed batch within budget
    And the producer checkout and project configuration remain unchanged

  Scenario: A source race requires a new review rather than silently refreshing the pin
    Given reviewed artifacts and a dirty producer checkout on a bounded storage epoch
    And a reviewed rollover policy pinned to that storage tip
    And the epoch boundary encounters "a source race"
    When another reviewed batch is published with that policy
    Then epoch publication fails with "atomic non-force epoch publication" and preserves concurrent references
    When another reviewed batch is published with that policy
    Then epoch publication fails with "does not match" and preserves concurrent references
    And the producer checkout and project configuration remain unchanged
