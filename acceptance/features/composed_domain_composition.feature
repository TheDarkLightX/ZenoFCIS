@v2_4 @composition
Feature: Check components that step together without enumerating their product
  A composed-domain manifest runs every participating component on every
  request, in merge order on one pre-state. The checker plans local, assume
  and couple obligations with their exact sizes, refuses a plan over budget
  before evaluating anything, and checks a plan within budget exhaustively.
  A separately written reference recomputes the joint step, a breadth-first
  oracle over the reachable product and every obligation.

  @atdd-composed-domain-ladder
  Scenario: Prove the ten-core ladder without enumerating its product
    Given ten declared core-component instances bound by one composed-domain manifest with three couplings
    When check-composition plans and checks its obligations through the library and the built CLI
    Then the verdict is proved and every obligation evaluates exactly its planned size
    And the monolithic product it did not enumerate exceeds ten to the twelfth pairs

  @atdd-composed-domain-refusal
  Scenario: Refuse the wide coupling before evaluating anything
    Given the ladder with a coupling over three components with large state, and the ladder under a lowered total
    When check-composition plans their obligations
    Then each verdict is refused with the offending obligation or total and the factor over the limit
    And no row, decision or formula value was evaluated

  @atdd-composed-domain-faults
  Scenario: Refuse or violate every planted composition fault
    Given manifests with a coupling false at genesis, a coupling broken by one step, a binding of a later component, an out-of-domain binding, an invariant not preserved, two owners of one field and a read-only component that changes its state
    When check-composition validates and checks each one
    Then each is refused at validation or violated with its first witness, and every joint-step rule and refusal has its own test
    And the independent reference reaches the same verdict on each fault

  @atdd-composed-domain-stateless
  Scenario: Accept a stateless guard under the composed-domain profile and refuse it under version one
    Given an authorization guard whose single state field has one value
    When it is bound read-only in a composed-domain manifest and as a leaf of a version one manifest
    Then the composed-domain check proves the composition and version one refuses a leaf that owns no state
    And a contract with no state field at all is refused by the contract compiler

  @atdd-composed-domain-reference
  Scenario: Agree with an independent reference on fresh-seed random compositions
    Given plain and wired random compositions of declared core-component instances and fixture contracts from a fixed seed
    When each is checked and the reference explores its reachable product
    Then no proved verdict meets an oracle violation and the joint step agrees on every reachable pair
    And with either comparison stubbed the self-test fails
