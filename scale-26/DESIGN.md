# Numeric law review

The subject is the compiled decision graph, schemas, decision table and ordered
law programs. The declared input product is computed without enumeration. The
existing exhaustive review remains selected when that product fits its cap.

Every graph node denotes an exact SMT integer or Boolean. Each checked operation
adds its width and definedness predicates, including unused nodes and unselected
arms. Fused multiplication/division keeps the full mathematical product and
checks the quotient; product comparisons retain all three factors. Input enums
are constrained to their exact variant set. Unsigned ranges above the signed
model transport use two bounded 64-bit limbs, reconstructed without narrowing;
smaller unsigned ranges use one integer with the same declared bounds.

Let L be satisfaction of the state laws on the pre-state, C a selected and
constructible candidate, and H_i satisfaction of applicable law i. The ordered
law question is satisfiability of L and C and H_0 through H_(i-1) and not H_i.
A separate question searches technical refusals from lawful pre-states. Thus
arithmetic failures cannot make preservation vacuously pass. Rejects are checked
under their native scopes. The native Authority checks genesis during binding.

Each conclusive result needs matching answers from Z3 and cvc5. Both sat models
must replay against the native Authority. Unknown, timeout, disagreement, failed
replay or missing solver gives UNKNOWN. Positive evidence is solver-checked
(z3 and cvc5 agree), with no independently checked SMT proof claim.

Before law queries, each contract runs differential queries against native
Authority outcomes on endpoint neighbours, zero/one, paired extrema, fixed-seed
random samples for every command variant and solver-generated arithmetic-edge
witnesses. Arithmetic and domain refusals are kept distinct. This is sampled
translation evidence, not a proof of encoding soundness. Every sampled input
must also have an encoding model, so narrowing the encoding cannot pass the
mismatch check vacuously. Edge generation that
cannot conclude also prevents admission. Solver output and query hashes are
recorded; ZENO_FCIS_SYMBOLIC_QUERIES retains exact queries and outputs in a new
directory. Failure to retain requested evidence prevents a positive status.

Mutants use the same compiled encoding and guard. Exact native replay confirms
differences. Equivalence questions conservatively leave unrepresented refusal
details UNKNOWN. State lemmas check native initiation and symbolic consecution;
transition lemmas check actual admitted commits and require a committing witness.
Proposed lemmas are never installed as admission laws of the subject.

The SMT term/printer and process runner are reused. A single typed translation
handles signed and unsigned compiled graphs instead of modifying the existing
state-only symbolic API or its established verdict policy. No acceptance checks
were removed. Changed scope and new report identities are intentional.

Ownership review: encoding builders, edge witnesses and sample vectors have one
invocation-local owner. The immutable compiled contract is only borrowed. All
reports own their data; no mutable policy alias escapes. Solver sessions and
query retention are shell preparation handles, with no decision, publication or
resource-usage authority. Native evaluation and measured refusal work remain
unchanged. This addition has no source-to-runtime correspondence theorem.

Concrete owner-example findings use the existing native review with one probe
and its owner-example mutant witnesses. Such a finding refuses early, in
`native-examples` mode, and issues no symbolic law verdict. The factory checks
that the emitted project, rules and schema hashes match the retained review.
Numeric durable applications remain explicitly refused until their native
server adapter exists; bounded UI evidence cannot replace that adapter.

Branch-selection edges first try the first existing concrete guard input.
A matching SAT pair supplies a witness; a matching UNSAT pair for that one input
still requires the unrestricted edge query. UNKNOWN refuses immediately. Both
queries remain in the report when unrestricted search is needed. Other edges
keep the original single search. No cache or alternative verdict rule is added.

Constructibility D implies program validity P. The technical-refusal question
L and not D is split into L and not P, and L and P and not D. Their union is
exactly the original set. This removes the branch selector from the first
question without removing a refusal check, changing law order or weakening
dual-solver agreement and native replay. Every part must conclude.

Program validity is a conjunction g_0 through g_n. Its failure is partitioned
into first-failure questions L and g_0 through g_(i-1) and not g_i. Their union
is exactly L and not P; no prefix is assumed without retaining its own failure
question. All questions must conclude. This preserves refusal coverage and
helps a solver use established denominator predicates before division bounds.

The differential observation is the decision class, reason, post and outbox;
refusals distinguish arithmetic, domain and the first failing law ID. A law's
undefined arithmetic and false predicate both mean failure of that law.
Diagnostic refusal text and measured native work are not modeled as symbolic
outputs. No complete refusal-detail or resource-usage equivalence is claimed.
Native Authority execution and its refusal accounting remain unchanged.
