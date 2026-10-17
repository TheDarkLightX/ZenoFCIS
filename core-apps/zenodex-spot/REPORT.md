# Spot code-stage report

Implemented the Pool and Balance contracts and a complete native composition
under `core-apps/zenodex-spot`, on starting HEAD
`fda32144231bd5f8adbe201eff3a958caa2caaf6`. This is the requested code-only
stage. **D4/R6 acceptance remains incomplete.**

## Delivered source

- [Pool](components/pool/project.zeno) and its ordered rules use D1
  `mul_div_floor`, `mul_div_ceil`, and `products_ge`. All five commands are
  present. Fee 30, protocol share 2000, lock 1000 and quote-gap cap 200 are
  fixed constants, with no fee/tier parameter or fee state.
- [Balance](components/balance/project.zeno) implements the complete u128
  semantic range through four little-endian u32 limbs. It checks ingress
  role, ingress overflow, funded debit, then ordinary overflow, in that order.
- The [complete contract](project.zeno) derives component views and the exact
  plan from one snapshot, checks all eight canonical complete Balance keys,
  then independently checks complete conservation and pool/LP equations.
  It emits transfers, signed LP deltas and a separate numeric observation.
  The [composition manifest](composition.json) gives ownership, bindings,
  ordering, slots and output positions. This manifest is advisory metadata.
- Generated Rust and schema/policy bytes are retained for all three contracts.
  They were generated and checked for drift; **Rust was not compiled**.
- The independent [big-integer oracle](tools/spot_reference.py), native
  comparator, bounded judge builders, tests and source inventory are included.

The complete application profile fixes one ledger, ordered pair X/Y and two
ordinary users. Other account slots are reserved Pool, lock and ingress roles.
This is not a general multi-pair mounting adapter. The root contract computes
role, ownership and mirror views itself; the standalone Pool context is a
projection interface, not authority for a caller's assertions.

The Balance representation is an explicit adaptation to the 2.4 scalar CLI:
public leaves are i64, so an unrestricted u128 leaf cannot be authored there.
Limb arithmetic preserves the intended mathematical domain and has bounded
native comparison evidence below. It is **not qualification of the separate
wide-numeric library route, D5, or a machine-checked full-range refinement**.

## Laws and ordering

The declared laws cover swap k nondecrease, exact fee/inverse/output rounding,
LP ratio mint and pro-rata floor payouts, positive burn payouts, LP/lock
equations, per-asset conservation including ingress, ingress monotonicity,
eligibility and refusal necessity. The case tables explicitly retain the
specification's first-reason order, including conservative pre-extraction
gross bounds, lock-actor priority, and ZERO_PAYOUT before burn minima.

Immediate swap reversal is bounded by `post_x * delivered <= gross * pre_y`:
even a fee-free reverse quote cannot return more than the original input.
For immediate add/burn, `minted * post_r_i <= used_i * post_S` bounds the
floor payout by the deposit. These are restricted two-operation statements,
not claims about arbitrary intervening commands or trading profitability.

Rule evaluation is eager. Unused branches therefore use safe positive
denominators and nonnegative product operands. There is no fee carry state;
numeric limb carry/borrow is invocation scratch. The named ARITHMETIC case is
unreachable by construction on the intended admitted arithmetic bounds.
Unexpected evaluator arithmetic/resource failure would still be a technical
refusal, not evidence of the semantic ARITHMETIC reason. Full-range source
proof and resource qualification were not run.

## What the pinned judges found

Pinned CLI: `zeno-fcis-2.4.0`, observed version `2.4.0`, SHA256
`697b4f9c7591973e8945cc21d30f3bb0dcfd314192bb74907ec382d1b187fbff`.
Each final project passed `review --gaps`, `lemma`, and `scenario`, with no
original Authority refusals. Limits were 512 tuples and 2,000,000 gap-work
units, with a 40-second process limit and low scheduling priority.

| Declared project | Tuples | Determined | Class freedom | Successor freedom | Reject-reason freedom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Create, absent or balanced 1001 | 16 | 1 | 0 | 0 | 15 |
| Exact-in, reserves 1..4, A=2, both directions | 32 | 10 | 0 | 0 | 22 |
| Exact-out, reserves 1..4, w=1, max-in 1..3, both directions | 96 | 16 | 0 | 0 | 80 |
| LP, correlated r0=r1=S in 1000..1002 | 48 | 24 | 0 | 0 | 24 |
| Ingress, actual u128 U-1/U, debit 0/1, credit/expected 0..2 | 36 | 3 | 0 | 0 | 33 |

Across these **228 tuples**, all 54 accepting decisions are determined.
All 174 rejecting decisions have their class determined but retain reason
ambiguity. The small swap projects specialize reserve caps to 4 and seed
S=1000; the LP cap is 1002. Constants f, p, K and the gap cap are unchanged.
Seeded tiny swap reserves are not asserted reachable in the full application.
Profiles with correlated fields are exact encodings, not independent samples.
Inductive lemma verdicts apply only to these declared graphs and their genesis.

The important residual gap is real: **2.4 declared `.zeno` formulas cannot
observe a reject's reason**, and the gap judge excludes framework law 991.
Thus declared laws cannot fully determine first reasons for this multi-reason
interface. Scenarios pass with exact reasons on their written paths, and the
runtime framework binds every decision to the ordered table. Neither fact
closes this law-only gap. No full-root gap verdict is claimed: that judge does
not support contracts with deliveries.

Controls record useful negative evidence:

- Removing LP's refusal/progress law changes determined tuples from 24 to 0
  and class freedom from 0 to 24; reject-everything becomes admitted. With the
  final law restored, every project's scenarios kill reject-everything.
- Candidate lemma `S<=1001` is refuted: one add of 2 moves 1000 to 1002, with
  an explicit trace from genesis. The intended bounded invariants are inductive.
- Increasing a computed quote by one causes the complete native Authority to
  refuse on declared arithmetic **law 502**, independently of framework 991.
- Removing the k law from an isolated copy fails the frozen source inventory.
  This last control is an identity check, not a runtime disabled-law theorem.
- The pinned lemma ID allocator fails with `has no free law ID` on a bounded
  source containing literal UINT32_MAX. Moving identical range bounds into
  supported I128 leaf bindings makes the ingress lemma pass. The domain and
  arithmetic base stay unchanged. The separate full-domain lemma attempt is
  inconclusive over its tuple limit.

Initial larger swap judge attempts hit the 40-second limit. They were not
credited as passes. The reduced closed domains above completed successfully.

## Fresh light evidence

- 17 Python tests pass: exact owner-policy boundaries, malformed/extra fields,
  zero and reserved-role guards, immutable snapshots and no-op refusal,
  alias aggregation, conservation, swap and add/burn round trips, and
  observation-comparator perturbations. Corpus/native encoding refuses values
  outside u128 before narrowing; it cannot silently truncate an invalid balance.
- **27 native comparisons, zero mismatches**: five successive commands through
  the complete Authority, 14 composition boundaries, and eight Balance probes.
  Comparisons include exact class/reason, all successor fields, every economic
  delivery and all eight output numbers. They include actual u128 carry/borrow,
  U-6/U-5 ingress, zero P at U, conservative gross bounds and diagnostic key order.
  Destination/ordinal/idempotency binding is supplied by framework conformance;
  the text comparison itself observes channel and numeric payload fields.
- All three generated contracts pass `generate contract --check`. The six
  authored contract files regenerate byte-for-byte. All 13 Python files parse.
  Pinned Rust 1.97.1's rustfmt accepts all three generated Rust files.
- All **2864 starting files retain their initial hashes**. Existing dirty work
  was not included in this change. No ZenoDEX repository was accessed.

The native five-step run is an authoring journey. It is not an HTTP J-SPOT
runner, ZenoLedger publication, restart/retry test, or durable-store acceptance.

## Prepared corpus harness

[C1](corpus/c1.jsonl) contains one explicitly rebased retained v8 record and
30 boundaries. The retained record has A=1000, o=996, F=3, P=0. Expectations
were materialized by the independent oracle; no old root is a new-policy golden.
[The intake plan](corpus/plan.json) preserves the historical formula/source pin.

[The replay harness](tools/replay_corpus.py) is prepared and its plan command
passes without starting the CLI. Native C1 and L207 replay **were not run**.
L207 still needs its normalized source export: 207 burns plus two history
swaps, the five source groups, all original labels, history continuity, and
explicit applicability. All 24 newly ZERO_PAYOUT labels must be rebased.
Missing or duplicate rows, unexplained exclusions and malformed goldens fail
intake. Source pins are checked before a later explicitly enabled replay.
Skipped out-of-scope cases remain separate from matches. Mismatches fail.

## Ownership, assurance and remaining work

The source-pinned review covers each new invocation boundary. `.zeno` decisions
consume snapshot scalars and return a complete checked candidate. The oracle's
World/Pool/Command graphs contain immutable exact scalars and tuples; its new
local lists never escape before freezing and are discarded on refusal.
Decoder dictionaries, generator Contract handles, subprocesses and serialization
are preparation/checking utilities with no decision or publication authority.
No new mutable Rust API or escaping mutable reference is introduced in the
generated source. Existing library lifetime/purity proofs were not requalified.

Four u32 limbs add representation and carry obligations but preserve u128's
specified domain. An i64 balance is smaller and loses required values; five
31-bit limbs add state and interactions just to avoid an advisory allocator
bug. Supported leaf bindings preserve the four-limb design and identical test
domain. No removed acceptance check or behavior-preserving refactor is claimed.

Remaining acceptance work includes the law-only first-reason gap, L207 intake
and both corpus replays, a mounting adapter and authenticated identity binding,
full-range arithmetic/limb refinement, resource and generated-Rust qualification,
HTTP J-SPOT, ZenoLedger atomic publication, restart/retry and release gates.
Source generation, a finite judge, or a source hash does not close those gates.

No Cargo, Verus, ATDD, network, commits, pushes or release work ran. Heavy
checks remain deferred under the user's instruction; this is not a 2.5 release
or production qualification.

## Review packet

[SOURCE_PINS.json](SOURCE_PINS.json) binds 32 local files, the owner/spec hashes,
CLI identity and starting-source inventory. The root source is
`db3193b3e803b992572b0e96ef0f264d51b431949e042bec2e8d5dac9872b516`;
rules are `6d0ba0ba9d2a79327c1cdf4167fc191960f002c69600d82795c9b44ba8299c56`.
The source-pin manifest SHA256 is
`c7ecb38e94a0e13088e78dfb8b3f52731a67d9fa248b6721c5b3ebf649ee790f`.
Progress and this report are excluded from that code inventory and included
in the final patch inventory instead.

Private retained packets and command/exit records are in
`/home/trevormoc/Downloads/ZenoFCIS-evidence-archive/v231-crucible-port/post-reset/spot-build/evidence/`.
`SUMMARY.json` distinguishes final passes, negative controls and unrun gates.
The patch is `spot-build/spot.patch` beside that evidence directory. It adds
only this application against the starting file hashes; the existing dirty
checkout's unrelated changes are excluded. `FINAL_PACKET.json` records its
digest and reconstruction check. Nothing in this packet was published.
