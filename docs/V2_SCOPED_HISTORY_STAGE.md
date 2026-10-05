# Exact history binding and bounded publication

**Historical stage contract.** The smaller-V2 S1 implementation removes the
History wrappers and pair APIs described below. The replacement uses one
schema-9 SQLite shell and full Authority publications with compact storage;
see the [current migration guide](V2_PROGRAM_API_MIGRATION.md#s1-storage-migration)
and [V2 plan](V2_VERIFIED_CORE_PLAN.md). The original contract is retained here
as historical evidence, including its earlier source-binding assumptions.
It does not describe the current API or qualify the replacement.

Status: implementation contract, 2026-10-02. This is original V2 work.
The prepared-counter publication limit remains 16,384 bytes under its original
command, successor, authorization and bundle accounting. The original
successful path is not qualified until its positive and negative tests pass.

## Mathematical boundary

Let B(A) be the complete canonical policy, evaluator sources and build pins
already computed by the checked authority A. Existing standalone subjects
contain B(A) and retain their exact equality and replay contract.

A fixed-size standalone identifier cannot injectively represent arbitrary
unbounded byte strings. A hash would introduce an explicit collision assumption;
this stage instead factors a history into an admitted binding H and record R:

    admit(A, H) succeeds exactly when H = B(A)
    checked_history(A, H).replay(x, R) succeeds only after admit(A, H)
        and recomputation of every record byte from x through A

The semantic persisted artifact is the pair (H, R), not R alone. Its binding
is stored once in the history namespace. Reopening must admit its exact bytes
before any history record can be replayed. Replacing H changes the artifact;
the old pair must refuse under a different policy or evaluator. This does not
prove database authenticity, atomicity or external delivery.

Every checked-history use rechecks the exact immutable binding before core
execution. A private guard and Rust borrows retain A and H. No caller-supplied
identity, authorization verdict, candidate, usage or conversion callback can
replace the checked route. Guard construction and rechecking are outside the
existing invocation's logical meter, as standalone authority construction is.

Record framing has a distinct scoped marker. Standalone replay must reject
those bytes. A scoped publication is a distinct private capability retaining
its admitted history binding; its complete poststate, class, reason, patches,
effects, outbox and work/law reports come from the actual checked producer.
No API converts it into a standalone publication or accepts a naked record as
evidence of a complete historical binding.

## Source and behavior boundary

Root owns authority guard construction, scoped execution/sealing, mathematical
relations, exports, source-getter regeneration and proof/control gates. The
template worker integrates an exact frozen dependency port afterwards. Normal
shell integration must enforce the same admission order on create, reopen,
import and recovery. Existing standalone APIs and historical oracle artifacts
are preserved. No cryptographic primitive or new cryptographic assumption is
introduced to make the publication fit.

The original prepared tests remain the acceptance oracle: all 216 inputs,
partition equivalence, incomplete finish, exact limit and one byte below,
seven crash points, interrupted delivery, stale head/context, ABA and second
handle refusal, plus exact restart/replay. Receipt and ordered outbox bytes
remain included in the complete bundle. Genesis/catalog/history-binding
admission remains separate from the original per-transition aggregate, as in
the existing template; transition authorization and bundle bytes are fully
counted. A record-size result alone cannot establish this integration.

## Required evidence

- Ordinary strict native compilation and supported no-std compilation.
- Whole-source Verus proof of every new executable body with zero executable
  preconditions, non-vacuous result relations and no admits or external bodies.
- The private guard follows only from exact binding admission; every bridge
  derives its premises from executable checks and verified callees.
- Changed policy/source/pin/header, omitted admission, stale inputs, modified
  record bytes, truncation, reordered effects and scoped/standalone confusion
  must refuse. Include correct business rejection and technical refusal.
- Native independent complete-output comparisons, current source closure and
  proof inventory, original tests and meaningful mutation controls.
- The actual original prepared positive fits the unchanged aggregate, and its
  one-under case still refuses before publication. Scope admission cannot be
  dropped or moved into an unverified application callback to obtain this.
- Independent Astra review of the final implementation and exact-head combined
  checks precede any release claim.

The [integration blocker](V2_INTEGRATION_BLOCKERS.md) remains open while this
contract is implemented. This stage does not supply checked continuation,
full V2 completion, commits, publication or release authorization.
