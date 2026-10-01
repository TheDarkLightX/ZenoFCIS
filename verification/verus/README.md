# Verus integration pilot

Status: both functions pass the pinned verifier with three obligations. The
ordinary Rust boundary tests pass, and four deliberate mutations are caught.
The no-std authority build also passes. GitHub CI remains a separate check.

The pilot verifies the actual finite-domain arithmetic used by the optional
decision-contract interpreter. It keeps the application's Rust 1.97.1 build
and runs the pinned Verus release with its separate Rust 1.98.1 toolchain.
Specifications are enabled only while Verus checks the shared Rust source.

## Definition of done

- Check the same function bodies compiled into `zeno-fcis-authority`.
- Prove exact success and refusal conditions for interval cardinality and
  bounded products over all machine inputs, without application assumptions.
- Require `--no-cheating`; verify that deliberate arithmetic mutations fail.
- Pin the verifier archive and toolchain; record source hashes, command,
  verification result, trusted dependencies, and uncovered obligations.
- Compile the shared code with Rust 1.97.1, including the no-std crate path.
- Preserve the existing finite-input limit and admitted-input behavior.
- Document lessons, compatibility, follow-up work, and the fork policy.

## Proof sketch

1. A signed 64-bit interval's mathematical width fits signed 128-bit
   arithmetic. Accept precisely positive widths representable by `u64`.
2. The product of two `u64` values fits `u128`. Accept precisely products at
   most the supplied `u64` limit; only then narrow the result.
3. Specify both `Some` and `None`, so an implementation that rejects every
   input cannot satisfy the contracts.
4. Keep the executable bodies independent of verifier configuration; only
   specification attributes, imports, and erased proof blocks depend on it.
5. Mutation checks exercise off-by-one, narrowing/limit, and specification
   omission failures. Integration tests retain complete template behavior.

This verifies two admission calculations. The enumeration loop, decoder,
complete decision interpreter, resource meter, law engine, genesis, canonical
encoding, and shell remain separate obligations. Verus/Z3 verification has its
own trusted base and does not produce ZenoFCIS's Lean `KernelChecked` evidence.

## Reproduce

This profile is qualified for Linux x86-64. Install both toolchains:

```sh
rustup toolchain install 1.97.1 --profile minimal
rustup toolchain install 1.98.1 --profile minimal
python3 tools/test_check_verus.py
python3 tools/check_verus.py --install --out /tmp/zeno-fcis-verus-evidence.json
cargo +1.97.1 check -p zeno-fcis-authority --no-default-features --locked
```

`--install` downloads upstream release `0.2026.09.27.3cf1832`, checks its
archive size and SHA-256, and extracts nine explicitly pinned runtime files.
Subsequent runs can omit `--install`. The checker rechecks those files before
and after execution and pins the bundled Z3 rather than using a system solver.
The archive, source commit, Rust versions, and individual tool file hashes are
recorded in [toolchain.json](toolchain.json).

[domain_bounds.rs](domain_bounds.rs) imports the actual authority source by
path. Its ordinary Rust tests use the same file. The runtime calls both
functions while admitting finite contracts. The interpreter's evaluator
identity includes both its own source and the extracted helper source using
a canonical source tuple under `finite-decision-evaluator` version 2.
Existing persisted identities are therefore not interchangeable with this
revision; no migration or old-history compatibility is claimed.

The receipt records exact source hashes, the Git revision and dirty state,
commands, tool versions, proof coverage, native tests, and four mutations:
wrong interval width, rejecting a product exactly at its limit, bypassing the
product limit, and omitting one function's specification. The last specimen
passes its reduced Verus check but is refused by the coverage gate.

The three verification obligations comprise two functions and one explicit
nonlinear arithmetic assertion. They are not three application properties.
The hypotheses come from machine integer types; the functions have no
application preconditions or application `assume`/`external_body` declarations.
Verus, its imported library specifications, Z3, Rust compilation/erasure, and
the host remain trusted.

The regular acceptance suite runs the native and evidence-admission tests.
The separate [Verus workflow](../../.github/workflows/verus.yml) runs the pinned
solver and all mutations. Passing ordinary ATDD alone does not imply a Verus
proof passed. The owner fork is an extension workspace; this integration uses
the pinned upstream release and makes no claim about a fork-built verifier.

See [lessons and V2 plan](../../docs/VERUS_LESSONS_AND_V2_PLAN.md) for the
complete follow-up scope and evidence policy.
