@assurance @verus
Feature: Bind Verus evidence to shared executable Rust
  Native tests and report admission run in ordinary acceptance.
  The separate pinned Verus workflow must establish the proof result.

  @atdd-verus-integration
  Scenario: Check Verus evidence admission and shared runtime arithmetic boundaries
    Given the interval and bounded-product functions imported by the authority
    When the shared source is compiled and tested with the application Rust toolchain
    Then machine boundaries and refusal cases have the expected results
    And malformed, incomplete, failed or wrong-version verifier reports are refused
    And a changed tool file fails its pinned digest check
    And the deliberate mutation anchors still apply to the runtime source

  @atdd-finite-execution-proof
  Scenario: Preserve complete finite execution and reject missing proof contracts
    Given the admission and eager evaluation functions used by the library
    When the shared source runs on the pinned application Rust toolchain
    Then all ten instructions, eager traps, refusal order and buffer cleanup agree with the specification
    And the coverage gate rejects missing functions and absent or changed contracts
    And the runtime proof profile declares no narrowed executable preconditions

  @atdd-v2-metered-execution
  Scenario: Keep V2 instruction charges private and retain exact refusal usage
    Given the actual library-owned V2 meter and eager scalar interpreter
    When native execution is compared with independently written wider-arithmetic rules
    Then unused and trapping instruction attempts consume their steps before refusal
    And failed execution returns usage and exposes no partial result
    And external callers cannot forge counters, replace reports or supply initial usage
    And operational body coverage is required separately from the extensional proof
