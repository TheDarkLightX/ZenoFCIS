# Fiat-Crypto compatibility and assurance boundary

Inspected 2026-10-08. Upstream subject:
[`mit-plv/fiat-crypto` at `7049792ee1b95c4f47607f6961baa398bc456194`][upstream].
ZenoFCIS subject: `8ace37e3d13edb3efe3c45761a4364bd25b840ad`.
This G5 report inspects source and assurance documentation. It does not replay
Rocq/Coq proofs, generate code or test compatibility. **No adoption occurred.**

**Recommendation: defer adoption.** Consider a separate evaluation only when
a concrete modular cryptographic primitive is required. Fiat-Crypto does not
solve G3's checked financial arithmetic requirement; adopting it would add
representation and compiler obligations without satisfying that contract.

## Arithmetic compatibility

Fiat-Crypto synthesizes cryptographic arithmetic for specified moduli and
representations. The inspected generated Rust example represents the field
modulo `2^255 - 19` using five limbs. Its multiplication contract equates the
decoded output with the product *modulo the field modulus*, under documented
limb bounds. Public array wrappers do not themselves enforce those bounds.
The appearance of `u128` intermediates does not make this a checked U128
accounting API. [Generated field types and multiplication][rust-example]

ZenoFCIS's actual [atom operations](../crates/zeno-fcis-synthesis/src/finite/execution_v2/laws/atoms.rs)
use `checked_add`, `checked_sub` and `checked_mul` on I128/U128 and return
`Failure::Undefined` on overflow/underflow. The
[mathematical specification](../crates/zeno-fcis-synthesis/src/finite/execution_v2/laws/spec.rs)
requires the integer result to remain in range. The
[scalar evaluator](../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs)
likewise returns `Failure::Arithmetic` for overflowing i64 addition/subtraction.
Eager predicates also charge the shared meter before evaluating a node.
[Predicate execution](../crates/zeno-fcis-synthesis/src/finite/execution_v2/laws/predicate.rs)

For example, financial `0_u128 - 1` refuses; in a field modulo `p`, `0 - 1`
represents `p - 1`. Checked `u128::MAX + 1` refuses, whereas arithmetic modulo
`2^128` would return zero. The latter illustrates wrap semantics, not a claim
that every Fiat-Crypto generator accepts that modulus. Neither modular result
preserves ZenoFCIS's trap/refusal behavior, ordering or resource observations.
Fiat-Crypto is therefore **not a drop-in replacement for balances, accounting
conservation laws, amounts, fees or full-width zUSD arithmetic**.

## What is proved, and where the proof stops

| Stage | Evidence in the inspected upstream subject | Boundary |
| --- | --- | --- |
| Arithmetic specification to internal expression | `WordByWordMontgomery` includes correctness lemmas for multiplication, squaring, addition, subtraction, negation and representation conversions, conditional on valid parameters and successful synthesis. [Synthesis proofs][synthesis] | The modulus, representation, validity predicate and operation specification must match the intended primitive. |
| Internal expression transformations | `BoundsPipeline` composes partial evaluation, arithmetic rewriting, bounds analysis, cast-related rewrites, dead-code elimination and substitution/inlining. `BoundsPipeline_correct` establishes preserved interpretation, output bounds and well-formedness for a successful result, with well-formed input, bounded arguments, type and translation premises. [Pipeline definitions and theorem][pipeline] | This theorem concerns internal expressions under its premises; it does not prove arbitrary emitted source or an application. |
| Bedrock2 AST | The inspected backend has function-correctness lemmas, such as `mul_func_correct`, connecting the generated function to the field operation under its validity, environment and memory/specification premises. [Bedrock2 proof][bedrock] | An AST-level theorem is not proof of printed C text or a shipped native binary. |
| C/Rust and other textual backends | Upstream explicitly says only the Bedrock2 backend has the internal-AST correspondence proof; ordinary C/Rust backends lack it. Even Bedrock2's C string printing is unproved. [Backend assurance statement][backends] | Language casts, printing and downstream compilation remain outside those proofs. |

The actual [Rust printer][printer] constructs target-language strings; the
[OCaml extraction entry][extraction] extracts the generator entry point.
For an extracted generator run, the extraction path, OCaml compiler/runtime,
generator inputs and artifact handling require trust or separate validation.
Then rustc/LLVM (or the selected C toolchain), optimization flags, target integer
semantics, linking, ABI, memory layout, calling wrappers and execution platform
remain additional steps. No compiled end-to-end chain was qualified here.

The pinned README reports build checks and generated-code testing for Rust,
including Dalek tests, and for C/Bedrock2 through BoringSSL. Those are upstream
testing statements, not tests run for this report or proofs of a ZenoFCIS
integration. They do not remove the documented backend gaps. [Backend table][backends]

## Named assurance boundary: proved arithmetic AST to deployed primitive

A future integration must bind a concrete operation, modulus, word size, limb
bounds, canonical encoding, and any Montgomery conversion to its exact
theorem. Caller validation, serialization and invalid-input refusal need their
own contracts. The wrapper must preserve ZenoFCIS's immutable inputs and owned
scratch, meter behavior and authority separation. The inspected generated
output accepts caller-provided mutable output buffers; that interface alone
does not establish these integration properties. [Generated interface][rust-example]

Functional arithmetic correctness also does not establish timing/side-channel
security of a chosen compiled artifact, key management, protocol security,
financial laws, external delivery or whole-application correctness. A new
dependency or generated source would need the existing source-identity and
proof-gate review; this report creates no theorem inventory entry,
`KernelChecked` evidence, adoption permission or release authority.

## License and future trial

The inspected [COPYRIGHT][copyright] offers **MIT OR Apache-2.0 OR BSD-1-Clause**.
The actual [MIT][mit], [Apache notice][apache] and [BSD-1-Clause][bsd] files were
read; the Apache file refers to the full Apache 2.0 terms. The generated Rust
package declares the same choices. [Package manifest][manifest]

MIT is a straightforward candidate for a separately reviewed integration with
ZenoFCIS's [MIT-or-Apache package](../Cargo.toml), with the upstream copyright
and permission notice retained. This assesses the inspected project's offered
license, not every bundled submodule, build tool, dependency or eventual binary;
the exact chosen distribution would require its own notice/license inventory.

If a real cryptographic requirement arises, a future bounded trial should:

1. Fix one operation, modulus, representation, allowed input bounds and backend;
   pin its upstream/submodule revisions, Rocq/Coq and extraction toolchain,
   generation arguments, generated bytes and native compiler/target/flags.
2. Replay the relevant synthesis/pipeline proofs and list remaining axioms and
   compiler assumptions. Check the actual generated wrapper's bounds and byte
   encoding against an independent mathematical oracle, including malformed
   inputs and planted arithmetic/serialization defects.
3. Measure latency, code size and build cost against the existing selected
   primitive, with reproducible inputs and all failures reported. Review the
   compiled target's side-channel requirements separately if secrets are used.
4. Seek an explicit adoption decision only after the complete source-to-artifact
   boundary and incremental benefit are reviewed. Keep financial arithmetic
   under its checked-integer contract.

These are proposed prerequisites, **not completed experiments**. No performance,
binary compatibility, constant-time or whole-app proof claim follows here.

[upstream]: https://github.com/mit-plv/fiat-crypto/tree/7049792ee1b95c4f47607f6961baa398bc456194
[rust-example]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/fiat-rust/src/curve25519_64.rs#L48-L217
[synthesis]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/src/PushButtonSynthesis/WordByWordMontgomery.v#L877-L930
[pipeline]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/src/BoundsPipeline.v
[bedrock]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/src/Bedrock/Field/Synthesis/New/WordByWordMontgomery.v#L354-L384
[backends]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/README.md#status-of-backends
[printer]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/src/Stringification/Rust.v
[extraction]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/src/ExtractionOCaml/fiat_crypto.v
[copyright]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/COPYRIGHT
[mit]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/LICENSE-MIT
[apache]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/LICENSE-APACHE
[bsd]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/LICENSE-BSD-1
[manifest]: https://github.com/mit-plv/fiat-crypto/blob/7049792ee1b95c4f47607f6961baa398bc456194/fiat-rust/Cargo.toml
