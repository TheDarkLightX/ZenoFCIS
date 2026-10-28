# Full-width numeric foundations for 2.7

This development checkpoint adds explicit, default-off `Value2`, codec2 and
Schema2 routes. It does not complete the full-width program, proof, Authority or
application pipeline. The full requirement remains [F27-19](FACTORY_GAPS_27.md).

`zeno-fcis-value/value-profile-2` provides owned nominal `U256` and `I256`
leaves within the recursive value grammar. `U256` covers zero through
`2^256 - 1`; `I256` covers `-2^255` through `2^255 - 1`. A narrow integer or
signed zero is not an unsigned 256-bit value merely because its payload looks
similar. Conversions to the legacy profile refuse wide leaves.

`zeno-fcis-codec/value-profile-2` selects a disjoint codec2 envelope. New wide
leaves use fixed 32-byte big-endian payloads; signed payloads use two's
complement. Legacy-only bare values retain their existing encoding. The legacy
envelope decoder does not select codec2 implicitly.

`zeno-fcis-schema/schema-profile-2` provides a closed, acyclic Schema2 grammar
with full-width numeric intervals, products, sums, vectors and maps. Its
canonical schema bytes include names, revisions, root, every definition and
the exact ranges. Application revision and schema/value format are separate.
Schema construction reapplies the supplied caps to previously built types.

The owned schema witness checks the selected type, schema identity, value
shape, numeric ranges and structural resources. Its expected-envelope route
performs complete codec2 decoding first, then type, schema hash and admission.
A matching hash alone cannot certify the supplied value. Schema admission is
an inert data boundary; it grants no publication or effect authority.

## Evidence and its exact scope

The original foundation arithmetic source had SHA-256
`db196c5d746d27bf21579e9a3a3ce32d556ec74ea62ba929e4c8a462a6130131`.
The current source explicitly factors its comparison into
[`ordering256.rs`](../crates/zeno-fcis-value/src/profile2/ordering256.rs), shared
by production callers and the proof harness. The remaining original arithmetic
bytes are unchanged. The [retained patch](../experiments/256_foundations_27/ordering256-runtime-factor.patch)
and [comparison qualification record](../experiments/256_foundations_27/ordering256-qualification.json)
bind the original and current identities. Formatter exclusion preserves the
remaining capsule formatting; it does not imply current byte identity with the
original source. Related proofs and vectors do not establish refinement of
every Rust arithmetic method or its callers.

### Shared comparison proof

For arbitrary arrays `a, b: &[u128; N]`, the actual comparator returns `Equal`
exactly when the arrays are equal. Otherwise `Less` or `Greater` agrees with
the highest differing limb. The proof covers every `N`, including zero, without
an input precondition. The loop has a decreasing index and inspects at most `N`
limbs. This establishes a linear scan; no benchmark speedup is claimed.

The complete isolated comparison harness passed two verification conditions:
the actual function entry and its loop. The gate checks both source locations,
the exact local function inventory, pinned source and tool identities, and
native configuration admission. All three deliberate defects were rejected:
returning equality at a mismatch, reversing the pivot comparison, and skipping
the lowest limb. Thirteen focused gate tests check admission and refusal
behavior; they are separate from those mathematical verification conditions.

Replay with the pinned Verus toolchain available, under the same process and
resource containment as the other proof gates:

```sh
python3 tools/check_ordering256.py --out /tmp/ordering256-proof
```

The output directory must be fresh. `--cache DIR` selects an installed cache;
`--install` explicitly permits installation of the hash-pinned verifier files.
The command writes a success receipt only after the whole comparison proof,
both body checks, all three semantic refusals and source/tool postchecks.
Its configuration guard is a reviewed source-dialect check, not a compiler or
ghost-erasure theorem. The Verus/vstd/SMT and compiler assumptions remain.

Fresh Rust 1.97.1 checks at frozen source manifest
`66955d656527d0a3a99f3b75fca0bc407780f99b32c775594d054093097459bd`
passed 39 value and 39 codec unit executions, 40 enabled Schema2 unit executions,
10 public consumer executions, and the allocation-only/no-std schema build.
The qualification record preserves the exact compiled inputs and original
results. Later proof-tool and documentation additions reuse that native
evidence after checking these inputs remain unchanged.

This comparison result is C1. Lifting it to the numeric order of the actual
unsigned and signed wrappers (C2/C3), and proving the other arithmetic methods,
remains open. It does not complete 256-bit arithmetic or Schema2 refinement.

### Earlier schema checkpoint

The frozen assembled source manifest
`08a8fc6aa9e1ad32bfe8127acd6fcc3e2e21ef30fef6d1fe0e3f06be0cb9fae8`
passed the following focused native checks with pinned Rust 1.97.1:

- 14 legacy schema unit executions;
- 40 enabled-profile unit executions, including those same 14 legacy methods;
- 10 public consumer methods exercising full-width balances, signed positions,
  exact bytes, identities, nominal types, refusal order and limits;
- two documentation compile-fail checks;
- an explicit allocation-only/no-std schema check on the native target.

The five-command native window used 139.338843 CPU seconds and 233.329638 elapsed seconds,
including its source/tool/cache and process checks. All 4,925 source files were
checked after execution. The 17 recorded process identities and five owned
sessions had exited, with no cleanup signals. These are qualification costs,
not an algorithm-speedup or platform-performance comparison. Separate prior
identity/cache preparation used 33.017893 CPU and 62.718357 elapsed seconds;
assembly, formatting and source audits are additional work.

The original result record remains attached to that frozen source. Later
formatting, import/module ordering and the formatter-skip annotation are a
separate source review; the original tests are not described as fresh on a
different source. See the [qualification record](../experiments/256_foundations_27/qualification.json).

## Required proofs and remaining factory links

Property tests and native integration checks supplement the formal argument.
They do not discharge these missing implementation-refinement obligations:

1. **C0:** actual Schema2 construction implies normalized, closed, acyclic
   definitions under the selected caps.
2. **S1:** successful selected-type admission implies every recursive nominal
   kind and full-width range, and the witness retains the exact value/report.
3. **S2:** expected codec2 admission preserves complete canonical consumption,
   selected type/schema identity, structural admission and refusal precedence.
4. **S3:** the explicit format2 route remains disjoint and legacy/default
   behavior remains unchanged, including its resource/profile boundaries.

The existing finite schema-binding proof concerns the earlier canonical
execution-description profile. It does not prove this owned Schema2 API.
New comparator, encoder, resource, constructor and witness dependencies must
be checked explicitly. Physical stack/cost behavior at large supplied limits
also needs qualification; the current tests do not establish every such bound.

Next come exact-width metered programs, their laws and symbolic models,
complete schema/program/source binding, Authority/publication, synthesis and
code generation, and complete DeFi applications with their effect/recovery
contracts. The current full-width state fixture is a data consumer, not an
executable DeFi application. No new controlled-English grammar, verified
machine-code arithmetic backend or release readiness is claimed here.
