# Boolean and checked-integer diagnostic benchmark design

Status: proposed benchmark architecture; public seed fixtures locally checked.
Prepared 2026-10-04. Diagnostic effectiveness remains an untested design goal.
This packet implements neither `transform` nor a new optimizer or runtime.

## 1. Purpose and place

The suite should expose why a transformation succeeds, fails, or cannot be checked.
It should help maintainers distinguish semantic mistakes from weak search.
A smaller returned graph is useful only after the exact fixed contract passes.
Finish smaller V2, including Authority → Publication → SQLite, first.
Preserve the supported compound/U128 prerequisite and roadmap tracks 1–5.
Track 6 begins with supplied-candidate Boolean checking, then receipts and search.
The checked-i64 cases here specify future qualification; they enable nothing.
This design adds no condition to completing V2 or the maintenance pilot.
See [roadmap](../V2_1_FACTORY_PLAN.md), [transform design](../V2_1_TRANSFORM_DESIGN.md),
[loop design](../neurosymbolic-loop/DESIGN.md) and [specifications](../neurosymbolic-loop/specs/INDEX.md).

## 2. The Herbie analogy and its boundary

Herbie's PLDI 2015 section 6 combines 28 textbook examples from Hamming's
Chapter 3 with wider numerical application examples. This suggests combining
small named pathologies with independently sourced practical workloads.
That is an analogy in diagnostic design, not a transferred effectiveness result.
No floating-point expression, error measure, or speed claim is imported here.
Source: [Herbie paper, section 6](https://herbie.uwplse.org/pldi15-paper.pdf).
Herbie platforms make operation and representation costs explicit and target-dependent.
Our first objective instead measures actual stored DAG nodes and canonical bytes.
Source: [Herbie 2.3 platforms](https://herbie.uwplse.org/doc/2.3/platforms.html).
The CERES workshop abstract describes a rewrite scheduler intended to reduce
memory demand; its reported Herbie results are not ZenoFCIS measurements.
No full algorithm, implementation identity or license has been qualified here.
Source: [CERES EGRAPHS 2026 abstract](https://pldi26.sigplan.org/details/egraphs-2026-papers/2/CERES-Making-Equality-Saturation-Memory-Scalable).

## 3. Four layers, with different jobs

L0 is this public regression/development catalog: 32 handwritten program pairs.
L1 will contain independently extracted project cases with reviewed contracts.
L2 will contain generated stress cases, retaining every seed and construction rule.
L3 will be a future sequestered evaluation set, collected after splitting is frozen.
None of L1–L3 exists merely because this document describes it.
L0 is visible to models, authors and maintainers; it can never become hidden.
The old 100 synthetic Boolean cases remain a separate calibration-only collection.
They are not new independent evidence, held-out tasks or neural results.
Retain historical failures, resource amendments and successful exploratory results.
Do not overwrite the published experiment at a69ed8db594d95279a46bff0f65185ef67d51f98.

## 4. Contracts and observations

Each pair has one immutable ordered input ABI, output ABI and domain descriptor.
Candidates cannot narrow domains, delete unused inputs, or reorder output fields.
Boolean inputs range over both values; a zero-arity product has one empty tuple.
`FunctionalBoolV1` admits 0–6 inputs, 1–16 outputs and 1–256 eager nodes.
Its only opcodes are Input, Bool, And, Not and Select.
OR, XOR and Boolean equality must lower before artifact admission.
Every node is admitted and evaluated, including unused nodes and unselected arms.
Boolean evaluation is total after correct admission; an unexpected error is inconclusive.
The future i64 profile adds Int, Add, Sub, Eq and Lt with checked signed arithmetic.
Contiguous inclusive integer ranges must have a full product of at most 65,536 rows.
Existing scalar structural admission permits at most 32 input fields and 16 outputs.
The future profile still needs separate resource/admission qualification.
Small input ranges near MIN and MAX are useful; the full i64 input interval exceeds
this benchmark's enumeration ceiling. Full-width output intervals remain valid.
All declarations and literals use exact decimal strings, never floating-point JSON values.
Integer observations are complete ordered success tuples or exact scalar error tags.
The scalar tags are InputDomain, Reference, Arithmetic and OutputDomain.
Well-admitted pairs on their own input domains should not produce InputDomain/Reference.
Those tags remain relevant to evaluator/admission qualification, not successful search.
Arithmetic executes before root projection and output-domain validation.
On error there is no successful output prefix, partial tuple or fabricated zero result.
Current Arithmetic contains no source-node location; do not invent that observation.
A future richer error ABI would require a new version and fresh evidence.

## 5. Functional equality is deliberately narrow

The evaluator returns functional values/errors; V2 also returns metered usage.
Changing x+0 to x preserves functional output but removes two attempted instructions.
With Step limit 1 the original attempts another charge and refuses; the candidate succeeds.
At a generous limit the outputs agree while Step usage still differs (3 versus 1).
That is a qualification distinguisher, not an exact-application improvement.
Application laws, traces, reasons, resource refusal, identity and authority stay separate.
A fixture domain does not establish coverage of an application's original domain.
A policy-inspired Boolean guard is not an extraction of the complete application.
Even a qualified functional receipt cannot activate or publish a replacement application.

## 6. Fixture schema v1

`cases.json` has schema_version, status, enumeration and a cases array.
The schema version is `zenofcis-benchmark-seeds-v1`; unknown versions must refuse.
Enumeration is `ordered-product-last-input-fastest-v1`: Bool false then true,
integers increasing, rightmost ABI position advancing fastest.
A case contains id, family, profile, role, provenance, input_domains, output_domains,
abi, original, candidate, expected_relation, rationale, witness, target_status, eligibility.
IDs are unique stable labels, not identity commitments to mutable content.
Domain is exactly {kind: Bool} or {kind: Int, min: decimal-string, max: decimal-string}.
Bounds and Int literals lie in [-9223372036854775808, 9223372036854775807].
Decimal strings use canonical signed decimal: no plus sign, leading zeros or negative zero.
Input/output ABI names are unique within each ordered side, retained across both programs.
Programs are {nodes: [...], roots: [...]}; nodes and roots retain their stored order.
Node arrays have exact arities: Input/Int/Bool/Not take one argument;
Add/Sub/Eq/Lt/And take two; Select takes condition, yes and no references.
Input arguments are input indices; operation references name strictly earlier nodes.
Bool literals are JSON Booleans, Int literals decimal strings, indices JSON integers.
Inputs/literals count as stored nodes. Duplicate stored nodes count twice until removed.
Roots are ordered integer node references and must match output count and kind.
Eq requires same-kind operands; Lt/Add/Sub require integer operands.
And/Not require Boolean operands; Select needs a Boolean guard and same-kind arms.
Admission validates every node, even when no output reaches it.
No multiplication, division, shift, bitwise or short-circuit opcode is available.
`expected_relation` is Equivalent or Different; cost selection is a separate question.
`witness` is null for Equivalent, otherwise {ordinal, tuple, original, candidate}.
Witness observations are {ok: [typed values]} or {error: exact-scalar-tag}.
Boolean values use JSON Booleans and every integer observation uses a decimal string.
The supplied witness is the first different tuple in the declared enumeration.
Fixture JSON is a development interchange format, not the production artifact codec.
Its whitespace, key order and file length do not define canonical artifact cost.

## 7. Boolean family rationale

B01 diagnoses involution and the separation between values and instruction usage.
B02 requires absorption after disjunction lowering while preserving an unused input.
B03 factors distributed conjunctions; DAG sharing can change the benefit of lowering.
B04 simplifies a mux using information implied by its guard.
B05 merges shared work across ordered outputs, retaining repeated output positions.
B06 compares parity as a disjunction of products with a mux-based circuit.
B07 compares majority circuits, a different structure from a padded identity.
B08 is a synthetic permit/deny/ready guard; it exercises negation and policy gating.
B09 uses all six inputs and isolates a true terminal row, exercising full coverage.
B10 constant-folds a zero-input graph; it prevents empty-product vacuity.
B11 is a plausible but wrong De Morgan candidate with a distinguishing row.
B12 detects branch polarity; B13 detects priority lost by flattening into OR.
B14 preserves a root multiset while changing ordered output behavior.
B15 and B16 are no-improvement controls, including a compact shared multi-output DAG.
These are circuit/policy-inspired handwritten seeds, not production workload samples.

## 8. Checked-i64 family rationale

I01 is a harmless value identity that intentionally changes resource observations.
I02 checks self-subtraction on every tuple of a narrow MIN-endpoint range.
I03 and I08 pair ordinary-range cancellation with the MAX overflow counterexample.
Their domains are separately frozen originals; nobody narrows I08 to make it pass.
I04 and I09 pair safe reassociation with an intermediate-overflow counterexample.
I05 combines a Boolean input and integer comparison in a redundant clamp guard.
I06 shares integer offsets across multiple outputs and retains duplicate positions.
I07 preserves an Arithmetic row while removing a later neutral operation.
It checks that legitimate equal-error behavior remains admissible.
I07 alone cannot detect skipped error rows because its expected relation is Equivalent.
I08 detects success-only comparison; I14 detects erased scalar-error distinctions.
I10 rejects deleting a dead overflowing instruction.
I11 rejects deletion of an overflowing unselected arm, with a false-guard witness.
I12 distinguishes Boolean-style involution from checked arithmetic double negation at MIN.
I13 rejects silently repairing OutputDomain by clamping to a valid output.
I14 distinguishes Arithmetic from OutputDomain when both are superficially failures.
I15 is a compact direct-sum control; I16 is an equal-node alternate comparison form.
The integer catalog covers success/error observations; I09/I11/I14 have error-only originals.
Small ordinary and endpoint domains make exhaustive diagnosis cheap and interpretable.

## 9. Targets, cost and small optimum lane

Handwritten targets are feasible baselines to aim for, never minima by definition.
Seventeen positive pairs reduce stored node count in the development representation.
Their production-byte cost and guarded selection eligibility remain unmeasured.
Four controls and eleven incorrect candidates complete the 32-pair catalog.
Actual objective V1 is lexicographic (stored nodes, canonical artifact bytes).
Both components must be at most the original; one must strictly improve.
The selected candidate must also beat the incumbent lexicographically; ties retain it.
A later incumbent may have fewer nodes and more bytes than the prior incumbent,
provided both stay within original bounds. Do not enforce a stronger accidental rule.
Raw candidate cost/outcome and returned checked fallback are reported separately.
Runtime, memory and latency need distinct measurements on pinned targets.
A low node count is neither a byte result nor a runtime result.
The bounded optimum lane starts only with 0–2 Boolean inputs, one output,
Input/Bool/Not/And/Select, and at most three stored nodes.
Enumerate every legal topological DAG and root under that grammar, retaining constants
and repeated operands; compare complete truth vectors and actual canonical encoding.
For a target found at N<=3, exhaustive absence below N proves a node minimum
within this exact profile; exhaustive equal-N encodings are needed for the byte minimum.
An arbitrary canonical-renumbering quotient needs proof before pruning equivalent encodings.
No target found by the finite bound means unknown, not no implementation exists.
B15's one-node lower bound is immediate from the profile's nonempty-node requirement;
no general global-optimum lane was run or implemented for this design.
Do not run an expensive complete search as an unstated benchmark prerequisite.

## 10. Splitting before generation

Split construction families and provenance sources before generating any descendants.
Use connected groups that union family ancestry, shared source contracts, aliases,
alpha-renamings, constant/range substitutions and known semantic duplicates.
Here renaming includes input-name changes and type/domain-preserving input permutations,
with output positions fixed. This is a split rule, not permission to change a candidate ABI.
A new label or random seed does not make one independent task.
Known same-behavior originals here are B01/B15, B12/B13, I01/I03 and I08/I10.
Thus this catalog has 32 pairs but 28 distinct original finite behaviors.
That count uses fixed input positions. B02/B04 also share a group because swapping
their two Boolean inputs makes their original behaviors identical on all four tuples.
The independence_group field conservatively also joins cross-domain cancellation
and its eager-removal aliases: I01/I03/I08/I10 stay in one group.
Equivalent safe/unsafe family contrasts such as I04/I09 also stay together.
These groups do not retroactively turn any public fixture into hidden evaluation.
For L1 preserve the source revision, extraction boundary, contract and transformation map.
An independent maintainer accepts the contract before optimizer authors see outcomes.
For L2 freeze grammar/depth/sharing/guard/domain distributions and rejection rules first.
Generate maximum-node, output-count and resource-pressure strata alongside easy cases.
Retain failed/oversized constructions with disposition; avoid filtering for solver success.
For L3 restrict access, log exposure and use a separate acceptance custodian.
Any leaked family is contaminated and becomes development material in a later version.

## 11. Qualification and optimization are different denominators

Checker qualification includes domain shrinking, duplicated/missed rows and ABI mutation.
It also includes forged/stale receipts, bad resource observations and interrupted checking.
These scenarios are detailed in PROTOCOL; they are not optimizer success tasks.
Incorrect candidates in L0 test rejection of a supplied artifact, not search achievement.
For an optimizer task, its original can later receive a separately registered request;
the deliberately wrong target must not enter the success baseline or oracle.
Correctness-defect detection is measured against a predeclared planted-defect population.
The expected negative controls cannot inflate the checked-improvement rate.
Application-related scenarios document a boundary and do not qualify full applications.

## 12. Is the suite diagnostically useful?

Measure planted-defect detection with exact denominators and per-defect-family results.
Track false acceptance, false refusal and indeterminate outcomes separately.
Map each family to the source boundary and defect/search weakness it is intended to expose.
Compare fixed local rewrites, e-graphs and later neural arms under identical contracts.
Report where strategies differ, which witnesses explain that difference, and all null results.
Have maintainers judge whether a new independent case reproduces a real maintenance issue.
Record review time, reproducibility and whether the diagnosis led to a retained regression.
Do not invent a weighted effectiveness score after seeing outcomes.
A useful suite must survive independent case additions and known-defect mutation tests.
Equal effectiveness to Herbie cannot be concluded from case count or local fixture checks.
A future comparison must first define comparable diagnostic goals and external evaluation.

## 13. Evaluation extension and remaining work

Preserve the six-arm neural protocol: local-only, fixed rewrite e-graph, semantic e-graph,
model-only proposals plus checker, hybrid without feedback, hybrid with feedback.
Scheduler comparison is orthogonal, with identical rules, extractor, checker and budget.
A CERES implementation was not located or qualified for this design. Its algorithm,
pins and licenses need qualification; its abstract does not specify an implementation.
Use family-balanced paired metrics and task/family clusters, not retries as new tasks.
Preregister counts, repeat seeds, stop rules, failure treatment and uncertainty before runs.
The protocol proposes defaults for future freezing; this packet runs no optimizer study.
Source-pinned production decoding, native correspondence, proofs, receipts, meter tests,
resource enforcement and application qualification remain separate unperformed work.
The development scripts only validate these authored fixtures over their declared domains.
