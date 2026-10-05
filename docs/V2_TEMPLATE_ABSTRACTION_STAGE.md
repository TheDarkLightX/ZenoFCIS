# Raw account abstraction stage

Subject base: 1ed6f88f7b1098bed6c409bc9aa3c12c99d43c3c. This is an additive
library decoder and expression stage, with no authorization or publication.

The actual account domain is attempts 0..2, deadline 0..4102445700,
original last_seen and now 0..4102444800, canonical Bool admin, and the
payload-free top-level Sum type 101 with variants 120,121,122. These are
schema bounds, not synthesis caps. All schema-admitted states are supported,
including states that do not satisfy law 500; law admission belongs downstream.
Reject must preserve the ORIGINAL last_seen, not replace it with now.

Counterexample: state (0,0,0), command120, context(0,false) accepts;
state (0,0,1) with the same command/context rejects200. Both satisfy law500.
Any abstraction omitting original last_seen identifies distinct decisions.
The tracked baseline has no account finite synthesis study; it is handwritten.

The closed arithmetic language is Identity/Add/Less/Equal over i128. Add is
checked over the entire i128 domain. Canonical integer field decoding permits
arbitrary u16 IDs and inclusive i128 bounds, including both signed extremes.
The account decoder fixes the exact authored descriptor, never clamps input.
It derives backwards, locked and third-failure facts, raw count+1 and now+900,
and retains the complete raw tuple. Finite control consumes the facts; complete
successor and alert construction consumes original values or computed i128s.

Order: state Byte, state header, Reads110/111/112; command Byte, Read101 and
complete Sum; context Byte, header, Reads130/131; then five Step attempts
(backwards, locked, third, increment, deadline). Every charge precedes protected
work. First refusal stops, keeps all counters and attempted Read records, and
returns no partial facts. Private composition retains arbitrary initial eight
counters and attempt prefixes. No Candidate/Write/Effect/WitnessByte/Depth
charges occur. Metadata, allocation, hashing and physical costs are excluded.

Proof plan: exact primitive byte specifications; exact charged helper behavior;
sequential original-byte decoder; arithmetic range from admitted raw bounds;
finite branch equivalence and raw reconstruction for every admitted input;
whole-source function/spec/body inventory; independent native canonical encoding
and raw policy oracle; proof-failing semantic mutants and verifying coverage
mutants. Compiler/erasure, vstd/Z3 and platform remain named trusted assumptions.

The decoder is pure and owns fresh local scratch; authoritative input slices
are immutable, output fields privately constructed. No shell/CAS/crash protocol
changes are made. Root owns catalog binding, module registration, evaluator
identity, complete artifacts and authority/replay integration. Inventory/order
already fit the existing scalar domain, but order needs Enum successor support,
all templates need typed reasons and exact outbox mapping, and account needs its
top-level Sum bytes bound directly, not re-encoded into a fictitious record.

## Producer and consumer contract

`account_abstraction_v2::execute` accepts the three ORIGINAL canonical Value
slices. The account command remains its actual top-level Sum. The only public
configuration is `Limits`; initial consumption, raw facts, replacement usage,
and application callbacks are not accepted. `execute_into` is the private
same-meter producer for root composition.

`Facts::original()` returns (failed, until, original_seen, command_variant, now,
admin), `finite()` returns integer Bool codes (backwards, locked, third), and
`computed()` returns full i128 (failed+1, now+900). The latter increment can be 3
when third is true; that slot is not an admitted successor count until the
checked branch chooses reset 0. Retaining that exact arithmetic value avoids
clamping and makes branch-specific successor admission explicit.

`execution_sound` derives from the original-byte execution specification both
range and raw semantic correspondence. Ghost branch codes 0/1/2 are Reject
reasons 200/201/202; code 3 is accepted login; code 4 is committed failure 203 with
Locked alert; code 5 is committed failure 203 without alert; code 6 is accepted
admin unlock with Unlocked alert. `raw_post` and `fact_post` agree on the complete
three-field state and alert kind/deadline. These are reconstruction semantics,
not an implemented receipt, patch, outbox builder or finite branch graph.
Root must bind its actual decision producer to this relation and must preserve
empty effects, destination `security-team`, channel 300, ordinal 0 and reason
classes when constructing complete artifacts.

The six Read requests describe eager canonical input admission, including
admin for every well-formed invocation. They do not claim to reproduce the
legacy handwritten adapter's conditional observation calls or supplied zero
usage. The V2 ingress cost profile is explicit and must join the downstream
actual access footprint and declared whole-context 102 policy.

Run the final source/coverage/control gate under the shared heavy lock:
`flock /tmp/zenofcis-v2-parallel/heavy-check.lock python3 tools/check_abstraction_v2.py --out EVIDENCE_DIR`.
Coverage refresh is an explicit development operation, never an automatic
acceptance repair. The gate proves the registered actual production closure,
checks every translated executable contract and body, and distinguishes
semantic proof failures from verifying-but-refused operational coverage controls.
Independent native checks use standard-library canonical byte construction and
an authored raw policy oracle. The public integration checks use the actual
Value encoder and all 20 reviewed template examples, without changing their
canonical command shape. Byte/catalog identity, complete decision artifacts,
laws/genesis, mandatory authority/replay and shell guarantees remain separate.

Completeness is checked separately from success preservation. An independent
wire-offset predicate fixes complete lengths 62/8/27 and every tag, ID, payload
and raw value. `complete_decode`, `complete_facts` and `complete_execution`
prove that every such legal raw invocation succeeds with sufficient resources
and consumes exactly 97 Byte, 6 Read and 5 Step above arbitrary initial counters.
`decoded_canonical` proves the converse canonical-byte property on success.
These proofs do not assume a pre-state invariant or cap any legal timestamp.
