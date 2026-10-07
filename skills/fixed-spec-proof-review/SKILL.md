---
name: fixed-spec-proof-review
description: Review agent-generated formal proofs against a frozen theorem, its transitive definitions, and permitted assumptions. Use when compilation alone could accept a weakened statement, changed helper, or extra axiom; applies to Lean proof challenges and verification-backed software changes.
---

# Fixed specification proof review

A proof must establish the reviewed statement with the reviewed definitions and
allowed assumptions. Start by freezing that obligation separately from the
candidate proof. Compiler acceptance alone does not establish that they match.

## Freeze the obligation

Record the exact theorem name and type, the definitions it depends on, the
allowed axioms, and the required toolchain. Include transitive helpers: changing
an authorization helper can change the meaning while leaving the visible
theorem text unchanged. Distinguish proof terms that may change from definitions
and propositions that must remain fixed.

For a software theorem, also name the actual program artifact, legal input
domain, observations, and evaluator semantics. Specify what connects the
formal model to the shipped implementation. A proof about a separate model is
evidence about that model until this connection is established.

Save the reviewed obligation outside the candidate's edit scope. Identify its
source revision and hashes. Hashes identify bytes; an editable manifest is not
independent approval or protection against the person editing it.

## Check the candidate

1. Compile the candidate with the pinned toolchain in a fresh build environment.
   Protect the challenge, checker, and their dependencies from candidate edits.
   For arbitrary submissions, use a qualified containment mechanism; a compiler
   project can execute build-time code.
2. Compare the elaborated theorem statement with the frozen statement. Check
   the transitive definition closure, rather than only matching source text.
   An equivalent-looking formula needs an explicit reviewed equivalence rule
   if the challenge permits it.
3. Audit the axioms used by the proof against the stated allowlist. Refuse
   unfinished proof escapes, additional axioms, and unchecked external bodies.
   Do not assume every legitimate project has an empty axiom set.
4. Replay the compiled proof in the required kernel/checker. Record whether an
   independent kernel was used; repeated compilation with the same tool is not
   independent validation.
5. Check that all compared sources and tools still match the frozen identities
   after the run. Keep complete diagnostics and a source-bound receipt.

An optimization or refactoring proof should freeze the original program and
prove the declared observation equivalence with the candidate. It should not
require the candidate implementation to be byte-identical to the original.
Preserve overflow, refusal, resource limits, and effect ordering when those are
part of the observation contract.

## Challenge the review mechanism

Use at least one valid alternative proof and targeted negative controls:

| Control | What it tests |
| --- | --- |
| Weaken the theorem | Statement comparison |
| Change a direct definition | Definition binding |
| Change a transitive helper | Complete dependency binding |
| Introduce an unapproved axiom | Assumption audit |
| Leave a proof unfinished | Escape rejection |
| Supply an invalid proof | Actual kernel/compiler rejection |

Require the intended diagnostic for each control. A timeout, missing executable,
unrelated module error, or transport failure is an inconclusive experiment,
not evidence that the intended defect was detected. Do not keep a negative
control whose setup fails before reaching the checker under test.

## Report the result

State the exact obligation, artifact/domain connection, tools and assumptions,
matched definitions, axiom audit, kernel replay, and control outcomes. Separate
these from requirements adequacy and runtime authorization. A valid proof does
not authenticate a supplied Boolean, issue a durable commit capability, or
establish correctness of an unconnected Rust implementation.

## Lean and ZenoFCIS

[OpenAI's mathematics repository](https://github.com/openai/math) provides the
motivation for checking formal proof submissions against their intended
statements. [Lean Comparator](https://github.com/leanprover/comparator) is a
concrete tool for statement/definition comparison, assumption auditing, and
proof replay. Consult its actual pinned version before choosing commands.
This skill is our engineering workflow, not a theorem supplied by that repo.

ZenoFCIS's `experiments/proof-challenge/README.md` describes a small frozen
Boolean pilot: two valid proofs and six planted defects. Run
`python3 tools/test_proof_challenge.py` for runner checks and follow that README
for the pinned Comparator experiment. The pilot accepts only its reviewed
fixtures; its development shim provides no sandbox and is unsuitable for
arbitrary agent submissions. It proves no source-to-Rust correspondence.
