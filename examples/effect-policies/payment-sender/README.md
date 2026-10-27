# Send an invoice payment safely after a timeout

An invoice is authorized by the application's core and placed in its committed
outbox. The shell sends that exact record. An effect policy adds a conservative
safety check: reserve its key before calling the payment provider, then refuse
another send under that key even when the provider's reply is lost.

## 1. See the lost-reply case

From the repository root:

```sh
cargo +1.97.1 run --locked --offline -p zeno-fcis-shell \
  --example payment_policy_check -- --demo
```

The provider accepts invoice 1 but returns `Unknown`. The delivery program
waits before retrying. Its policy refuses the second send. There is one payment
of invoice 1, its outbox record stays pending, and the run remains uncertain.
The policy never turns the timeout into evidence that no payment occurred.

## 2. Declare the guard at the generated shell boundary

Generate the `withdrawal-queue` application and enable its `shell-effects`
feature. Its payout sender uses the same delivery program and production
SQLite adapter as this example. At startup, before sending:

```rust,ignore
use zeno_fcis_shell::effects::policy::Policy;

let mut shell = shell_effects::Effects::open_with_policies(
    path, lineage, transport, time,
    vec![
        Policy::AtMostOne { capacity: 1024 },
        Policy::allowlist(vec![b"payment-provider".to_vec()]),
    ],
)?;
let result = shell.deliver(epoch)?;
let transcript = shell.transcript();
```

The key is `(Store.identity, Delivery.id)`, shared across handles, rather than
the amount or payload hash. Every attempted send consumes its reservation,
including `NotSent`, `Unknown` and a transport panic. Capacity exhaustion
refuses explicitly. A later delivery run on the same shell keeps reservations;
a refusal preserves the pending record. A policy cannot authorize an invoice,
change its recipient or amount, or create an operation.

A monitored shell requires private transcript capture and adapter provenance.
Each request records the bound policy identities, original state, proposed
successors and verdicts. Handler-free replay recomputes these values. Removing
or changing a verdict refuses replay even after the hash chain is rebuilt.
A hash chain provides integrity evidence, not authentication.

## 3. Check every modeled response through K

```sh
cargo +1.97.1 build --locked --offline -p zeno-fcis-shell \
  --example payment_policy_check
python3 tools/check_payment_policy_product.py \
  --binary target/debug/examples/payment_policy_check \
  --bound 6 --out payment-policy-result.json
```

The checker compares an independent Python controller model with the real Rust
`DeliveryProgram` and policy stack. It compares exact operation, response,
verdict and policy-state bytes, external payment occurrence counts and pending
obligations. The report retains every compared trace and counterexamples.

The domain has two fixed authorized invoices, one store and serial relay,
retry budgets 0–2, exact successful time observations and zero jitter.
`policy_product::ALPHABETS` lists every modeled response. Every prefix through
K requests is explored. A stopped action stays incomplete; finite closure is
separate from eventual delivery. The independent acceptance check requires:

- Every committed action belongs to the authorized invoices.
- One key occurs at most once in the committed multiset.
- Every authorized action is committed, explicitly refused, or still pending
  and classified incomplete.

The source controls bypass runtime admission, count distinct keys instead of
commit occurrences, and accept a silently dropped invoice. Each control must
build, fail its named assertion, and pass again after restoration.

## 4. State the provider and lifetime assumptions

The main model allows at most one provider execution per transport invocation
and assumes no provider idempotency. A separate profile permits duplicate
transport copies and assumes atomic provider idempotency. Removing that
assumption exposes two commits from a single send, despite the policy.
Provider idempotency is an environment assumption, never a shell claim.

Reservations survive retries and unwinding on the same generated shell handle.
A new handle starts a new lifetime. A qualified durable reservation persistence
and restoration adapter is required before claiming safety across real process
loss; that adapter is not supplied here. The example provides finite model and
runtime-test evidence, with no claim of exactly-once external execution,
eventual payment, concurrent relays or production readiness.

## Background, contribution and scope

Background: Fred B. Schneider, *Enforceable Security Policies*, ACM Transactions
on Information and System Security, 2000, studies security policy enforcement
by execution monitors. Edmund M. Clarke, Orna Grumberg and Doron A. Peled,
*Model Checking*, MIT Press, 1999, presents transition-system product checking
and counterexamples. Matija Pretnar, *An Introduction to Algebraic Effects and
Handlers. Invited Tutorial Paper*, Electronic Notes in Theoretical Computer
Science, 2015, explains effect requests and handlers.

What is new here: the integration binds these monitor verdicts to the existing
shell protocol and its replay evidence. Comparative prior-art search pending.
No novelty claim is made.

Scope: exhaustively checked over the declared domains through K, plus the
specified generated-shell tests. The core still decides. See
[the shell effects reference](../../../docs/SHELL_EFFECTS.md) for the registry,
production adapter and wider simulation premises.
