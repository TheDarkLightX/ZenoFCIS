---
name: handoff-completion-gate
description: Use when implementing a task from any handoff, issue, brief, or referenced task document; ensure complete intake and inspect required final validation before declaring completion.
---

# Handoff completion gate

Before editing, read the entire handoff and every referenced part needed to
understand the requested work. For a file, check its total line count and read
through the last line. If a tool returns only a prefix or truncates output,
fetch the remaining chunks. Do not treat a partial read as a complete brief.

State a short definition of done: the requested behavior or artifact, material
constraints, required final checks, and any requested publication or approval
boundary. Resolve a missing requirement with the user when it changes the
outcome; continue independent work while waiting.

After the last edit, run the required final checks on the final source state.
Inspect each exit status and relevant output. Fix failures and rerun affected
checks; if a check cannot run, report that limit and leave the work marked
unfinished. A convincing implementation or earlier passing run does not
substitute for a failed, skipped, or uninspected final check.
