# Repository Agent Guidance

Use plain, purpose-based names for folders and types. Prefer names such as
`test-projects`, `test-data`, and `DecisionMismatchRecord` to abstract jargon.
Keep existing public names compatible when renaming types, and preserve
versioned identifiers and canonical formats.

In Code Mode, within each bounded stage, run independent, `functions.exec`-available tool calls concurrently in one `functions.exec` call. Use `await Promise.allSettled([...])` when partial results are useful, and inspect every result; use `await Promise.all([...])` only when any failure should abort the batch. Keep dependencies, waits/resumes, approvals, conflicting or interdependent mutations, and adaptive investigations where each result may change the next step sequential. Do not split otherwise batchable inspections across outer tool calls.

Keep combined output bounded. Set a useful per-call output limit, return only the fields needed for the stage, and split a batch when its combined evidence may exceed the shared output limit.

When the optional pinned Probity hook is installed, obey `probity.config.ts`.
Treat it as a deterministic development guardrail, not proof or release
authority. Run `python3 tools/atdd.py run --all` immediately before committing.
