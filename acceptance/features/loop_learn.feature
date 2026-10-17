Feature: Learning proposals from examples in a fixed checked vocabulary
  Vocabulary enumeration proposes counts, minimum costs, partitions and conflicts.
  The existing library evaluator checks every lowered example.
  Owner answers enter only through the terminal confirmation gate.

  @atdd-loop-learn-meanings
  Scenario: Enumerate vocabulary meanings and confirm the lowered examples with the existing evaluator
    Given finite input and output domains and an owner-declared vocabulary
    When the learn door enumerates expressions bottom-up within its budgets
    Then an independent syntax reference agrees on counts and minimum vocabulary costs
    And every reported determined program reproduces every example through the existing evaluator
    And minimax questions and minimum conflicts remain labelled as proposals
    And planted wrong counts, questions, conflicts and programs fail their comparisons
    And the comparison self-test fails when comparison is stubbed
    And every budget stop is inconclusive with no count, program, core or attempt

  @atdd-zal-loop-learn-owner
  Scenario: Round-trip learning acts and confirm examples only on the owner channel
    Given a revision-bound learn workspace and a pending question
    When a model suggests an answer and the owner confirms a complete output tuple
    Then the suggestion remains advisory until the terminal owner confirmation
    And every learn act and transcript reply round-trips in both ZAL surfaces
    And models cannot answer or forge judge acts in any workspace
    And stale questions cannot add examples
    And an in-domain owner answer outside the vocabulary is retained
    And a later budget stop preserves the confirmed examples
