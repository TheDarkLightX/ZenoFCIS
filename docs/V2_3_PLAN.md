# ZenoFCIS V2.3: reusable formally checked cores

Status: owner-requested roadmap, updated 2026-10-09. The component catalogue has
passed the local qualification described below; integration and release gates
remain pending. Other sections retain their stated completion obligations.
V2.2 retains its existing requirements; this roadmap does not silently defer them.

## Intended result

ZenoFCIS intends to be a neurosymbolic, state-of-the-art high-assurance software
factory. V2.3 should make its checked application path easier to reuse and improve
the evidence connecting specifications to actual execution. That is a target,
not a demonstrated comparative ranking.

The central addition is a **Zeno Core Standard Library**: useful functional cores
distributed as declarative rules and bounded program data, with laws, assumptions,
independent examples, composition obligations, and revision-bound evidence.
Application agents should instantiate and compose those cores before inventing
equivalent decision callbacks.

## What the existing proof inventory does

The proof inventory binds a verifier run to implementation functions, contracts,
body coverage, and source identities. It helps detect omitted functions and
weakened verification. It is not a catalog of application behaviors, and it does
not establish that separately checked components compose correctly.

A component library adds reusable application semantics on top of the checked
engine. Its evidence must distinguish the engine's implementation proof from
proofs about a particular component, its parameters, and its composition.

## A. Zeno Core Standard Library

V2.2 G1's reservation pool, rate limiter, and approval queue are the first seeds.
They remain V2.2 work. V2.3 expands those seeds into a curated library: first ten
well-supported components, then a larger catalog driven by useful applications.
Hundreds of cores are a growth target, not the initial release criterion.

The [initial ten-family catalogue](../core-components/README.md) contains
reservation pools, fixed-window rate limiters, single-request approval queues,
bounded counters, consumable budgets, versioned registers, idempotency slots,
retry budgets, finite phase machines and logical deadlines. Local qualification
covered all 77 declared parameter instances and all 107,518 raw inputs, 308
required generated-app native tests, 17 actual law mutants, seven certificate
tamper controls and proof references in all 77 scaffolded apps. Stored references
remain Identified; exhaustive replay supports Proved only for their closed finite
scope under the [named trusted base](../core-components/FINITE_PROOFS.md).

Six proposed apps still need coupled implementations and evidence: warehouse
reservations, expense approval, reliable jobs, document release, API quotas and
equipment booking. Isolated catalogue qualification does not discharge their
cross-component obligations. A separate epoch-quota core remains a candidate;
the current rate limiter has fixed-window semantics.

Every published core must include:

- immutable state, command, and supplied-context schemas;
- supported bounds and parameters, a valid genesis, and explicit outcomes;
- required successful examples and business-refusal examples;
- laws covering accepted decisions and any stronger claims being made;
- assumptions, including supplied authorization, time, and external observations;
- composition requirements: ownership, read/write footprints, shared resources,
  delivery obligations, and relevant cross-component invariants;
- canonical contract/program identities and reproducible evidence;
- one useful worked application and documentation of the assurance boundary.

Catalog metadata grants no publication or store authority. Instances continue
through the ordinary checked admission and execution route. Reference code,
examples, solver results, and proof status labels do not bypass it.

For composition, check the combined decision as well as component obligations.
In particular, conservation can require a cross-component invariant even when
each component is correct in isolation. A failed or unfinished obligation prevents
the stronger composition claim.

Completion: the first ten components have reproducible evidence and independently
reviewed examples; at least six applications show useful combinations; planted
parameter, mapping, and shared-resource defects are refused. Report each theorem's
actual evidence class. A family-wide claim requires evidence over the whole stated
parameter range, rather than tests of a few instances.

## B. Stronger symbolic proof evidence

Current symbolic UNSAT results are solver attestation. Solver proof text and a
second solver's agreement do not themselves establish a checked proof. SAT
witnesses can be checked by replay against the actual library execution.

There are separate obligations:

1. **Encoding refinement:** the emitted logical query must describe actual
   admitted execution, including eager traps, integer bounds, guards, refusal
   classes, output order, and the declared observations.
2. **Proof checking:** an independent, pinned checker must validate the exact
   certificate for the exact query. Name the checker and its trusted assumptions.
3. **Contract compilation:** any claim about source rules must also connect those
   rules to the program being checked.
4. **Artifact binding:** bind the result to the contract, program, domain,
   observations, toolchain, and generated artifact; compilation assumptions remain
   visible.

Start with a restricted, explicitly supported scalar query fragment. Keep UNKNOWN,
timeouts, incomplete proofs, unsupported expressions, and certificate failures
inconclusive. Never promote Attested by renaming a status. Recheck a deliberately
false query, a tampered certificate, altered bounds, and a trapping execution.

Restore or replace the public Lean path only with a working checked implementation
and applicable evidence. An existing KernelChecked enum or a fixed-fixture Lean
experiment is not public Lean support.

Completion: the supported fragment has an independently reviewed refinement
argument and reproducible certificate checking, with exact-artifact binding and
negative controls. Unsupported fragments retain their weaker status.

## C. Better ZAL authoring UX

Preserve the checked semantics while making human review concrete:

- show the proposed decision, changed state, deliveries, and matching rule;
- explain why a guard disabled a rule and why a refusal occurred;
- display supplied assumptions separately from authenticated facts;
- preserve owner-labeled examples and show distinguishing cases between revisions;
- preview semantic changes and stale evidence before adoption;
- connect CLI, MCP, and skill entry points to one documented authoring workflow.

The current ZAL authoring checker/lowering is tested, not machine-proved. Preserve
that distinction from the checked engine used for factory correspondence. Its
initial profile is a small finite state machine, without arithmetic or deliveries.
Improve the existing workflow before widening its semantics.

Completion: representative authoring, explanation, trace, change, and recovery
journeys pass with user-facing messages that state the actual guarantee. For
example, an `authorized` Boolean guard must not be described as authentication.

## D. Additional language profiles

Rust, JavaScript, and Python already have generation paths. Distinguish access to
the checked engine from an independently generated implementation in a new
language: these have different assurance obligations.

Add one language profile at a time. Specify exact integer, Boolean, byte, enum,
refusal, and serialization semantics. Test cross-language execution against the
same canonical corpus, including endpoint arithmetic and malformed inputs.
Calling the verified engine does not prove an application's imperative shell;
emitting source does not prove the emitter or target compiler.

Completion for each added profile: a reproducible toolchain, conformance and
negative cases, an installation journey, clear trust assumptions, and one worked
component application. Advertise only the guarantee its evidence supports.

## Order and release gate

1. Finish the supported V2.2 path and its existing G1 seeds.
2. Publish the first standard-library components and worked applications.
3. Qualify the restricted symbolic proof path; keep weaker paths visibly weaker.
4. Improve ZAL UX and qualify the next language profile in bounded stages.
5. Grow the catalog from demonstrated demand and reproducible evidence.

Existing V2.2 items that explicitly permit a V2.3 slip remain separate: optimizer
depth, the agent maintenance study, macOS packaging, and minor authoring fixes.
Any actual deferral must be recorded explicitly rather than implied by this draft.

Every implementation stage uses the current acceptance, proof, native, package,
source-identity, independent-review, and hosted-CI gates. Run the full acceptance
suite immediately before committing. No catalog size or marketing phrase replaces
these gates.

Companion design inputs, completed without implementation or fresh formal runs:

- [Core library design](research/V23_ZENO_CORE_STANDARD_LIBRARY_DESIGN.md)
- [24 candidate cores](research/V23_CORE_CANDIDATE_CATALOG.json)
- [Symbolic assurance and language review](research/V23_SYMBOLIC_ASSURANCE_AND_LANGUAGE_REVIEW.md)

The catalog records proposed requirements and pending proofs, with no authority.
Source pointers in the dated review identify the inspected working snapshot;
they are not evidence that later implementations have passed those obligations.
