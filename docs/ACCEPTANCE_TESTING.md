# BDD and acceptance testing

ZenoFCIS uses behavior-driven scenarios to make the RC3 product contract
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

## RC3 scenario registry

| Scenario ID | Acceptance boundary |
|---|---|
| `minimal-core` | Immutable transition and logical budget example. |
| `checked-backend` | Bounded tool-neutral backend request example. |
| `external-consumer` | Isolated downstream compile against the public authoring API. |
| `project-bootstrap` | Deterministic generated starter and negative vectors. |
| `composed-program` | Fixed domain-machine and global composition portfolio. |
| `production-authority` | Catalog, invocation, law, genesis, and nominal commit authority. |
| `sqlite-authority` | Crash-atomic authorized history and durable outbox. |
| `release-contract` | Local formatting, lint, tests, docs, assurance, and packaging gate. |
| `probity-guardrails` | Hostile and permitted coding-agent workflow commands. |
| `security-hotspots` | Prompt-inert deterministic EPI model and exact review-queue baseline. |
| `rc3-project-new` | Create a bounded project without overwriting files. |
| `rc3-mini-os-check` | Check the Mini Determinator project in one command. |
| `rc3-spec-canonical` | Equivalent source produces identical typed AST bytes. |
| `rc3-composition-diagnostics` | Composition blockers are complete and deterministic. |
| `rc3-mini-os-replay` | Private-workspace coordination is completion-order invariant. |
| `rc3-mini-os-conflict` | Conflicting merges reject without authoritative change. |
| `rc3-temporal-modes` | Finite execution and unbounded obligations stay distinct. |
| `rc3-formal-tools` | Formal output binds exact claim and tool family. |
| `rc3-formal-fail-closed` | Hostile outcomes block and models replay before refutation. |
| `rc3-input-inert` | Shell, traversal, environment, and instruction text stays inert. |
| `rc3-derived-views` | Graph and explanation views are deterministic and diagnostic only. |
| `rc3-generated-drift` | Generated source and manifests reproduce and detect drift. |
| `rc3-resource-envelopes` | Deep parsing, huge horizons, and oversized exports stop within fixed limits. |
| `rc3-process-boundary` | Timeout, solver names, and execution bind to exact checked bytes. |
| `rc3-cli-json-contract` | Valid and invalid projects return versioned deterministic JSON. |
| `rc3-package-binary-inventory` | Every declared binary receives one unique checked archive. |

The permanent adopter-acceptance workflow runs the checker and complete
portfolio from the exact source revision.
