Feature: Explicit checked scalar arithmetic
  Authored arithmetic uses the library evaluator and its exact wide helpers.
  Source refinement, complete finite application checks and emitted-code
  conformance have separate qualification scopes.

  @atdd-scalar-arithmetic
  Scenario: Execute explicit scalar arithmetic through authored rules and the Authority
    Given checked multiplication, signed rounded division and nonnegative fused arithmetic
    When independent arithmetic vectors and authored contract controls run
    Then every complete result or refusal agrees with the independent oracle
    And unused and unselected traps retain their actual Step charges
    And exact two-factor and three-factor product predicates run through the library laws
    And wrong rounding, false laws and successful-path stubs are refused
