# Finite trace-game certificates

A developer checking a withdrawal policy wants to know whether an adversarial
command can move value without permission, prevent a legitimate exit, or miss a
declared deadline. A bounded trace search answers “checked up to K under the
declared capabilities.” It can also return a replayable counterexample or
UNKNOWN when a resource budget stops the search.

For a completely enumerated finite model, the certificate workflow adds a
different answer: “safe on every finite history of the declared model.” A
separate checker verifies the certificate. The workflow can select one guard
before execution, certify a reactive strategy, or show why no controller in a
declared class can meet a requirement. These are different evidence kinds.

## Formal model

The product state contains the native state, independent reference state,
coalition, pending request entitlement, bounded deadline counter, ordered-event
monitor and an absorbing violation flag. Request identities and absolute time
are omitted because the fixture's transition rules, permitted moves and
objectives read only the entitlement and relative age. A violation is terminal:
reaching it already refutes prefix safety.

The withdrawal fixture has balances and paid amounts in 0..2 and two Boolean
flags. An approved request starts a counter at two. Each subsequent command,
including a rejection, consumes one deadline event. Completion clears the
request; otherwise reaching zero is a violation. Alarms never reset the counter.
The declared environment allows one intervening move and then forces a Process
attempt. It does not assume that this attempt succeeds. Every approved request
is monitored, regardless of when it occurs; there is no horizon cutoff here.

The composition fixture couples governance, collateral, oracle and vault
states. Its native table covers the declared fixed request variants. Both
fixtures retain the normalized v1 move alphabets: two account labels and their
three nonempty coalitions, up to one compromised privileged signer, fresh or
stale withdrawal approvals, and two oracle choices. Withdrawal opcodes are
0..4, actors 0/1, age 0 except withdrawal ages 0/1, and bound allowed=true.
Composition requests use the fixed variants in the fixture, with both permitted
account labels for unprivileged calls. Duplicate opcodes and other argument
variants are outside the declared alphabet.

The graph builder enumerates every reachable product state and every permitted
event and controller response, including rejected and stuttering transitions.
It returns UNKNOWN if a state, edge, time, domain or solver budget prevents
completion. An empty environment at a nonterminal state also gives UNKNOWN.
The default limits are 10,000 product states, 100,000 edges, 30 seconds for
construction, and 1,000,000 fixed-point work units with a separate 30-second
solver limit. Search/check processes have a 1 GiB address-space cap; the build
uses two jobs, no debug information and a separate 3 GiB per-process cap.

## Evidence kinds and exact scope

| Kind | What is checked | Claim |
| --- | --- | --- |
| Fixed-controller safety | Initial states belong to W; all of W is safe; every permitted successor stays in W. | Safe on every finite history of the declared model. |
| Fixed guard | One guard is chosen before execution from G = {true, actor == 0, false}, then checked as a fixed controller. | Guard g, from grammar G, is certified safe. |
| Reactive strategy | After the environment presents an event, a grammar policy chooses a response; every selected successor stays in W. | Strategy sigma, in grammar Sigma, is certified winning. |

The fixed-controller solver computes `nu W. Safe intersect Pre(W)`. The checker
verifies the certificate directly; it does not repeat fixed-point search. It
reconstructs permitted moves and successors independently, so an omitted solver
edge cannot establish closure. Every winning certificate also checks nonempty
moves at nonterminal states in W and a specified legitimate reference journey
completing inside W from **each** initial state and coalition.

The native guard location controls direct Withdraw authorization. It changes
neither the environment's capabilities nor the approved reference. All three
candidates have complete native tables; refusal of a legitimate withdrawal is
a safety violation of the declared legitimacy requirement.

The reactive grammar is a finite table from the current actor and request to
one of those three guards. The observation order is environment first,
controller responds, then native execution and resulting observations. A policy
cannot inspect native or reference state, the deadline monitor, a hidden field,
or a future result. The checker refuses undeclared reads and rule fields.
The solver computes a full-information upper bound, then extracts a policy
uniform across equal observations. Failure of this conservative extraction is
UNKNOWN, not an impossibility result or a complete partial-observation solver.

A separate abstract quantifier-order fixture has two events with actor=0 and
two hidden initial states. One event requires a true response; the other requires
false. Each event has some safe guard, but no fixed guard handles both. Exhausting
G means exactly “no guard in G.” A reactive event table wins on the same model
without reading the hidden state. These certificates describe the abstract
fixture, not native application execution.

## Unrealizability

An environment-attractor certificate contains ranked states and one permitted
environment event at each positive rank. The checker verifies that **every**
controller response to that event decreases rank, and that rank zero is Bad.
At least one initial state must be ranked. This supplies the scoped claim:
“no controller in the declared class satisfies the requirement under the
declared model.” It does not say that no implementation can do so.

The abstract oracle-withholding settlement fixture requires settlement within
two subsequent events. The environment may publish or withhold an oracle value.
Controllers may wait, settle with an available oracle, or cancel; cancellation
and settlement without an oracle violate the requirement. A rank-2 withholding
strategy certifies impossibility for that response class, even with arbitrary
history-dependent choices among its responses. This is bounded response, not a
fairness theorem or an unbounded liveness result. Native settlement and real
oracle infrastructure are outside this fixture.

## Witness wording and independent replay

“Shortest” describes the first witness found by complete unreduced breadth-first
discovery under the declared alphabet. “Locally minimal” describes a witness
after chunk deletion and argument shrinking; it does not imply global shortest
length. Certificate acceptance verifies counterexample replay, not a separate
optimality theorem. Historical v1 minimized witnesses retain that local scope.

From the repository root, with the pinned Rust toolchain and dependencies
already cached, run:

```sh
python3 -B tools/trace_game/run_v11.py --out ../trace-game-evidence
python3 -B tools/trace_game/certificate_check.py ../trace-game-evidence withdrawal-safety.json
```

Use a committed source tree and a fresh output directory outside it. The runner
keeps native tables, certificates, UNKNOWN attempts, command logs and peak RSS.
It binds the exact revision, source bytes, lockfile, native binary and input
artifacts. The independent checker uses only the Python standard library and
imports no solver or engine code. A changed binding or an unbound specimen is
refused. Local bindings record provenance; they are not authenticated release
signatures or a proof of the compiler.

These finite fixture checks do not cover additional accounts, keys, money,
argument domains, real schedulers or markets, native recipient/delivery
completeness, or the deployed shell. Composition callers and recipients are
model labels. They grant no FCIS authorization, application-law exemption,
automatic adoption, deployment permission or release qualification.
