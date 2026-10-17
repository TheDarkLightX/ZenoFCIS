Feature: Public-schema knowledge engine on one strategy surface
  Knowledge and plan outputs are proposals without program or owner authority.

  @atdd-loop-strategy-knowledge
  Scenario: Query bounded relations and propose only code-consistent typed plans
    Given the neutral example derived only from public code signatures and plans
    When facts, recursive rules, safe negation and comparisons are queried
    Then the independent reference reproduces the complete proposed rows
    And every reserved signature agrees with the closed code registry
    And a planted signature mismatch fails with the comparison enabled
    And stubbed comparisons must fail their negative self-tests
    And type-directed composition is checked before proposing a named strategy
    And work, row, state, depth and deadline stops are inconclusive without partial results
    And the engine runs without an external or private knowledge file
    And both example-only ZAL surfaces round trip without accepting a program

  @atdd-loop-strategy-knowledge-privacy
  Scenario: Keep external user knowledge out of source and release packages
    Given exactly one pinned neutral example in the packaging include list
    When another knowledge file appears anywhere in the source tree or a release archive
    Then privacy and packaging refuse hidden, ignored, untracked and renamed knowledge files
    And changed example bytes and path aliases are refused
    And a user knowledge file is acquired only through an explicit external path
    And knowledge operation reports are never exported into the checkout by the CLI
