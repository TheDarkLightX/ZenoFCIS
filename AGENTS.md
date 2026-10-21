# Repository Agent Guidance

For any handoff or referenced brief, use
[`skills/handoff-completion-gate/SKILL.md`](skills/handoff-completion-gate/SKILL.md)
before editing. Read the complete source, state the definition of done and
required checks, then inspect every final check before claiming completion.

Review functional-core purity at each declared authoritative invocation
boundary. Inputs and bound policy must remain transitively immutable, and
checked results must not expose mutable aliases. Internal mutation needs
explicit exclusive ownership, caller flow, lifetime and refusal accounting;
invocation scratch must not become retained state. A mutable reference is a
review lead, not by itself a purity defect.

Classify stateful preparation handles and caller-buffer serialization utilities
separately from the pure decision API. They cannot supply decision, resource
usage or publication authority. Source-pin the ownership review for any new
mutable interface or escaping alias. Preserve measured work on refusal and
retry; changing an interface's syntax does not establish verification.

Choose the smallest understandable design that preserves the required
assurance, with demonstrated behavior preservation for each simplification.

For each simplification:

- State the behavior and assurance that must remain intact, including relevant
  authority boundaries, canonical data, rejection order and resource limits.
- Compare the existing design with the simpler option. Count the concepts,
  state and interactions a reader must understand; fewer lines or a lower
  cyclomatic-complexity score alone do not establish a simpler design.
- Retain appropriate before-and-after evidence, such as regression checks,
  differential execution, replay or proof checks. State what that evidence
  covers and its limits. Scale validation to the affected behavior and risk.
- Justify removed checks using established invariants. Preserve independent
  acceptance checks and show that the simplification retains their protection.
- Identify intentional changes, including refreshed source-bound identities,
  separately from behavior claimed to be preserved.

Prefer direct code, clear ownership and explicit data flow. Extra abstractions,
caches or mutable bookkeeping require a stated need, a comparison with the
simpler alternative and evidence that their assurance obligations are met.

Use plain, purpose-based names for folders and types. Prefer names such as
`test-projects`, `test-data`, and `DecisionMismatchRecord` to abstract jargon.
Keep existing public names compatible when renaming types, and preserve
versioned identifiers and canonical formats.

In Code Mode, within each bounded stage, run independent, `functions.exec`-available tool calls concurrently in one `functions.exec` call. Use `await Promise.allSettled([...])` when partial results are useful, and inspect every result; use `await Promise.all([...])` only when any failure should abort the batch. Keep dependencies, waits/resumes, approvals, conflicting or interdependent mutations, and adaptive investigations where each result may change the next step sequential. Do not split otherwise batchable inspections across outer tool calls.

Keep combined output bounded. Set a useful per-call output limit, return only the fields needed for the stage, and split a batch when its combined evidence may exceed the shared output limit.

When the optional pinned Probity hook is installed, obey `probity.config.ts`.
Treat it as a deterministic development guardrail, not proof or release
authority.

## Avoid validation churn

Use the smallest set of checks that covers the changed behavior and risk.
Ordinary commits and pushes do not require the full acceptance suite. Check
documents and links for documentation edits; run affected tests for code,
tooling and workflow edits. A typo, CI timeout increase or inventory refresh
does not by itself justify rerunning application acceptance or formal proofs.

Before starting an expensive rerun, record the changed inputs, the checks they
affect, and why existing evidence is insufficient. Reuse passing evidence only
when its relevant inputs and assumptions are unchanged; record the original
revision and the reviewed difference. Never describe reused evidence as a fresh
pass on a different revision. Changed semantics, specifications, dependencies,
toolchains, proof assumptions or test coverage require the affected checks;
uncertain impact requires broader validation.

Batch related repairs before qualification. Check test selection, platform
assumptions, dependency availability and isolated-copy source bindings first.
After repeated failures, diagnose and collect the remaining failures before
launching another complete run; do not fix one test and blindly restart the
same expensive sequence. Preserve completed results and logs.

Freeze the release inputs while long checks run. Keep unrelated cleanup,
documentation and policy edits separate from that frozen tree. Do not restart
a healthy job or invalidate its evidence for an incidental edit. Stop and
repair a demonstrated failure; a timeout or incomplete run is never a pass.

Run `python3 tools/atdd.py run --all` for a new version or release candidate
after the release changes are assembled. Subsequent non-behavioral edits may
use focused checks plus a recorded evidence-reuse review instead of restarting
the full suite. Preserve final release checks, source/artifact provenance and
proof obligations; this is not permission to waive a failed check.

This policy replaces unconditional full-suite-before-every-commit instructions
in older plans and handoffs. Keep `probity.config.ts` and its tests consistent
with it; report a conflicting instruction instead of silently repeating work.

Before publishing documentation, follow `docs/PUBLICATION_POLICY.md`. Curate
for library users and check the full contents and attachments. Private research,
project details, Library records and agent coordination stay outside GitHub.
Run `python3 tools/check_public_docs.py` and the relevant link/privacy checks;
adding a file to the public document list requires a content review.
