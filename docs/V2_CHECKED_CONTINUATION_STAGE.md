# Checked ordered continuation

Status: original V2 implementation contract, 2026-10-02. The historical native
preparation constructors remain private reference oracles, not authority.

The checked replacement admits a closed scalar fold `(accumulator, item) ->
accumulator`, owns the initial values, ordered input items and exact context,
and retains an immutable declared graph. Only library code owns its cursor,
private meter and intermediate accumulator. Chunks specify offset/count; they
cannot substitute values, graph, context or a usage report.

Its mathematical state is the admitted input together with a processed prefix.
A successful chunk returns exactly the eager sequential fold on that prefix.
An invalid offset/range or failed item preserves the accumulator and cursor.
Finish returns a copy of the complete result only after all items and an exact
state-root/version/invocation context comparison. Drop cancels; recovery
re-admits and recomputes original inputs. No partial output or publication
capability is exposed. No fold result substitutes for the complete authority.

Preserve every original admission boundary and diagnostic precedence in
`verification/kernel-laws/src/oracle/finite/preparation.rs` and its original
tests. Input/output capacity includes the exact canonical scalar tuple frames.
Original Read/Write/Candidate/Byte reservations remain distinct from actual
instruction attempts. The V2 Step counter is shared across chunks, charges
before each attempted instruction, retains consumed work after a failed chunk,
and cannot be reset by retry. This additional report does not turn a complete
cost reservation into a claim about elapsed time or allocation.

Required positive: the original accumulator domain [-100,100], item domain
[-3,3], initial 0, items [1,2,3], chunks two and one, result 6. Finish between
chunks refuses as incomplete. This is separate from the prepared-counter's
count 0..3 and delta -1..1 domains. Preserve the order-dependent result 11,
full-width arithmetic and eager traps, all original partitions/cancellation,
stale context/ABA, byte-limit precedence, empty input and privacy negatives.

Qualification requires whole-source Verus correspondence for all new bodies,
checked admission/loop/finish bridges, actual strict native and no-std builds,
all preserved original comparisons and refusal cases, meaningful cursor,
ordering, partial-output and shared-meter mutations, updated complete source
closure and independent Astra review. Do not add executable proof preconditions,
an external body, narrower domain or ignored test to obtain success.

The [original blocker](V2_INTEGRATION_BLOCKERS.md) remains open until this
replacement is executed and checked through the normal supported API. Template
scratch preparation, private legacy execution and obsolete-example retirement
do not close this capability.
