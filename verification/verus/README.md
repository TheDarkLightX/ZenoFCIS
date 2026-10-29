# Shared-source Verus verification

The scalar arithmetic extension in development source changes the evaluator
and law source closure. Its new scalar bodies, node/admission cases, unsigned
law projection and signed-law division projection require fresh whole-source
proofs and reviewed translated inventories. Historical counts and receipts
below do not qualify these changed bytes. Qualification is pending.

The [Verus workflow](../../.github/workflows/verus.yml) runs 23 qualification
gates. Each gate first runs its script's own tests, then the script itself;
for the `shared-runtime` gate these are `python3 tools/test_check_verus.py` and
`python3 tools/check_verus.py --out` with an evidence path. The script requires
the complete exact-source proof and the gate's control suite. A separate job
checks the authority and synthesis crates without default features.

| Gate | Script | Subject |
| --- | --- | --- |
| `shared-runtime` | `check_verus.py` | [Finite-domain arithmetic](#arithmetic-unit) in the shared runtime source |
| `scalar-query` | `check_scalar_query_refinement.py` | Scalar runtime/query correspondence |
| `scalar-arithmetic` | `check_scalar_arithmetic.py` | Native emitted arithmetic conformance; separate [source proofs and Rust AST translation validation](#arithmetic-correspondence) |
| `finite-execution` | `check_finite_execution.py` | [Finite admission and eager evaluation](#v2-finite-admission-and-eager-execution) |
| `metered-execution` | `check_metered_execution.py` | [Owned V2 instruction metering](#owned-v2-instruction-metering); not transition authorization |
| `shared-finite-checker` | `check_checker.py` | The [shared finite checker](#shared-finite-checker) against its real metered evaluator |
| `lemma-kernel` | `check_lemma_kernel.py` | [State lemma decision core](#state-lemma-decision-core) over validated finite tables |
| `reachable-probe` | `check_reachable_probe.py` | [Reachable probe](#reachable-probe-and-shared-fixed-points), effective traces and fixed-point verdict forms |
| `canonical-bytes` | `check_canonical_bytes.py` | [Production V2 integer byte readers](#v2-integer-byte-interpretation); not schema admission |
| `value-decoder` | `check_value_decoder.py` | Production `decode_value`, abstract ZCVE model, resource invariants and refinement; qualification status is pinned in `value-decoder.json` |
| `protected-input` | `check_protected_input.py` | [Owned V2 protected record projection](#v2-protected-flat-record-projection) |
| `envelope-frame` | `check_envelope_frame_v2.py` | Original-envelope framing and exact payload custody |
| `schema-binding` | `check_schema_binding_v2.py` | Complete original schema bytes, closed metadata and private custody |
| `decision` | `check_decision_v2.py` | Complete typed decisions; byte/authority composition is separate |
| `account-data` | `check_account_graph_v2.py` | Native account rule-data controls only; the specialized proof was removed |
| `laws` | `check_laws_v2.py` | Laws and genesis, with custody tests |
| `composition` | `check_composition_v2.py` | The production composition module; no catalog or shell authority |
| `catalog` | `check_catalog_v2.py` | Catalog correspondence |
| `authority` | `check_authority_v2.py` | Canonical artifacts on the complete registered source closure |
| `original-output` | `check_original_output_v2.py` | Exact original-wire output from typed values |
| `publication` | `check_publication_v2.py` | Publication, with original-wire oracles and custody controls |
| `guarded-laws` | `check_guarded_laws_v2.py` | Guarded laws, with raw coverage and independent oracles |
| `checked-assembly` | `check_checked_assembly.py` | [Checked assembly](#checked-assembly): tables, assembly and route specialization over the evaluator specification |

Every V2 harness includes the complete production source closure, so fourteen
of the installed profiles expect the same whole-crate count, 1,161 obligations.
The `shared-finite-checker` and `scalar-query` harnesses include further sources
and expect 1,282 and 1,325; `checked-assembly` expects 1,188 whole-crate and
27 restricted obligations, with zero selected Exec bodies; the arithmetic unit
expects three. These are reviewed positive inventory snapshots; complete
qualification also requires each gate's controls and final source binding.
The gates differ
in the source their reviewed coverage manifests and controls examine, so their
counts must not be added. The unit sections below give each unit's subject and
controls as introduced; their record and obligation counts predate the shared
whole-unit profiles. Each gate's manifest and receipt record its current
inventory and results.

Together these gates make up the smaller V2 qualification: admission,
original-byte interpretation, scalar execution, complete decisions,
laws/genesis and mandatory authority/replay. Per-stage proof runs are
provisional; final source-bound profiles, controls and integration checks must
pass together. Local results, independent review and exact-head GitHub CI
remain distinct evidence. See the
[candidate record](../../docs/RELEASE_CANDIDATES.md) for qualification limits.

The gates keep the Rust 1.97.1 build and run the pinned Verus release with its
separate Rust 1.98.1 toolchain. Specifications are enabled only while Verus
checks the shared Rust source.

## Shared finite checker

The `shared-finite-checker` gate's [direct-source harness](checker.rs) includes
the shipped CLI/SQLite checker, the public library data projection API and the
unchanged real metered evaluator source closure. The public API requires fresh
whole-unit proof and executable inventory qualification; the earlier
frozen-core result does not cover it. `tools/check_checker.py` requires a
successful pinned whole-unit proof, independently reviewed translated contracts
and executable bodies, native checks through both consumers, stable source
hashes, and the CI negative controls. An emitted candidate inventory or the
cheap `v2-shared-finite-checker` ATDD scenario does not establish that gate.

## Arithmetic unit

- Check the same function bodies compiled into `zeno-fcis-authority`.
- Prove exact success and refusal conditions for interval cardinality and
  bounded products over all machine inputs, without application assumptions.
- Require `--no-cheating`; verify that deliberate arithmetic mutations fail.
- Pin the verifier archive and toolchain; record source hashes, command,
  verification result, trusted dependencies, and uncovered obligations.
- Compile the shared code with Rust 1.97.1, including the no-std crate path.
- Preserve the existing finite-input limit and admitted-input behavior.

## Proof sketch

1. A signed 64-bit interval's mathematical width fits signed 128-bit
   arithmetic. Accept precisely positive widths representable by `u64`.
2. The product of two `u64` values fits `u128`. Accept precisely products at
   most the supplied `u64` limit; only then narrow the result.
3. Specify both `Some` and `None`, so an implementation that rejects every
   input cannot satisfy the contracts.
4. Keep the executable bodies independent of verifier configuration; only
   specification attributes, imports, and erased proof blocks depend on it.
5. Mutation checks exercise off-by-one, narrowing/limit, and specification
   omission failures. Integration tests retain complete template behavior.

This verifies two admission calculations. The input enumeration loop, decoder,
complete decision construction, resource meter, law engine, genesis, canonical
encoding, and shell remain separate obligations. Verus/Z3 verification has its
own trusted base and does not produce ZenoFCIS's Lean `KernelChecked` evidence.

## Reproduce

This profile is qualified for Linux x86-64. Install both toolchains:

```sh
rustup toolchain install 1.97.1 --profile minimal
rustup toolchain install 1.98.1 --profile minimal
python3 tools/test_check_verus.py
python3 tools/check_verus.py --install --out /tmp/zeno-fcis-verus-evidence.json
cargo +1.97.1 check -p zeno-fcis-authority --no-default-features --locked
```

`--install` downloads upstream release `0.2026.09.27.3cf1832`, checks its
archive size and SHA-256, and extracts nine explicitly pinned runtime files.
Subsequent runs can omit `--install`. The checker rechecks those files before
and after execution and pins the bundled Z3 rather than using a system solver.
The archive, source commit, Rust versions, and individual tool file hashes are
recorded in [toolchain.json](toolchain.json).

[domain_bounds.rs](domain_bounds.rs) imports the actual authority source by
path. Its ordinary Rust tests use the same file. The runtime calls both
functions while admitting finite contracts. The interpreter's evaluator
identity includes both its own source and the extracted helper source using
a canonical source tuple under `finite-decision-evaluator` version 2.
Existing persisted identities are therefore not interchangeable with this
revision; no migration or old-history compatibility is claimed.

The receipt records exact source hashes, the Git revision and dirty state,
commands, tool versions, proof coverage, native tests, and four mutations:
wrong interval width, rejecting a product exactly at its limit, bypassing the
product limit, and omitting one function's specification. The last specimen
passes its reduced Verus check but is refused by the coverage gate.

The three verification obligations comprise two functions and one explicit
nonlinear arithmetic assertion. They are not three application properties.
The hypotheses come from machine integer types; the functions have no
application preconditions or application `assume`/`external_body` declarations.
Verus, its imported library specifications, Z3, Rust compilation/erasure, and
the host remain trusted.

The regular acceptance suite runs the native and evidence-admission tests.
The separate [Verus workflow](../../.github/workflows/verus.yml) runs the pinned
solver and all mutations. Passing ordinary ATDD alone does not imply a Verus
proof passed. This integration uses the pinned upstream release and makes no
claim about a fork-built verifier.

## V2 finite admission and eager execution

[finite_execution.rs](finite_execution.rs) imports the actual
[shared source](../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs).
The production `Program` constructor, synthesis checks and finite decision
route call these functions. The proof covers:

- Exact structural refusal and its order: shape and domain bounds, every
  instruction's type and preceding-node references, then output root kinds.
- Exact Boolean 0/1 and inclusive integer admission, the original ten scalar instruction cases, checked `i64` addition/subtraction,
  and invalid references. The development extension adds checked multiplication,
  mathematical floor/ceiling division, explicitly nonnegative fused mul-div and
  exact product comparison, with separately admitted canonical program profiles.
- Eager graph evaluation, including unused or unselected nodes, projection in
  ABI order, and output-domain refusal.
- Empty scratch and output buffers on every failed evaluation.

No executable function in this unit has a `requires` clause. Mathematical
sequence recursion and its induction lemmas have their stated ghost premises.
This does not narrow the actual inputs to make verification succeed. The
allocation operations use the bundled standard-library specifications;
allocation cost, failure and platform behavior remain part of the trusted base.
Loop measures establish the local progress obligations. They do not establish
V2 resource accounting; see the metering unit below.

```sh
python3 tools/test_check_finite_execution.py
python3 tools/check_finite_execution.py --install --out /tmp/zeno-fcis-finite-evidence.json
cargo +1.97.1 test -p zeno-fcis-synthesis --locked
cargo +1.97.1 check -p zeno-fcis-synthesis --no-default-features --locked
```

The native harness has nine tests covering instruction and machine boundaries,
refusal cleanup and 2,753,237 admission comparisons against retained baseline
`4dd979b`. The baseline oracle is a behavior-preservation control, not a proof
of application requirements. Existing independent synthesis and inventory
conformance tests continue to challenge those requirements.

The pinned proof uses `--no-cheating`, whole-unit verification and two verifier
threads. Twelve incorrect-behavior mutations must fail verification. Four
coverage controls omit or weaken the final postcondition, narrow its input
domain, or add an uncontracted function. They must be refused even when Verus
reports success. Deleting the final postcondition left the original count of
34 verified obligations intact in a controlled probe.

The gate parses the complete pinned Verus intermediate representation (VIR).
The reviewed [coverage manifest](finite-execution.json) fixes the inventory,
translated signatures/preconditions/postconditions, and mathematical
specification bodies.
Missing, extra, bodyless, changed or unknown-format entries refuse. Updating
this manifest changes the reviewed proof specification; it is not an automatic
repair step. The manifest guard is a development check, not another theorem
checker. Verus/Z3 establishes the actual contract result.

Both runtime and synthesis identities include all extracted execution,
admission and specification sources. This intentional V2 identity change does
not qualify old receipts for the new implementation. The source-bound receipt
also records the Git revision, dirty state, tool and compiler versions, native
tests and every mutation result. A dirty-tree receipt is development evidence;
it must be replayed at the clean committed head for exact-head qualification.

Canonical decoding and schema projection, diagnostic wrappers, complete
decision construction, meter/state-view enforcement, laws/genesis and authority
composition remain outside this unit. Rust compilation/erasure, Verus/Z3 and
the standard library are named assumptions; a source receipt is not a
source-to-binary proof. See the
[candidate record](../../docs/RELEASE_CANDIDATES.md) for release scope.

## Owned V2 instruction metering

[metered_execution.rs](metered_execution.rs) imports the same scalar helpers and
the actual [V2 execution unit](../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs).
Its public `execute_v2` entry point creates a private zero-usage meter from
policy limits and returns both the complete scalar result and opaque usage.
Callers cannot construct usage, access the meter, replace the report or supply
initial consumption. `Program::execute_v2` is a convenience adapter tested
against that entry; its own body remains outside the current Verus subject.
The low-level entry specifies scalar execution even for malformed graphs; it
does not replace structural/type admission or certify a synthesis contract.

The mathematical theorem covers exact success/refusal and all eight counters.
One Step is consumed for each attempted eager instruction, including unused,
unselected and trapping nodes. An exhausted charge prevents its instruction;
earlier charges remain consumed. Invalid initial scalar tuples refuse before
instruction charges, and output/refusal checks add no Steps. The other seven
resources remain zero in this unit. Private charge contracts cover every
resource, exact overflow/exhaustion diagnostics and unchanged counters on a
failed charge, with no executable preconditions.

Final counters alone cannot establish physical ordering. A controlled mutation
computes an instruction result before charging but preserves every final
result/counter, so it still verifies. The reviewed
[metered coverage manifest](metered-execution.json) additionally fixes all
translated executable bodies and refuses that reorder. This is separate
development evidence for the inspected charge-before-node structure, rather
than an additional theorem about physical execution cost. Updating this body
manifest requires source and order review; a successful proof is insufficient.

```sh
python3 tools/test_check_metered_execution.py
python3 tools/check_metered_execution.py --install --out /tmp/zeno-fcis-metered-evidence.json
cargo +1.97.1 test -p zeno-fcis-synthesis --test v2_execution --locked
```

The original meter subject had 73 translated records and 57 obligations. With
the integer readers, record projection and the since-retired multi-record
execution in its dependency closure, it had 182 translated records, comprising
81 executable functions including clones, 75 mathematical functions/constants
and 26 ghost proof helpers; 145 verification groups. These are overlapping
counts. The 14 native tests use
independent arithmetic and wire-format oracles, and sweep quotas, eager traps,
machine boundaries, malformed references, output order, refusal cleanup and
every resource. Four external-consumer compile refusals challenge custody of
the counters/report.
Nine behavior mutations must fail verification; seven coverage mutations must
verify successfully and then be refused, including the charge reorder.

The V2 cost profile is `zeno-fcis/finite-instruction-meter/2`. Its profile bytes
and all new executable/specification sources join the evaluator identity.
Finite scalar wire semantics remain unchanged. Allocation, scalar admission,
root projection, hashing and schema navigation are not charged by this
instruction profile. The separate protected-record profile below charges Byte
and Read for its bounded record language. The `composition` gate covers their
composition with scalar execution. Grouped operations, complete
decisions, laws/genesis and the mandatory V2 authority/
replay route are outside this unit. A verified private meter is not a proof that every
authority operation uses it, nor a whole-core or V2 completion claim.

## V2 integer byte interpretation

[canonical_bytes.rs](canonical_bytes.rs) directly includes the production
[byte readers](../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs).
`read_big_endian` interprets zero through 16 bytes as a base-256 unsigned integer;
`read_signed_128` interprets exactly 16 bytes as a signed two's-complement value.
Success returns the exact integer and next offset. Invalid offsets, truncation
and excessive width return `None`. Both functions cover their entire Rust input
domain, including zero-width reads and both signed extremes.

The reviewed [coverage manifest](canonical-bytes.json) fixes 11 translated
records: two executable functions, five mathematical specifications and four
induction lemmas, with nine verification obligations. Native tests compare
against independent `from_be_bytes` conversions, exhaust all 65,536 two-byte
values and cover byte order, widths, truncation, offset overflow and signed
boundaries. Nine behavior mutants must fail proof; five mutants must verify
and then fail the contract/specification coverage guard. Both actual source
files are included in the finite evaluator identity.

```sh
python3 tools/test_check_canonical_bytes.py
python3 tools/check_canonical_bytes.py --install --out /tmp/zeno-fcis-byte-evidence.json
cargo +1.97.1 test -p zeno-fcis-synthesis --test v2_execution --locked
```

These functions are integer primitives and do not alone establish canonical
record, schema or envelope admission. They do not charge a meter. The protected
record implementation below calls them after its specified charges. Original
envelope admission, complete decisions, laws/genesis and mandatory authority/
replay are outside this unit. The receipt is
scoped Verus/Z3 evidence with the named compiler, verifier, standard-library
and hardware assumptions; it does not become Lean `KernelChecked` evidence or
establish source-to-binary correspondence.

## V2 protected flat-record projection

[protected_input.rs](protected_input.rs) includes the actual private
[input view](../../crates/zeno-fcis-synthesis/src/finite/execution_v2/input_view.rs)
and its complete shared-source dependency closure. The public
`project_record_v2` wrapper constructs one zero-usage private meter and returns
an opaque result, usage and descriptor-based Read-attempt report. The private
`project_into` helper reuses an existing meter and attempt prefix, so later
composition need not reset accounting. No executable function has an input
precondition. External compile refusals challenge forged outcomes, replaced
reports, caller-supplied usage and access to the private helper.

The exact specification first refuses invalid descriptors, then charges the
whole raw slice as Byte before checking its record header. It charges one Read
before each field ID or payload and records the expected descriptor ID plus
whether that charge permitted access. Integer, Boolean, closed Enum and
payload-free Sum leaves preserve canonical widths and tags. Closed maps permit
arbitrary inclusive i64 code intervals and variant order, including negative
codes. Type/variant/field IDs preserve their full widths; generic ID zero,
empty records and records larger than 16 fields remain supported. Success
requires complete byte consumption. Every failure clears projected scalars
while retaining all eight counters and the previous attempt prefix.

These attempts describe logical permission, not physical tracing. Cached
parsing before a charge can satisfy the final-result theorem. Two controlled
reorders demonstrate that distinction and must be refused by the separately
reviewed executable-body inventory. The profile does not model allocator work,
metadata validation cost, physical CPU instructions or I/O.

```sh
python3 tools/test_check_protected_input.py
python3 tools/check_protected_input.py --install --out /tmp/zeno-fcis-protected-evidence.json
cargo +1.97.1 test -p zeno-fcis-synthesis --test v2_protected_input --locked
```

The [coverage manifest](protected-input.json) fixes the same 182-record,
145-obligation dependency closure described above, with exact contracts,
mathematical specifications and executable bodies. Twenty-three behavior
mutations must fail verification; seven coverage controls must verify and then
fail the guard. Native tests compare with independent standard-library byte
conversion and set-based descriptor validation, including malformed metadata,
truncation, byte mutations, quotas, arbitrary initial counters and attempt
prefixes, all four leaf shapes, and empty/32/100-field records. The public test
compares actual canonical `Value` encoding with this implementation.

The `zeno-fcis/finite-protected-record/2` profile and all shared source/specification
files join the finite evaluator identity. Source-bound template certificates
must be regenerated rather than relabeled. This unit does not prove catalog
descriptor extraction, original envelope/hash admission, full decisions,
laws/genesis or the mandatory
V2 authority and replay route. Proof receipts retain the named Verus/Z3, compiler,
standard-library, allocator and platform assumptions.


## Retired record-execution harness

The separate original-record execution harness and its gate were retired. Its
mutation controls now target the production composition boundary in the
`composition` gate. [record-stage-migration.json](record-stage-migration.json)
maps each retired control: 20 moved to a composition control, three were
retired with the interface or proof step they targeted, and one was removed
because the type of the surviving boundary now excludes its fault. The final
replay of the migrated controls is pending. The native tests of original-record
composition remain:

```sh
cargo +1.97.1 test -p zeno-fcis-synthesis --test v2_record_execution --locked
```

Verus/vstd/Z3, erasure/compiler correspondence, allocation and the platform
remain named trusted premises; these proofs do not become Lean KernelChecked
evidence or confer release authority.

## Checked assembly

[checked_assembly.rs](checked_assembly.rs) verifies three proof modules together
with the evaluator specification and its production source closure:

- [checked_assembly_joint.rs](checked_assembly_joint.rs): for disjoint
  components, initialization, a local preservation check, each route's command
  assumption, the selected transition and the framing of every unselected
  position give the conjunction of every component's invariant at every state
  of a finite history (`joint_induction`).
- [checked_assembly_tables.rs](checked_assembly_tables.rs): the composition
  checker's local, StateFact and interface tables imply that local check
  (`tables_imply_local_check`). From a genesis satisfying every leaf's initial
  condition, every history of an aggregate satisfying the assembly step
  equation keeps the joint invariant at every state
  (`aggregate_histories_keep_the_joint_invariant`). A register ring of any size
  of at least two instantiates every premise.
- [checked_assembly_route.rs](checked_assembly_route.rs): route specialization
  preserves evaluation. The assembly validator compares an aggregate's decision
  program with each leaf's through canonical terms: on route `k` the target
  input is the constant `k`, a leaf's inputs are renamed to the aggregate's,
  and the constructors fold constants and apply identities. A term's value is
  defined by the evaluator specification's `node`. On every input the
  specification executes without failure, each output is its canonical term's
  value, so outputs with equal terms are equal and selection chains with the
  same conditions choose the cases the validator pairs
  (`validated_routes_agree`).

The tables, the step equation and genesis are premises of these lemmas, and
the theorem's decision functions are abstract. The composition checker
establishes the tables by exhaustive enumeration with the leaves' bound
Authorities, and the assembly validator establishes the step equation
structurally. A differential test compares the validator's canonical terms,
route specialization and case pairing with the library evaluator: every
aggregate input for two leaves and, for 4 to 32 leaves, every input of each
route's leaf with the remaining slots drawn from a fixed seeded sample, on the
register ring and chain; unit tests check each constructor. The trusted base,
tested and not proved, is the same list the composition result states:

- The checker's exhaustive enumeration of each leaf's local, StateFact and
  interface tables with the leaf's bound Authority.
- The group checks of equation 2.8: their tables and the discharge and rise
  of each child's assumption.
- The assembly validator's canonical terms, route specialization and case
  pairing, compared with the library evaluator on the register ring and chain
  and constructor by constructor in unit tests.
- The assembly validator's comparison of classes, reasons, assignments and
  frames, its field, genesis and routing checks, and its law-implication
  tables.
- The interval analysis showing that no eager node can trap, and the meter
  check with its cost model of one Step per program or law node.
- The link from the bound Authorities' actual decisions (case selection,
  assignments, law and meter refusals) to the abstract decision functions of
  the Verus theorem.

The application-level safety statement is therefore machine-checked over the
evaluator specification, with the validator implementation tested, not
proved.

```sh
python3 tools/test_check_checked_assembly.py
python3 tools/check_checked_assembly.py --install --out /tmp/zeno-fcis-checked-assembly.json
```

The positive run is unfiltered and verifies the proof modules with the
evaluator closure under `--no-cheating`. The reviewed
[profile](checked-assembly.json) fixes the obligation count, the target lemmas
and the translated inventory of the proof modules: 76 specification functions,
23 proof functions and no executable function. Three planted controls must
each fail one intended obligation in a run restricted to the proof modules: a
leaf providing its own interface, an aggregate allowed to write unselected
slots, and the identity `true && x = x` applied to a non-Boolean `x`. The proof
makes no claim about liveness, fairness, traces, continuous dynamics, or
asynchronous or shared-write coupling.

## Spot refinement

[spot_refinement.rs](spot_refinement.rs) proves the compiled ZenoDEX Spot
decision program against the Spot core specification and the integer Step
contracts that `core-apps/zenodex-spot/proofs/NativeBridge.lean` states. It is
an application gate, not one of the workflow's qualification gates, and runs
through `core-apps/zenodex-spot/tools/check_refinement.py`.

- [spot_native_program.rs](spot_native_program.rs) is generated from
  `core-apps/zenodex-spot/src/v2_contract.rs`: the 1,377 instructions, roots
  and declared domains, each node's value as a function of the input, and
  `program_terms`, which proves from the evaluator specification's `prefix`
  and `projection` that a successful `execution` computes exactly those
  values. Per accepting branch it also proves the guard and successor terms
  the branch selection implies.
- [spot_refinement_proof.rs](spot_refinement_proof.rs) states the
  specification's successor, rounding included, and proves `spot_refinement`:
  on every input `execution` accepts, an accepting branch assigns exactly that
  successor, which meets the Step contract for every coalition containing the
  actor.

The closure is `evaluation/spec.rs`, `value/scalar.rs` and `value/wide.rs` by
path. The evaluator's `Domain`, `Op` and `Failure` declarations come from a
generated verbatim copy ([spot_evaluator_types.rs](spot_evaluator_types.rs))
because their module also owns executable bodies this unit does not need. The
generator's `--check` mode keeps both generated files fresh. That the
executable evaluator, decision construction and publication meet the
specification is the subject of the gates above, not of this unit.

The check runs the whole crate under `--no-cheating`, requires the reviewed
obligation count and theorem names, and runs five planted program defects
that must each fail the proof.
## Composed-domain outcome and invariant theorem

The harness `composed_domain.rs` includes the production ordered resolver,
concrete state application and outside-failure grouping scan, plus a conditional n-component finite-history
induction. Its premises include local decision-table preservation, delivered
input assumptions and read-only invariants, and coupling checks at member and
outside-failure endings. A concrete three-component pilot discharges them.
The reviewed counts, contracts, specifications and runtime body inventory are in
`composed-domain.json`. The [proof boundary](../../docs/COMPOSED_DOMAIN_PROOF.md)
separates proved code from tested parsing, planning, table and binding bridges.

```sh
python3 tools/test_check_composed_domain.py
python3 tools/check_composed_domain.py --out evidence/composed-domain.json
```

The gate runs unfiltered positive verification and restricted semantic controls
for omitted cuts, dropped read-only invariants, wrong prefixes, own-only
application, read-only writes, an extra callback after Reject and an omitted
immediate predecessor in the shared cut scan. Additional theorem controls
distinguish the cut index, pre-member boundary and
which earlier members must continue. The checkpoint bridge remains conditional
on the native/model bindings described in the proof boundary. Native
specimens and reference disagreement can be replayed separately with
`python3 tools/test_composed_domain_runtime_controls.py --out evidence/composed-native.json`.


## State lemma decision core

[lemma_kernel.rs](lemma_kernel.rs) includes the production CLI
[decision kernel](../../crates/zeno-fcis-cli/src/contract/review/lemma_kernel.rs)
by path. The abstract specification defines Reach as the least genesis-containing
set closed under effective steps, Init as genesis admission, and Cons as
preservation by every committed edge. A separate induction theorem proves
Init and Cons imply admission of every reachable state. The executable table
validator, FIFO search with parents, complete consecution count, violation
selection, trace construction and verdict refine these specifications.

`Inductive` implies Init, Cons and admission of every reachable state.
`False` has a genesis failure or a real trace to an excluded state; its final
step leaves an admitted reachable state. `NotInductive` admits every reachable
state but supplies a committed induction counterexample and its complete count.
These statements quantify over arbitrary finite, representable tables admitted
by the validator. The frozen [profile](lemma-kernel.json) binds every translated
contract, mathematical function and executable body and fixes the obligation
count. Internal helper preconditions are discharged by the unconditional
`decide` boundary after table validation.

Before translation, the shared `check_native_configuration` helper in
`tools/check_verus.py` checks configuration at every lexical scope in the kernel.
It admits direct `cfg(verus_keep_ghost)` guards and
`cfg_attr(verus_keep_ghost, verus_spec(...))` annotations. Other configuration
forms, including unknown target/feature predicates, nested conditional removal
and `cfg!` expressions, refuse with a source line. The profile's exact exception
list records the full native-only `Verdict` derive attribute, its declaration
and a reason. Missing, moved, duplicated or changed exceptions refuse. This is
a conservative source guard; macro expansion and compiler correspondence remain
trusted. The other gates can call the shared helper but do not thereby inherit
this gate's configuration qualification.

```sh
python3 tools/test_check_lemma_kernel.py
python3 tools/check_lemma_kernel.py --out evidence/lemma-kernel.json
```

The gate checks the pinned whole crate with `--no-cheating`, native allocation,
slice, vector, option and checked-product contracts, an independent FIFO oracle
on every small table, and transitive-closure verdict comparisons. Six planted
defects must compile and fail their named native assertion. Five fail a Verus
obligation: omitted initiation, approval of an excluded genesis, a noncommit
treated as a commit, premature search exit and an admitted counterexample
successor. The sixth inserts a native-only verdict override: its translated
proof still passes, but the configuration guard must reject its exact line.
Each specimen is restored byte for byte, its SHA-256 recorded, then rebuilt,
tested and verified again.

The method builds on Klein et al., *seL4: Formal Verification of an OS Kernel*
(SOSP 2009), and the [seL4 refinement proof organization](https://github.com/seL4/l4v).
This is a scoped specification/refinement application, not a whole-system
verification claim. Verus/vstd/Z3, Rust compilation and erasure, the allocator
and the host remain trusted. Native tests exercise the assumed standard-library
contracts. Shortest-trace minimality, the Machine/table adapter, `state_holds`,
the law evaluator, generic `induction<S>`, unsigned/symbolic routes and JSON
rendering are outside this theorem; adapter behavior and rendering are checked
separately. No evaluator identity or application authority is changed.

## Reachable probe and shared fixed points

The [shared specification](finite_machine_spec.rs) defines declared state and
input ordinals, genesis, raw successor and commit tables, the effective step,
`F(X) = {genesis} union post(X)`, and Reach. A noncommitting outcome keeps its
source state. The specification has no dependency on either judge.
[Shared proofs](finite_machine_proofs.rs) establish that Reach is a fixed point
and is contained in every pre-fixed point. They follow D. Park, *Fixpoint
induction and proofs of program properties*, Machine Intelligence 5 (1969),
and the specification/refinement organization described by
[seL4](https://sel4.systems/Verification/proofs.html).

The [verdict theorem](lemma_fixed_point.rs) proves the fixed-point forms:
`Inductive` iff `F(H)` is a subset of `H`; `NotInductive` iff Reach is a subset
of `H` but `F(H)` is not; `False` iff Reach is not a subset of `H`.
The lemma harness retains its 32 obligations. Its profile now covers the shared
definitions too. Its six original controls remain required.

The [probe harness](probe_kernel.rs) includes the production
[probe core](../../crates/zeno-fcis-cli/src/contract/review/probe_kernel.rs)
and the same verified FIFO search and parent traversal. For a complete valid
table within the tuple limit, it returns a matching state exactly when some
state satisfying the query is in Reach. It selects the least matching state
ordinal, preserving the CLI's choice even when another match has a shorter
trace. The witness starts at genesis, follows effective steps, and ends at the
selected state. A singleton query specializes this to exact state membership.
An answer of no means no matching state is reachable, and includes the complete
reachable-state count used by the renderer.

Invalid tables refuse separately. Exceeding the tuple limit returns an
`Unknown` variant with no answer, count or witness fields. The shared size
predicate also refuses overflowed/unknown domain sizes. The existing adapter
checks the domain limit before enumeration and reports invalid genesis as a
separate refusal. No partial search is promoted to an answer.

```sh
python3 tools/test_check_reachable_probe.py
python3 tools/check_reachable_probe.py --out evidence/reachable-probe.json
```

The [profile](reachable-probe.json) freezes 58 records and 48 whole-crate
obligations, including the lemma unit and shared fixed-point proofs. These
counts overlap the lemma gate and must not be added. Both gates use the shared
native-configuration guard and the exact derive exception. Two probe controls
ignore commit flags or stop the FIFO search early; each must compile, fail a
named native assertion, fail its intended obligation, restore SHA-identically,
and rebuild/test/verify successfully. Native tests compare 1,127,116 cases with
independent transitive closure and simulate every reported trace.

The Machine/table and predicate projections, domain enumeration and JSON/exit
rendering remain tested boundaries. Native comparison with the prior search
and CLI differentials cover their behavior on the retained example and fixture
corpus; they do not supply a universal adapter theorem. Verus/vstd/Z3, Rust
compilation and erasure, standard-library contracts, the allocator and host
remain trusted. Shortest-trace minimality, physical cost/OOM, unsigned/symbolic
reachability and whole-CLI correctness remain outside this proof. The probe
remains advisory and grants no application authority.

## Value decoder refinement

`value_decoder.rs` includes the actual value and codec libraries with `#[path]`.
The qualification profile is [value-decoder.json](value-decoder.json). The gate
uses the tool files pinned in [toolchain.json](toolchain.json), verifies the
whole crate, requires the exact obligation count and local declaration inventory, and
refuses verifier errors, incomplete coverage, timeouts and nonsemantic control
failures. The CI matrix runs the same gate.

The proof layers are:

1. `zeno-fcis-value/src/decoder_model.rs`: canonical values, `wf`, `spec_encode`,
   resource accounting and wire order.
2. `zeno-fcis-codec/src/decoder_spec.rs` and `decoder_induction.rs`: a total
   decoder and depth/count induction for consumption and resource bounds.
   `value_decoder_completeness.rs` proves the reverse direction by structural
   node-count induction, with exact consumed prefixes and resource charges.
3. Named `Cursor::invariant` and `DecodeState::invariant` predicates, preserved
   by every reader and state method.
4. Contracts on the production constructors, validators, encoding functions,
   collection loops, `decode_value_inner`, `decode_complete_value`,
   `decode_value_with_metrics` and `decode_value`.

The top contract is
`decoded_result(&result, bytes@.len()) == spec_decode(bytes@, limits.model())`.
An accepted result additionally ensures `wf(value.model(), limits.model())`,
`bytes@ == spec_encode(value.model())` and
`bytes.len() <= limits.max_input_bytes`. The entry point has no precondition.
All executable recursion has decreasing measures, and every loop has a
termination argument. No application body or termination check is exempted.
This completes Tier 1 (T1 safety/termination, T2 canonical acceptance, T3 input
limit). `value_decoder_roundtrip.rs` adds the production call-boundary theorems:

- Tier 2, `t4_encoder_append`: representation-valid storage, `wf`, and
  `old(output).len() + spec_encode(value.model()).len() <= usize::MAX` imply
  successful `encode_value_with_limits` and exact append of the specification
  bytes. `t4_encoder_agreement` starts with an empty output buffer. The length
  precondition is explicit and does not establish allocation availability.
- Tier 3, `t5_decoder_complete`: `wf(v, L)` and
  `spec_encode(v).len() <= L.max_input_bytes` imply successful production decoding
  of those bytes with resulting model `v`. It invokes `spec_t5_complete` from the
  completeness module and the existing production correspondence.
- `production_roundtrip` composes both production calls under their respective
  byte bounds. Value equality is expressed by the `ModelValue` relation.

The completeness induction first proves that parsing an encoding with any suffix
consumes exactly that encoding and adds its exact node and payload costs. Separate
tag lemmas keep solver contexts small. Nested map frames retain both validation
and canonical re-encoding checks. None of these additions changes runtime code.

`value_decoder_vectors.rs` checks literal encodings, admission rules and
resource totals from the existing codec corpus and the public canonical-byte
examples. Map charges include both encoded frames and recursive payloads.
`value_decoder_differential.rs` emits seeded byte inputs, explicit limits and
canonical output or exact error `Debug` text for behavior comparison.

The trusted base is Verus (including translation, ghost erasure and bundled
`vstd`), Z3, rustc and the execution platform. Five additional `std`/`alloc`
contracts are isolated in `value_decoder_std.rs`: `Vec::into_boxed_slice`,
`String::into_boxed_str`, slice `to_vec`, byte-slice `is_ascii`, and
`str::from_utf8`; the UTF-8 error type is opaque. Their native sample tests are
in `value_decoder_std_tests.rs`. Slice cloning specifies the `Clone` relation,
not equality for arbitrary element types. The source audit admits only the
exact reviewed standard-contract file and rejects application trust escapes.
`--no-cheating` is incompatible with these explicitly admitted standard
contracts, so the gate uses this audit and `--no-external-by-default` while
retaining lifetime, erasure, overflow, bounds and termination checking.

The six defects are checked in separate copies: acceptance of trailing input,
removal of map-key order checking, removal of the depth check, and a
little-endian blob length, plus an encoder that appends an extra byte and a decoder
that refuses a valid empty vector. Each must produce a semantic proof failure in its
named production function. Normal and restored runs must both pass, and the
original source hashes must remain unchanged. A compiler error, solver resource
limit or timeout never counts as a successful control. Three further controls
compile different code natively than Verus checks: an early return under
`cfg(not(verus_keep_ghost))`, a replacement `Cursor::take_u8` in a native-only
`impl`, and a `cfg!(verus_keep_ghost)` expression that hands the decoder other
bytes. Each such copy still verifies; the source audit, which maps every
verified function to its declaration, must then refuse it at its named function.

```sh
python3 tools/test_check_value_decoder.py
python3 tools/check_value_decoder.py --install --out evidence/value-decoder/receipt.json
```

The profile qualifies Linux x86-64. Stack bytes and allocation availability are
outside the formal result; stack limits are exercised natively. Envelope and
commit-bundle decoders, machine-code correspondence and the native behavior of
unverified configurations are outside this proof. The contract review/lemma/
probe loops used for project contracts do not apply to this Rust decoder proof.
See [canonical bytes](../../docs/CANONICAL_BYTES.md#verified-value-decoder) for
the user-facing guarantee and the seL4 and Verus references.

## Arithmetic correspondence

The following separate checks assess arithmetic in generated Rust. They do not
change the twenty workflow profiles or their qualification status above.

| Check | Proof subject and boundary |
| --- | --- |
| [check_arithmetic_proof.py](../../tools/check_arithmetic_proof.py) | Production scalar and wide bodies included by path; abstract integer rounding, quotient/remainder, monotonicity and 128-bit reference expectations |
| [check_arithmetic_typed_proof.py](../../tools/check_arithmetic_typed_proof.py) | Complete production evaluator closure; legacy-node correspondence, checked unsigned operations, casts and signed-width projection |
| [check_arithmetic_emission.py](../../tools/check_arithmetic_emission.py) | Per-program emitted-AST translation validation of six shipped Rust templates and eighteen operation probes; byte-identical verified helpers and 64-bit reference expectations |
| [check_ordering256.py](../../tools/check_ordering256.py) | Actual shared all-N limb comparator: complete lexical outcomes, decreasing loop, exact entry/loop coverage and three semantic defects |
| [check_ordering_types256.py](../../tools/check_ordering_types256.py) | Actual U256/I256 constructors, views, sign and numeric order; complete typed/body closure, raw-bit endpoints and seven semantic defects. Public constants/conversions and remaining arithmetic are separate obligations |

[arithmetic_spec.rs](arithmetic_spec.rs) states mathematical floor and ceiling
for either divisor sign, checked final representability, positive-divisor fused
operations and exact product comparison. Its rounding lemmas cover both directions
without selecting a financial policy. [arithmetic_typed_spec.rs](arithmetic_typed_spec.rs)
relates these meanings to the source evaluator, including unsigned arithmetic
and checked conversions. The production bodies and these lemmas verify together.

[arithmetic_emitted_spec.rs](arithmetic_emitted_spec.rs) gives independent eager
AST semantics and proves `emitted_program_correspondence` with the production
evaluator specification. The companion emission gate decodes the complete
generated Rust subset and emits a Verus check for every captured program and
every input vector. The pinned [inventory](arithmetic-emission-inventory.json)
covers all shipped scalar Rust programs found under examples and templates;
the eighteen additional probes cover every legacy operation and eager refusal.
The existing emitter and shipped bytes remain unchanged.
The six shipped programs are the templates' standalone `synthesized/transition.rs`
controllers. Applications from `new --contract` run the library evaluator and
are outside this emitter result. Python and JavaScript emission have native
conformance checks, but no E1 correspondence proof is claimed for those targets.

This is per-program translation validation under a **trusted grammar decoder
and Program data printer**. Their source is pinned and tested; their correctness
is not proved. It is not a universal formatter theorem or a direct Verus proof
of native emitted bodies. The gate checks every instruction, input/output guard,
root and helper byte, rejects unrecognized executable text, and requires the
complete pinned whole-crate function inventory and obligation count.
The E2 check also resolves the emitter's two `include_str!` paths and the proof
harness's two `#[path]` declarations to the same scalar and wide source files.
Copied helper text must match raw bytes, including line endings.

The typed arithmetic and emission profiles expect 1,257 and 1,288 whole-crate obligations
respectively; these overlap and must not be added. Concrete specification checks
cover all 1,165 reference vectors at each of 64 and 128 bits. Native conformance
over bounded vectors remains distinct from universal mathematical correspondence.
Controls include ceiling error, wrapped fused arithmetic, truncating cast,
lost refusal propagation, lazy untaken computation and native-only helper code.
Each semantic control compiles, fails its named assertion, and restores exact bytes.

Run each script's unit tests and then its default gate with fresh `--out` and
`--work` directories. The gates require an owned `CARGO_TARGET_DIR`, cached locked
dependencies, Rust 1.97.1, and the verifier cache selected by
`ZENO_FCIS_VERUS_CACHE`. They do not install dependencies or access the network.
Profiles and source receipts bind exact files; changing them requires review
and replay rather than automatic pin replacement.

Scope is Linux x86-64, native Rust 1.97.1 and the verifier identified by
[toolchain.json](toolchain.json). Rust code generation/erasure, standard-library
checked integer contracts, Verus/vstd/Z3, the grammar/printer bridge, orchestration
and platform remain trusted. This establishes no shell, resource-metering,
application-policy, source-to-binary or release result.

Method lineage: Klein et al., *seL4: Formal Verification of an OS Kernel*, SOSP
2009; Klein et al., *Comprehensive Formal Verification of an OS Microkernel*,
TOCS 2014; Pnueli, Siegel and Singerman, *Translation Validation*, TACAS 1998;
Necula, *Translation Validation for an Optimizing Compiler*, PLDI 2000; and the
pinned Verus implementation. These supply the layered correspondence and
translation-validation methods, not inherited proofs of this code.
