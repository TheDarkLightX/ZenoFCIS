@v2_5 @refinement
Feature: Check representation refinement through both contract Authorities
  A pinned abstraction relates every raw concrete state to an abstract state
  or marks it invalid. Checking is exhaustive in state-major order and grants
  no authority. Work exhaustion cannot report refinement.

  @atdd-contract-refinement
  Scenario: Check all raw reservation pairs and detect refinement faults
    Given abstract and concrete reservation contracts with 26 abstract states, 64 raw concrete states and six inputs
    When refine-check binds both contracts and checks the abstraction over all 384 raw pairs
    Then the semantic reference agrees on refinement and the Authority evaluates every required pair
    And lost retirement, observation changes, invalid successors, missing coverage and changed genesis fail
    And each of the 228 invalid-state pairs has a native fault control that fails at that pair
    And limits of zero and 383 comparisons are inconclusive
    And disabling the comparison makes the comparison self-test fail
