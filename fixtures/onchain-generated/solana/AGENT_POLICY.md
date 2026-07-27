# Solana agent policy for TreasuryMachine

Machine hash: `bd5c612ad6484fa025e8f2b6dee9a868a9a54bd7beb8c2904760011b854039de`
Program ID: `cGfHiC6Kgg3FpFZvgwGcswsCRtp4aBP2fzuXRQPizuN`

An agent may edit only `crates/treasury_machine-core/src/project.rs` and tests. The pure crate has no dependencies, is `#![no_std]`, and forbids unsafe Rust. Regenerate all shell, account, PDA, token-binding, hashing, receipt, and manifest files.

Forbidden without a new reviewed generator profile: `UncheckedAccount`, `remaining_accounts`, raw `invoke` or `invoke_signed`, arbitrary CPI program IDs, dynamic account resizing or closure, upgrade-loader instructions, host I/O, randomness, floats, panic paths, and direct shell edits.

Required before production authorization: Anchor/Solana exact-version compilation, LiteSVM or Mollusk tests, program-test integration tests, compute ceilings, mutation and property tests, verified builds, deployed-program hash verification, explicit upgrade-authority disposition, independent review, and deployment-specific token-account checks.
