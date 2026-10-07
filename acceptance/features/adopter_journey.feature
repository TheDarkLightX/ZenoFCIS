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
  Scenario: Compile the V1 consumer through its documented V2 migration
    Given a V1 consumer outside the ZenoFCIS workspace package graph
    When it changes only its import to the legacy module and compiles against the locked graph
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
    And every admitted input matches the finite model and the independent examples through the executed application
    And the account-lockout, order-fulfillment, inventory-reservation, compliance-gateway, withdrawal-queue, and agent-treasury-guard examples each build as isolated packages
    And each passes its own tests and prints its expected demonstration summary
    And the inventory-reservation, compliance-gateway, withdrawal-queue, and agent-treasury-guard syntheses replay in Rust, Python, and JavaScript against independent oracles
    And the compliance-gateway oracle evaluates its rule base separately from the template's own evaluator
    And the withdrawal-queue controller is re-checked in Rust against its pinned contract, and by OrbitSynthesis's checker when ORBIT_SYNTHESIS_ROOT names a checkout
    And the agent-treasury-guard demonstration prints the scripted outcome of each of its 23 proposals and answers and ends with no swap outstanding
    And an application built with `zeno-fcis new --contract` from the dual-approval example contract builds as an isolated package
    And its tests check all 12 decision examples against the library Authority and run them as one SQLite session from genesis
    And `zeno-fcis new` binds every application it writes to this source tree, and the gate checks that binding rather than writing it

  @atdd-example-templates
  Scenario: Emit example applications whose laws all constrain their transitions
    Given the account-lockout, order-fulfillment, inventory-reservation, compliance-gateway, withdrawal-queue, and agent-treasury-guard templates
    When an adopter creates each one with the CLI
    Then every law formula in each project can constrain some transition and every law path resolves
    And a second creation into the same directory is refused without changing any file
    And each application template emits exactly the files in its directory

  @atdd-generated-contracts
  Scenario: Regenerate each template's V2 contract from its declarations and rules
    Given each template's project.zeno, reviewed rules file and original schema
    When the CLI generates src/v2_contract.rs and v2/policy.zcve into a fresh directory
    Then both files equal the committed files byte for byte for all eight templates
    And the library's own catalog binding accepts the generated policy before it is written
    And a check names each drifted file and changes nothing
    And each planted rule, declaration or library-rule error is refused at the entry that holds it
    And each channel's idempotency domain covers every ordinal the rules use, the Effect limit is the most deliveries of any case, and a committed failure without a failure law is refused at its case
    And a refusal by the library's catalog names the delivery, law or channel without which the library admits the contract
    And the rules reference documents exactly the keys, leaves, classes, operators, functions and law kinds the generator reads
    And `contract export-program` writes the current decision program that optimize and transform read, and the optimizer's receipt for it replays

  @atdd-finite-synthesis
  Scenario: Synthesize and replay one contract across languages
    Given a closed finite relational contract and typed implementation grammar
    When the synthesizer emits Rust, Python and JavaScript through separate target adapters
    Then all targets satisfy the complete independent decision table
    And signed integer boundaries and inert JavaScript inputs preserve exact semantics
    And contradictory contracts, inadequate grammars and incomplete budgets stay distinct
    And modified artifacts and missing target tools cannot produce conformance evidence
