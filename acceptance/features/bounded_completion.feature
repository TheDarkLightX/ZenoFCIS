@synthesis @completion @preparation
Feature: Complete bounded operations without exposing partial results
  Applications retain their existing authorization and publication boundaries
  while optional finite analysis detects dead ends and ordered work can pause.

  @atdd-bounded-completion
  Scenario: Verify finite exits and prepare bounded chunks without publication authority
    Given a closed finite transition and a separately declared terminal condition
    And an owned ordered operation with complete input and output reservations
    When exit search is checked independently and preparation advances in chunks
    Then every checked command decreases its exit rank
    And partial failed stale or over-budget preparation releases no result
    And completion equals the original whole operation without granting authority
