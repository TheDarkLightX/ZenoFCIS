@v2_2
Feature: Import and export an optional finite Tau second-judge kit
  The native contract compiler and Authority retain their existing decisions.
  Local Tau and SMT runs are optional, separate from hosted CI and runtime
  authority. Passing the parser or export tests is not solver qualification.

  @atdd-tau-finite-kit
  Scenario: Translate finite ladders and retain exact independent check inputs
    Given a complete pointwise unsigned bitvector ladder in the supported finite profile
    When the native compiler imports it and the actual Authority provides exported decisions
    Then modular addition and subtraction preserve the declared finite widths
    And larger widths require explicit scaling recorded separately from original-width evidence
    And the explicit finite allowance profile derives eleven cases from the parsed source with declared diagnostic and law annotations
    And its supplied authority, finite amount domain and closed source/native kind conversion remain explicit
    And unsupported time, control, arithmetic, incomplete outputs and stale metadata refuse
    And signed export checks every eager native and authored arithmetic intermediate before emitting queries
    And every query has its actual paired SMT input and planted false controls
    And the runner refuses trailing query periods and treats errors, unknowns and disagreements as incomplete
    And each positive tool result requires its exact SMT negation to be unsatisfiable
    And native and parser tests grant no Tau qualification or library authority
