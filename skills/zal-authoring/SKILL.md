---
name: zal-authoring
description: Discuss reviewed finite application behavior or explained ontology classifications through ZAL MCP tools in a configured coding harness.
---

Use the project's `zal` MCP server for behavior specification. Read
`.zeno-fcis/zal/install.json` for the shared workspace and human review command.
If setup is absent, use the library's `integrations/zal/setup.py` with the target
project; inspect its plan before applying it. Preserve existing configurations,
skills, workspaces and pending proposals.

For ontology/library questions, use the separate route below when its tools are
advertised. An installed server may expose fewer profiles than the development
source; inspect its capabilities instead of assuming an upgrade is already live.

Call `zal_read` before proposing or interpreting a behavioral revision. Its `model.revision`
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
For "why was this rejected" or "what would make it accept", call `zal_why_not`
and report its scope, minimal changes and deciding rule. Use `zal_what_if` to
show the effect of named input changes, and `zal_narrate` to tell a
counterexample or trace; quote its generated sentences rather than paraphrasing
them. `zal_guide` gives construct forms and meanings.

Ladder and temporal statements use their own revisions: call `zal_ladder_read`,
then pass the statement ID and exact revision to `zal_ladder_render`,
`zal_ladder_parse`, `zal_ladder_same`, `zal_ladder_explain` or
`zal_ladder_lower`. Keep the scope strip attached when you restate one. A
"different normal forms" answer does not show different meanings, and a
lowered claim is an inert obligation, not evidence.

For a requested behavior change, compare the complete candidate with `zal_compare` and
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

The behavioral review route above uses `zal/finite-fsm/1`: flat control labels, enumerated events,
fresh Boolean observations, fixed targets, explicit rejection and state invariants.
Arithmetic, effects, concurrency, fairness and liveness need explicit unresolved
obligations or a separately supported contract route. Never approximate these
requirements or present this profile as a money/effect specification.

Distinguish model consistency, stated invariants, human intent, external facts,
factory correspondence and verified execution. A zero-invariant consistency pass
does not prove an unstated safety or authorization policy. Local shared-file
review is cooperative; it does not authenticate approval against a same-user
agent. No ZAL operation authorizes adoption, deployment or publication.

## Ontology and shared structure

Use `zal_ontology_parse` for the admitted controlled-English rules, then
`zal_ontology_query` or `zal_ontology_classify` on the returned document. Explain
a classification with `zal_ontology_explain` and its exact fact ID. Report the
ground premises, rule, ontology digest and run completeness; supplied assertions
remain assumptions. These self-contained documents do not use the behavioral
workspace revision and do not record its human acceptance.

Use `zal_ontology_replay` to recompute a retained run after context changes.
Matching an incomplete or inconsistent run preserves that status. Missing facts
remain unknown. `zal_ontology_validate_record` checks explicit field syntax;
conforming receipt text does not establish valid evidence. A shared class can
suggest a Lens or checker to try, while behavioral equivalence requires its own
scoped check.

Read `integrations/zal/zal_notation/ONTOLOGY.md` under the source root recorded in
the install record for the admitted profile and local CLI. General ZAL is not
restricted to this positive ontology fragment or the finite review profile.
