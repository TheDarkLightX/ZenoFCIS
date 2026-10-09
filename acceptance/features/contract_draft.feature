@v2_2 @authoring
Feature: Draft contracts from descriptions and explicit decision labels
  Supplied labels constrain an advisory local draft and confer no authority.

  @atdd-contract-draft
  Scenario: Draft three contracts through explicitly supplied decision labels
    Given three visible intent descriptions and rewritten complete rules proposals
    And prerecorded template examples retaining their recorded review provenance
    And additional labels supplied by an explicitly simulated baseline-contract oracle
    When bounded drafting uses the existing generator library binding and advisory review
    Then every finalized draft agrees with all supplied observations and labels all current questions
    And each generated application passes its tests
    And an example catches a planted wrong proposal
    And missing labels stale revisions invalid proposals and exhausted budgets refuse finalization
    And a failed output write leaves no successful target
    And hosted models remain off and no adoption or publication authority is granted
