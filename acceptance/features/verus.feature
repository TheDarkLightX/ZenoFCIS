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
