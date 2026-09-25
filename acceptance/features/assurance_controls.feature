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
    And every solver result for a relational or temporal claim states that no system model was exported

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

  @atdd-inductive-claims
  Scenario: Prove invariants by induction over the laws the authority enforces
    Given an inductive claim that names the laws its step assumes on every commit, on accepts, and on committed failures
    When the project is elaborated
    Then an empty, repeated, or undeclared law, or an invariant that reads more than the pre-state, is refused
    And the claim's groups are part of the canonical project, and a project without inductive claims encodes as before
    When the step is exported
    Then each assumed law and the invariant before the step are asserted as defined and true, each on its own
    And the invariant after the step is the invariant rewritten from the pre-state to the post-state, asserted as not defined and true
    And laws assumed only on accepts or only on committed failures are guarded by the decision kind
    When a solver returns a model
    Then it is confirmed only if every law assumed for its decision kind and the invariant before the step hold on it
    And a model at which a claim has no value is reported as undefined, with the reason
    And a model with an undeclared variant, a bool other than 0 or 1, or an integer outside its declared range is never confirmed
    And the application checks the invariant on its exact genesis state
    And the law manifest confirms that each assumed law is enforced on the decisions it is assumed on
    When an int type declares an inclusive range
    Then the step asserts that range for every observed value of that type, and lowering makes it the schema's bounds and refuses a binding that contradicts it
    And a half-open, reversed, or non-int range is refused, and a project that declares no range encodes as before
    And the authority checks every command, context, and initial state against its own schema, whatever hasher built the envelope
    When a law declares the decisions it is enforced on
    Then elaboration refuses a claim that assumes it outside that scope, and a law without a declared scope encodes as before
    And a law manifest that enforces another scope or genesis applicability than the project declares, or lacks a declared law, is reported

  @atdd-reserved-domains
  Scenario: Keep project commitment domains out of the library namespace
    Given library identities such as candidate IDs in the zeno-fcis namespace
    When a project binds a state domain or profile prefix through the project constructors
    Then names inside the reserved namespace are rejected
    And names outside it and the V1 constructors are unchanged
    And the zUSD patch precondition hash stays byte-identical

  @atdd-determinism
  Scenario: Detect nondeterminism in decision code by static rules and repeated execution
    Given decision code that reads a clock, the environment, or randomness
    And code that keeps state between calls, iterates a hash map, or exposes an address
    When the purity check reads it, including through aliases, glob imports, and macro arguments
    Then each source is reported at its line with its rule
    And a crate is confined only when it is a library-only package that is no_std, forbids unsafe code, and depends only on semantic crates
    And a binary target, a renamed dependency, an unrecognized manifest form, include!, or a path attribute keeps a crate from being confined
    And a directory, link, or file the check cannot read makes the result unreadable, never clean
    And the decision code of every application template is clean
    When one invocation is executed repeatedly on fresh copies
    Then its decision is returned only if every execution produced identical canonical bytes
    And a program that changes its decision between runs is withheld with the differing run
    And a probe of a single execution is refused

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

  @atdd-gate-evidence
  Scenario: Publish gate evidence only for the unchanged committed revision
    Given a gate evidence recorder whose gates, versions, and git view are fakes
    When tracked source or the commit changes at any point before the record is written
    Then no record is written and the recorder fails
    And a clean run records its exact revision and tree
