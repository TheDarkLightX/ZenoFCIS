Feature: ZAL loop profile for the bounded loop's doors
  People and models speak about one core in zal/fcis-loop/1, in symbolic or controlled English.
  Only the owner answers questions; tool permission is not the owner's agreement.

  @stage-1
  @atdd-zal-loop-round-trip
  Scenario: Parse every loop act back from either surface to the same typed act
    Given documents with programs, sketches, laws, scenarios, probes, questions, rules, architectures and judge replies
    When each document is rendered in the symbolic and the controlled-English surface
    Then parsing either rendering gives back the same typed acts and the same document revision
    And unknown names, reasons, fields and free prose are refused, as are documents over 256 statements or 64 KiB

  @stage-1
  @atdd-zal-loop-model-cannot-answer
  Scenario: Refuse an answer from the model and offer no answer or say tool
    Given a loop workspace and the MCP server's tools
    When a model document carries an answer act
    Then the workspace refuses it and the transcript is unchanged
    And the MCP tools include no answer or say tool and keep the existing tools first and unchanged
    And deeply nested input is refused before the parser descends, and the MCP server answers the next request

  @stage-1
  @atdd-zal-loop-owner-answers
  Scenario: Answer the model's question and state a rule through the owner's terminal
    Given a loop workspace with the real CLI where the model asked the owner a question
    When the owner answers by option number and states a rule in the terminal
    Then the answer is recorded under the owner's channel and the rule waits for the contract judges
    And every transcript event re-parses in both surfaces to its recorded act

  @stage-1b
  @atdd-zal-loop-memory-by-meaning
  Scenario: Answer a refuted program written another way from memory
    Given a loop workspace in which the judge refuted a program
    When the model submits a program that means exactly the same, written differently
    Then the judge answers from memory, naming the earlier program and its verdict, and spends no attempt
    And a correct program is never answered from memory
    And an answer that is not an offered option leaves the question and the requirements unchanged

  @stage-2
  @atdd-zal-loop-questions
  Scenario: Ask the owner about the largest open region and record the answer as laws over exactly it
    Given a loop workspace on a core whose laws leave decisions open
    When the model probes for freedom and the owner answers the judge's question through the terminal
    Then the question names an exact formula for its inputs, an example and the decisions the laws allow there
    And the answer becomes laws checked by the gaps judge, and a difference from the core is a disagreement
    And a negated number keeps the loop language's meaning in the laws, and a successor that differs is a disagreement
    And an answer that leaves some input with no admissible decision is refused, with the question and the requirements unchanged

  @stage-2
  @atdd-zal-loop-invariant-cycle
  Scenario: Return a refuted invariant's counterexample to induction and accept the revised one
    Given a loop workspace on a bounded counter
    When the model submits an invariant too weak to hold and then a revised one
    Then the first is refuted with the counterexample to induction and the core's decision there
    And the revised one is inductive over every declared state, and the transcript records both rounds
    And the same invariant in other words is answered from memory by meaning
    And after the source contract changes the workspace's judges still read its own bound copy, a new workspace sees the change, and a changed copy is refused
