@assurance @controls
Feature: Report evidence that cannot distinguish system behavior
  A law, claim, or proof that holds for every behavior gives a reviewer no
  evidence about the system it appears in. The tools must say so directly.

  @atdd-claim-substance
  Scenario: Report laws and claims that cannot constrain any transition
    Given the shipped Mini Determinator whose law and claims are reflexive
    And a project whose only law reads the post-state
    When each project is checked with and without the substantive requirement
    Then every constant law and claim is reported with its substance code
    And the substantive requirement refuses only the vacuous project
    And every solver result states that no system model was exported

  @atdd-system-properties
  Scenario: Check properties against the exact finite transition program
    Given the durable-counter program bound to its shipped canonical bytes
    When each property is checked on every admitted input with a domain-only control
    Then transition-dependent properties are system properties
    And a property the output domains already imply is reported as domain-implied
    And a planted guard bug is reported as not total at its first input
    And an unreplayable solver model is refused

  @atdd-reserved-domains
  Scenario: Keep project commitment domains out of the library namespace
    Given library identities such as candidate IDs in the zeno-fcis namespace
    When a project binds a state domain or profile prefix through the project constructors
    Then names inside the reserved namespace are rejected
    And names outside it and the V1 constructors are unchanged
    And the zUSD patch precondition hash stays byte-identical
