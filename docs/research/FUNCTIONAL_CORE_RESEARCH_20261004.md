# Functional-core research brief: implications for ZenoFCIS

Date: 2026-10-04. Scope: research intake and current-plan comparison.
Source: user-provided `functional-core-research-brief.pdf`, all four pages read.
PDF SHA-256: `64864411111372088bc5c5b690ff003fb17086182516d390b8ced64f0514f309`.
The PDF reports three isolated experiments and a scoped audit. Its native/Lean,
mutation and performance evidence has not been replayed during this intake.

## A real application candidate for the benchmark design

The withdrawal-queue Boolean kernel defines:

```text
A = pa OR aa
B = pb OR ab
Any = A OR B

A = Select(pa, pa, aa)
B = Select(pb, pb, ab)
Any = Select(A, A, B)
```

This is a candidate project-derived benchmark, pending source extraction and
qualification. It supplies a useful target beyond the 32 public development pairs.
Keep it separate from those seeds until its exact source, ABI, domain, observation
profile, original lowering and full artifact are recorded.

Root's independent host recheck reproduced all 16 Boolean valuations and exhaustively
checked the stated richer gate grammar: four input leaves and two Boolean constants
are free; Not, And, Eq and Select cost one gate. There are 294 one-gate choices,
448 second-gate choices after any first gate, and 131,712 ordered two-gate programs.
None computes Any; the three-Select witness computes all three required functions.
This rechecks the finite Boolean model. It does not replay the reported Lean proof
or establish a Rust-to-Lean refinement or full-template minimum.

The current `tools/check_template_contracts_v2.py` still expands scalar OR into
Not/And/Not operations. The brief reports a 69-to-60-node full-graph rewrite;
this intake did not reproduce that complete graph or its native journeys.

## Metering determines whether replacement is allowed

The brief reports 69 versus 60 successful Steps, and original refusal versus
candidate success at budget 60. Preserving scalar outputs alone therefore does
not preserve the exact metered application contract. Padding can preserve logical
Step counts while physical optimization remains a separate measured question.

Use the existing [transform design](../V2_1_TRANSFORM_DESIGN.md): functional
checking comes first; application adoption needs its own resource/identity contract,
complete refusal observations, source-bound evidence and historical replay policy.
The [benchmark protocol](../benchmarks/PROTOCOL.md) already separates functional
outcomes, resource behavior, emitted size and physical performance. Retain that split.
Do not silently replace the template lowering or reuse an old certificate.

## DEX results inform the method, not this scalar milestone

The brief reports about 4.5x faster ordered-delta helper execution, but only about
7% improvement in an author's whole static-refinement run. Its sorted-unique,
exact-type, immutable-input admission boundary is essential; broader helper inputs
need retained behavior or an explicit contract change. Qualification belongs in DEX.

It also reports the exact integer identity, for naturals n, B and d with 0<d<=B:

```text
g = ceil(n*B/d)
floor(g*d/B) = n
ceil(g*(B-d)/B) = g - n
```

The proposed fee reconstruction removes one division but reportedly improves the
real wrapper by only about 3-6%. Exact normalized integer types, rounding and
intermediate-width/overflow obligations must survive. No timing claim from the
PDF was independently measured here. These Python/large-integer candidates are
outside the first total-Boolean transform profile.

## Bounded continuation

Finish smaller V2 under its existing completion contract. This brief adds no gate.
At the planned V2.1 transformation milestone, obtain the experiment reports and
proof files; repin the current source, extract this real Boolean kernel, compare
actual emitted artifacts and qualify its values/errors and observation profile.
Treat metered application activation as a separate reviewed step. Measure actual
whole-boundary performance before claiming runtime or throughput improvements.

The PDF's source freeze is `1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c` and says
an authoritative V2.1 plan was unresolved. The current local
[V2.1 roadmap](../V2_1_FACTORY_PLAN.md) now exists but remains planned; that local
state does not establish remote publication or implementation acceptance.
No runtime code, existing benchmark fixture or release condition changed in this intake.
