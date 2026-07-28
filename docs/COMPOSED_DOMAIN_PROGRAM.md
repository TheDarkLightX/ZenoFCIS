# Composed Domain Program v1

## Purpose

`zeno-fcis-composed-program` connects fixed-size domain machines to the existing
catalog and law-aware production authority. It follows the bounded-context
discipline of domain-driven design: each machine has one narrow local model,
while a closed global context map defines how the aggregate root invocation is
split and recombined.

## Inputs and outputs

The authority supplies one exact admitted root state, command, authenticated
context, catalog, state domain, invocation commitments, and transition limits.
The program owns:

- one `ExecutableComposition`;
- one nominal machine array;
- complete root paths for every local state, command, and context position;
- complete output treatment for every port;
- a closed reason domain for every machine row;
- reviewed per-machine implementation/configuration commitments;
- independent machine and projection budgets.

It returns one ordinary `TransitionDecision`. The existing
`CatalogCommitAuthority` performs relational-law verification and is the only
constructor of `CatalogAuthorizedTransition`.

## Authority boundary

This crate introduces no authorization witness, interpreter, database, shell,
or evidence callback. A caller cannot provide local matrices or a prebuilt
system execution. Every matrix cell is derived from the exact root invocation.
The authority's concrete program type remains carried nominally through policy,
invocation, authorization, and shell types.

## Construction laws

1. Local state ownership maps to pairwise nonoverlapping root paths.
2. Every interface envelope uses the exact catalog schema.
3. Every owned machine identifies the exact canonical component row.
4. An inactive interface port must use `Inactive`.
5. A routed port must use `Internal` and cannot also stage an external effect.
6. Every active unrouted port maps to exactly one catalogued effect or channel.
7. Effect and outbox payload types equal the interface output type.
8. Fixed outbox destinations satisfy the catalogued destination schema.
9. Outbox output paths are declared in both the executable effect boundary and
   the component's explicit outbox footprint.
10. Every executed machine's external output is present exactly once, while an
    unexecuted suffix after committed failure has no outputs.
11. Per-machine reason domains are closed and their global precedence is
    monotone in the explicit merge order.
12. The derived composition/projection/machine/budget hash equals both the
    project algorithm binding and authority transition-build binding.
13. Machine budgets and projection budgets are independently enforced.
14. Aggregate resource use is checked by addition without overflow.
15. Reject produces no patch, effect, outbox entry, or candidate.
16. Accept and committed failure project every changed state cell and every
    emitted external output before catalog sealing.

## Deterministic resource bounds

The fixed dimensions bound the number of state, command, context, and output
positions. Construction requires projection capacity for all reads, every
possible changed state cell, and every possible external output. Canonical byte
work is charged at runtime against an explicit projection byte limit. Each
machine receives its own immutable `BudgetLimits`; reported use is aggregated
with projection use only after all independent bounds pass.

## Negative cases

Construction or execution fails closed for overlapping root state paths,
machine substitution, catalog substitution, schema substitution, wrong output
or reason treatment, unknown effect/channel/reason IDs, payload-type mismatch,
fixed effect authority/subject mismatch, invalid outbox destinations, output
from an unexecuted machine, logical-limit overflow, budget exhaustion, malformed root paths,
or any downstream catalog/transition failure.

## Trusted dependencies

The crate trusts the selected approved commitment provider, the exact machine
implementation build named by the authority policy, the catalog and law engine,
and the existing fixed-domain executor, transition builder, and authorization
implementation. Reviewed per-machine build commitments identify the compiled
implementations/configurations; external build provenance must establish that
those commitments name the deployed code.

## Assumptions and nonclaims

- The exact `CompositionSpec` and its formal composition evidence are reviewed
  release inputs. This crate does not verify external proofs during a request.
- Machine implementations are pure and total within their supplied logical
  budgets.
- Sequential merge order is explicit protocol data. It is not a claim that all
  domain machines commute.
- Proof-carrying deterministic-parallel coordination remains a static promotion
  layer. V1 does not ship a concurrent scheduler or threaded shell.
- Production parallel promotion additionally requires complete static footprint
  witnesses for every component and full equality with the canonical sequential
  result.
- V1 projections are direct replacement-only subtrees and exclude map-key paths.
- This is not a mechanized end-to-end correctness proof.
- It does not make an incorrect catalog, machine, context map, law definition,
  interpreter, or deployment correct.
- It does not authorize production publication without the existing catalog
  authority and successful project-law evaluation.
