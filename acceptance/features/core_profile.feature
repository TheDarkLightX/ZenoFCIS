@v2_4 @judges
Feature: Profile, judge lemmas on, replay and probe every core-component instance exactly
  The judges read the whole declared domain through the library Authority. A
  separately written reference recomputes the profile, admissible counts,
  activation, lemma, scenario and three probe answers from the public rules,
  and fault controls routed through each comparison show that it can fail.

  @atdd-core-profile
  Scenario: Agree with an independent reference on all 77 core-component instances
    Given the 77 declared instances regenerated from core-components and four bounded-counter variants with a committed failure, a refusing law and a determining law
    When contract review with --gaps, profile, lemma, scenario and probe run through the built CLI
    Then every answer the reference recomputes, with every per-tuple and per-state table digest, equals the reference's
    And both equal the recorded known answers
    And on every core instance the declared laws admit the reject-everything core and one scenario kills it
    And every compared field perturbed in the CLI's answers, every recorded field perturbed, and contracts with a single-tuple fault at the first, last and a random ordinal are reported
    And a fault on a scenario's path, a false proposed law, malformed proposals and every limit get their own verdicts
    And with the comparisons disabled the self-test fails the controls and the whole check
