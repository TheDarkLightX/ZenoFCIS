# ZenoFCIS architecture

ZenoFCIS separates protocol meaning from runtime mechanism.

## Dependency rings

1. `zeno-fcis-core`: decisions, budgets, transition traits, stable reasons.
2. `zeno-fcis-value`: closed transitively immutable values and bounded owners.
3. `zeno-fcis-codec`: canonical encoding and commitment-provider interfaces.
4. Later rings add patches, plans, receipts, composition, synthesis, proof
   adapters, authenticated state, and imperative-shell reference semantics.

A lower ring never imports a higher ring. The semantic kernel is `no_std +
alloc`, forbids unsafe Rust, and has no ambient I/O.

## Isolation boundary

The semantic kernel restricts protocol-authoritative observations and outputs.
It does not sandbox the process running a compiler, generator, mounted runtime,
backend engine, database shell, or effect interpreter. See the
[execution and sandbox boundary](EXECUTION_SANDBOX_BOUNDARY.md).
