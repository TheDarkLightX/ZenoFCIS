# Spot stage 2 final progress

Governing source: BRIEF.md, read completely with the stage-1 report and all
required owner/spec documents. Approved Spot command semantics are unchanged.

1. No free value: checked every length 1..5 sequence in the declared domain:
448 initial states, two actors, 25 commands per actor; 9,114,582,400 paths.
1,318,000 distinct transitions; zero exact/floored-claim dominance violations.
K4 discovery took 27.245s; K5 discovery 73.768s in a 120s search allocation;
final K5 revalidation took 74.544s. Scope is finite, not an all-size proof.
All four named cases are retained. There is no M1 donation command; tests cover
mirror refusal and an explicit larger-reserve snapshot, not an invented action.
Three source defects fail named law checks; native mutations also refuse on
declared laws 502 (fee-free), 522 (ceil burn), 526 (stale-reserve mint).

2. Explicit state audit: EXPLICIT_STATE_AUDIT.md and source hashes retained.
Native complete-policy identity refusal test passes. Shell/identity specimens
fail the named audit check. Raw-byte/authenticated mounting remains unqualified;
closed_shape is an explicit external admission assertion. No silent repair.

3. First reasons: new law-only DecisionReason lowers to the existing observer.
Unsafe scopes refuse; ordinary rule variables retain their meaning. All 228
stage-1 reduced tuples pass (54 accepts, 174 rejects), zero reason freedom,
excluding framework 991. Removing the reason law turns the bounded check red.
This does not add full-root gap support or independently approve the case table.

4. L207: all 209 original labels/five source groups retained; 24 ZERO_PAYOUT
rebases. 105 replay rows, 104 explicit exclusions. Source history hashes bridge
excluded steps. Missing/duplicate/renamed rows, unexplained exclusions, false or
malformed goldens, altered history and missing rebases all fail intake.

5. Stable native run: C1 31/31 and L207 105/105 match; zero mismatches;
104 exclusions remain separate. Required CLI SHA256:
c8cca57d2fe5bd665b0542258af97b021cdac6feb910b12e906cd5892994ad3f.
Checked before/after every probe; executable bytes retained privately.
Earlier 20s timeout remains UNKNOWN (same-deadline isolated retry matched in
0.996s). A prior 136-match run changed executable identity and is unqualified.
Neither earlier attempt is credited to the final qualified census.

Final checks: 29 Python tests, 150 Rust tests/doctests, the separate 228-tuple
check, source inventory, rustfmt and six-file contract regeneration pass.
One fixture-writer Rust test remains intentionally ignored; the normally ignored
bounded reason test was run explicitly. No Verus or full ATDD ran.
Local CLI build fails only on the inherited missing DoorCommand::Evaluate arm
in checker_process.rs:1963; that unrelated file is preserved, not silently fixed.

Additional finding: an A=100 fee-zero mutation changes only channel 303 fee
metadata. Authority admits it, but the full-observation comparator refuses it.
The approved source is correct; independent metadata-law coverage is incomplete.

REPORT.md, spot2.patch, OWNER_REVIEW.md and FINAL_PACKET.json form the review
packet in the private spot-build2 archive. No commits, pushes, network or release.
