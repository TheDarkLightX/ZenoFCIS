@v2_4 @judges
Feature: Measure how completely a contract's declared laws pin down its decisions
  A review with --gaps proposes every decision a contract of the same schema could
  make and asks the library's law evaluator which of them the declared laws admit.
  Over its limits it is inconclusive and claims nothing.

  @atdd-specification-gaps
  Scenario: Find the decisions and decision tables the declared laws admit
    Given bounded counter, retry budget, versioned register, logical deadline and reservation pool instances from the core-component families
    When contract review runs with --gaps, and the profile, lemma, scenario and probe judges run beside it
    Then the review packet is version 3 with the extended mutant catalog and is byte-identical on repeat
    And each tuple's admissible decisions, each law's activation and each mutant the declared laws admit are reported with a witness
    And the declared laws admit the reject-everything core and a scenario that expects a commit kills it
    And a false proposed law is a false alarm and a contract that delivers is out of scope
    And a domain or a count of law checks over its limit is inconclusive and exits 2, never a pass
