# Game examples for recursive tables

Four declared games, two questions, each with a variant that fixes the problem. See
[docs/TABLE_GAMES.md](../../../docs/TABLE_GAMES.md) for the model and the scope.

```
zeno-fcis check --expand examples/contract-tables/games/<example>/project.zeno
```

The `from_<state>` laws print `true` where the controller (mover 0) can force the goal, and `false` where it cannot.
