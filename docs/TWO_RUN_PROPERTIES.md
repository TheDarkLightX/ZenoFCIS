# Does changing order or a private flag change the result?

A settlement operator asks whether swapping two users' requests changes either
request's decision or the resulting allocations. A ledger developer asks whether
changing a private flag changes anything a public caller can see. These
LLM-generated examples turn those questions into finite, executable comparisons.

`zeno-fcis contract two-run DIR` is advisory. It binds the contract's own
Authority and exhaustively checks the declared finite domain when that domain
fits the existing tuple ceiling. Exit 0 means `checked`, 1 means `refuted`, and
2 means `UNKNOWN`. Invalid paths or selectors are refused (exit 65); invalid
command options, including a negative `--within`, are refused (exit 64).
`--out` writes the JSON packet and prints one summary line.

## Formal model: self-composition

Let A(s, i) be the Authority's outcome on state s and complete input i, including
both command and context. The state domain contains every declared field value,
including states unreachable from genesis. An admissible state satisfies every
state law, using the same state-law evaluator as the other finite judges.
An Accept or CommittedFailure supplies the next state. Reject and Refused leave
state unchanged for sequencing; neither is silently converted into an Accept.

For order independence, the two selectors describe sets I1 and I2. They use the
contract expression language, including variables, but may read only command
and context. For every admissible s and (i1, i2) in I1 x I2, self-composition
uses T(s, i) for that effective next state and compares the runs
(A(s, i1), A(T(s, i1), i2)) and (A(s, i2), A(T(s, i2), i1)). Their final
states are T(T(s, i1), i2) and T(T(s, i2), i1). Each input must have the same
decision class in its two positions, including refusals. Every observed final
field must have absolute difference at most K, where `--within K` defaults to 0.
Reasons and outboxes are not part of this order-independence relation.

For noninterference, states s and t are related when they agree on every field
outside `--secret`. For every such pair and every identical complete input i,
compare decision class, reason, every requested public successor field, and the
entire ordered outbox (ordinal, channel, destination, payload and idempotency).
Reject has no successor or outbox; a refusal retains its diagnostic reason.
The implementation groups states by the nonsecret fields for each input and
compares each outcome with its group's representative. Equality is transitive,
so this needs one Authority invocation per admissible state/input tuple rather
than all state pairs. Group lookup uses direct finite-domain indices.

When `--public` lists every nonsecret state field, public successor equality
is step consistency for the same relation used to group initial states.
Equality of the decision class, reason and outbox supplies output consistency.
For a fixed deterministic model with admissible effective successors, these
are the synchronized-input form of the classical unwinding condition
(Goguen and Meseguer 1984; Rushby 1992). Reject and Refused use no-op successors.
The private-ledger command below includes all its nonsecret fields. This judge
still reports only the one-step result; it issues no all-lengths verdict.

A proper public subset need not preserve that relation. In the synthesized
`test-data/two-run/two-step-leak` fixture, input 0 copies the secret into an
unobserved nonsecret field, then input 1 copies that field into the public
value. The one-step check with only `post.100.110` public holds; including
`post.100.112` refutes it. The run-property check finds the public-value change
after inputs 0, 1. `tools/test_two_run.py` checks both one-step verdicts and
replays the two-step histories. This illustrates the scope boundary and does
not widen the two-run verdict.

Paths identify scalar state fields by ID, such as `pre.100.110` or
`post.100.110`; both spellings designate the same field in these options.
Unknown fields, duplicates, a field designated both secret and public, and
selectors that read state (including through a variable) are refused.
Undefined selectors, empty selected input sets, no admissible states, or a
domain exceeding `--max-tuples` yield UNKNOWN with the reason. The hard tuple
ceiling is 1,048,576; the packet reports min(requested cap, 1,048,576).
Order independence also has a fixed limit of 1,048,576 planned Authority
invocations. Before running either order, it counts admissible states x |I1|
x |I2| x 4, including calls that would refuse. If that count exceeds the limit,
it returns UNKNOWN (exit 2) with the count and reason, even if an early pair
would refute the property. The packet retains `planned_order_invocations`,
`max_order_invocations` and the actual invocation count (zero on this UNKNOWN result).
This bounds invocation count, not wall-clock time or cost per invocation.
No boundary sampling produces `checked`.

## What differs from a one-run judge?

A state invariant checks one state or transition. It can hold for every run
while two related runs disagree about a public value or an input's decision.
Self-composition makes that disagreement an explicit pair of runs. Its
counterexample contains the initial state(s), both complete inputs, both runs,
observed values, and the first difference. For noninterference the two input
entries are equal. For order independence they are replayed in opposite orders.

## Executable examples

Run from the repository root with a binary built from this source. The commands,
summary outputs and exits below are checked by `tools/test_two_run.py`.

### Constant-product swaps: exact equality is refuted

[The swap example](../examples/two-run/constant-product/README.md) uses
received = floor(amount * reserve_y / (reserve_x + amount)). Each user's received
amount is recorded separately, so equal total reserves cannot conceal a change
to an individual's allocation.

<!-- two-run-example: exit=1 -->
```sh
zeno-fcis contract two-run examples/two-run/constant-product --commute --first "command.101.120 == 0" --second "command.101.120 == 1" --observe post.100.110,post.100.111,post.100.112,post.100.113 --out swap-exact.json
```
```text
refuted constant-product: zeno-fcis/two-run/1
```

A tolerance of one also fails over this complete domain:

<!-- two-run-example: exit=1 -->
```sh
zeno-fcis contract two-run examples/two-run/constant-product --commute --first "command.101.120 == 0" --second "command.101.120 == 1" --observe post.100.110,post.100.111,post.100.112,post.100.113 --within 1 --out swap-within-one.json
```
```text
refuted constant-product: zeno-fcis/two-run/1
```

The independent reference finds a maximum observed difference of 3. The finite
reserve capacity also causes some requests to be refused in only one order;
raising K cannot repair a decision-class difference. There is no claimed
rounding tolerance that makes this example order independent over its full
domain, and no general front-running-resistance claim.

### Uniform-price batch settlement: exact equality holds

[The batch example](../examples/two-run/uniform-price/README.md) applies a fixed
batch price to the two users' independent settlement slots. It checks settlement
of already allocated quantities; it does not discover or optimize the price.

<!-- two-run-example: exit=0 -->
```sh
zeno-fcis contract two-run examples/two-run/uniform-price --commute --first "command.101.120 == 0" --second "command.101.120 == 1" --observe post.100.110,post.100.111,post.100.112,post.100.113,post.100.114 --out batch.json
```
```text
checked uniform-price: zeno-fcis/two-run/1
```

### Private ledger: a public observation must not reveal the flag

[The private ledger](../examples/two-run/ledger-private/README.md) increments a
bounded balance and toggles its public sequence independently of the flag.
Refusals at the balance limit remain part of the comparison.

<!-- two-run-example: exit=0 -->
```sh
zeno-fcis contract two-run examples/two-run/ledger-private --noninterference --secret pre.100.112 --public post.100.110,post.100.111 --out ledger-private.json
```
```text
checked ledger-private: zeno-fcis/two-run/1
```

[The leaking variant](../examples/two-run/ledger-leak/README.md) assigns the
public balance from the private flag instead.

<!-- two-run-example: exit=1 -->
```sh
zeno-fcis contract two-run examples/two-run/ledger-leak --noninterference --secret pre.100.112 --public post.100.110,post.100.111 --out ledger-leak.json
```
```text
refuted ledger-leak: zeno-fcis/two-run/1
```

## Scope and limits

Every packet includes this scope sentence:

> two runs from every admissible state over the declared finite domain; one step per side (noninterference) or two (order independence); longer interleavings are not claimed

The checker uses the existing scalar finite-domain binding (booleans, signed
integers and scalar variants). Unsupported input domains, including the legacy
binding's U128 refusal, yield UNKNOWN rather than a pass. Collection view fields
remain views, not arbitrary backing stores. The subject is the rules-compiled
contract, as for the existing scalar judges; retained program adoptions are not
replayed by this API. Execution cost for order independence grows with states
x selected input pairs. This judge supplies no timing, termination, arbitrary
trace, cryptographic secrecy, economic-security, release or runtime authority.

The independent Python reference implements the documented test-corpus subset
with exact integers and an all-pairs comparison. Agreement on generated
contracts is differential evidence, not an unbounded proof or language-wide
qualification. The packet's source hashes and checker identity identify the
actual checked subject.

## Lineage

**Background.** Goguen and Meseguer, *Security Policies and Security Models*,
IEEE Symposium on Security and Privacy, 1982, introduced noninterference.
Goguen and Meseguer, *Unwinding and Inference Control*, IEEE Symposium on
Security and Privacy, 1984, developed unwinding conditions. Rushby,
*Noninterference, Transitivity, and Channel-Control Security Policies*,
SRI Computer Science Laboratory Technical Report CSL-92-02, 1992, treats
unwinding and its security-policy premises.
Barthe, D'Argenio and Rezk, *Secure Information Flow by Self-Composition*, IEEE
Computer Security Foundations Workshop, 2004, developed self-composition for
information-flow verification. Clarkson and Schneider, *Hyperproperties*,
Journal of Computer Security 18(6), 2010, formalized properties of sets of traces.

**What is new here.** This is an integration of established two-run checking
into the existing finite contract Authority. Comparative prior-art search
pending; no novelty claim is made.

**Scope.** The checks concern the declared finite domains and the exact
observations above. Publication of this documentation supplies no additional
verification or runtime authority.
