Feature: Exact finite quantitative risk
  @atdd-quantitative-risk
  Scenario: Count declared inputs and bounded states and compute both exact reward extremes
    Given a finite swap-pool model with an integer declared fee reward
    When the judge counts the complete declared input domain including unreachable inputs
    Then the slippage cardinality share is exact and both kinds have witnesses
    And distinct reachable states include genesis through inclusive depth K
    And min-plus and max-plus sums include every run length from zero through K
    And an independent Python brute force matches all three answers and replays their witnesses
    And refused genesis and incomplete work produce UNKNOWN with no claim
