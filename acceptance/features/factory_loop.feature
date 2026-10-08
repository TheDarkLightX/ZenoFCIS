@v2_1 @factory
Feature: Check a candidate program before the factory may use it
  Untrusted proposers may suggest a smaller program. The factory accepts one
  only with a receipt from an exhaustive comparison on the verified evaluator.

  @atdd-operational-journey
  Scenario: Operate, deliver, evolve, migrate and replay one live application
    Given a fresh application generated from the spend-approval contract
    When its command line creates, approves and executes a payment request
    Then its dry run leaves the database and submission journal unchanged
    And relay delivery survives a crash after sending with two attempts and one idempotent receiver effect
    And a reviewed rule change upgrades the live store while a program successor on that route writes nothing
    And a forward-simulated layout migration keeps the decisions and pending delivery identifiers
    And a bad migration is refused without changing application files
    And the audit and submission history replay eight commits across three contract versions
    And every delivered identifier and canonical payload is retained
    And skipped steps, duplicate effects and changed migration observations fail the gate

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
    And a resume's replay is reserved durably before any replay work, so an interrupted replay stays charged
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
    And a store at any state, with a delivery pending, upgrades to the adopted program successor only after the shell itself establishes all five premises, comparing the two decision programs on every input tuple as the transform checker does, and the record binds the number of tuples compared and the lineage's receipt digest
    And with every premise established, both versions take every further command alike
    And a Step limit that binds, a program that decides differently, a missing law 991, a law that reads Step usage and a domain above the comparison cap each take the genesis route, and away from genesis refuse naming the premise with nothing written
    And a lineage with the same catalogs and other receipt digests does not audit the store
    And one bound lineage compares two programs once, never while the store's write lock is held, so another connection commits meanwhile
    And a contract that changes a law upgrades only a store whose state its genesis laws admit, which for a generated contract is the declared genesis state
    And each history segment replays under its own contract and pending deliveries are delivered exactly once with their original identifiers
    And a different schema, a refused genesis, a missing old contract, an altered record, an unreplayable receipt, a false usage claim, an unchanged program and an edit to a superseded version each refuse with nothing written

  @atdd-rule-change-upgrade
  Scenario: Change the rules of a live store after the owner reviews the change
    Given an escrow application whose store committed a funding and a shipment, and refused a dispute 14 days and one second after shipping
    When `contract evolve` replaces its contract with the study's 30-day dispute window, which `contract diff` classifies as a rule change
    Then the replaced contract and the plain-language diff are kept under `v2/evolutions/1/`, the rules bind both by SHA-256, and generation recomputes the diff and refuses an edited one
    And the new build audits the old store without changing a byte of it
    And `--upgrade` records a behaviour change: every state law of the new contract holds on the store's state, genesis exactness law 990 is reported as not evaluated, no decision programs are compared, and the record binds the review's digest while the store keeps its text
    And the store keeps committing: the same late dispute is now accepted, the escrow is split and both payouts are delivered, and an audit replays both segments, each under its own contract
    And a new state law or a declared claim that fails on the store's state refuses the upgrade naming it, with nothing written
    And a program successor, a layout change without a migration, an identical contract and an unrelated one are refused by `contract evolve` naming their kind, with nothing written
    And a forged or altered behaviour-change record, and a lineage that declares another review or an adoption for the step, refuse the store

  @atdd-data-migration-upgrade
  Scenario: Migrate the data of a live store across a layout change
    Given two spend-approval stores with committed history, one with a payment pending and one with a request awaiting approval
    When `contract evolve --migration` takes the spend-approval contract with an added urgent flag, which `contract diff` classifies as a layout change, with a migration that carries every old field and sets the flag to false
    Then the migration is admitted only by forward simulation over every state of the old contract's declared domain on which its state laws hold and every command and context: the genesis state maps, every migrated state satisfies the new contract's state laws, and every decision class and reason, every delivery and every successor state is kept
    And the migration is kept under `v2/evolutions/1/`, the rules bind it by SHA-256, and generation simulates it again and refuses an edited one
    And the new build audits both stores without changing a byte of them
    And `--upgrade` records a migration: the store's shell runs the same simulation itself, moves the state to the new layout, and the record binds the migration's digest, the tuples compared and the migrated state's root
    And the pending payment is delivered with the ID its old commit bound, the other store keeps committing under the new contract, and audits replay every segment under its own contract
    And a later rename of the flag is admitted at any state, after which all three segments replay
    And a migration that breaks a delivery, one that changes a decision, one whose genesis does not map, one that maps a state the old laws allow to one a new state law forbids, a shortcut that disagrees with the composed route of consecutive migrations, a domain above the cap of 2^20 tuples and a layout change without a migration are each refused with nothing written
    And a forged migrated state, and a lineage that declares another migration, a rename or an adoption for the step, refuse the store

  @atdd-contract-diff
  Scenario: Classify a contract change and name every changed item
    Given an old and a new contract directory
    When `contract diff` generates both as `generate contract` does and compares their canonical policies, schemas and structure
    Then exactly one kind is decided, the first that holds in a fixed order: identical, program-successor, rename, layout-change, rule-change or unrelated
    And a field renamed throughout is a rename, an added state field or a retyped state type is a layout change, a changed law formula, case guard or reason set is a rule change, and a changed channel or command set is unrelated
    And the summary names every changed law, case, field, variant, channel and reason, and the admission path the kind needs, saying which paths exist today
    And every adoption is a program successor, `contract adopt` refuses every other kind before it writes, the SQLite shell's own policy comparison accepts exactly the pairs called identical or program successors, and its Tier A admission admits only program successors
    And the document is byte-identical on repeat and for copies of the same contracts elsewhere, and a refusal names the side and writes nothing

  @atdd-app-journey
  Scenario: Build, optimize, adopt and upgrade applications through their command lines alone
    Given the app study's escrow and spend-approval contracts as CLI test fixtures
    When `zeno-fcis new` builds an application from the escrow contract in a new directory outside the repository
    Then the commands its README lists, run exactly as written, build it, check its decision examples and run its session, including the split that pays out in two deliveries
    And `contract export-program`, `optimize`, `transform replay` and `contract adopt` make the spend-approval contract's version 2 with no other tool
    And a version 1 store with four commits and a pending payment upgrades to version 2 as a program successor at commit 4 and delivers the payment under its original identifier
    And the version 1 build then refuses the store, and a store at another version refuses `--decide` and keeps its bytes
    And a version 1 store away from genesis upgrades at commit 2, keeps committing under version 2 with `--decide`, and its audit replays both segments
    And the version 1 and version 2 builds, sharing one target directory, print different identities with `version`
    And the spend-approval operation runs through the generated command line alone: `init`, a request created, approved by the CFO and the CEO and executed with `submit`, its payment delivered to a file exactly once with `deliver`, then `history` and `state` read back
    And a rejected, unknown or out-of-range submission writes nothing, and the version 1 build refuses the upgraded store with `Identity`
    And a CLI built as the release build builds it holds no path of this checkout, refuses `new` without `--source` and binds with it
