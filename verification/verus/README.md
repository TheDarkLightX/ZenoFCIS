# Shared-source Verus verification

Three source units are checked with the pinned verifier. The arithmetic unit
verifies two functions with three obligations and four mutation controls. The
V2 finite admission/execution unit verifies 20 executable functions with 34
obligations and 16 mutation controls. The V2 metered unit includes those scalar
helpers plus 18 new executable functions, for 38 bodies and 57 obligations.
The overlapping units must not be added as independent function counts.
Local results, independent review and
exact-head GitHub CI remain distinct evidence.

The units verify actual finite-domain arithmetic and scalar execution used by
the optional decision-contract interpreter. They keep the Rust 1.97.1 build
and run the pinned Verus release with its separate Rust 1.98.1 toolchain.
Specifications are enabled only while Verus checks the shared Rust source.

## Arithmetic unit

- Check the same function bodies compiled into `zeno-fcis-authority`.
- Prove exact success and refusal conditions for interval cardinality and
  bounded products over all machine inputs, without application assumptions.
- Require `--no-cheating`; verify that deliberate arithmetic mutations fail.
- Pin the verifier archive and toolchain; record source hashes, command,
  verification result, trusted dependencies, and uncovered obligations.
- Compile the shared code with Rust 1.97.1, including the no-std crate path.
- Preserve the existing finite-input limit and admitted-input behavior.
- Document lessons, compatibility, follow-up work, and the fork policy.

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
proof passed. The owner fork is an extension workspace; this integration uses
the pinned upstream release and makes no claim about a fork-built verifier.

See [lessons and V2 plan](../../docs/VERUS_LESSONS_AND_V2_PLAN.md) for the
complete follow-up scope and evidence policy.

## V2 finite admission and eager execution

[finite_execution.rs](finite_execution.rs) imports the actual
[shared source](../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs).
The production `Program` constructor, synthesis checks and finite decision
route call these functions. The proof covers:

- Exact structural refusal and its order: shape and domain bounds, every
  instruction's type and preceding-node references, then output root kinds.
- Exact Boolean 0/1 and inclusive integer admission, all ten scalar instruction
  cases, checked `i64` addition/subtraction, and invalid references.
- Eager graph evaluation, including unused or unselected nodes, projection in
  ABI order, and output-domain refusal.
- Empty scratch and output buffers on every failed evaluation.

No executable function in this unit has a `requires` clause. Mathematical
sequence recursion and its induction lemmas have their stated ghost premises.
This does not narrow the actual inputs to make verification succeed. The
allocation operations use the bundled standard-library specifications;
allocation cost, failure and platform behavior remain part of the trusted base.
Loop measures establish the local progress obligations. They do not establish
the unfinished V2 resource accounting contract.

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
[V2 implementation plan](../../docs/V2_VERIFIED_CORE_PLAN.md) for release closure.

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

The unit fixes 73 translated records: 38 executable functions including clones,
31 mathematical functions/constants and four induction lemmas; it checks 57
obligations. Native tests use independent `i128` instruction arithmetic and
`u128` charge arithmetic, and sweep quotas, eager traps, machine boundaries,
malformed references, output order, refusal cleanup and every resource. Four
external-consumer compile refusals challenge custody of the counters/report.
Nine behavior mutations must fail verification; seven coverage mutations must
verify successfully and then be refused, including the charge reorder.

The V2 cost profile is `zeno-fcis/finite-instruction-meter/2`. Its profile bytes
and all new executable/specification sources join the evaluator identity.
Finite scalar wire semantics remain unchanged. Allocation, scalar admission,
root projection, decoding, hashing and schema navigation are not charged by
this instruction profile. A protected raw-state view, grouped operations,
complete decisions, laws/genesis and the mandatory V2 authority/replay route
remain open. A verified private meter is not a proof that every authority
operation uses it, nor a whole-core or V2 completion claim.

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

These functions are integer primitives, and do not establish canonical record,
schema or envelope admission. They do not charge a meter. The protected view
must use them after its specified ingress/access charge and preserve the
original admitted bytes; that integration remains open. Complete decisions,
laws/genesis and mandatory authority/replay also remain open. The receipt is
scoped Verus/Z3 evidence with the named compiler, verifier, standard-library
and hardware assumptions; it does not become Lean `KernelChecked` evidence or
establish source-to-binary correspondence.
