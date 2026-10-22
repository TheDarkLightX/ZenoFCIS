---
name: zal-authoring
description: Discuss, inspect, or change precise application behavior through the ZAL MCP tools in a coding harness. Use for a configured ZAL workspace and supported finite control-state models.
---

Use the project's `zal` MCP server for behavior specification. Read
`.zeno-fcis/zal/install.json` for the shared workspace and human review command.
If setup is absent, use the library's `integrations/zal/setup.py` with the target
project; inspect its plan before applying it. Preserve existing configurations,
skills, workspaces and pending proposals.

Call `zal_read` before proposing or interpreting a revision. Its `model.revision`
and `object_ids` bind subsequent calls. After a stale refusal or human acceptance,
read again. Each context supplies every declared observation as a JSON Boolean;
a fact named `authorized` is an assumption, not authenticated authorization.

`zal_read` also shows retained owner examples, their corpus digest and separate
check/review/factory freshness. Show conflicts and out-of-domain cases using the
same unchanged expected labels in both revisions. Only the person supplies or
replaces expected values and provenance through the terminal's `label` command;
never infer owner labels from model outputs or enter them on the person's behalf.
Labels are cooperative intent assumptions, not authenticated owner identity.
Explanation/trace results separate supplied assumptions from authenticated facts;
the current profile supplies none of the latter.

Use `zal_help` with an empty topic for the index, an exact typed ID for an object,
or `grammar:TOPIC` for grammar. Use `zal_explain` for one complete input and
`zal_trace` for a hypothetical sequence. Explain their supplied assumptions.
These calls use deterministic logic; free prose is not normative source.

For a requested change, compare the complete candidate with `zal_compare` and
show a concrete distinguishing input. Stage it with `zal_propose` using the exact
base revision and declared IDs. State the changed behavior and remaining
requirements plainly. Never silently repair ambiguity or weaken a requirement
to obtain a pass.

The person reviews `candidate` in the separate terminal and chooses `accept` or
`reject`. They can review an initial model using `show` and `accept-current`.
Do not perform semantic acceptance through a shell, fabricate an agreement event,
edit the workspace directly, or interpret tool permission as agreement.

After acceptance, refresh with `zal_read`. Export through the recorded
`workspace.py export` command with `--revision` set to the exact reviewed revision
and an absent/empty `--out` directory. Verify the export with `workspace.py verify`
before passing it to the existing ZenoFCIS generator and actual Authority tests.
Exports are declarations, not application publication or proof receipts.

Reading preserves old checks. For stale checker evidence, restart if directed,
then explicitly check and have the person review again. A changed example corpus
requires a new display/review; restage a pending candidate against the current
corpus first. Never describe a bound corpus as proof that its labels agree, and
never refresh a stale agreement on the person's behalf.
Reviewed meaning/export can retain a displayed example conflict or out-of-domain
case. State it plainly: corpus binding is not an owner-example pass or a
machine-proved requirements route.

The current profile is `zal/finite-fsm/1`: flat control labels, enumerated events,
fresh Boolean observations, fixed targets, explicit rejection and state invariants.
Arithmetic, effects, concurrency, fairness and liveness need explicit unresolved
obligations or a separately supported contract route. Never approximate these
requirements or present this profile as a money/effect specification.

Distinguish model consistency, stated invariants, human intent, external facts,
factory correspondence and verified execution. A zero-invariant consistency pass
does not prove an unstated safety or authorization policy. Local shared-file
review is cooperative; it does not authenticate approval against a same-user
agent. No ZAL operation authorizes adoption, deployment or publication.
