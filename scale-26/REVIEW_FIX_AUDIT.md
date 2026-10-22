# Transition lemma scope repair

`PreStates::TRANSITION` in `crates/zeno-fcis-cli/src/contract/review/lemma.rs:47`
defines the pre-state set once: every declared pre-state, including unlawful ones.
Signed finite membership (`lemma.rs:152`), unsigned finite membership
(`unsigned.rs:1076`) and symbolic constraints (`numeric_symbolic/lemma.rs:204`)
consume it. State induction keeps the lawful-state and proposed-lemma premises.
The former design independently chose this set in each route; the replacement
adds one shared selector over native Booleans or symbolic terms, with no cache.
Native decisions, admission laws, rejection order, canonical formats and limits
are preserved. Including previously omitted transition inputs is the intended fix.

The following inventory uses paths relative to `crates/zeno-fcis-cli/src/`.
Each entry was inspected for an unintended lawful-pre restriction:

| Query or route | File:line | Pre-state treatment |
| --- | --- | --- |
| Declared input encoding | `contract/review/numeric_symbolic/encode.rs:360` | Range/enum constraints only; lawful term at :479 is defined, never asserted here. |
| Arithmetic-edge queries | `contract/review/numeric_symbolic.rs:199`, `:212` | Full declared inputs, plus the requested edge or concrete assignment. |
| Sample existence | `contract/review/numeric_symbolic.rs:263` | Concrete native inputs; no lawful-pre filter. |
| Sample mismatches | `contract/review/numeric_symbolic.rs:295` | Concrete native inputs; no lawful-pre filter. |
| Technical-refusal partitions | `contract/review/numeric_symbolic.rs:398` | Program/construction failures across all declared inputs. |
| Law counterexamples | `contract/review/numeric_symbolic.rs:419` | Selected constructed decision and earlier satisfied laws; no lawful-pre filter. |
| Mutant differences | `contract/review/numeric_symbolic.rs:495` | Both encoded graphs share unrestricted declared inputs. |
| Lemma differential guard | `contract/review/numeric_symbolic/lemma.rs:247` | Sample assignments and native law results, no lawful-pre filter. |
| Transition nonvacuity | `contract/review/numeric_symbolic/lemma.rs:300` | Shared declared pre-state set; admitted commits only. |
| Transition counterexample | `contract/review/numeric_symbolic/lemma.rs:339` | Same shared set and admitted commits; no lawful-pre premise. |
| State initiation/consecution | `contract/review/numeric_symbolic/lemma.rs:285` | Genesis checked natively; lawful pre-state at :209 and proposed lemma at :346 intentionally retained. |
| Older invariant induction | `contract/symbolic.rs:477` | Explicit induction scope: :295 supplies state-law/strengthening premises; :514/:518/:540 use that base. Domain-only control :526 omits it. |
| Transform output/failure/control | `symbolic/equivalence/queries/base.rs:30`, `pieces.rs:20`, `pieces.rs:100`, `pieces.rs:160` | Raw declared inputs; defined-output and failure partitions do not assume contract state laws. |
| Probe | `contract/review/probe.rs:113`, `:255` | Finite/native route; cap refuses, conditions read actual decisions including repairs. No symbolic route. |
| Scenario | `contract/review/scenario.rs:258` | Native replay from the written first pre-state, then actual successors. No symbolic route or lawful-pre filter. |
| Gaps | `review_command.rs:341`, `contract/review/gaps.rs:551` | Symbolic counting explicitly UNKNOWN. Finite full and lawful subsets are separately labelled and counted. |

The six-input fixture regression compares native counts with both solvers on
each fixed input of the actual emitted obligations: 6 admitted, 2 failing, 4 holding.
The producer also retains CLI queries, answers and a table of those exact counts.
These counts concern this fixture only; wide-domain reports retain symbolic scope.
Controls restore both old premises and, separately, the nonvacuity premise alone.
All new data is invocation-owned. The shared selector takes owned values and a
single-use closure; it introduces no mutable interface, retained state or alias.
Native Authority references remain immutable; solver preparation has no runtime
decision, metering or publication authority. Final source identity is recorded by
`run_review_fix.py`; its results are scoped regression evidence pending review.

## Background

C. A. R. Hoare, *An Axiomatic Basis for Computer Programming*, Communications
of the ACM, 1969, supplies standard assertion-based program reasoning.
Leonardo de Moura and Nikolaj Bjørner, *Z3: An Efficient SMT Solver*, TACAS,
2008, and Haniel Barbosa et al., *cvc5: A Versatile and Industrial-Strength
SMT Solver*, TACAS, 2022, describe the two solver systems used here.

## What is new here

prior-art search pending (Commons gap-fb324b6fe17f). This is a maintenance
repair of transition scope and its regression evidence. No comparative
prior-art search was performed offline; the lead will record the search.

## Scope

The inventory concerns the stated declared-domain obligations and the
six-input cross-mode regression. Encoding guards remain sampled comparisons.
The Background references supply assertion and solver context, rather than
validation of this encoding. Independent soundness review remains pending.
