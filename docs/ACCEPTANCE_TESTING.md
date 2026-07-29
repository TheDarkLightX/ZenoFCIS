# BDD and acceptance testing

ZenoFCIS uses behavior-driven scenarios to make the RC2 product contract
readable by adopters, reviewers, and coding agents. It uses acceptance-test
driven development to bind each scenario to a fixed executable command set.

## Roles

```text
BDD feature text
    states the user-visible requirement and nonclaim

closed ATDD registry
    maps one stable scenario ID to fixed argv arrays

Rust tests and checkers
    provide the executable evidence behind the scenario

formal/refinement evidence
    separately supports its exact theorem or bounded claim
```

BDD does not prove a transition law. ATDD does not make a finite test portfolio
complete. They close adoption requirements and detect public workflow drift.
Canonical, property, differential, mutation, Miri, crash, refinement, and formal
evidence retain their existing roles.

## Commands

Check that every feature scenario has exactly one registered executable ID and
that the registry has no hidden scenarios:

```bash
python3 tools/atdd.py self-test
python3 tools/atdd.py check
```

List or run scenarios:

```bash
python3 tools/atdd.py list
python3 tools/atdd.py run --scenario minimal-core
python3 tools/atdd.py run --all
```

The runner uses fixed argument arrays and does not execute command text from a
feature file. This keeps Gherkin prose outside execution authority.

## RC2 scenario registry

| Scenario ID | Acceptance boundary |
|---|---|
| `minimal-core` | Immutable transition and logical budget example. |
| `checked-backend` | Bounded tool-neutral backend request example. |
| `external-consumer` | Isolated downstream compile against public API. |
| `project-bootstrap` | Deterministic generated starter and negative vectors. |
| `composed-program` | Fixed domain-machine and global composition portfolio. |
| `production-authority` | Catalog, invocation, law, genesis, and nominal commit authority. |
| `sqlite-authority` | Crash-atomic authorized history and durable outbox. |
| `release-contract` | Complete formatting, lint, tests, docs, assurance, and packaging checks. |
| `probity-guardrails` | Hostile and permitted coding-agent workflow commands. |

The permanent adopter-acceptance workflow runs the checker and complete
portfolio from the exact source revision.
