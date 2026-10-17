Feature: Untrusted search for smaller equivalent decision programs
  The search proposes smaller programs over complete-domain signatures.
  Only the transform checker accepts a program; the search carries no authority.

  @atdd-program-search
  Scenario: Propose smaller programs that agree on every tuple and stop with no claim over budget
    Given a canonical finite program and explicit tuple, cell, work and round budgets
    When resubstitution, top-down synthesis and decision-list folding search for a smaller program
    Then every returned program has the original's input and output domains and its table on every tuple
    And trapping additions, unselected arms and output-domain refusals keep their failures
    And two runs return identical bytes and provenance
    And an exhausted budget returns a typed stop and no program
    And on two core families every reported program passes the transform checker and replays
    And every planted single-tuple fault is refused at its tuple by the checker and the reference
