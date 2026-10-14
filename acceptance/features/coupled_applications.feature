@v2_3 @components
Feature: Compose catalogue instances into coupled applications
  Each coupled application composes qualified catalogue instances through one
  ordinary contract. Composition metadata grants no authority, and static
  checks are not execution evidence; execution receipts are recorded separately.

  @atdd-coupled-applications
  Scenario: Check six coupled applications against independent references and refuse planted composition defects
    Given warehouse reservation, expense approval, reliable jobs, document release, API quota and equipment booking applications
    When each application's composition, component sources and complete decision examples are checked statically
    Then every component's sources equal its qualified catalogue instance, including the family certificate reference
    And every one of the 373056 raw inputs has a written decision example equal to the independent reference
    And each journey agrees with the reference decision and every reject leaves state unchanged with no delivery
    And each contract's rules, read through the names the composition's views use, reproduce every one of the 373056 written decisions
    And on every accepted input, each stepping component's input and successor, derived from the composition's views, lie in its instance's domains, and its local reference accepts that input and yields that successor
    And components that take no step keep their state, the owners partition the state, and the declared effect is the whole outbox
    And every accepted input fires at least one declared occurrence, one with no steps where nothing changes, and every declared occurrence fires on at least one accepted input
    And 27 planted ownership, instance, mapping, step, occurrence and effect defects are refused by this composition check for their stated reasons
    And 11 planted changes that fit every accepted input (authorization views, step order, larger capacities) are refused only because the composition differs from its reviewed pin
    And no application metadata grants publication authority or owner adoption
