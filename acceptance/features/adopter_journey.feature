@rc3 @adoption
Feature: Adopt the ZenoFCIS core library
  A software team should be able to evaluate the release candidate through
  small, deterministic, executable entry points before defining production
  authority.

  @atdd-minimal-core
  Scenario: Run the immutable functional core example
    Given the current RC3 source and locked Rust dependencies
    When an adopter runs the minimal core example
    Then the accepted successor has the expected balance and resource usage
    And the immutable pre-state remains unchanged

  @atdd-checked-backend
  Scenario: Construct a tool-neutral checked backend request
    Given a reviewed project and a bounded backend request
    When an adopter runs the checked backend example
    Then the request has a nonzero canonical commitment
    And no external checker is treated as trusted merely because it is mounted

  @atdd-external-consumer
  Scenario: Compile an isolated downstream consumer
    Given a consumer outside the ZenoFCIS workspace package graph
    When the consumer compiles against the RC3 umbrella authoring API and locked graph
    Then the documented public imports and feature selection remain usable

  @atdd-project-bootstrap
  Scenario: Generate a reviewable project starter
    Given an owner-reviewed schema profile and catalog
    When the bootstrap generator emits the starter package and negative vectors
    Then deterministic regeneration and generated consumer checks pass
    And the generator grants no schema or release authority

  @atdd-generated-application
  Scenario: Run an authored application through durable authorization
    Given authored record and command shapes with explicit scalar bounds and runtime laws
    When an adopter creates and builds the durable counter as an isolated package
    Then all bounded input cases obey the reviewed decision table
    And rejection publishes no state, replay or delivery rows
    And committed failure, exact replay, database reopen and delivery retry preserve the expected state

  @atdd-finite-synthesis
  Scenario: Synthesize and replay one contract across languages
    Given a closed finite relational contract and typed implementation grammar
    When the synthesizer emits Rust, Python and JavaScript through separate target adapters
    Then all targets satisfy the complete independent decision table
    And signed integer boundaries and inert JavaScript inputs preserve exact semantics
    And contradictory contracts, inadequate grammars and incomplete budgets stay distinct
    And modified artifacts and missing target tools cannot produce conformance evidence
