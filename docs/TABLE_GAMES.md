# Games and liveness with recursive tables

## Journey
"Can the maintainer always get a change released, however reviews go?" or "Can a user keep withdrawing forever,
whatever the operator does?" Often the honest answer is "only if the other side is fair": "Can users keep
withdrawing, provided the operator reopens the window now and then?" These are questions about what one party can *force* while others choose freely. In
ZenoFCIS you declare the game as tables, and a recursive derived table computes the answer, exactly, over the
declared game. Then `check --expand` prints the answer for every state, and laws can state it.

```
zeno-fcis check --expand examples/contract-tables/games/release-pipeline/project.zeno
```

Each `from_<state>` law expands to `true` where the controller wins and `false` where it does not. In the
fairness examples, read the `stable` law first: the answers count only when it expands to `true`.

## Recurrence table: some run

"Does some run from this state visit a good state infinitely often?" is a simpler question than a game.
For this question on a finite domain, use a **recurrence table**: one binary least-fixed-point table `path[i][j]`
for paths with at least one move. Its rule is a direct edge from `i` to `j`, or an edge from `i` to some `k`
followed by `path[k][j]`. Starting from false excludes zero-step paths. The query is
`exists a { good[a] && path[s][a] && path[a][a] }`, with the quantifier over the state table.

This is the standard lasso argument for finite Büchi acceptance: an infinite good run visits some good state
repeatedly, so there is a path to it and a nonempty walk back to it. Conversely, repeat that return walk forever.
The recurrence table applies to the **some run** form on finite domains only. For **every run** and for games,
use the layered construction below with the appropriate predecessor. It does not follow that any party can
force the run found by the recurrence table.

```
zeno-fcis check --expand examples/contract-tables/games/recurrence/project.zeno
zeno-fcis check --expand examples/contract-tables/games/recurrence-reflexive-trap/project.zeno
```

In `recurrence`, entry leads to good, good leads to return, and return leads back to good. An isolated state is
also good, but has no move. The expanded laws `from_entry`, `from_good`, and `from_return` evaluate to `true`;
`from_isolated` evaluates to `false`.

The negative example `recurrence-reflexive-trap` has just one isolated good state. Its deliberately wrong rule
adds `i == j`, so every state reaches itself in zero steps. The checker exposes the problem by printing
`law 500 from_isolated on commit = (1 == 1 && true && true);`, which is true although no infinite run exists.
Expansion checks the supplied rule;
it does not validate its intended meaning. The regression compares nonempty paths against independent simple
lasso enumeration on all 4,164 graphs and good-state sets with 1–3 states, including dead ends.

## Formal model for games
A game declares, for each state `i`:
- who moves there (`mover[i]`: 0 for the controller, 1 for the environment);
- whether it is a goal (`goal[i]`);
- its moves, as successor slots `next0[i]`, `next1[i]`, ... (value `j+1`, or 0 for none);
- for a fairness question, whether the environment's assumption holds there (`fair[i]`).

The controllable predecessor of a set X holds at state i when one of these holds:
- the controller moves at i and some move leads into X;
- the environment moves at i, has at least one move, and every move leads into X.

- **Reachability:** the controller can force a goal. This is the least fixed point
  `win = goal ∨ CPre(win)`, the attractor. It is one derived table, positive in `win`, and inside the safe fragment
  of [recursive tables](CONTRACT_TABLES.md#recursive-tables).
- **Recurrence:** the controller can force goals infinitely often (Büchi). This needs a greatest fixed point
  outside a least one. The outer chain shrinks and stabilizes within N steps on N states, so it is unrolled into N
  strata: `z1 .. zN`, where each `zk` is the least fixed point of `CPre(zk) ∨ (goal ∧ CPre(z(k-1)))`, and `z1`
  starts from every state. Each stratum reads only the one below it.
- **Fairness:** the controller can force goals g1..gm infinitely often, provided the environment makes assumptions
  a1..an hold infinitely often (a GR(1) game). If the environment breaks an assumption, the controller wins. The
  winning region is `νZ. ⋀j μY. ⋁i νX. (gj ∧ CPre(Z)) ∨ CPre(Y) ∨ (¬ai ∧ CPre(X))`. Each inner greatest
  fixed point is the complement of one recursive table, `wi = μW. ¬A ∧ (ai ∨ OPre(W))`, where
  `OPre(W) = ¬CPre(¬W)` is what the environment can force. The two outer chains settle in a few steps, so a solver
  proposes how many strata each needs and the tables build exactly those. The `stable` law checks that the last
  stratum of each chain equals the one before it: an iteration that repeats has reached its fixed point, so the
  answer is exact when `stable` expands to `true`. `check --expand` prints `stable` but does not refuse when it is
  `false`: read `stable` first, and use the `from_` answers only when it is `true` (the guide test and ZAL's
  `export-game` both enforce this). The complements of the
  outer strata are positive in the `w` tables, so no recursive table negates another. A last, non-recursive `win`
  table negates the top stratum.

## Examples
| Example | Question | Answer |
|---|---|---|
| `games/release-pipeline` | Can the maintainer force a release? | No, from draft, in review or changes requested: the reviewer can request changes forever. Yes, from approved and released. |
| `games/release-pipeline-bounded` | The same with at most two review rounds, then escalation | Yes, from every state. |
| `games/withdrawal-window` | Can a user withdraw infinitely often while the operator may pause? | No, from every state: the operator can keep the window paused. |
| `games/withdrawal-window-auto-resume` | The same, where a pause lasts one step | Yes, from every state. |
| `games/withdrawal-window-fair` | The same as the first withdrawal question, assuming the operator reopens the window infinitely often | Yes, from every state (No without the assumption). |
| `games/release-pipeline-fair` | Can changes be released infinitely often, assuming the reviewer approves infinitely often? | Yes, from every state (No without the assumption). |

Each example was compared with an independent Python solver on every state. The fairness encoding was also
compared on 1,576 games (every two-state game in which every state has a move, with one assumption and one
guarantee, and 1,000 random games of 3 to 8 states with up to two of each, again with a move from every state): the tables, a direct nested fixed point and the trace game's parity solver agreed on
every state, and `stable` held. Every certificate from the parity solver was replayed by the trace game's
independent checker. Proposing one stratum too few was refused by `stable` in all 876 games tried; ignoring the
assumption produced disagreements.

## Difference
Without this, a check of "the maintainer can always release" needs either a proof or a search over every behaviour
of the reviewer. Here the table language computes the exact winning region. It is checked by the same expansion,
evaluation and judges as every other table, and the table itself is the certificate.

## Scope
- The game is declared finite data. Turns follow the declared movers; an alternating honest and adversary schedule
  is a fairness assumption, and must be stated.
- Every state has at least one move; a game with dead ends needs its own terminal convention.
- An assumption is a set of states the environment visits infinitely often (justice). A single "if asked
  infinitely often, answered infinitely often" is exactly this form (assumption: asked; goal: answered). Several
  such request and answer pairs that must each hold separately (a general Streett condition) are not expressible
  this way.
- Each fairness table quantifies over every state in every round, so expansion grows quickly with the number of
  states, and whether a game fits depends on its moves. In a replayable sample of five random games per size, with
  one guarantee and one assumption every game up to 24 states expanded, two of five at 28 and none at 32; with two of
  each, every game up to 16 states, two of five at 20 and none at 24. Each took at most about two seconds, and every
  expanded answer agreed with both independent solvers. A game that does not fit is refused at the formula-node
  limit; it is never answered wrongly.
- `rounds` must equal the number of entries exactly. A table holds at most 64 entries. Constant table IDs must not
  collide with field IDs.
- Expansion grows with the size of the game. One table over every state, with nested quantifiers, reaches the
  expansion limit (10^6 formula nodes) at a few dozen states. Split a game into components that cannot reach each
  other.
- Results are exact over the declared game, and say nothing about behaviour outside it.

## Background
- Acceptance of infinite sequences: J. R. Büchi, "On a Decision Method in Restricted Second Order Arithmetic",
  Proceedings of the 1960 International Congress for Logic, Methodology and Philosophy of Science, 1962,
  pp. 1–11. The recurrence table uses the standard finite accepting-lasso argument; no new method is claimed.
- Least fixed points of logic programs: van Emden and Kowalski, "The Semantics of Predicate Logic as a Programming
  Language", JACM 1976.
- Stratified negation: Apt, Blair and Walker, "Towards a Theory of Declarative Knowledge", 1988.
- Games on graphs: Büchi and Landweber, "Solving Sequential Conditions by Finite-State Strategies", Trans. AMS 1969;
  McNaughton, "Infinite Games Played on Finite Graphs", APAL 1993; Zielonka, "Infinite Games on Finitely Coloured
  Graphs", TCS 1998.
- Nested fixed points for Büchi games: Emerson and Jutla, "Tree Automata, Mu-Calculus and Determinacy", FOCS 1991.
- Fairness assumptions (GR(1)): Piterman, Pnueli and Sa'ar, "Synthesis of Reactive(1) Designs", VMCAI 2006.
