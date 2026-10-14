Feature: Wider checked application decisions
  The library retains its scalar profile and admits a separately bounded
  numeric profile without replacing unfinished checks with verdicts.

  @atdd-wide-value-applications
  Scenario: Execute wide values and larger scalar contracts through checked publication
    Given the retained twelve-type compound fixture and full-width zUSD fixture
    When the numeric boundary, exact wide arithmetic, publication and replay tests run
    Then every reachable zUSD reason is exercised and all fixture hashes reproduce
    And a scalar escrow with more than 256 nodes generates, commits and replays
    And programs beyond the 4096-node ceiling are refused before execution
    And the 4096-node, 64-output and 64-input limits each admit their bound and refuse one more
    And the program commands refuse a decision program in the larger profile
