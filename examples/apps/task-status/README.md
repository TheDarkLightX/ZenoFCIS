# Local task status

A deliberately small application contract for one non-value-moving local task.
The runtime assembler selects this exact profile; it is not a generic job queue.

## Decisions and visible status

The state is `status, attempt`. Status is Idle (150), Pending (151), Running
(152), Succeeded (153), Failed (154), Unknown (155), or Cancelled (156). The
attempt is 0 before the request, then 1 or 2. Commands carry `action,
observed_attempt, artifact`; artifact 1 selects this profile, while artifact 2
is a negative-control input. This scalar discriminator is not a content hash,
producer signature, or proof that an observation is true.

The configured principal supplies one context role: Operator (170), Adapter
(171), or Observer (172). The pure core, not the browser, checks that role.

| Command | Role | Required status | Successor and delivery |
|---|---|---|---|
| Request (160) | Operator | Idle | Pending, attempt 1; no delivery |
| Start (161) | Adapter | Pending | Running; one channel 300 intent to `local-test`, carrying the attempt |
| Succeed (162) | Adapter | Running or Unknown | Succeeded; no delivery |
| Fail (163) | Adapter | Running or Unknown | Failed; no delivery |
| Timeout (164) | Adapter | Running | Unknown; no delivery |
| Cancel (165) | Operator | Pending | Cancelled; no delivery |
| Retry (166) | Operator | Failed, attempt below 2 | Pending, next attempt; no delivery |

Every command must name the current attempt and artifact 1. Refusal order is
wrong role (200), wrong artifact (201), stale attempt (202), wrong stage (203),
then exhausted retry budget (204). A rejected command leaves state unchanged
and emits nothing. Exact request-key replay belongs to the existing durable
publication protocol; submitting a terminal receipt as a new command refuses.

Unknown is not failure: it permits neither retry nor cancellation. A later
correlated adapter report can resolve it. Cancellation is supported only before
Start, which durably publishes the intent. No command promises to undo a send.
The profile has no reset, third attempt, compensation, remote payment, dynamic
adapter, tenant isolation, or query pagination. All configured parties share
one task and may read it; Observer has no mutation permission.

## Trust boundary

The existing authenticated ingress binds a credential to its configured role.
The Adapter role is an explicitly trusted observation producer. Its authority
to report an outcome does not establish that the report is true; use it only
with the admitted local test receiver and reviewed receipt procedure. A
transport acknowledgement or local intent file does not by itself prove task
completion. Only a separate accepted core command changes the domain outcome.
A full production receipt protocol binding payload digest, producer, build,
revision and delivery ID is a separate qualification obligation, not supplied
by the small integer artifact field.

The laws require the role, artifact, attempt, stage, exact successor, exact
outbox count and payload, fixed destination, and first applicable refusal
reason. The generated core uses the existing Authority and checked contract
path. Neither the UI metadata nor an assembly receipt grants publication power.

## Checks

Run these with the repository's native CLI:

```sh
zeno-fcis check examples/apps/task-status/project.zeno --require-substantive --require-resolved-paths
zeno-fcis generate contract examples/apps/task-status --out SCRATCH
zeno-fcis contract review examples/apps/task-status
zeno-fcis contract scenario examples/apps/task-status
zeno-fcis contract lemma examples/apps/task-status --lemma '(post.100.110 == 150 && post.100.111 == 0) || (post.100.110 != 150 && post.100.111 > 0)'
```

`tests/decision-examples.txt` challenges role, artifact, stale attempt, timeout,
late success, duplicate completion, cancellation and retry exhaustion.
`tests/decision-scenarios.txt` checks ordered runs through the actual Authority.
These bounded contract checks are distinct from SQLite, crash, browser and
installed-package checks. The full Cartesian input domain is 2,646 tuples;
not every declared pre-state is reachable or satisfies the state invariant.
Do not interpret invalid-state law refusals as reachable application failures,
or finite checks as host isolation or power-loss durability evidence.

### What a failure observation means

`Fail` is a definitive task-result observation supplied by the trusted adapter
party. It is never inferred from a send error, timeout, pending delivery or
connector exit. Transport uncertainty leaves the task Running until the adapter
explicitly reports Timeout, which moves it to Unknown and prohibits Retry.
A definitive Fail permits the operator to start the second bounded attempt;
a result naming attempt 1 cannot complete attempt 2. Old pending delivery IDs
remain in the outbox, even after a new attempt or a task result. They are not
silently cancelled or acknowledged.

This example's local receiver only durably records intent bytes: it does not
execute a job or verify the truth of an adapter's observation. Replaying an old
pending intent can still reach that receiver. This policy therefore does not
establish safe retry for arbitrary external work or value-moving operations.
A real job adapter needs its own qualified outcome and reconciliation contract.
