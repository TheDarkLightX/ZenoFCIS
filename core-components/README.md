# Bounded F1 component seeds

These three data packages propose reusable application behavior through the
existing F1 compiler, library Authority and generated SQLite application. They
add no Rust decision callback, alternate evaluator or authority API. They are
V2.2 G1 source work and V2.3 seeds, not a completed standard-library release.

The [finite certificate route](FINITE_PROOFS.md) covers every declared parameter
instance and all 6,158 raw inputs, including invalid prestates. It executes the
mandatory Authority and checks class, reason, successor fields and deliveries
against an independent complete reference. Qualification also requires three
actual law-violating policies, seven certificate tampering controls, and proof
references surviving ordinary scaffolding for all 21 instances.

Stored certificates have level **Identified**. Successful current-build exhaustive
replay supports **Proved for this complete finite scope**, under the named trusted
base in the certificate document. Source-hash agreement alone does not replay a
proof. The reference was authored by a separate agent from the public semantics
before that agent read these templates; it is test-only, not an owner adoption
label. This route establishes no Lean `KernelChecked` or unbounded family theorem.

The separate retained generated-app report records all 84 native tests across
21 applications: independent examples, genesis, persistent commit/replay and the
shared parser. Its SHA256 is
`ed41a32a80dffb3a41893e46a048c5167effd3946a9b543a96ae71e7ce73ac83`.
Certificate issuance inherits that genesis evidence only when every canonical
generated artifact is identical. A new transition replay does not rerun those
native tests or prove a general temporal induction theorem.

## Instantiate and use

Use a pristine source checkout whose stored certificates match its source inputs.
Generated files under `crates/` can make this conservative source binding stale;
the instantiator then refuses. Keep build and application outputs outside the
crate trees. From the source tree, with the parent output directory already
present and the locked dependencies available locally:

```sh
component_target="$PWD/target/core-components"
component_host="$(rustc +1.97.1 -vV | sed -n 's/^host: //p')"
cargo +1.97.1 build -p zeno-fcis-cli --locked --offline \
  --target "$component_host" --target-dir "$component_target"
component_cli="$component_target/$component_host/debug/zeno-fcis"
python3 tools/instantiate_core.py reservation-pool /tmp/my-pool-contract --param C=4 --param Q=3
"$component_cli" generate contract /tmp/my-pool-contract
"$component_cli" contract review /tmp/my-pool-contract --out /tmp/my-pool-review.json
"$component_cli" new /tmp/my-pool-app --contract /tmp/my-pool-contract --source "$PWD"
```

The instantiator refuses missing, repeated, extra, noninteger or unsupported
parameters, and refuses an existing output directory. It substitutes integers
and a derived package name in fixed data templates. The resulting ordinary F1
contract uses existing export, transform and adoption commands unchanged.
`core-instance.json` records the exact source set, domains and Identified,
replay-required certificate reference. Its provenance comment also survives
`new --contract` in `project.zeno`.
Generation and review do not authorize production use.

Generated READMEs explain the application commands. Shell code authenticates
principals/authorization, obtains time and owns persistence. All three seeds
return plain complete successor data or business Reject with unchanged state.
They emit no deliveries; consuming inventory or executing approval here records
logical state, not an external shipment or payment.

## Complete supported space

| Family | Parameter instances | Raw input tuples per instance | Declared successor-state product |
| --- | --- | --- | --- |
| Reservation | C=1..4, Q=1..min(C,3): 9 | 8 Q (C+1)^2; max 600 | (C+1)^2; max 25 |
| Rate limiter | W=1..3, N=1..3: 9 | 98 (N+1); max 392 | 7 (N+1); max 28 |
| Approval | K=1..3: 3 | 432 | 24 |

Totals: 2,216 reservation + 2,646 rate + 1,296 approval = 6,158 raw
input tuples across all 21 instances. Input products include invalid but
schema-admitted prestates, every supplied authorization value and every command.
They are not reachable-state samples. There is no synthesis output enumeration:
ordinary F1 compiles the ordered cases and F2 enumerates actual Authority inputs.
Actual graph sizes, output counts and Step budgets are recorded by execution
qualification, not inferred from these products. Unsupported parameters fail;
there is no fallback to boundary sampling or silently narrower schemas.

## Reservation pool

State is `available,reserved` in 0..C. Command is Reserve(150), Release(151),
Consume(152) or Replenish(153), with quantity in 1..Q. Context is an explicit
supplied authorized Boolean. Genesis is (C,0).

Ordered refusals are invalid state a+r>C (204), unauthorized (200), reserve
shortage (201), release/consume shortage (202), then replenishment over capacity
(203). Otherwise reserve maps (a,r) to (a-q,r+q), release to (a+q,r-q), consume
to (a,r-q), and replenish to (a+q,r). Reject changes nothing. Reserve/release
conserve total; consume removes exactly q; replenish adds exactly q. The
nontrivial invariant a+r<=C holds at genesis and after every commit. Every
command has a feasible positive witness with q=1; capacity and request-maximum
thresholds appear in the independent full corpus.

The rectangular schema includes states with a+r>C. The first case rejects them;
state invariants apply at genesis/commit, and framework reject/conformance laws
apply to rejection. Thus no invariant is falsely asserted of an arbitrary bad
input state. Qualification requires zero technical refusals even on those inputs.
From genesis, induction over the exact committing laws preserves capacity.

## Fixed-window rate limiter

State is `start` in 0..6 and `used` in 0..N. Command is Acquire(150), and context
is `now` in 0..6 and a supplied authorized Boolean. Genesis is (0,0). Reject
unauthorized (200), then now<start (201), then a full current window
(now<start+W and used=N, reason 202). At now>=start+W, accept with (now,1);
otherwise accept with (start,used+1). Reset and charge are one transition.

This is a fixed window that rolls over on its first request after expiry, not a
sliding-window or token-bucket limiter. A time regression still within the same
window is allowed and cannot refund quota; only before-start time refuses. Real
clock acquisition and any stronger monotonic-clock assumption are external.
The horizon 0..6 is the complete supported supplied-time domain, not a claim
about unbounded timestamps. Expiry beyond 6 simply cannot roll over in this
profile. Every W admits the exact-expiry positive example now=W.

The quota state law follows already from the scalar domain; it is explicitly
not a substantive conservation theorem. `exact_charge` is substantive: every
accepted request consumes exactly one quota unit, and only expiry resets it.
Within a fixed start, induction bounds accepted requests by N; each rollover
charges its first request. Reject-all fails positive examples.

## Single-request approval queue

State is phase Empty(150), Pending(151), Executed(152), plus three Boolean votes.
Commands are Enqueue(160), Vote(161), Execute(162); context supplies principal
0..2 and authorization. Genesis is Empty with no votes. There is one request,
three fixed principal slots and quorum K; no FIFO, reset, cancel or second
request is modeled. Reusing an instance for another subject is not supported.
The shell must bind these facts to the intended immutable request and principal.

Reject invalid state (200: Empty with votes, or Executed with fewer than K),
then unauthorized (201), then wrong phase (202), duplicate Vote (203), and
Execute below quorum (204). Enqueue changes Empty to Pending with zero votes;
Vote sets only the supplied principal's previously unset bit; Execute changes
Pending to Executed and preserves all votes. No command accepts after execution.

State laws hold on genesis and commits; invalid raw prestates reject as for the
pool. Vote conservation requires exactly one new distinct vote and frames the
other bits; execution preserves all votes. The full corpus includes duplicate
votes, K-1 refusal, exact K success, all three principals and post-execution
refusal. From genesis the phase laws and bit conservation prove the informal
inductive safety argument: execute at most once, with K distinct supplied slots.
They do not prove authenticated human approval or a coupled budget policy.

## Eager arithmetic, ownership and checking

All admitted decisions use immutable input bytes and library-owned checked
outputs. The new Python shell only writes fresh source directories; it does
not retain decision state or supply trusted callbacks. Existing library custody
and generated app APIs are unchanged. All arithmetic intermediates are small
signed integers even in inactive eager branches: pool arithmetic lies in -3..11,
rate arithmetic in 0..9, vote arithmetic in 0..4. No multiplication, division,
wrapping arithmetic, lazy semantics or domain-shrinking shortcut is introduced.

The execution qualification command takes an already built exact-source CLI:

```sh
python3 tools/check_core_components.py --cli /path/to/zeno-fcis \
  --work-dir /disk/new-seed-check --target-dir /disk/seed-target
```

Run it only under the allocated test slot and resource wrapper (4 GiB,
swap 0, CPU 100%, one Cargo job). It sequentially instantiates all 21 parameter
values, generates and regeneration-checks, performs full F2 review, requires
zero technical refusals over every raw tuple and exact agreement with all 6,158
independent expectations, scaffolds each app, validates the dependency binding,
and runs its generated tests (genesis, every example, persistent commit/replay,
and shared parser). It plants a lost reservation credit, a free quota request
and a lost approval vote; each must cause an actual lawful-prestate law refusal.
The report binds source inputs, artifacts, CLI, review packets and test logs.

The fixed verifier inventory, source-bound certificate replay, global acceptance,
package gates and independent review remain separate obligations. Each claim
must name its actual scope and evidence class. The finite route covers the whole
closed parameter set; general parameter theorems, kernel proofs and release
approval require their own evidence.
