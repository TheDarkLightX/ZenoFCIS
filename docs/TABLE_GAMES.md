# Games and liveness with recursive tables

## Journey
"Can the maintainer always get a change released, however reviews go?" or "Can a user keep withdrawing forever,
whatever the operator does?" These are questions about what one party can *force* while others choose freely. In
ZenoFCIS you declare the game as tables, and a recursive derived table computes the answer, exactly, over the
declared game. Then `check --expand` prints the answer for every state, and laws can state it.

```
zeno-fcis check --expand examples/contract-tables/games/release-pipeline/project.zeno
```

Each `from_<state>` law expands to `true` where the controller wins and `false` where it does not.

## Formal model
A game declares, for each state `i`:
- who moves there (`mover[i]`: 0 for the controller, 1 for the environment);
- whether it is a goal (`goal[i]`);
- its moves, as successor slots `next0[i]`, `next1[i]`, ... (value `j+1`, or 0 for none).

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

## Examples
| Example | Question | Answer |
|---|---|---|
| `games/release-pipeline` | Can the maintainer force a release? | No, from draft: the reviewer can request changes forever. |
| `games/release-pipeline-bounded` | The same with at most two review rounds, then escalation | Yes, from every state. |
| `games/withdrawal-window` | Can a user withdraw infinitely often while the operator may pause? | No: the operator can keep the window paused. |
| `games/withdrawal-window-auto-resume` | The same, where a pause lasts one step | Yes, from every state. |

Each example was compared with an independent Python solver on every state.

## Difference
A hand-written check of "the maintainer can always release" needs either a proof or a search over every behaviour
of the reviewer. Here the table language computes the exact winning region. It is checked by the same expansion,
evaluation and judges as every other table, and the table itself is the certificate. To ask the same question of a
contract's own states, against compromised actors, use the trace game's liveness mode (see
[TRACE_GAME.md](TRACE_GAME.md)).

## Scope
- The game is declared finite data. Turns follow the declared movers; an alternating honest and adversary schedule
  is a fairness assumption, and must be stated.
- `rounds` must equal the number of entries exactly. A table holds at most 64 entries. Constant table IDs must not
  collide with field IDs.
- Expansion grows with the size of the game. One table over every state, with nested quantifiers, reaches the
  expansion limit (10^6 formula nodes) at a few dozen states. Split a game into components that cannot reach each
  other.
- Results are exact over the declared game, and say nothing about behaviour outside it.

## Background
- Least fixed points of logic programs: van Emden and Kowalski, "The Semantics of Predicate Logic as a Programming
  Language", JACM 1976.
- Stratified negation: Apt, Blair and Walker, "Towards a Theory of Declarative Knowledge", 1988.
- Games on graphs: Büchi and Landweber, "Solving Sequential Conditions by Finite-State Strategies", Trans. AMS 1969;
  McNaughton, "Infinite Games Played on Finite Graphs", APAL 1993; Zielonka, "Infinite Games on Finitely Coloured
  Graphs", TCS 1998.
- Nested fixed points for Büchi games: Emerson and Jutla, "Tree Automata, Mu-Calculus and Determinacy", FOCS 1991.
