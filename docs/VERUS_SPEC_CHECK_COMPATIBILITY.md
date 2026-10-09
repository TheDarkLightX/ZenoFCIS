# verus-spec-check compatibility and assurance boundary

Inspected 2026-10-08. Upstream subject:
[`verus-lang/verus-spec-check` at `44cde1b910571a7c0fb5a20a01d0eb88d15ef446`][upstream].
ZenoFCIS subject: `8ace37e3d13edb3efe3c45761a4364bd25b840ad`.
This G5 report is a source inspection, not a build, compatibility trial or proof
replay. **No adoption occurred.**

**Recommendation: evaluate in an isolated development experiment; do not adopt
as an acceptance gate or runtime dependency now.** Testing specification
adequacy is useful, but the release pairing, source adapter and result
classification below need qualification first. Adoption is a separate decision.

## What the pinned tool does

Annotations generate executable checks of Verus contracts, including function
contracts, assertions and assumed specifications. The default backend is
proptest; Bolero supplies fuzzing and Kani modes. Mutation coverage changes
implementation bodies and tests whether contracts detect them. Branch coverage
searches for inputs admitted by preconditions and distinguishes reachability
from engagement of postconditions. These are useful bug-finding mechanisms;
coverage percentages do not establish that a specification captures the owner's
intent. [Upstream overview][readme]

This is auxiliary specification testing, distinct from the production Verus
proof path. A Kani result concerns its generated harness, assumptions and
configured bounds; it does not automatically become a Verus theorem or a
ZenoFCIS certificate. The tool lowers specifications to executable companions,
constructs generators and emits test modules through an unverified macro
pipeline. No semantics-preservation theorem for that pipeline was established
by this inspection. [Architecture and expansion stages][architecture]

Supported constructs still need executable models. External specification
functions may need trusted stubs; opaque views need sampling/injection and
realization functions; generics need concrete test instantiations. Documented
limitations include loop-invariant testing, `external_trait_specification`,
prophetic specifications, asynchronous and dynamic types. Some mutable-return
and generic-receiver shapes need wrappers. This rules out assuming automatic
coverage of the entire core. [Usage and limitations][usage]

## Compatibility with ZenoFCIS

The inspected [toolchain pin](../verification/verus/toolchain.json) selects
Verus `0.2026.09.27.3cf1832`, commit
`3cf18325f0fd0c3040fbdec8c0f2255c0504c91a`, verifier Rust 1.98.1 and runtime
Rust 1.97.1. [check_verus.py](../tools/check_verus.py) hashes the pinned
tool files, invokes `--no-cheating`, checks complete-crate results and coverage,
and retains native and deliberate-defect checks. The
[Verus workflow](../.github/workflows/verus.yml) runs the separate proof gates.
This report inspected those mechanisms; it did not rerun or certify them.

By contrast, the external tool's [workspace manifest][manifest] is paired with
`0.0.0-2026-09-20-0158`. Its documented setup patches
`verus_builtin_macros` with an overlay and requires matching Verus/vstd versions.
The extra `vstd_ext` support also reaches non-test paths. This is an unresolved
compatibility mismatch, not evidence that the combination cannot work.
[Setup][readme], [dependency and version caveats][usage]

ZenoFCIS's [finite bounds](../verification/kernel-laws/src/oracle/authority/finite_bounds.rs)
and [evaluator](../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs)
use `cfg_attr(verus_keep_ghost, verus_spec(...))` on ordinary shared Rust bodies;
the external examples annotate items in `verus!`. A proposed wrapper must
preserve the exact subject and contracts, including refusal cases, rather than
silently test a rewritten copy. A workspace dependency or macro change would
also need the existing identity and proof-gate review. A useful first subject
is `bounded_product`: its exact success/refusal contract makes omitted overflow
or limit conditions observable.

## Mutants, vacuity and result interpretation

- The mutation runtime has `Killed`, `Survived` and `Inconclusive` outcomes;
  the last is excluded from the percentage denominator. Skipped targets and
  targets without mutation sites are separate report cases. Thus a percentage
  alone omits material coverage information. [Runtime accounting][mutation]
- **Static concern, not reproduced here:** the emitted mutant runner uses 64
  cases and a 65,536 global rejection limit, then maps *any* `run_result.is_err()`
  to `Killed` before its no-admitted-input check. This includes the error path
  intended to count body panics; it does not distinguish a runner abort from a
  postcondition violation. The runtime's documented inconclusive intent is
  therefore insufficient evidence that rejection exhaustion is classified
  correctly. Require a separate abort control before trusting a kill rate.
  [Actual runner and outcome order][runner]
- Proptest filters preconditions. Bolero's generated post-run guard detects
  all-skipped sampling in ordinary test runs, but the source says it does not
  fire for a non-returning coverage-guided run and is disabled under Kani,
  where preconditions become assumptions. Separately establish that the
  constrained domain is nonempty. These guards are not a general proof of
  non-vacuity. [Harness guards][vacuity]
- A survivor may be equivalent, expose a weak contract, or simply lack a
  sampled witness. A killed mutant may instead panic. Preserve and replay
  concrete witnesses against the unchanged subject; classify errors, skips
  and equivalent mutants separately. Never promote a timeout, compilation
  failure or empty harness to success.

## Named assurance boundary: contract-to-test translation

Trust for an auxiliary result includes annotation discovery, specification
lowering, custom executable stubs, generators, mutation/instrumentation code,
backend, result accounting, Rust compilation and the host. The optional
pretty-printer renders token streams for human inspection and snapshots; the
compiled harness comes from macro tokens. Printed expansion text and terminal
coverage reports are diagnostics, not proof certificates or source-to-binary
evidence. [Pipeline][architecture], [diagnostic printer][printer]

The existing Verus/Z3, imported specifications, ghost erasure, Rust compiler,
ABI and runtime assumptions remain. This tool closes none of those production
boundaries and confers no whole-application theorem, `KernelChecked` status,
upgrade authority or release approval. See the existing
[shared-source verification scope](../verification/verus/README.md).

## License and future trial

The inspected [LICENSE][license] is MIT: retain its copyright and permission
notice when distributing copies or substantial portions. ZenoFCIS declares
`MIT OR Apache-2.0` in [Cargo.toml](../Cargo.toml). A separate MIT development
tool appears compatible with that distribution model if its notices are
preserved. This is a project-level assessment, not a legal determination or an
audit of all overlay, backend and transitive-dependency licenses.

A future bounded trial should pin the complete external toolchain separately,
record adapter and expansion hashes, and first demonstrate version pairing and
unchanged production proof inputs. Use the exact bounded-product contract, a
deliberately weakened contract, an equivalent mutant, a panic mutant,
impossible/sparse preconditions and a missing-harness control. Record admitted
samples, every outcome, reproducible seeds/witnesses, elapsed work and failures;
compare useful findings with the existing native and mutation controls. Stop
if a control yields a false success or requires weakening the production
contract. Only a reviewed incremental benefit would justify a later optional
development integration. **No trial results or performance claims are made.**

[upstream]: https://github.com/verus-lang/verus-spec-check/tree/44cde1b910571a7c0fb5a20a01d0eb88d15ef446
[readme]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/README.md
[usage]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/USAGE.md
[manifest]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/Cargo.toml
[architecture]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/source/CODE.md
[mutation]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/source/runtime/src/cov_mutate.rs#L65-L160
[runner]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/source/expand/src/harness_emit.rs#L2133-L2220
[vacuity]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/source/expand/src/harness_emit.rs#L1443-L1544
[printer]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/source/expand/src/render.rs
[license]: https://github.com/verus-lang/verus-spec-check/blob/44cde1b910571a7c0fb5a20a01d0eb88d15ef446/LICENSE
