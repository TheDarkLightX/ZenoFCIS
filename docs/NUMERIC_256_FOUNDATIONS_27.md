# Full-width numeric foundations for 2.7

ZenoFCIS 2.7.0 adds explicit, default-off `Value2`, codec2 and
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
The comparison checkpoint factored its comparator into
[`ordering256.rs`](../crates/zeno-fcis-value/src/profile2/ordering256.rs), shared
by production callers and the proof harness. The subsequent type/order checkpoint
factors the actual `U256`/`I256` declarations and seven methods into
[`number256.rs`](../crates/zeno-fcis-value/src/profile2/number256.rs), and shares
the existing finite-wide mathematical definitions with its harness. Public
reexport paths remain available; the defining Rust module path changes.

The historical [comparison patch](../experiments/256_foundations_27/ordering256-runtime-factor.patch)
and [qualification](../experiments/256_foundations_27/ordering256-qualification.json)
retain their original subject. The later [type/order patch](../experiments/256_foundations_27/ordering-types256-runtime-factor.patch)
and [qualification](../experiments/256_foundations_27/ordering-types256-qualification.json)
bind the factor from that checkpoint. All seven ordinary method bodies retain
their original tokens after removing only the new ghost proof blocks. This
source review does not prove compiler or ghost-erasure correspondence. The
remaining arithmetic algorithms are unchanged by this factor; their complete
implementation refinement still needs proof.

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

This comparison result is C1. The following checkpoint establishes the numeric
order lifting for the actual types; other arithmetic and Schema2 refinement
remain separate obligations.

### Actual type and numeric-order proof

The actual unsigned constructors and views preserve both 128-bit limbs. Their
mathematical value is `low + 2^128 * high`. The signed interpretation subtracts
`2^256` when that unsigned value is at least `2^255`. The actual sign test and
both comparisons agree with those interpretations for every representable
input, with no added input precondition.

The complete isolated harness passed 21 verification conditions. Its closure
includes the seven actual methods, two derived clone bodies, shared comparator,
and eight checked helper bodies. The gate checks all 36 local function bodies
and contracts, nine typed method/derive bindings, and all 21 solver sections.
At that checkpoint, raw-bit endpoint lemmas established inhabited unsigned and
signed extrema. They did not certify the separate public associated constants
or narrow constructor conversions. The constants refinement below covers the
public constants separately.

Each of seven deliberate actual-method defects produced an intended semantic
failure in a whole-harness run: swapped constructor limbs, a dropped high limb,
reversed unsigned order, zeroed signed constructor/view bits, the wrong sign bit,
and the wrong opposite-sign comparison. The unchanged generic comparator also
passed fresh checking and its three separate defects. Thirty focused gate tests
cover source, configuration, typed coverage and output admission; those tests
are separate from the mathematical verification conditions.

```sh
python3 tools/check_ordering_types256.py --out /tmp/ordering-types256-proof
```

Use a fresh output directory and the same pinned-tool/process containment as
the comparison gate. A success receipt requires the complete proof, exact
coverage, all seven semantic refusals and source/tool postchecks. This remains
a reviewed source-dialect route with the stated Verus/vstd/SMT/compiler
assumptions.

Fresh native checks at frozen source manifest
`4d8bba31eb96c3720affeffde7af6d9ebca483edc187d844bbad1cbf3ae36699`
passed 39 value, 39 codec, 40 Schema2 and 10 public-consumer test executions,
plus the allocation-only/no-std schema build. The window used 79.019007 CPU
seconds and 108.400953 elapsed seconds, including its source/tool/cache checks.
All 4,943 source files and 62 tool/control pins stayed unchanged; all 19 recorded
native process identities exited without cleanup signals. These are
qualification costs, not benchmark speedups. Later documentation and provenance
additions preserve all 26 proof and 61 package/configuration input bytes and
reuse those exact results without relabeling them as fresh runs on a later
commit.

### Actual associated constants

The abstract specification uses the same integer interpretations as the
type/order proof: two unsigned limbs mean `low + 2^128 * high`, and signed
values subtract `2^256` at the `2^255` boundary. The wider unsigned values use
the corresponding four- or six-limb radix expansion. Constants have no inputs
or state changes and produce owned fixed-width values.

| Actual associated constant | Exact integer value |
| --- | --- |
| `U256::ZERO` | `0` |
| `U256::ONE` | `1` |
| `U256::MAX` | `2^256 - 1` |
| `I256::ZERO` | `0` |
| `I256::MIN` | `-2^255` |
| `I256::MAX` | `2^255 - 1` |
| `U512::ZERO` | `0` |
| `U768::ZERO` | `0` |

These equations are proved for the actual bodies under the stated assumptions.
The [numeric harness](../verification/verus/numeric256_harness.rs) imports the
production arithmetic module directly. Each actual executable constant has its
own integer postcondition. Checked representation and raw-bit lemmas support
those refinements; the raw-bit lemmas alone do not certify a public constant.
The executable initializer expressions are preserved, with erased proof blocks
and separate implementation blocks for the selected proof scope.

The [reviewed profile](../verification/verus/numeric256_profile.json) lists all
eight constant obligations, complete translated bodies/contracts, solver sections,
source bindings and semantic defects. At the constants checkpoint its complete positive closure contained
53 verification conditions, including imported type/order and legacy wide
dependencies. Its four semantic controls change unsigned `MAX` by one, move
the signed `MIN` sign bit, and give each wider `ZERO` a nonzero high limb.
Every control must reach its intended semantic failure. A parse, build,
timeout or incomplete result cannot catch a control.

```sh
python3 tools/check_numeric256.py --out /tmp/numeric256-proof
```

Use a fresh output directory and the same pinned verifier/process containment
as the ordering gates. The profile producer is a development tool: its positive
run does not qualify a profile without review and the complete controls gate.
The updated type/order closure also checks a representation lemma while
preserving all earlier type/order contracts and bodies.

Assumptions remain explicit: soundness of the pinned Verus frontend, its vstd
specifications and SMT solver; Rust integer and array semantics represented by
that environment; and correspondence of native compilation with the proof
frontend, including erasure of ghost code and constant annotations. The
configuration guard and source review do not prove compiler/erasure
correspondence. Unproved arithmetic blocks are explicitly excluded from this
harness and supply no callable specifications to the checked closure. Native
integration tests remain separate evidence.

### Actual conversions

The conversion contracts use the integer meanings above for every legal input,
without an added precondition. Write `U`, `S` and `W` for the unsigned, signed
and wider unsigned integer meanings. Every refusal in this table is exactly
`Failure::Overflow`; an in-range input must succeed with the exact value.

| Actual method | Exact result and acceptance condition |
| --- | --- |
| `U256::from_u128(x)` | `U(result) = x` |
| `U256::try_u128(x)` | Returns `U(x)` exactly when `U(x) < 2^128` |
| `I256::from_i128(x)` | `S(result) = x`, including both signed 128-bit endpoints |
| `I256::magnitude(x)` | `U(result) = abs(S(x))`, including `abs(-2^255)` |
| `I256::from_magnitude(x, negative)` | Returns `-U(x)` or `U(x)` exactly when that signed value fits `[-2^255, 2^255)`; either sign accepts zero |
| `I256::try_unsigned(x)` | Returns the exact `S(x)` exactly when `S(x) >= 0` |
| `I256::difference_unsigned(a, b)` | Returns `U(a) - U(b)` exactly when the difference fits the signed range; neither operand is narrowed beforehand |
| `U512::from_u256(x)` | `W(result) = U(x)` |
| `U512::narrow(x)` | Returns the exact `W(x)` exactly when `W(x) < 2^256` |
| `U768::from_u256(x)` | `W(result) = U(x)` |
| `U768::narrow(x)` | Returns the exact `W(x)` exactly when `W(x) < 2^256` |

These statements are proved for the actual bodies under the stated assumptions.
The same source-bound numeric harness checks the complete verification closure,
including the earlier constants and imported dependencies. The profile checks every translated
local body and contract, the exact input and result types, and every solver
section. Supporting refinements establish the actual two-complement helper,
two-limb carry/borrow loops and `U256::checked_sub` used before signed narrowing.
The loop result claims here concern two limbs; they do not certify every wider
helper instantiation.

Eight additional semantic controls remove sign extension, retain negative bits
as a magnitude, admit a nonzero high word, reject the signed minimum magnitude,
return the wrong failure, reverse a difference's sign, or ignore either wider
type's top word. Together with the four constant controls, all twelve must
reach their intended semantic failures in complete harness runs. A missing
input, parse failure, verifier failure or timeout is inconclusive.

The executable source uses wrapping results with explicit carry/borrow tests,
primitive Boolean casts, enum patterns and four ordered zero comparisons for
the 768-bit narrowing guard. These preserve the preceding operations' values,
flags and refusals while avoiding unspecified library calls in the proof
frontend. They do not preserve executable source bytes. Native comparisons
against the preceding overflowing loops cover boundary values, carry/borrow
chains and deterministic generated inputs across all used widths. A separate
comparison covers all sixteen zero/nonzero patterns of the four high words.
These tests supplement the actual-body proofs; they are not exhaustive native
word enumeration or a compiler-equivalence proof.

Inputs and results remain owned fixed-width values. The helpers mutate only
their exclusive local arrays and flags; refusals retain no scratch or aliases.
No decision, metering, publication or effect authority is added. The existing
Verus/vstd/SMT, integer/array semantics and compiler/ghost-erasure assumptions
remain explicit. No custom trusted primitive specification is added, and no
physical runtime-cost bound is established by these refinements.

### Checked addition, subtraction and negation

The same integer meanings give exact contracts for all five existing checked
256-bit addition, subtraction and negation methods. Every fitting result must
succeed with its exact value; every other result must be exactly
`Failure::Overflow`. The contracts impose no added input precondition.

| Actual method | Mathematical result and accepted range |
| --- | --- |
| `U256::checked_add(a, b)` | `U(a) + U(b)` in `[0, 2^256)` |
| `U256::checked_sub(a, b)` | `U(a) - U(b)` in `[0, 2^256)` |
| `I256::checked_add(a, b)` | `S(a) + S(b)` in `[-2^255, 2^255)` |
| `I256::checked_sub(a, b)` | `S(a) - S(b)` in `[-2^255, 2^255)` |
| `I256::checked_neg(a)` | `-S(a)`; the sole refusal is `S(a) = -2^255` |

These statements are proved for the actual bodies under the stated assumptions.
Unsigned subtraction retains its already checked conversion-dependency
contract. The actual carry/borrow helpers and sign tests establish the exact
overflow partitions; no separate executable arithmetic model replaces them.
A checked endpoint lemma establishes the unique minimum bit pattern needed
by the actual negation guard.

That guard compares the same two representation words reached by the preceding
derived wrapper equality. The comparison keeps the minimum-value refusal and
introduces no state or interface. A native regression compares the preceding
guard on 64 boundary word pairs, checks exact zero, minimum and maximum
outcomes, and checks admitted double negation. This supports the source review;
it does not prove compiler or ghost-erasure correspondence.

Five additional semantic controls miss unsigned carry/borrow boundary cases,
use the wrong signed addition/subtraction sign partition, or refuse zero
instead of the minimum. The complete numeric gate retains all seventeen
constant, conversion and checked-arithmetic controls. Each must reach its
intended semantic failure in a whole-harness run, with exact typed/body and
solver-section coverage and source/tool postchecks.

`U256` has no public `checked_neg` method; none is added. At this checkpoint the
multiply/divide, root and rounding families, `I256::try_i128`, wider arithmetic
and wider raw constructors/views remained outside this proof scope. So did
carry/borrow result claims for helper widths other than two limbs, compiler/ghost
erasure, physical resource costs, and the Schema2 and factory obligations below.
The next sections narrow that list.

### Exact unsigned division

Write `N` and `D` for the unsigned meanings of the numerator and denominator.
Both contracts hold for every input, with no added precondition.

| Actual method | Exact result and refusal |
| --- | --- |
| `U256::div_rem(n, d)` | For `D > 0`, returns `(Q, R)` with `N = Q*D + R`, `Q >= 0` and `0 <= R < D`, the unique canonical pair; for `D = 0`, exactly `ZeroDivisor` |
| `U256::checked_div(n, d, mode)` | `ZeroDivisor` exactly when `D = 0`; otherwise `Inexact` exactly when `mode` is `Exact` and `N mod D > 0`; otherwise `floor(N/D)`, or `ceil(N/D)` for `Ceil`, with the exact `Overflow` partition of that value |

These statements are proved for the actual bodies under the stated assumptions.
The refusal order matches the code: the zero divisor is refused before rounding
is considered. For `checked_div`, `ceil(N/D) <= N`, so its `Overflow` branch is
never reached; the contract states the partition rather than relying on that.

The proof reaches the private restoring divider used by every width. Its
contract holds at every limb count `N`: the result is the canonical witness of
the limb meanings, `ZeroDivisor` exactly for a zero divisor, or `Domain` only
when `128*N` exceeds `usize::MAX`. Both internal `Invariant` refusals, the
borrow/carry mismatch and the final remainder check, are proved unreachable.
The loop invariant states that after the bits above position `k` are consumed,
the remainder and quotient are exactly `floor(N / 2^k) mod D` and
`floor(floor(N / 2^k) / D) * 2^k`. Each step uses the extracted limb bit, the
doubled remainder with its shifted-out carry, and the subtraction's borrow.
The two-limb carry and borrow helpers now also have contracts at every width.
The 512-bit and 768-bit division wrappers call the same proved divider, but
those wrappers remain outside the proved closure.

To state those invariants, the helpers use indexed `while` loops instead of
the earlier iterator loops; the pinned verifier has no contract connecting
`for limb in &mut array` with the array. Two expressions changed form:
`compare(..) != Ordering::Less` became `!matches!(compare(..), Ordering::Less)`,
and `r != Self::ZERO` in `checked_div` became `r.0 != Self::ZERO.0`. The pinned
library specifies neither `Ordering` inequality nor the derived wrapper
equality; the derived equality compares that same single field. A native test
compares the indexed helpers with verbatim copies of the iterator helpers on
boundary words, single-limb and top-limb shapes and deterministic generated
inputs: division at widths 0, 1, 2, 3, 4 and 6, multiplication at seven shapes
including outputs too small for the product, and word insertion at every
position, including the scratch state a refusal leaves. These tests support
the source review; they are not exhaustive and do not prove compiler or
ghost-erasure correspondence.

The numeric gate now also accounts for the termination conditions of the three
recursive limb-meaning definitions, and it admits semantic controls inside a
named private helper. Such a control must fail inside that helper's own proved
body. Six new controls are refused semantically: three in the divider (dropping
the incoming bit, setting the wrong quotient bit, ignoring the shifted-out bit),
`div_rem` returning its results swapped, and `checked_div` inverting the
remainder test or always rounding up. With the seventeen earlier controls, all
twenty-three must reach their intended semantic failure in whole-harness runs.
The complete closure has 153 verification conditions. Comparing the translated
records before and after this step, all retained functions except the two carry
and borrow helpers are identical apart from compiler-assigned identifiers.

Restoring binary division is a standard multiple-precision technique; see
Knuth, *The Art of Computer Programming*, Vol. 2, section 4.3.1, for the
classical algorithms. The final step uses the pinned library's Euclidean
division lemma. The contribution here is the proof of these actual bodies, not
a new division method.

### Exact unsigned products

Write `U` for the unsigned 256-bit meaning and `W` for the 512-bit meaning.

| Actual method | Exact result and refusal |
| --- | --- |
| `U256::full_product(a, b)` | Always succeeds, with `W(result) = U(a) * U(b)` |
| `U256::checked_mul(a, b)` | Returns `U(a) * U(b)` exactly when it is below `2^256`; otherwise exactly `Overflow` |

These statements are proved for the actual bodies under the stated assumptions,
with no added public precondition. The proof reaches the private schoolbook
multiplier and its word insertion helper. The multiplier is proved at every
shape whose output has at least as many limbs as both inputs together; both
callers, 2x2 into 4 limbs and 4x2 into 6 limbs, meet that private condition.
There it returns the exact product, and its `Invariant` refusals are
unreachable. Word insertion is proved at every width: it adds the word at its
limb weight exactly, and refuses with `Invariant` exactly when the sum would
not fit. Each partial product comes from the already proved two-word kernel.
The 768-bit `U512::multiply_u256` wrapper calls the same proved multiplier but
remains outside the proved closure.

Five further controls are refused semantically: word insertion dropping a
carry, the multiplier adding the high product word at the low position or at
the wrong limb position, and the two public methods multiplying the wrong
operands. All twenty-eight controls must reach their intended semantic failure
in whole-harness runs, and the complete closure has 174 verification
conditions. Apart from compiler-assigned identifiers, every retained translated
function is unchanged except the representation lemma, which gained the
512-bit and 768-bit meanings.

### Exact 512-bit and 768-bit wrappers

Write `W` and `W768` for the 512-bit and 768-bit unsigned meanings.

| Actual method | Exact result and refusal |
| --- | --- |
| `U512::checked_add(a, b)` | Returns `W(a) + W(b)` exactly when it is below `2^512`; otherwise exactly `Overflow` |
| `U512::multiply_u256(a, b)` | Always succeeds, with `W768(result) = W(a) * U(b)` |
| `U512::div_rem(n, d)` | For `W(d) > 0`, the canonical 512-bit witness; for `W(d) = 0`, exactly `ZeroDivisor` |
| `U768::div_rem(n, d)` | For `W768(d) > 0`, the canonical 768-bit witness; for `W768(d) = 0`, exactly `ZeroDivisor` |

These statements are proved for the actual bodies under the stated assumptions,
with no added public precondition. Each wrapper calls the carry helper, the
multiplier or the divider already proved at every width, so the new proof
obligations are the limb-meaning bridges for four and six limbs. Four controls
are refused semantically: a sum that misses the top-limb carry, a product that
swaps the factor's words, and quotient/remainder results or operands swapped.
All thirty-two controls must reach their intended semantic failure, and the
complete closure has 179 verification conditions. Removing the now empty
excluded 768-bit block renumbers the following signed block in the translated
records; apart from that reviewed renumbering and compiler-assigned
identifiers, every retained translated function is unchanged.

### Complete-product division, rounding and comparison

Write `P` for the complete integer numerator and `D = U(den)`. Each refusal
below is checked in the listed order, and each contract holds for every input
with no added precondition.

| Actual method | Exact result and refusal order |
| --- | --- |
| `U256::mul_add_div_rem(a, b, offset, den)` | `P = U(a)*U(b) + U(offset)`. `ZeroDivisor` exactly when `D = 0`; then `Overflow` exactly when `floor(P/D) >= 2^256`; otherwise the canonical witness `P = Q*D + R`, `0 <= R < D` |
| `U256::mul_div(a, b, den, mode)` | `P = U(a)*U(b)`. `ZeroDivisor`; then `Overflow` when `floor(P/D) >= 2^256`; then `Inexact` for `Exact` with `P mod D > 0`; otherwise `floor(P/D)`, or `ceil(P/D)` for `Ceil` with the exact `Overflow` partition of that value |
| `U256::mul_div_half_up(a, b, den)` | `P = U(a)*U(b)`. `ZeroDivisor`; otherwise `floor((P + floor(D/2)) / D)` exactly when it is below `2^256`, else `Overflow` |
| `U256::mul3_div(a, b, c, den, mode)` | As `mul_div`, with the complete product `P = U(a)*U(b)*U(c)` of up to 768 bits |
| `U256::products_ge(a, b)` | Always succeeds with `U(a0)*U(a1)*U(a2) >= U(b0)*U(b1)*U(b2)` |

These statements are proved for the actual bodies under the stated assumptions.
The numerator is never narrowed early: the product plus offset always fits 512
bits, so the internal sum never overflows. Quotient narrowing precedes rounding,
so with `a = b = 2^256 - 1` and `D = 2^256 - 2` the quotient `2^256` is refused as
`Overflow` before any `Inexact` decision. A `Ceil` result can still overflow when
the floor quotient is `2^256 - 1`. The half-up offset is the two-limb right shift
of the denominator, proved to equal `floor(D/2)`.

These bodies changed form in four places, with unchanged values: the zero tests
`den == Self::ZERO` and `r != Self::ZERO` / `r != U768::ZERO` compare the same
single field, the product comparison uses `!matches!(.., Ordering::Less)`, and
`map(|(q, _)| q)` became `let (q, _) = ..?; Ok(q)`. A native test compares all
five methods with verbatim copies of the previous bodies on boundary and
generated inputs in every rounding mode, and checks the 768-bit zero test on
all 64 zero/nonzero limb patterns. These tests support the source review; they
are not exhaustive and do not prove compiler or ghost-erasure correspondence.

Adding these proofs made three earlier proofs, word insertion and both
narrowing methods, unstable under the pinned solver. They now use explicit
bound lemmas; their contracts are unchanged apart from compiler-assigned
identifiers. Five further controls are refused semantically: a zero test that
ignores the low limb, a `mul_div` that adds one to the product, a half-up offset
that drops the carried bit, an inverted 768-bit remainder test and a strict
product comparison. Two of these fail at the proof step that states the changed
fact, the others at the method contract. All thirty-seven controls must reach
their intended semantic failure, and the complete closure has 196 verification
conditions.

### Signed multiplication, division and narrowing

Write `S` for the signed meaning. Signed division rounds toward the signed
infinities: `Floor` is `floor(N/D)` and `Ceil` is `ceil(N/D)` for the exact
rational `N/D`, whatever the signs. The contract defines `floor(N/D)` with
integer division by the positive divisor `|D|` after moving the sign into the
numerator, so negative quotients round down rather than toward zero.

| Actual method | Exact result and refusal order |
| --- | --- |
| `I256::checked_mul(a, b)` | `S(a)*S(b)` exactly when it is in `[-2^255, 2^255)`; otherwise exactly `Overflow` |
| `I256::checked_div(n, d, mode)` | `ZeroDivisor` exactly when `S(d) = 0`; then `Inexact` for `Exact` when `S(d)` does not divide `S(n)`; otherwise the signed floor or ceiling, with the exact signed `Overflow` partition (for example `-2^255 / -1`) |
| `I256::mul_div_positive(a, b, den, mode)` | `Domain` exactly when `S(den) <= 0`; then `Overflow` when `floor(|S(a)*S(b)| / S(den)) >= 2^256`; then `Inexact`; otherwise the signed floor or ceiling of `S(a)*S(b)/S(den)` with the exact signed `Overflow` partition |
| `I256::nonnegative_mul_div(a, b, den, mode)` | `Domain` when `S(a) < 0` or `S(b) < 0`; otherwise exactly `mul_div_positive` |
| `I256::mul_unsigned_signed_div(a, b, c, den, mode)` | With `P = S(a)*U(b)*S(c)` and `D = U(den)`: `ZeroDivisor` when `D = 0`; then `Overflow` when `floor(|P|/D) >= 2^256`; then `Inexact`; otherwise the signed floor or ceiling of `P/D` with the exact signed `Overflow` partition |
| `I256::nonnegative_products_ge(a, b)` | `Domain` when any of the six factors is negative; otherwise whether the first signed triple product is at least the second |
| `I256::try_i128(x)` | `S(x)` exactly when it is in `[-2^127, 2^127)`; otherwise exactly `Overflow` |

These statements are proved for the actual bodies under the stated assumptions,
with no added public precondition. Each method computes magnitudes with the
proved unsigned methods and applies the sign with the proved `from_magnitude`.
A checked lemma shows that the private magnitude rounding selection realizes
signed rounding in every sign case, including a zero numerator with either
sign, and that a remainder test on magnitudes is exactly signed divisibility.
The magnitude quotient check precedes the inexact check, so an oversized exact
quotient with a remainder is refused as `Overflow`, as the code does.

Two bodies changed form with unchanged values: the zero test
`den == Self::ZERO` in `mul_div_positive` compares the same two words, and
`nonnegative_products_ge` checks the six factors in the same order with an
indexed loop and builds the bit arrays directly instead of through
`iter().chain(..)` and `map(..)`. A native test compares both with verbatim
copies of the previous bodies on boundary and generated inputs in every
rounding mode, and checks each position of a single negative factor.

The closure growth again unsettled two earlier proofs, the lexicographic order
lemma and the two-limb subtraction clause, so they now state their steps
explicitly; their contracts are unchanged apart from compiler-assigned
identifiers. Seven controls are refused semantically: a product with the wrong
sign, reversed magnitude rounding, a positive-denominator check that admits
zero, a nonnegativity check that skips the second factor, a mixed ratio that
uses raw bits instead of a magnitude, a factor scan that stops one short and
a narrowing test that ignores sign extension. All forty-four controls must
reach their intended semantic failure, and the complete closure has 229
verification conditions. As a development check, the positive closure also
verified with twelve different solver seeds.

The square-root methods (`U512::sqrt`, `U512::sqrt_ratio` and
`U256::geometric_mean`) and the raw 512-bit word views remain outside this
proof scope. So do compiler/ghost erasure, physical resource costs, and the
Schema2 and factory obligations below.

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
