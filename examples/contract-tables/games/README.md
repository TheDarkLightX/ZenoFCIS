# Game examples for recursive tables

Six declared games and two questions. Each question has a variant that fixes the problem and a variant that
answers it under a fairness assumption about the other side. See
[docs/TABLE_GAMES.md](../../../docs/TABLE_GAMES.md) for the model and the scope.

```
zeno-fcis check --expand examples/contract-tables/games/<example>/project.zeno
```

The `from_<state>` laws print `true` where the controller (mover 0) can force the goal, and `false` where it cannot.
