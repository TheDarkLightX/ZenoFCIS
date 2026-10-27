Feature: Exact finite distributions
  @atdd-distribution-laws
  Scenario: Check exact finite distributions given uniform independent random bits
    Given declared random-bit inputs that are uniform and independent of each other and the fixed inputs
    When every random value is folded for every non-random tuple within the work limits
    Then exact rational targets are compared without sampling or retry conditioning
    And modulo bias has a minimal excess witness
    And a committed failure is one distinct outcome and a reject is UNKNOWN
    And invalid widths and weights are refused
    And every result retains its randomness premise
    And an incomplete fold is UNKNOWN
