# V1 accumulator width correction (Round 3)

`Accumulator256` represents `high*2^128+low`, with private U128 limbs and
canonical high-first tuple encoding through existing value kinds. Its byte
conversion is exactly 32 big-endian bytes. L, S/G, F, snapshots and five feedback
errors use this type. Amounts, principal, stakes, custody and payouts remain U128.
This corrects the earlier V1 conformance error; it is not an economic departure.

A stored accumulator must be <=2^256-1. Checked add refuses overflow, including
MAX+1. Per-unit computation retains wide quotients, and final claim computation
uses a 384-bit product before checked U128 narrowing. In particular, an SP
first+second sum of 2^128 is valid and pays 340 when divided by Q^2. Full-width
exhaustion still refuses; the correction does not assert a bound for arbitrary
infinite histories. Inside this profile's U128 monetary domain, scratch/storage
widths cover V1's defined uint256 accumulator arithmetic. A sum needs <=257 bits;
U256 times U128 needs <=384. Feedback numerators need <=257 even for arbitrary
admitted U256 errors, and per-unit times P<=Q needs <=316 before a checked store.

The local addition law checks the two limbs independently of producer Wide::add.
The product law never calls a producer divider: for d>0 it requires q*d<=n<(q+1)*d,
and permits Arithmetic exactly for d=0 or n>=2^128*d. These conditions uniquely
determine the result. Bigint vectors also reject wrong values, all 29 business
reasons and spurious Arithmetic. This is scoped arithmetic checking, not a full
application law binding.

The earlier U128 representation used the same numeric type for amounts and
accumulators and refused valid V1 operations. Widening adds one stored type and
one bounded division operation over existing private six-limb scratch. It adds
no dependency, cache, state handle, library kind or backend. Keeping a separate
U128 quotient path preserves unchanged numeric callers and their work tariffs.
All independent validation, custody and acceptance checks remain in place.

The intended changes are the accumulator domain, wire/schema/profile identities,
record byte charges and new arithmetic charges. Monetary floors, rejection order,
source partitions, reserve discharge, D1-D4, inventory bounds and atomic refusal
remain the required behavior. Existing complete-record integer references are
regenerated with typed limbs; SP offset references now allow 256-bit S/G rather
than incorrectly marking high-width outputs as refusal. The independent models
remain Python integer relations, not native-output snapshots.

Redemption tests the original hints after detaching the edited vault; it preserves
whether an unresolved hint was supplied until that test fails, then uses V1's
sanitization and directional fallback. The self-hint tie witness yields [B,A].

The front-end rounding subtraction deliberately retains V1's Arithmetic refusal.
The 5/2/3/4 atom sequence is permanent; its full B exit refuses, its zero-principal
claim accepts, and the refused input remains exact. No clamp is introduced.
The compositional witness retains all exact monetary legs: SP withdrawal is
500Q-1000 atoms, profit 35.625Q-1000 atoms and unexplained residual zero. Some
review prose rounded those values; the independent raw driver agrees exactly.
Pending-price timing and unlocked staking behavior remain as in V1.

The public accumulator conversions are immutable preparation/serialization
utilities, not decision, usage-receipt or publication authority. Checked arithmetic
starts its own private Work. Internal Wide buffers, quotient/remainder and new
candidate records are exclusively invocation-owned, never retained or aliased.
Refusal preserves inputs and completed Step/Read/Byte prefixes. Frame admission
checks the wider tuple schema and all existing explicit table/byte limits.

Evidence is bounded: reviewer regressions, complete-record reference rows,
independent bigint boundary vectors, source-pinned ownership review and unchanged
base compatibility. Complete application dispatch/general laws, authenticated
profiles/custody, publication, the full atlas and all-history proofs remain open.
