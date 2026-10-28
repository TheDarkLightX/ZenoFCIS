Feature: Explicit observations over finite ZAL models
  Model checks describe their source, observer, environment and checking allowance.

  @atdd-zal-observation-contracts
  Scenario: Check observation-limited strategies and complete finite paired histories
    Given a lost-reply model whose two physical worlds share one observation
    When the controller must choose from its observation history
    Then the partial-observation model is unrealizable and the perfect-information model is checked
    And the declared idempotent transition admits a uniform retry strategy
    And response reasons can refine knowledge without changing business state
    And hidden event costs do not reveal physical branches to the controller
    And bounded service rejects a safe model that never serves the request
    And paired histories find a secret copied and revealed after two events
    And exhausted checking allowances remain UNKNOWN without completion evidence
    And replay refuses changed subjects, stale sources, forged verdicts and insufficient allowances
    And generated ZAL diagrams contain the checked graph and replayed counterexample edges
