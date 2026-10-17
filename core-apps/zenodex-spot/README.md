# ZenoDEX Spot example

Authored for the 2.5 D4/R6 acceptance example on the 2.4.0 CLI. This directory
contains three executable declarative contracts: Pool, u128 Balance, and the
complete composition. The composition has one fixed ordered pair X/Y, one fake
ledger and two ordinary accounts. This is a declared example profile, with no
multi-pool, custody or deployment qualification.

The owner policy is fixed: 30-bps ceiling fee, a 2000-bps floor protocol share
of that fee, reserved per-asset ingress, 1000 permanently locked LP shares,
positive payouts of both assets on burn, and a 200-bps exact-out quote-gap cap.
No command accepts a fee/tier parameter. Only CreatePool initializes.

`project.zeno` and `v2/policy.json` are the complete native contract. Its one
physical component stages Pool, eight complete Balance keys and LP ownership
together. The standalone component directories expose smaller interfaces for
bounded checking. `composition.json` documents their exact bindings and is
advisory metadata, not a checked-composition receipt.

Balance encodes each u128 holding as four little-endian u32 limbs because the
2.4 scalar authoring route has i64 public leaves. Debit sufficiency precedes
using credit. Column carry/borrow never reconstructs a u128 in signed i128.
Conservation compares five normalized columns of totals across all holders,
so even a per-asset total above u128 is checked exactly. This supplies no D5
or wide-numeric-main-route qualification.

Commands are normalized tagged scalar records. Action 0..4 means Create,
ExactIn, ExactOut, Add, Remove. Actor/recipient slots are in `composition.json`.
The application derives role and mirror views from its snapshot. Pool does
not accept shell-proposed quotes or effect plans. Generic unused scalar fields
are zero; create/add use recipient=actor. Ledger identity and actual account
authentication belong to the future mounting adapter.

Channels 300/301 contain underlying transfers, 302 signed LP deltas, and 303
the eight output numbers documented in `composition.json`. Transfer zeroes are
omitted. These deliveries describe the decision; ZenoLedger must commit the
complete state and exact economic plan atomically. No publisher, HTTP server,
durable store, restart/retry implementation or fake payment executor is added.

The generator uses D1 fused mul-div and ProductsGe rules. The laws separately
check exact rounded equations, k, conservation including ingress, lock/LP
equations and restricted immediate round trips. For immediate swap reversal,
`post_x * delivered <= gross * pre_y` bounds even a fee-free reversal by the
original input; actual reversal fees can only lower the return. For add/burn,
`minted * post_r_i <= used_i * post_S` bounds the floor burn by the deposit.
Neither statement concerns arbitrary intervening commands or market prices.

Stage 2 adds the law-only `decision_reason` observation to the contract
compiler. The five reduced judge profiles freeze an explicit finite reason
relation: all 228 tuples pass, including 174 rejecting tuples with no reason
freedom, excluding framework law 991. This does not extend the existing gap
judge to the complete contract with deliveries. The main economic contract
bytes and approved command semantics are unchanged.

The independent oracle's declared small domain has exhaustive isolated-user
sequence evidence through length 5. Both exact pro-rata and floored LP claim
valuations found no dominance gain. This is finite evidence, not a proof for
all amounts or arbitrary command alphabets. M1 has no donation command; the
named donation tests cover mirror rejection and explicit donated snapshots.

Light local commands, from the repository root:

```sh
python3 -m unittest discover -s core-apps/zenodex-spot/tests -v
python3 core-apps/zenodex-spot/tools/check_bundle.py
python3 core-apps/zenodex-spot/tools/replay_corpus.py plan --out /tmp/cx-spot/corpus-plan
python3 core-apps/zenodex-spot/tools/build_judge_projects.py --out /tmp/cx-spot/judge-projects
```

Authored source regeneration uses `tools/build_contracts.py`, then
`tools/build_composition.py`; use the pinned CLI's `generate contract` for
generated source. `tools/write_manifest.py` deliberately refreshes local source
identities and must follow review and the affected checks. It cannot authorize
a modified contract or a release.

C1 contains one explicitly rebased retained v8 record plus 30 boundaries.
L207 intake retains all 209 labels (207 burns and two swaps), all five source
groups and the 24 ZERO_PAYOUT rebases. It prepares 105 economic replays and
records 104 explicit exclusions. Account projections preserve separate
participating principals; unmodified external balances and metadata are
excluded explicitly. Original history hashes bridge excluded nonce/context
steps. See `corpus/l207-inventory.json` and each row's scope evidence.

The replay command defaults to the lead's 2.5 integration CLI under
`~/.cache/zf-targets/v250/debug/zeno-fcis`. It checks exact source and CLI pins;
`replay --allow-replay --l207 FILE` enables the authorized native comparisons.
It counts exclusions separately and fails on any mismatch. Native observations
are not authenticated receipts or publication evidence. `PROGRESS.md` records
current measured status; the stage-1 `REPORT.md` remains a historical record.
