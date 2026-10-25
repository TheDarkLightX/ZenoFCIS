# Runtime assembly design models

These are executable finite design examples, **not a runtime assembler or a
production adapter implementation**. They exercise the existing declarative
contract compiler, native Authority, exhaustive decision tables and scenario
judge. They do not replace the existing shell effects, correlated response,
plan or durable outbox protocols.

From the repository root, with a compatible native ZenoFCIS CLI:

```sh
python3 tools/check_runtime_assembly_design.py --cli /path/to/zeno-fcis --out /tmp/assembly-design-check.json
```

The design was checked using the native `zeno-fcis 2.6.0` development CLI
built from public source commit
[`e573ab3bd95c3d33095b4a4796b91730a75ce952`](https://github.com/TheDarkLightX/ZenoFCIS/tree/e573ab3bd95c3d33095b4a4796b91730a75ce952).
Its executable SHA-256 was
`2165b86e7497ea5c8471662ab86f7a78d011587202925031c90a7acc4d2d8958`;
its reported library identity was
`4f787c439cf4dc22d292dcc9285774069872745d1057d632de743158581c3559`.
A rebuild may have different executable bytes; record its identity and rerun the
checks rather than treating the version string as equivalent evidence. This
pin is an experiment tool, not a released 2.6 qualification or a claim that
later source has identical behavior. The PR's model source hashes are recorded
separately by the runner. To build that tool in a separate checkout, use its
committed toolchain and lockfile:

```sh
git checkout e573ab3bd95c3d33095b4a4796b91730a75ce952
cargo build --locked -p zeno-fcis-cli --bin zeno-fcis
```

The runner records the executable and model hashes, requires complete baseline
reviews and passing native scenarios, then removes each gate independently and
requires concrete scenario failures. Parse errors, timeouts, UNKNOWN and generic
nonzero exits do not count as killed mutants. The temporary copies are deleted;
the original model files are checked for drift. The compact result has no
runtime, adoption or publication authority.

## Assembly model

Two states (`0` unchecked, `1` packaged), one command and four Boolean facts
produce **32** declared input tuples. Packaging requires a supported profile,
matching source identity, compatible schema and compatible capability binding.
The scenarios independently deny each missing fact and deny repeated assembly.
This is the proposed *acceptance predicate*, not code which reads manifests,
computes hashes, proves refinement, grants capabilities or writes packages.
A matching hash alone does not establish semantic compatibility.

## Effect lifecycle model

Seven states, seven events and eleven Boolean observations produce **100,352**
declared tuples, including unreachable state/context combinations. State labels:

| Value | Meaning |
| --- | --- |
| 0 | Pending: the business core has already accepted an intent |
| 1 | Running: delivery may have occurred |
| 2 | Unknown: timeout or interrupted delivery; outcome not established |
| 3 | Cancellation requested: not a rollback guarantee |
| 4 | Succeeded with correlated durable outcome evidence |
| 5 | Failed with definitive durable negative evidence |
| 6 | Cancelled before dispatch, or with definitive durable cancellation evidence |

All commands require independent artifact, revision, attempt, producer, evidence
kind, target and stable operation identity matches, plus command authorization.
This applies to cancellation and timeout observations too. `command_authorized`
is an abstraction of an authenticated command and the pure core's policy check;
the shell cannot invent it. No authentication or business authorization
implementation is supplied here.

`durable` means the state and intent are durably committed before dispatch or
redelivery, and the outcome evidence is durably retained before completion.
It is a modeled premise, not a filesystem/database durability test. An inbox
may retain an untrusted observation, but the accepted receipt and domain
successor must publish atomically through Authority; persistence never
authorizes an observation. Timeout
requires no durable outcome because none is known. Cancellation before dispatch
changes only the pending logical intent; it cannot race a dispatch outside the
same serialized state transition.

`definitive_negative` represents trustworthy sink evidence of final failure or
cancellation with no later successful effect for this operation. A transport
error or acknowledgement of a cancellation *request* is not that evidence.
The sink protocol must justify this premise. Cancellation requested can race
with success: success is allowed to win before a definitive cancellation
receipt. After a definitive terminal receipt, later events cannot change state.
An adapter unable to establish finality must retain Unknown or cancellation
requested; this model does not solve a dishonest or contradictory sink.

Redelivery from Unknown requires the same stable operation/attempt identity and
a declared sink deduplication contract. This is redelivery of one operation,
not authorization of a fresh business attempt. A new attempt needs a separate
pure-core decision and budget check, not supplied by this example. An outbox
alone cannot make a remote effect exactly once.

Exact duplicate commands here reject unchanged at the core. An actual adapter's
idempotency-key lookup must return the original durable response for an exact
retry and reject changed payloads under that key; that storage mechanism is not
modeled. Likewise, each match bit represents equality against retained identity,
not a signature check. The implementation must construct the facts from bound
immutable input, never trust arbitrary client Boolean flags.

## Evidence boundary

The native review checks all declared one-step inputs against the authored
laws. Linked scenarios cover durable dispatch, independent stale/foreign
identity inputs, repeated completion, timeout/reconciliation, safe redelivery,
cancellation before dispatch, success/cancel races and confirmed cancellation.
Seventeen explicit policy mutants cover the four assembly gates, eleven effect
gates, timeout-as-failure and cancellation-as-rollback. The laws remain intact
when these producer-policy gates are removed, so native Authority refusal or a
wrong decision breaks the scenario. This tests the independent acceptance
boundary; it does not claim every mutation in an unbounded implementation is
caught or that the laws completely specify human intent.

There is no real sink, network, concurrency scheduler, durable store, crash
injection, receipt verifier, UI, migration or packaging implementation here.
The scalar model has one logical operation and no row capacity or retry budget.
Reachability, liveness, fairness, crash refinement and adapter correspondence
remain separate obligations. Ordinary native checks are used; no persistent
loop containment backend is required or bypassed.
