@v2_2 @formal
Feature: Check every rule case with the pinned solvers when the domain is too large to enumerate
  Money and time ranges put an application's domain far beyond enumeration.
  One SMT query per rule case still checks it. A counterexample counts only
  after the library replays it; an unsat is attested by CVC5, corroborated by
  Z3, and never called proved.

  @atdd-symbolic-per-case-checks
  Scenario: Check rule cases with pinned solvers beyond the enumeration cap
    Given a contract's bound decision program, decision table and state laws
    And an optional strengthening invariant written in the rules' expression language
    When each committing case and each state law becomes one query to the pinned CVC5 and Z3
    Then a model counts as a counterexample only after the library evaluators and the Authority replay it
    And an unsat answer is reported as attested by CVC5 and corroborated by Z3, never as proved
    And an unknown answer, a solver disagreement or an unsupported construct is reported as inconclusive
    And every run asks planted controls that must be refuted, and enumeration decides wherever the domain fits
    And a symbolic transform receipt is never accepted where an exhaustive receipt is required
    And the pinned-solver tests run when both solvers are configured, and are skipped with a documented line otherwise
