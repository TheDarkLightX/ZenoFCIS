# Agent policy for TreasuryMachine

Machine hash: `bd5c612ad6484fa025e8f2b6dee9a868a9a54bd7beb8c2904760011b854039de`

An agent may implement only `_commandAdmissible`, `_invariant`, `_decide`, and tests in a derived contract. Regenerate the base instead of editing it. Use generated `_event<Name>` and `_effect<Name>` builders.

Forbidden without a new reviewed generator profile: raw `.call`, `.delegatecall`, arbitrary calldata, assembly, unchecked arithmetic in the pure core, upgrade hooks, new token bindings, dynamic storage, and direct writes to generated storage.

Required before production authorization: exact solc compilation, compiler-known-bug review, source digest retention, unit/property/invariant tests, static analysis, formal analysis proportional to value at risk, independent review, deployment binding verification, and post-deployment code-hash verification.
