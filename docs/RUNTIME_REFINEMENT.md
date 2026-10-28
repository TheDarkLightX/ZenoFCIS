# A checked refinement pilot, not an executable assembly proof

This prototype takes one step toward correctness-preserving assembly for the
existing task-status profile. An untrusted producer proposes a different finite
representation. The existing native verifier independently executes both
contracts and checks their correspondence. It does not accept a producer's
claimed proof result. This is deliberately narrower than certifying the
assembled application, its database, or its host.

## Frozen intent and executable semantics

The separately authored [reference](../examples/runtime-refinement/task-intents/reference/SEMANTICS.md)
defines two durable intent slots, their pending/applied/acknowledged phases,
one volatile packet and one successful-send capability. Its 96 declared states
include 56 invalid states: those are checked technical refusals, not silently
excluded inputs. There are 12 events and 40 reachable lawful states.

The [producer](../tools/make_runtime_refinement.py) encodes the same proposed
state in one integer, with a different enumeration, and emits rules plus a total
abstraction. The [checker](../tools/check_runtime_refinement.py) does not import
that producer. It invokes the existing `contract refine-check`, which checks
admitted and related genesis, coverage, all 1,152 state/input pairs, unchanged
refusals, exact ordered observations and related successors. It also reruns the
reference invariant, four run properties and positive scenarios. Initial-state
correspondence and complete one-step correspondence, together with the checked
invariant closure, support induction over arbitrary finite histories **of this
fixed finite model**, not arbitrary installations or unbounded identities.

The reference requires approved durable publication before receiver application.
Dispatch, application, acknowledgement persistence and crash are distinct steps.
A crash loses the volatile packet/capability, not the published intent or durable
receiver fact. Replay can recover a lost acknowledgement without a second
application event. A stale capability cannot clear a different slot. Earlier
pending work can survive publication and completion of a later slot.

These are independently authored requirements, not postconditions inferred from
whatever the candidate happens to do. Candidate generation may change; the
reference, required observations and controls must not silently change with it.
A reference can itself be wrong: representation equivalence alone does not prove
its safety. The reference's invariant and run properties are separate checks.

## Run the pilot

Build the repository's pinned native CLI and use a trusted local installation.
The CLI, Python interpreter and host are part of the trusted computing base;
recording their hashes does not prove their implementation. `TASK_PACKAGE` is a
previously generated task-status assembler package, not an arbitrary contract.
Use fresh output directories outside the repository.

```sh
python3 tools/make_runtime_refinement.py --out "$CANDIDATE"
python3 tools/check_runtime_refinement.py \
  --cli "$CLI" --candidate "$CANDIDATE" --package "$TASK_PACKAGE" --out "$EVIDENCE"
python3 tools/check_runtime_refinement_controls.py --cli "$CLI" --package "$TASK_PACKAGE"
```

`receipt.json` is written only after the checks succeed. It states
`claim_level: MODEL_REFINEMENT`, `executable_assembly_refinement: false`, and
`publication_authority: false`. Requesting `--claim EXECUTABLE_ASSEMBLY_REFINEMENT`
refuses even when all byte bindings match. Unknown fields, changed inputs,
missing/extra mapping entries, incompatible profiles, incomplete checking,
nonzero verifier exits, timeout and inconclusive results cannot produce an
accepted receipt. Inputs are frozen for checking and rechecked for drift.
The receipt binds verifier, checker, model sources, package identity and the
retained check outputs. It does not install or modify the package.

The package check rehashes its manifest-bound bytes and compares its checked
core sources with the reviewed task profile. It does not execute the package or
trust its embedded success reports. This is identity/source binding, **not** a
proof that its binary implements those sources. A bound package cannot upgrade
a model-only claim.

## Separate implementation correspondence evidence

The task's 21 raw states and 126 command/context inputs exceed the existing
refinement tool's 64-input cap. The pilot does not drop unauthorized roles,
wrong artifacts or malformed lifecycle states to fit that cap.

Instead, [an independent Rust oracle](../tools/runtime-assembly/refinement_task_oracle.rs)
compares all 2,646 tuples directly against the generated core's actual Authority:
class, reason, pre/post state, patch, effects, destination, payload and ordered
outbox identities. The expected distribution is 19 accepted decisions, 2,620
business rejections and seven typed state-law refusals. The oracle is installed
as a test in a fresh generated application scaffold and run with `cargo test
--locked --offline --test refinement_task_oracle`; it is not derived from the
policy or the candidate certificate. The checker additionally reruns the actual
task's native lifecycle induction and per-attempt approved-intent uniqueness.
The intentionally false global-uniqueness claim must produce the legal
Request → Start → Fail → Retry → Start counterexample.

The [table pilot](../crates/zeno-fcis-collections/tests/runtime_refinement_insert.rs)
uses the existing `BoundedMap<OrderedMap>::insert`, not a replacement collection.
Its independent view includes logical entries **and** capacity/item/snapshot
limits and refusal results. It checks key uniqueness, replacement at capacity,
lookup/update, unrelated-entry framing and preservation of old immutable
snapshots. Run it with:

```sh
cargo +1.97.1 test --locked --offline -p zeno-fcis-collections \
  --features ordered-map --test runtime_refinement_insert
```

The stated finite domain has 54 policies, 198 admitted policy/state pairs and
1,476 insertion executions across initial insertion orders: 768 accept, 456
Full, 144 SnapshotTooLarge and 108 ItemTooLarge. Six comparator controls reject
incorrect outputs, refusal precedence, unrelated-entry changes, old-version
changes and reject-all behavior. These finite implementation differentials are
not a parameterized proof of Rust, `rpds`, allocation behavior, SQLite or storage
crashes. Existing collections proofs retain their own stated storage premises;
this pilot does not broaden them.

Native candidate controls rehash altered policies before checking, so stale
hash detection is not the reason they fail. They alter premature application,
acknowledgement of the wrong slot, publication, payload, destination, duplicate
application, invalid-state protection and abstraction coverage. The core oracle
also has copied-observation and substituted-input controls; these are identified
as comparator controls, not advertised as mutations of the production binary.
`test_runtime_refinement.py` runs only cheap admission tests; the separate
`check_runtime_refinement_controls.py` runner requires the CLI and package and
runs the native qualification controls too. Normal and optimized admission tests are included in the
existing `app-factory-journey` acceptance path. Full prototype qualification
remains manual; local evidence is not a hosted CI pass.

## The bridge still to prove

| Obligation | Current evidence and remaining gap |
|---|---|
| Representation preserves the approved finite transport model | Native complete one-step comparison and initiation/closure checks. Does not certify a runtime. |
| Emitted task core matches independently intended decisions | Full 2,646-tuple Authority differential. Not a proof of compiler, arbitrary source emission or copied binary. |
| One bounded immutable insertion preserves its view and refusals | Finite-domain production differential. Not general persistent-container verification. |
| Model Commit means a real approved Start | **Open.** Relate exact core candidate, plan and accepted evidence to the actual publication API. Hash/source binding does not discharge this. |
| Publication atomically retains state, revision, evidence and intent | **Open.** Establish the SQLite transaction/recovery refinement under explicit filesystem assumptions. Existing crash tests are separate evidence. |
| Model slot identifies exactly one real delivery | **Open.** Relate installation/contract identity, commit, lane, ordinal, destination, payload and delivery ID, including conflict and substitution cases. |
| Model capability/ack corresponds to the original pending entry | **Open.** Relate the typed send/ack API and durable FileReceiver ledger, including loss after apply but before acknowledgement. |
| Authenticated ingress, installed executable and host preserve these relations | **Open.** Authentication identifies the reporting party; it does not prove a manual outcome true. |

The smallest next implementation-proof target is **one approved task Start
publication**: given the actual immutable Authority candidate and a valid store
snapshot, prove the concrete publication transition either refuses without
changing persistent state or atomically produces the projected successor,
revision, accepted evidence and exactly its original intent. The proof must
frame unrelated entries and preserve prior snapshots, and must expose capacity
or failure observations. Then extend that relation across recovery and the
receiver/ack transition. Do not assume this postcondition merely because the
model requires it; do not strengthen its precondition to hide a real failure.

Task outcomes remain trusted manual observations. Succeeded need not imply a
receiver record, and Cancelled can coexist with an earlier attempt's pending
intent. Unknown is not definitive failure or permission to retry. Two modeled
slots correspond only to the profile's two attempts; arbitrary external work,
external exactly-once execution, power-loss recovery, concurrency, liveness and
fairness are not established. The model's invalid-capability/conflict events are
specification cases, not new production endpoints or a defense against arbitrary
forged packets delivered directly to the receiver.

The broader correctness-preserving assembly goal therefore remains open.
This certificate makes a limited checked claim explicit and refuses the stronger
one rather than presenting tests, source hashes or model induction as an
end-to-end implementation proof.
