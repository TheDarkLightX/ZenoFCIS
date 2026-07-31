@rc2 @architecture @authority
Feature: Build deterministic composed applications without bypassing authority
  Software architects should be able to compose narrow domain machines while
  keeping canonical sequential execution and nominal commit authorization as
  explicit global decisions.

  @atdd-composed-program
  Scenario: Execute fixed domain machines through one global composition
    Given fixed machine interfaces and an explicit canonical composition
    When the composed program executes its bounded test portfolio
    Then local results merge through the declared global order
    And proof-carrying parallel claims do not replace the sequential oracle

  @atdd-production-authority
  Scenario: Admit only catalog and invocation bound transitions
    Given an authority-owned catalog program laws provider deployment and genesis
    When the authority crate runs its complete test portfolio
    Then raw bundles and caller-selected authority inputs cannot enter the production port
    And rejection cannot produce a committable transition

  @atdd-sqlite-authority
  Scenario: Persist an authorized transition and its exact outbox obligations
    Given a nominally authorized genesis and transition
    When the SQLite shell runs publication replay reopening corruption and delivery tests
    Then the authoritative rows reconstruct the exact committed candidate
    And external work remains a replay-safe outbox delivery obligation

  @atdd-security-hotspots
  Scenario: Rank security hotspots without interpreting source as instructions
    Given the owner reviewed EPI model and exact source inventory
    When the deterministic hotspot scanner runs its hostile fixtures and baseline check
    Then every score decomposes into bounded review-priority components
    And repository text remains inert data rather than reviewer instructions
