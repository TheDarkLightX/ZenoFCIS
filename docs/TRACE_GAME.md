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

## A journey

**The ask, in one line:** "Can anyone in this withdrawal policy move money without permission, or block my exit
past its deadline?"

1. **Say what is allowed.** The fixture declares who acts (two accounts and their coalitions, plus at most one
   compromised signer), what they may send (the move alphabet), and the rules every history must respect: no
   unauthorized movement, and an approved exit completes within its deadline.
2. **Try to break it within a bound.** The bounded trace search explores every history up to K moves. It answers
   "checked up to K", or returns a replayable counterexample: the exact moves, the state after each move, and the
   rule broken.
3. **Ask for a certificate.** When the model is small enough to enumerate completely, the runner builds the whole
   product graph. It then returns one of four things:
   - a safety certificate ("safe on every finite history of the declared model");
   - a guard chosen before execution;
   - a reactive strategy;
   - an unrealizability certificate showing that no controller in the declared class can meet the rule.
4. **Check it independently.** `certificate_check.py` verifies the certificate with the Python standard library
   only. It does not rerun the search, and it refuses a certificate whose bindings (revision, sources, lockfile,
   binary, inputs) changed.
5. **Read the scope line** that comes with every result. It names the model, the alphabet and the limits. A
   certificate covers that model, not the deployed application.

## How this differs from bounded search
| | Bounded trace search | Trace-game certificate |
|---|---|---|
| Question | Is there a bad history of at most K moves? | Is every history safe, of any length, in this finite model? |
| "No problem found" means | Nothing found up to K | Safe for every length, within the declared model |
| Evidence | A counterexample, or "checked up to K" | A fixed-point certificate, checked by a separate small checker |
| Can it show a rule cannot be met? | No | Yes, for a declared controller class (unrealizability) |
| Cost | Grows with K | Grows with the number of product states; UNKNOWN when over budget |

Both answer only for the declared model: its accounts, its alphabet and its argument domains. Neither says
anything about extra accounts, real schedulers, markets or the deployed shell.

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

For fixture certificates without repository-wide documentation and style gates,
use `run_v11.py --certificates-only --out OUT`. This mode still builds native
tables, independently checks every certificate, runs the fixture tests and checks
source stability. Its binding records `profile.repository_checks: "not-run"`.
It is not release qualification. The default workflow retains those repository
checks as well.

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

## Explicit partial-observation safety games

`arena.py` accepts `zeno-fcis/arena/1`: a total finite directed graph, roots,
node owners (0 controller, 1 environment), observation strings, and labelled
edges `[action, target]`. At a controller node its action selects an edge;
at an environment node the environment selects an edge. No random choices are
implicit. Equal observations must reveal the same owner and enabled controller
actions. Environment actions are unobserved; introduce intermediate observation
nodes when an event is visible before the response. Represent terminal outcomes
by explicit absorbing self-loops. The objective is `{"kind":"safety","states":[...]}`,
where `states` lists Bad. Set `observation` to `partial` or `perfect`.

The partial route computes the exact knowledge-based subset game. A memory
state contains all concrete states consistent with the observed history.
Controller actions update this belief; the environment selects the next
observation. A belief is unsafe if it contains any Bad state. Strategies read
only the belief obtained from observations and their own preceding actions.
This permits finite observation memory, beyond the legacy observation-table/1
grammar described above. The legacy `reactive` API retains that narrower grammar;
use the explicit arena route for complete sure-safety synthesis.

```sh
python3 -B tools/trace_game/arena.py solve --game test-data/trace-games/observation-memory.json --out ../arena-evidence
python3 -B tools/trace_game/arena.py check --out ../arena-evidence
python3 -B tools/trace_game/certificate_check.py ../arena-evidence certificate.json
```

The independent checker verifies every subset transition, initial belief,
winning closure, observation strategy, losing attractor rank and full certificate
state coverage. It does not run a winning-set solver. Default bounds are 10,000
knowledge vertices (including action vertices), 100,000 edges, 1,000,000 work
units and 30 seconds. Lower them with `--max-states`, `--max-edges`, `--work`,
`--seconds`; exceeding any bound returns UNKNOWN. This route checks the declared
arena; it supplies no native transition provenance or application authority.
Partial-observation omega objectives require richer constructions and are refused.
The solver checks its deadline on entry and after certificate assembly, including
paths needing no fixed-point iterations. `--seconds 0` therefore returns UNKNOWN
with exit code 2 and writes no binding or certified verdict. These checkpoints
do not add fixed-point work units; the independent checker has its own deadline.

Verdicts separate input node IDs from knowledge vertex IDs:

- `winning_concrete_roots` and `losing_concrete_roots` partition the declared
  input roots, in input order. The compatibility fields `winning_roots` and
  `losing_roots` also use concrete IDs in both modes.
- `winning_knowledge_roots` and `losing_knowledge_roots` partition the initial
  knowledge vertices in partial mode. `root_mapping` lists
  `{"knowledge_root": k, "concrete_roots": [...]}` for each initial belief.
  These three fields are empty lists in perfect mode, which has no knowledge graph.

For example, a safe concrete root `[1]` may map to knowledge root `0` with belief
`[1]`. Its verdict reports `winning_concrete_roots: [1]`,
`winning_knowledge_roots: [0]`, and that mapping. In partial mode every concrete
member inherits its initial belief's result under the declared uncertainty;
this does not claim that each losing member would lose if its identity were
revealed. Earlier partial-mode verdicts used knowledge IDs in the compatibility
fields; consumers needing those IDs should use the explicit knowledge fields.

## Explicit omega objectives

The same explicit arena input supports perfect-information reachability,
Büchi, co-Büchi and parity. `objective.states` means the target for reachability,
the infinitely recurring set for Büchi, or the eventual permanent set for
co-Büchi. For parity use `{"kind":"parity","priorities":[...]}`, one integer
0..255 per node: the **greatest** priority occurring infinitely often must be
even. This convention must be converted explicitly when using a minimum-priority
specification. The controller and environment have positional strategies on
this declared arena; a monitor product can carry application history.

Büchi uses priorities 2 on its recurring set and 1 elsewhere. Co-Büchi uses
0 on its permanent set and 1 elsewhere. Zielonka's recursive attractor algorithm
solves these parity games. Reachability uses a ranked attractor. Every returned
certificate partitions the complete arena and gives both sides' positional
choices. The independent checker verifies closure, reachability ranks, and the
absence of adverse cycles at **every** priority threshold. Checking only the
maximum priority of a whole strongly connected component would miss adverse
subcycles. An omitted certificate state or action is refused.

The state, edge, work and time bounds above also apply here; recursion exhaustion
returns UNKNOWN. The independent checker has its own work/time limits and may
refuse a result it cannot finish checking. Quantitative objectives (mean payoff,
energy and discounted rewards) remain deferred to 2.7. Stochastic objectives,
a direct GR(1) syntax, and partial-observation omega objectives remain unsupported;
a supplied deterministic parity-monitor product can express omega-regular
requirements with perfect information. No LTL-to-monitor compiler is provided.

## Native acquisition profiles

`game.py` acquires signed scalar contracts through immutable Authority inputs,
including signed 128-bit values encoded as decimal strings. Unsigned or typed
policy profiles return UNKNOWN with this one-line reason:
`unsigned/typed acquisition requires typed Authority frames; scalar i128 frames are not exact`.
The scalar table framer has no unsigned atom/domain path; converting unsigned
values to signed values would change the declared program, including values
above the signed maximum. The general typed runtime remains a separate route.

Retained adoptions or evolutions return UNKNOWN with:
`retained-lineage acquisition requires the retained programs and receipt chain; base-policy replay is not exact`.
The acquisition request snapshots the project and current policy, not the
retained candidate bytes and their equivalence/evolution evidence. Replaying
only the base policy cannot certify the retained program. These refusals occur
before table generation or publication replay; no fallback certificate is issued.
A future exact route must acquire and bind that full lineage and use typed
frames without narrowing. Explicit arena certificates do not remove this native
provenance requirement. This follows the supervisory-control separation between
a declared plant model and its controller, cited in Background.

## Background

The finite-game methods build on these published sources:

- Alonzo Church, “Applications of Recursive Arithmetic to the Problem of Circuit
  Synthesis,” Summaries of the Summer Institute of Symbolic Logic, 1957.
- J. Richard Büchi and Lawrence H. Landweber, “Solving Sequential Conditions by
  Finite-State Strategies,” Transactions of the American Mathematical Society
  138, 1969.
- Peter J. Ramadge and W. Murray Wonham, “Supervisory Control of a Class of
  Discrete Event Processes,” SIAM Journal on Control and Optimization 25, 1987.
- Amir Pnueli and Roni Rosner, “On the Synthesis of a Reactive Module,” POPL, 1989.
- Robert McNaughton, “Infinite Games Played on Finite Graphs,” Annals of Pure
  and Applied Logic 65, 1993.
- Wiesław Zielonka, “Infinite Games on Finitely Coloured Graphs with Applications
  to Automata on Infinite Trees,” Theoretical Computer Science 200, 1998.
- John H. Reif, “The Complexity of Two-Player Games of Incomplete Information,”
  Journal of Computer and System Sciences 29, 1984.
- Krishnendu Chatterjee, Laurent Doyen, Thomas A. Henzinger and Jean-François
  Raskin, “Algorithms for Omega-Regular Games with Imperfect Information,”
  Logical Methods in Computer Science 3(3:4), 2007.

What is new here: bounded input, evidence packaging and independent local
checks for these established algorithms; no new synthesis algorithm is claimed.
Scope: finite declared arenas, sure safety under partial observation, and
perfect-information reachability and omega objectives under the stated
conventions. This is separate from native contract execution and release
qualification.
