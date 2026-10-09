@v2_2 @components
Feature: Reuse component families with complete finite evidence
  A stored certificate identifies evidence. A current build must replay the
  entire declared finite space before reporting that the family laws hold.

  @atdd-core-component-families
  Scenario: Replay every supported finite component instance and refuse bad family evidence
    Given nine reservation-pool, nine rate-limiter and three approval-queue instances
    When the CLI is built from the current source and fresh certificates are issued in an isolated copy
    Then every one of the 6158 raw state command context inputs is replayed through the mandatory Authority
    And every decision agrees with the independent complete reference and has no technical law refusal
    And each family's conservation law holds over every declared parameter value
    And three law-violating policies and seven certificate tampering controls are refused
    And all 21 ordinary generated applications carry the exact certificate reference
    And qualification preserves the stored certificates and refuses changed source or executable inputs
    And inherited genesis evidence is named separately and no Lean kernel or unbounded theorem is claimed
