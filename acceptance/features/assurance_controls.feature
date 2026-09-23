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
    And totality is decided on every input before any property result
    And a solver model outside the declared domains, or one that does not replay, is refused
    And a domain-only model is replayed with its proposed outputs
    And the solver and exhaustive routes agree on a bounded collection of small programs

  @atdd-reserved-domains
  Scenario: Keep project commitment domains out of the library namespace
    Given library identities such as candidate IDs in the zeno-fcis namespace
    When a project binds a state domain or profile prefix through the project constructors
    Then names inside the reserved namespace are rejected
    And names outside it and the V1 constructors are unchanged
    And the zUSD patch precondition hash stays byte-identical

  @atdd-zusd-lane-gaps
  Scenario: Pin what the zeno language cannot state about the zUSD lane
    Given the single-vault zUSD lane written in zeno version 1 from the pinned native semantics
    When its laws are evaluated with the library evaluator on concrete states
    Then an unguarded effect law holds for an accepted and a rejected deposit alike
    And a guarded deposit law matches the native outcomes and rejects the opposite ones
    And the solvency law overflows inside the declared domain
    And unresolved paths, literals beyond u64, and a leading parenthesized scalar behave as recorded

  @atdd-effect-spellings
  Scenario: Reject effect spellings that bypass qualified-path rules
    Given hostile witnesses for standard I/O, grouped, glob and aliased std imports, thread-local storage, atomics, cells and hash-ordered collections
    When the assurance checker runs its self-test
    Then every hostile witness is rejected by its own rule
    And no safe witness such as WorkspaceCell or BTreeMap is rejected

  @atdd-law-paths
  Scenario: Report law and claim paths that name no declared type or field
    Given a project whose law reads an undeclared field and whose claim reads a command type as state
    When the project is checked with and without the resolved-paths requirement
    Then each unresolved path is reported with its law or claim and the missing type or field
    And the resolved-paths requirement refuses the project
    And every shipped example and template resolves without warnings

  @atdd-kernel-laws
  Scenario: Check kernel laws exhaustively over small stated domains
    Given one predicate each for budget charges, reason choice, canonical decoding, and patch overlap
    When each predicate is enumerated over its stated small domain
    Then every case satisfies its law
    And each harness enumerates exactly its stated number of cases
