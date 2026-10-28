Feature: Bounded arithmetic research replay
  Keep the declared affine example reproducible without transferring its
  integer or four-bit evidence to checked native runtime semantics.

  @atdd-bounded-arithmetic-replay
  Scenario: Replay the bounded affine research example with its explicit claim limits
    Given the frozen arithmetic grammar, expected programs and scratch precondition
    When the independent replay runs under normal and optimized Python
    Then all nine programs match the expected symbolic results and 2304 modular cases
    And both planted scratch and program defects have counterexamples
    And checked-overflow equivalence and native runtime performance remain open
