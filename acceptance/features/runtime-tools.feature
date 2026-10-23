@assurance @tools
Feature: Run complete proof and application tool regressions
  Tool runs check their declared evidence scope and do not authorize deployment.

  @atdd-lean-proof-tools
  Scenario: Check Lean source closure, axiom auditing and proof-gate controls
    Given the pinned Lean gate and its independent module replay controls
    When every proof-tool regression runs
    Then forbidden axioms and unchecked kernel declarations are refused

  @atdd-effect-policy-tools
  Scenario: Check complete effect control spaces and payment policy products
    Given the effect launchers and independent payment reference
    When all control-space and payment diagnostic tests run
    Then low-space refusal and payment violation order stay explicit

  @atdd-meaning-client-exits
  Scenario: Preserve verdict exit classes in meaning clients
    Given positive negative and unknown meaning results
    When their complete client regressions run
    Then inconsistent or unknown packets grant no meaning key

  @atdd-service-runtime-tools
  Scenario: Check generated services and their independent controls
    Given a fresh generated service and complete control-site regressions
    When the byte-stream journey commits refuses recovers reopens and replays
    Then all eight committed receipts agree without a live socket claim

  @atdd-two-run-reference
  Scenario: Compare two-run properties with the independent finite reference
    Given the complete documented two-run commands and their witness replay
    When every CLI regression runs with the configured binary
    Then each verdict and refusal retains its exact declared finite scope
