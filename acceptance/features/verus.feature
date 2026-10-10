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
