Feature: Example-guided capture with the existing exact finite checker
  The model proposes. Only full-domain transform check and receipt replay decide captured-exact.

  @atdd-black-box-capture-exact
  Scenario: Accept capture only after full-domain transform check and exact receipt replay
    Given a contained reference over a complete declared finite domain
    When a candidate agrees with selected examples but differs elsewhere
    Then examples-equivalent is intermediate and the checker returns a counterexample
    And a correct candidate's full-domain receipt replays independently

  @atdd-black-box-capture-refusals
  Scenario: Reference failures refuse and pre-decision budgets stop inconclusive with no candidate attempt
    Given reference crashes malformed labels and observed nondeterminism
    When capture is requested under bounded resources
    Then the whole reference is refused with no partial session
    And a pre-decision search budget stop exits inconclusive without spending an attempt
    And decided refutations spend an attempt despite advisory feedback limits
    And confinement denies outside content network and process access while scratch access succeeds
    And parent descriptors and environment are absent and unsupported platforms refuse with exit 1

  @atdd-black-box-capture-zal
  Scenario: Round trip paired capture acts and refuse owner/judge claims on the model channel
    Given a separate owner-provisioned capture workspace
    When the model reads submits or translates through MCP
    Then capture acts round trip in symbolic and controlled English
    And owner answers reference provisioning private status and judge claims have no model path
