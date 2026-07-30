@rc3 @authoring @composition @formal
Feature: Author and compose bounded ZenoFCIS projects
  Authors should receive deterministic typed outputs and complete diagnostics
  while every authority-affecting decision remains in existing checked APIs.

  @atdd-rc3-project-new
  Scenario: Create a bounded project without overwriting files
    Given an empty target directory
    When the closed new-project binding runs twice
    Then the first run creates fixed files and the second refuses overwrite

  @atdd-rc3-mini-os-check
  Scenario: Check the Mini Determinator project in one command
    Given the self-contained Mini Determinator source
    When its parser and elaborator binding runs
    Then the project is a valid bounded typed specification

  @atdd-rc3-spec-canonical
  Scenario: Produce identical typed AST bytes from equivalent source
    Given formatting comments and declaration permutations
    When both sources elaborate
    Then their typed canonical bytes are identical

  @atdd-rc3-composition-diagnostics
  Scenario: Report composition blockers completely and deterministically
    Given duplicate and unresolved composition declarations
    When elaboration accumulates diagnostics
    Then all retained blockers appear in canonical order

  @atdd-rc3-mini-os-replay
  Scenario: Replay shared-nothing coordination independently of completion order
    Given disjoint private worker writes
    When worker results arrive in different orders
    Then the complete accepted result is identical

  @atdd-rc3-mini-os-conflict
  Scenario: Reject conflicting private workspace merges without authority change
    Given two private workers write the same slot
    When the canonical merge runs
    Then one stable conflict is returned and the pre-state remains unchanged

  @atdd-rc3-temporal-modes
  Scenario: Keep finite execution and unbounded proof obligations distinct
    Given a strong next formula at the final logical event
    When finite and unbounded modes are evaluated
    Then finite mode returns a counterexample and unbounded mode returns an obligation

  @atdd-rc3-formal-tools
  Scenario: Bind formal output to the exact claim, runtime, and checked arithmetic
    Given bounded claims with signed division and closed backend identifiers
    When deterministic source and a Lean runtime inventory are produced
    Then claim identity checked arithmetic finite trace lengths and runtime bytes are fixed

  @atdd-rc3-formal-fail-closed
  Scenario: Block hostile formal outcomes and replay models before refutation
    Given missing mismatched timed-out crashed and uncertain formal tools
    When the process adapter and classifier evaluate their bounded outputs
    Then only a model replayed for the exact claim can refute that claim

  @atdd-rc3-input-inert
  Scenario: Keep shell traversal environment and instruction syntax inert
    Given shell traversal environment and instruction-like source data
    When the bounded lexer and parser inspect it
    Then executable syntax is invalid and instruction-like identifiers remain data

  @atdd-rc3-derived-views
  Scenario: Render deterministic diagnostic graphs and explanations only
    Given one valid typed project
    When graph projections are rendered repeatedly
    Then their bytes are stable and grant no authority

  @atdd-rc3-generated-drift
  Scenario: Regenerate source and manifests reproducibly and detect drift
    Given one valid typed project
    When generated Rust and manifest bytes are produced repeatedly
    Then both artifacts are byte-identical

  @atdd-rc3-resource-envelopes
  Scenario: Stop deep parsing, huge horizons, and oversized exports within fixed limits
    Given source and formulas that exceed a reviewed resource limit
    When parsing elaboration and formal export run
    Then each operation stops with a stable resource diagnostic before unbounded work

  @atdd-rc3-process-boundary
  Scenario: Bind timeout, solver names, and execution to exact checked bytes
    Given a child that does not read input and names that used to collide
    When the fixed process and export adapters run
    Then the timeout covers input delivery and the admitted executable bytes remain fixed

  @atdd-rc3-cli-json-contract
  Scenario: Return versioned deterministic CLI JSON for valid and invalid projects
    Given one valid project and one invalid project
    When the published command reports JSON
    Then the schema version field order status and diagnostics are deterministic

  @atdd-rc3-package-binary-inventory
  Scenario: Package every declared binary in one unique checked archive
    Given the complete declared binary inventory
    When the release packager derives artifact paths and provenance commands
    Then each binary has one unique archive containing the intended executable
