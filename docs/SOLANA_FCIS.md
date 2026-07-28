# Solana / Anchor FCIS backend

The Solana backend renders the shared `OnchainMachineSpec` into a modular Anchor workspace with two different trust domains:

```text
reviewed machine model
    -> dependency-free no_std core crate
    -> Anchor shell with exact accounts and PDA constraints
    -> bounded validated event/effect plan
    -> atomic account commit
    -> typed token CPI through exact bound accounts
    -> transition receipt
```

## Pure core boundary

The generated functional core is a separate crate with:

- `#![no_std]`;
- `#![forbid(unsafe_code)]`;
- no dependencies;
- fixed-size state, command, context, decision, event, and effect values;
- stable reason, event, and capability IDs;
- generated typed event/effect constructors;
- a single agent-editable `project.rs` implementation.

The default implementation is inert: command admission and initialization invariants return false. A generated workspace therefore cannot initialize or move assets until project logic is deliberately implemented. This fail-closed starter is intentional: forgetting to replace or review the project core produces an unusable program rather than a permissive one.

The source policy rejects Anchor/Solana runtime access, account types, CPIs, remaining accounts, host I/O, randomness, floats, unsafe code, panic paths, and incomplete implementation macros in the agent-authored core.

## Anchor shell boundary

The generated program owns:

- an exact program ID and machine hash;
- a state PDA derived from a reviewed fixed seed and authority;
- exact account space and fixed-size account data;
- actor signatures and shell-captured `Clock` context;
- expected-state-hash and sequence preconditions;
- recomputed state-root corruption checks;
- decision, reason, invariant, plan-bound, canonical-order, and inactive-slot validation;
- checked sequence updates;
- SHA-256 state, command, context, event/effect plan, and candidate receipts;
- generic committed domain events only after successful effects;
- no `UncheckedAccount`, `remaining_accounts`, account resizing, closing, or upgrade-loader instruction surface.

## Fungible token capability

Each shared `FungibleTransfer` capability is bound to:

- an exact mint public key;
- a closed `SolanaTokenProgram` choice whose v3 surface admits only the canonical legacy SPL Token program;
- an exact source vault token account;
- the state PDA as vault authority;
- a destination token account whose owner must equal the normalized recipient selected by the reviewed capability policy;
- a shared asset ID, amount ceiling, and per-transition use ceiling.

The shell uses Anchor's typed token-interface `TransferChecked` CPI with PDA signer seeds. It does not accept arbitrary CPI program IDs, instructions, account metas, or remaining accounts. Token-2022 is intentionally rejected in v3: transfer-fee and transfer-hook extensions change net-value and external-effect semantics and require a separate reviewed profile.

All capability accounts are present in the fixed `Execute` account set even when a particular transition does not use them. This improves inspectability and avoids dynamic account-list authority, at the cost of transaction size and account-loading overhead.

## Solana-specific threat model

The generated boundary addresses common classes of account substitution, signer confusion, stale-state execution, arbitrary CPI, token-program substitution, mint substitution, vault substitution, dynamic account authority, and unbounded plan growth.

It does not by itself prove:

- business or economic correctness;
- absence of account-lock denial of service;
- compute-budget safety for every legal state;
- resistance to transaction ordering, priority-fee, oracle, or MEV effects;
- absence of direct self-recursion in separately added code;
- safety of upgrade authority, governance, or deployment operations;
- deterministic reproducibility of an unverified local build;
- audit completion.

## Validation layers

Repository Rust CI validates the `zeno-fcis-solana-anchor/3` generator implementation, deterministic rendering, source-policy checks, and public API integration. Initialization requires the authority to be both a signer and a System Program-owned account, matching the authority type admitted by later execution.

The retained generated-workspace gate runs host `cargo check --workspace --all-targets --locked` against the exact retained dependency graph. It does not invoke Anchor CLI or `build-sbf`, produce SBF/ELF, execute instructions, establish a verified build, or bind a deployed program. The `solana=3.1.10` manifest field records the declared target-toolchain family; the current host gate does not exercise that toolchain. See the [execution and sandbox boundary](EXECUTION_SANDBOX_BOUNDARY.md).

## Required promotion evidence

Before a generated program is production-authorized, retain at minimum:

1. exact Anchor, Solana/Agave, Rust, crate, and action pins;
2. clean generator and core/shell source-policy reports;
3. exact workspace and generated-file digests;
4. unit and property tests for the pure core;
5. LiteSVM or Mollusk instruction tests;
6. `solana-program-test` or equivalent integration tests for token effects and failure atomicity;
7. compute-unit ceilings over boundary and adversarial cases;
8. mutation tests for account constraints and effect validation;
9. verified-build evidence and deployed program-data hash comparison;
10. explicit upgrade-authority disposition, preferably a reviewed multisig/timelock or final removal where appropriate;
11. independent security review.

The generator pins Anchor `1.0.2` and `solana-sha256-hasher` `3.1.0`, and records Solana/Agave `3.1.10` in its manifest. The hasher's `sha2` feature supports deterministic host compilation and testing while the Solana target uses the platform SHA-256 syscall. These pins are evidence inputs, not a claim that a generated program has passed instruction-level, verified-build, or deployment verification.
