# Contributing

Changes should preserve the following direction:

```text
bytes -> bounded canonical values -> pure transition -> immutable decision
      -> exact commit bundle -> imperative interpreter
```

Protocol-visible behavior requires:

- a stable schema and version;
- canonical ordering and encoding;
- typed rejection or failure semantics;
- explicit resource bounds;
- tests or proofs for the affected laws;
- an honest list of nonclaims.

Do not introduce `unsafe`, ambient I/O, wall-clock reads, randomness, floating
point, unordered protocol iteration, executable closures inside effect plans,
or shell-side recalculation of domain semantics into semantic-kernel crates.
