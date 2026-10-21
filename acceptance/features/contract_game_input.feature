Feature: Finite contract game inputs
  @atdd-contract-game-input
  Scenario: Exercise native Authority game inputs and independent certificate replay
    Given declared actors and capabilities over a signed-scalar contract
    When native Authority outcomes supply the finite game table
    Then the independent checker reconstructs product closure and replay
    And the objective observes event outcomes with genesis admitted by Authority
    And incomplete, unsigned or retained-lineage profiles remain unknown
