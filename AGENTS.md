# Repository Agent Guidance

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
authority. Run `python3 tools/atdd.py run --all` immediately before committing.
