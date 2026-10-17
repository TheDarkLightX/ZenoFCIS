Feature: Teach-back and decomposition doors for the bounded loop
  A restated rule and an architecture of components are held against the core's decisions on the whole
  declared domain. The doors explain and propose; the core's decisions come from the library Authority.

  @stage-2
  @atdd-loop-teach-back
  Scenario: Judge restated rules exactly and summarise a complete set
    Given a bounded counter instantiated from its template and rules restating how it decides
    When the teach-back door judges each rule on every input
    Then an exact rule is exact, a rule with one reason wrong reports exactly the affected inputs as exceptions
    And a rule that holds reports the inputs it misses, and five exact rules are a complete summary
    And two rules get equal meaning keys only when every judged count and the verdict agree
    And a rule the confirmed answers contradict is reported forbidden with a witness, never required
    And the independent reference recomputes every count and rejects planted false ones

  @stage-2
  @atdd-loop-decompose
  Scenario: Project a decomposition, explain a missing read and blame a wrong case
    Given an architecture of components in merge order over a core's fields and reasons
    When the decompose door holds it against the core's required behaviour per component
    Then a skeleton gets case tables projected from the core
    And a component that cannot see a field it needs gets an interface counterexample naming that read
    And a component with a wrong case is blamed with a region, and a later-owned read asks for a reorder
    And an architecture with a reason no component decides is unrealizable even with tables
    And these results are labelled as the door's own evaluation, not a checker's

  @stage-3
  @atdd-loop-decompose-compose
  Scenario: Compose a decomposition with the composition checker's joint step and compare it with the core
    Given a decomposition whose components all have case tables, given or projected from the core
    When the decompose door composes it as a composed-domain composition
    Then the composition checker's joint step equals the core on every input and its obligations are proved
    And the exported manifest is checked again by contract check-composition
    And a planted wrong component makes the composed check differ from the core
    And a last guard that never fires is kept as written and the decomposition is refuted, never accepted
    And an existing export directory is refused, and the controls fail when their comparison is stubbed
