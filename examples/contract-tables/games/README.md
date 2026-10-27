# Game examples for recursive tables

Six declared games and two questions. Each game question has a variant that fixes the problem and a variant that
answers it under a fairness assumption about the other side. See
[docs/TABLE_GAMES.md](../../../docs/TABLE_GAMES.md) for the model and the scope.

```
zeno-fcis check --expand examples/contract-tables/games/<example>/project.zeno
```

The `from_<state>` laws print `true` where the controller (mover 0) can force the goal, and `false` where it cannot.
In the fairness examples, read the `stable` law first: the answers count only when it prints `true`.

The `recurrence` example asks whether **some run** visits a good state infinitely often, using a binary nonempty
path table. Its answers are true from entry, good and return, and false from the isolated good state.
`recurrence-reflexive-trap` is a negative example: a zero-step path wrongly makes an isolated good state's answer
evaluate to true. These are finite path questions; every-run questions and games use the layered construction in the guide.
Background: J. R. Büchi, "On a Decision Method in Restricted Second Order Arithmetic", Proceedings of the 1960
International Congress for Logic, Methodology and Philosophy of Science, 1962, pp. 1–11; the standard finite
lasso argument. The recurrence table is a checked example of that method.
