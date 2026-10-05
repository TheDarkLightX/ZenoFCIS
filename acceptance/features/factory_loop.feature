@v2_1 @factory
Feature: Check a candidate program before the factory may use it
  Untrusted proposers may suggest a smaller program. The factory accepts one
  only with a receipt from an exhaustive comparison on the verified evaluator.

  @atdd-checked-transform
  Scenario: Accept a candidate program only with an exhaustive equivalence receipt
    Given an original finite program and a candidate with the same input and output ABI
    When the transform checker runs both programs on every tuple of the declared input domain
    Then equal results on every tuple, with the declared Step limit never binding, yield a canonical receipt
    And replaying the receipt against the same programs reproduces it byte for byte
    And the first differing tuple is reported as a counterexample and no receipt is written
    And a binding Step limit, a domain above the cap or a changed ABI never yields an equivalence

  @atdd-checked-optimizer
  Scenario: Propose smaller programs and accept only checked ones
    Given a canonical finite program and a strategy from the closed phase set
    When the e-graph optimizer runs its phases within their bounds and extracts a candidate after each
    Then every candidate is judged by the transform checker on every tuple of the declared input domain
    And the result is the best accepted candidate with a replayable receipt, or no checked improvement
    And an instruction that may trap is never removed, merged away or folded, and an unsound rule yields only refused candidates
    And the same program and strategy give byte-identical output

  @atdd-neural-loop
  Scenario: Improve a program only through checked proposals in the bounded loop
    Given an admitted original program frozen in a request with resource limits
    When proposers suggest candidates and the loop checks each one against the original on every input tuple
    Then only a complete equivalence with fewer nodes or bytes, never above the original's, replaces the incumbent
    And every other outcome, including a worker panic, a timeout or a late report, leaves the incumbent unchanged
    And every attempt, model call and check is reserved before its work and never refunded
    And a stale, tampered or rolled-back session resumes with no trusted incumbent

  @atdd-contract-review
  Scenario: Review a contract with distinguishing examples before it is trusted
    Given an application's project.zeno, v2/policy.json and its reviewed decision examples
    When the review runs every input of a small domain, or a deterministic boundary set of a large one, through the library Authority
    Then the packet records each decision and agrees with every owner example
    And every rule mutant of the fixed catalog is distinguished by a witness written as a decision example, refused, or classified without a claim of equivalence on a boundary set
    And a planted wrong constant yields a witness that contradicts an owner example
    And a law refusal on a pre-state that satisfies every state law is a finding, while refusals on pre-states the state laws exclude are counted apart
    And the review reads the decision examples with the grammar every application built from a contract compiles
    And the packet is byte-identical on repeat and the application is unchanged

  @atdd-checked-upgrade
  Scenario: Adopt a checked candidate and upgrade a live store under its lineage
    Given an application whose decision program a receipt shows equivalent to a candidate
    When the owner adopts the candidate as the next contract version
    Then the generator replays the receipt against the program it re-derives before emitting the candidate's graph
    And the superseded version is kept exactly beside the current one and the rules name the candidate, receipt and superseded policy digests
    And a store at any state, with a delivery pending, upgrades to the adopted program successor, whose policy differs only in its decision program and Step limit, and the record binds the adoption's receipt digest and states which premises held
    And with law 991, an equivalent receipt, Step limits that never bind and no law observing Step usage, both versions take every further command alike
    And a contract that changes a law upgrades only a store whose state its genesis laws admit, which for a generated contract is the declared genesis state
    And each history segment replays under its own contract and pending deliveries are delivered exactly once with their original identifiers
    And a different schema, a refused genesis, a missing old contract, an altered record, an unreplayable receipt, a false usage claim, an unchanged program and an edit to a superseded version each refuse with nothing written

  @atdd-app-journey
  Scenario: Build, optimize, adopt and upgrade applications through their command lines alone
    Given the app study's escrow and spend-approval contracts as CLI test fixtures
    When `zeno-fcis new` builds an application from the escrow contract in a new directory outside the repository
    Then the commands its README lists, run exactly as written, build it, check its decision examples and run its session, including the split that pays out in two deliveries
    And `contract export-program`, `optimize`, `transform replay` and `contract adopt` make the spend-approval contract's version 2 with no other tool
    And a version 1 store with four commits and a pending payment upgrades to version 2 as a program successor at commit 4 and delivers the payment under its original identifier
    And the version 1 build then refuses the store, and a store at another version refuses `--decide` and keeps its bytes
    And a version 1 store away from genesis upgrades at commit 2, keeps committing under version 2 with `--decide`, and its audit replays both segments
    And a CLI built as the release build builds it holds no path of this checkout, refuses `new` without `--source` and binds with it
