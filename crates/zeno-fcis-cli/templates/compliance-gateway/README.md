# Generated compliance gateway

This local, non-value-moving application screens transfers for one customer
with an expert system's rule base. It shows five patterns:
- a rule base as the contract: `rules.txt` holds prioritized rules over
  finite features, `synthesis.json` states them as a relation, and
  `zeno-fcis synth` selects a decision program that satisfies it on all 2,880
  inputs, including both actions and reviewer values;
- every decision names the rule that fired: a blocked transfer is a committed
  failure under that rule's reason, a held transfer queues a review ticket
  naming it, and a rule base with a conflict, a gap, or a rule that never
  fires fails the build;
- a blocked transfer is a *committed failure*: the transfer is refused, but
  the strike is recorded, and three strikes freeze the account until a
  reviewer reinstates it;
- notices leave through the outbox: review tickets and block alerts each stay
  pending until the destination acknowledges them;
- an inductive claim for `zeno-fcis prove`: every change the laws admit
  keeps the strikes within their bounds, for every integer. CVC5 attests the
  induction step, and the tests check the rest of the argument.

`src/program.rs` calls the synthesized step in `synthesized/transition.rs`
for the complete finite decision, then maps its output into typed staging.
The law checker in `src/laws.rs` evaluates the formulas in
`project.zeno` against every decision, evaluates `rules.txt` itself through
`src/rules.rs`, and refuses any decision that breaks either.

`project.zeno` owns record fields, command variants, reason order, channel
types, and relational formulas. `build.rs` supplies explicit scalar bounds and
catalog meanings, then checks and generates the schema and project bindings.
`profile.rs` binds the exact source, including the rule base and the synthesis
files, and the runtime-only law manifest.

## The rules

The standing has one field, `strikes` (field 120), from 0 to 3: the transfers
blocked since the last reinstatement. Every value is a valid standing, and it
starts at zero. Each request has an `action` (field 130) and describes one
transfer; its context describes the customer and the caller.

| Field | ID | Meaning | Range |
| --- | --- | --- | --- |
| `region` | 131 | where the transfer goes | 160 `Allowed`, 161 `Restricted`, 162 `Sanctioned` |
| `amount_band` | 132 | the amount, banded by the shell: in the demonstration, 0 is under 100, 1 under 1,000, 2 under 10,000, 3 under 50,000, and 4 above | 0 through 4 |
| `counterparty_risk` | 133 | the shell's rating of the recipient | 165 `Low`, 166 `Medium`, 167 `High` |
| `identity_tier` | 140 (context) | how far the customer's identity is verified | 0 (unverified) through 3 |
| `reviewer` | 141 (context) | whether the caller is a compliance reviewer | 0 or 1 |

| Action | ID | Effect when accepted |
| --- | --- | --- |
| `Screen` | 150 | the rule base decides the transfer: allowed, held, or blocked |
| `Reinstate` | 151 | clears the strikes; the transfer fields are ignored |

The rules apply in this order, and the first that applies decides:

1. `Reinstate` without `reviewer` is rejected with `not_reviewer` (200).
2. `Reinstate` of an account with no strikes is rejected with `no_strikes`
   (201).
3. `Reinstate` is accepted: `strikes` becomes 0.
4. `Screen` is decided by the rule base below. Among the rules whose
   conditions all hold, the one with the highest priority decides, and the
   decision names it (variants 170 to 181 of `RuleId`, in the order of the
   table):
   - *block*: the transfer commits a failure under the rule's reason
     (210 to 214, in the same order), `strikes` increases by one, up to 3, and
     one alert is queued with the rule and the strikes now on record;
   - *hold*: the transfer is accepted, `strikes` is unchanged, and one review
     ticket is queued with the rule and the amount band;
   - *allow*: the transfer is accepted; nothing changes and nothing is queued.

| Priority | Rule | When | Verdict |
| --- | --- | --- | --- |
| 100 | `sanctioned_region` | region is sanctioned | block |
| 90 | `frozen_account` | strikes are 3 | block |
| 85 | `unverified_high_risk` | identity tier is 0 and counterparty risk is high | block |
| 80 | `restricted_large` | region is restricted and amount band is at least 3 | block |
| 80 | `unverified_large` | region is allowed, identity tier is 0, and amount band is at least 3 | block |
| 70 | `high_risk_counterparty` | counterparty risk is high | hold |
| 60 | `restricted_region` | region is restricted | hold |
| 50 | `unverified_transfer` | identity tier is 0 and amount band is at least 1 | hold |
| 40 | `partially_verified_large` | identity tier is 1 and amount band is at least 3 | hold |
| 30 | `repeat_offender` | strikes are at least 2 and amount band is at least 2 | hold |
| 20 | `medium_risk_large` | counterparty risk is medium and amount band is 4 | hold |
| 0 | `default_allow` | always | allow |

A rejection changes nothing and queues nothing. Review tickets go to
`review-queue` on channel 300 and carry the rule (field 145) and the amount
band (146). Block alerts go to `compliance-team` on channel 301 and carry the
rule (147) and the strikes after the block (148). The shell bands the amount,
rates the counterparty, verifies the identity tier, and authenticates the
reviewer; the core decides only what those facts mean.

## The rule base

`rules.txt` is written for a compliance officer to review. It declares each
feature with its finite range or its values, then one rule per line:

```text
rule restricted_large priority 80 when region == restricted and amount_band >= 3 then block
```

Two rules may share a priority only if no transfer matches both: the two at
priority 80 above are separated by the region. `src/rules.rs` parses the file
and checks it over all 720 combinations of its features:
- *consistent*: no two rules of the same priority match the same transfer,
  whatever they conclude, because the decision must name one rule;
- *total*: every transfer matches some rule, which `default_allow` assures;
- *live*: every rule decides some transfer; a rule that a higher priority
  always covers, or whose conditions never hold together, is refused.

A rule base that fails a check is refused with the transfer, or the rule,
that shows it. A rule that never decides is either redundant or misplaced in
the priority order; either way the officer, not the code, should resolve it,
so the shipped rule base has no such rule, and each of its twelve rules
decides at least one of the examples in `tests/decision-examples.txt`.
`build.rs` loads the rule base through `src/rules.rs`, so a refused rule base
fails `cargo build`, and the law checker loads it again when the authority is
built, so no application decides under a rule base that was not checked.
`tests/rule_base.rs` shows the refusal: a same-priority pair with different
conclusions, the same pair with the same conclusion, the shipped rule base
with one more rule at an existing priority, the shipped rule base without its
default rule, a rule shadowed by a higher priority, and a rule whose
conditions never hold together.

## The synthesized step

`synthesis.json` has two parts:
- a contract, written from `rules.txt` and the reinstatement rules by
  `rules_to_synthesis.py`: for every standing, transfer, action, and reviewer
  flag, the decision, rule, and strikes after the decision;
- a sketch: a hand-written decision program in priority order with three
  holes, the freeze threshold, the boundary of a large amount, and the verdict
  code of a hold. Its reinstatement branch has no holes.

`zeno-fcis synth run` evaluates hole assignments over all 2,880 inputs and
selects the first that satisfies the contract on every one: freeze at 3
strikes, large from band 3, and 1 as the code of a hold. The selected program,
its complete input and output vectors, and the emitted Rust are checked in
under `synthesized/`. For reinstatement, the synthesized result has first
precedence for a missing reviewer, then no strikes, then an accepted reset.
Its rule field is set to the last rule index as a fixed placeholder; no
screening rule fires on reinstatement, and the adapter ignores that field.

That selection is exhaustive verification against the contract, for the
program as the library's interpreter runs it. The emitted Rust carries the same
result once `synth verify` has replayed it:

```sh
zeno-fcis synth run synthesis.json --out synthesized --check
zeno-fcis synth verify synthesis.json --out synthesized --receipt rust-conformance.json
zeno-fcis synth run synthesis.json --target python --out python-step
zeno-fcis synth verify synthesis.json --target python --out python-step --receipt python-conformance.json
zeno-fcis synth run synthesis.json --target javascript --out javascript-step
zeno-fcis synth verify synthesis.json --target javascript --out javascript-step --receipt javascript-conformance.json
```

It is only as right as the contract, and the contract is only as right as its
derivation from the rule base. Two evaluators of `rules.txt` that share
nothing with `rules_to_synthesis.py` or the synthesizer check that:
`tests/rule_base.rs` compares the selected program with `src/rules.rs` and
the reinstatement rules on every input, and `tools/check_synthesis.py` in the
library compares the vectors with a separate Python evaluation. The examples in
`tests/decision-examples.txt` check the running application against this
README.

After editing `rules.txt`, regenerate the contract and the program:

```sh
python3 rules_to_synthesis.py rules.txt synthesis.json
rm -rf synthesized
zeno-fcis synth run synthesis.json --out synthesized
```

The sketch names the shipped rules, so a new rule needs a branch there, a
`RuleId` variant in `project.zeno`, and, if it blocks, a committed-failure
reason; `tests/conformance.rs` checks that those declarations match the file.

## What is checked

Every decision is checked at run time, before it can be published:
- law 500: the strikes stay within their bounds. Every value is a valid
  standing, so this law only restates the schema bounds;
- law 501: an accepted screening, allowed or held, keeps the strikes, and,
  whatever the rule base says, a transfer to a sanctioned region or from a
  frozen account is never accepted;
- law 502: only a reviewer reinstates, only an account with strikes, and
  reinstating clears them;
- law 503: only a screening commits a failure; a blocked transfer adds one
  strike, and a frozen account stays at three;
- in Rust, because formulas cannot see them: the rule base itself, evaluated
  on the screening's features; the kind of decision and its reason; the
  exact ticket or alert, or the absence of both; the absence of effects; and
  a zero genesis.

Law 500's bound is also claimed by induction, for `zeno-fcis prove`:

```text
claim 600 strikes_stay_in_bounds all inductive accept [501, 502] failure [503] = pre.100.120 >= 0 && pre.100.120 <= 3;
```

The authority refuses every decision that breaks a law enforced on it, so
laws 501 and 502 bound every accept and law 503 every committed failure.
Each law declares that scope in `project.zeno`, so elaboration also checks
the claim's groups against the declared scopes, and `authority()` checks the
manifest against them before it builds the authority. The induction step
asks whether any transition those laws admit can take a
standing within the bound outside it. It is checked for every integer, not
only the schema's domain. The step assumes that each enumerated field holds
one of its declared variants, as admission guarantees; without that, a
command that is neither a screening nor a reinstatement would satisfy both
accept laws vacuously. CVC5 answers `unsat`, which is attested, not
independently checked.

The step says something about this application only together with
`tests/claims.rs`, which checks four things:
- the law checker's own observer, `laws::trace_step`, reads every field the
  invariant reads, and the invariant evaluates on every admitted standing;
- the invariant holds on the exact genesis standing that a new shell
  stores;
- the law manifest enforces laws 501 and 502 on accepts and law 503 on
  committed failures, as the claim assumes;
- the manifest enforces each law exactly on the decisions `project.zeno`
  declares for it, and a manifest that bound law 501 to every commit, or to
  the genesis, is reported.

`tests/conformance.rs` also evaluates the invariant before and after every
decision the application commits. The section
[How ZenoFCIS built and checked this](#how-zenofcis-built-and-checked-this)
quotes what each backend reported.

A rejection publishes nothing by construction: a rejected decision carries no
patch, effect, or outbox entry, and the law framework records law 509 as
satisfied for every rejection. That law has no formula, so `profile.rs`
registers it instead of `project.zeno` carrying an always-true placeholder.
As a result, this passes:

```sh
zeno-fcis check project.zeno --require-substantive --require-resolved-paths
```

The tests check the running application:
- `tests/rule_base.rs` checks that the rule base is consistent, total, and
  live, that the synthesized step agrees with `src/rules.rs` and the
  reinstatement rules on all 2,880 inputs, that the step refuses inputs outside
  its domain, and that
  conflicting, incomplete, shadowed, and malformed rule bases are refused at
  the line, transfer, or rule that shows the fault.
- `tests/conformance.rs` runs all 2,880 admitted inputs (every standing,
  action, region, amount band, counterparty risk, identity tier, and reviewer
  flag) through admission, the authority, the adapter, the law checker, the
  committed patch, and the outbox, and compares each outcome with the rule
  base and the reinstatement rules; the invariant of claim 600 must hold
  before and after every committed decision. It also checks that schema admission
  matches the finite domain in both directions, that genesis is exactly zero
  strikes, that all 4 standings are reachable from it, that the `RuleId`
  variants and the committed-failure reasons match `rules.txt` in name and
  order, and that the 28 examples in `tests/decision-examples.txt` match and
  reach every rule of the rule base.
- `tests/claims.rs` checks claim 600's base case on the exact genesis
  standing, its law checker's observer, its assumptions against the law
  manifest, and the manifest against the scopes `project.zeno` declares.
- `tests/laws.rs` gives the law checker decisions a faulty program could
  make, such as a block under the wrong rule, a ticket for the wrong band, a
  held transfer that adds a strike, a reinstatement without a reviewer, or
  strikes outside their bounds, and requires it to refuse each one. Every law
  refuses at least one of them.
- `tests/lifecycle.rs` runs the demonstration, shows that a frozen account
  stays frozen across a database reopen, and that rejections leave the
  standing and the outbox unchanged.
- `tests/determinism.rs` decides 144 inputs (strikes at 0 and 3, identity
  tier at 0 and 3, every region, amount band at 0 and 4, and counterparty
  risk at low and high, with each screening requested by a customer and each
  reinstatement by a customer and by a reviewer) eight times each through
  `execute_probed`, then again in three child processes with a changed
  environment, and requires every decision digest to match.

The conformance test's reference model and the law checker both evaluate the
rule base through `src/rules.rs`, whose author also wrote the program and
`rules_to_synthesis.py`. They catch binding and adapter errors, and a contract
that departs from the file, not a misreading of the README shared by all
three. The examples file is the check on that: its header records who wrote
the examples and whether the project's owner has reviewed them.

```sh
zeno-fcis purity src/program.rs src/laws.rs src/rules.rs synthesized/transition.rs
```

All of these are detectors: agreement shows that these runs matched and that
no error-level rule fired. The conformance test covers every admitted input,
so for this finite domain it checks every decision the application can make.

## How ZenoFCIS built and checked this

Every command below was run on 2026-09-24 from a ZenoFCIS checkout with the
CLI built (`cargo +1.97.1 build -p zeno-fcis-cli --locked`; `zeno-fcis
--version` prints `zeno-fcis 1.1.0`), Rust 1.97.1, Python 3.12.3, and Node.js
v22.23.1, in a fresh copy of this template made with
`zeno-fcis new gateway --template compliance-gateway`. Outputs are quoted as
observed; long JSON is reduced to the fields named.

1. **Check the authored project.** Every law and claim can constrain a
   transition, and every path names a declared field.

   ```text
   $ zeno-fcis check project.zeno --require-substantive --require-resolved-paths
   checked project.zeno: project=1 components=1 claims=1 unresolved_obligations=2 semantic_program_hash=e296669471dc86b7701dd1685a4ad0603dedf0bca9235b34a95d465efe13620b
   ```

2. **Generate the bindings.** `cargo build` runs `build.rs`, which first
   loads `rules.txt` through `src/rules.rs` and stops on a rule base that is
   not consistent, total, and live. It then lowers `project.zeno` with the
   scalar bounds and writes the typed schema (`generated`) and project
   bindings (`bindings`): the record types, the reason IDs, and the
   `read_strikes`, `update_strikes`, and `enqueue_channel_300`/`301` staging
   methods that `src/program.rs` calls. Nothing in `src/` restates them.

3. **Write the contract from the rule base.**

   ```text
   $ python3 rules_to_synthesis.py rules.txt synthesis.json
   12 rules; contract 87 nodes, sketch 79 nodes
   ```

4. **Synthesize the decision.** The run selected the fourteenth of eighteen
   hole assignments, the first that satisfies the contract on all 2,880 inputs;
   `--check` regenerates the artifacts and compares them with the checked-in
   ones.

   ```text
   $ zeno-fcis synth run synthesis.json --out synthesized --check
   {"status":"current","semantic":"complete-finite","runtime_conformance":"not-run", "manifest":{"status":"selected","input_coverage":2880,"assignments_evaluated":14,"certificate":"281b9f68bae317c212da78e9ad64995d54ae0fb35a18815e17460cb1c9396ebc", ...}, ...}
   ```

5. **Replay the emitted source in three languages.** Each target ran all 2,880
   inputs and produced the same output bytes under the same certificate.

   ```text
   $ zeno-fcis synth verify synthesis.json --out synthesized --receipt rust-conformance.json
   {"status":"passed","inputs_checked":2880,"certificate":"281b9f68…396ebc","stdout_sha256":"098d3146…bf365d","tool":{"version":"rustc 1.97.1 (8bab26f4f 2026-07-14)", ...}, ...}
   $ zeno-fcis synth run synthesis.json --target python --out python-step
   $ zeno-fcis synth verify synthesis.json --target python --out python-step --receipt python-conformance.json
   {"status":"passed","inputs_checked":2880,"certificate":"281b9f68…396ebc","stdout_sha256":"098d3146…bf365d","tool":{"version":"Python 3.12.3", ...}, ...}
   $ zeno-fcis synth run synthesis.json --target javascript --out javascript-step
   $ zeno-fcis synth verify synthesis.json --target javascript --out javascript-step --receipt javascript-conformance.json
   {"status":"passed","inputs_checked":2880,"certificate":"281b9f68…396ebc","stdout_sha256":"098d3146…bf365d","tool":{"version":"v22.23.1", ...}, ...}
   ```

6. **Check the decision code statically.**

   ```text
   $ zeno-fcis purity src/program.rs src/laws.rs src/rules.rs synthesized/transition.rs
   purity: clean (0 errors, 0 warnings) in 4 files
   ```

7. **Check the induction step.** The solvers and Lean are not part of the default
   gate. They need a tools manifest naming the installed binaries, their
   versions and hashes, and Lean's distribution root and tree hash; the
   library's `docs/FORMAL_TOOLS_RC3.md` gives its schema. The
   pinned tools were CVC5 1.3.3, Z3 4.16.0, and Lean 4.30.0 (the qualified
   Linux x86-64 distribution). `prove` keeps every run's exact input and
   output under `.zeno-fcis/evidence/` next to the project.

   ```text
   $ zeno-fcis doctor --tools zeno-fcis.tools.json
   cvc5 1.3.3 e8d7870d57ab55e81619d2373b043da05ea1d37ca393931bdb5d8b9788cd64c4
   z3 4.16.0 e583c4186a45e72411fa2cb2048401eed03f0f8e5f24694676a8f6271a50b765
   lean 4.30.0 3e0d0d3d801675359f2d4cf9815bfdb417b20b92fdd9d48b3b14c95bbae28bbf
   ```

   CVC5 answered `unsat` for the induction step of claim 600. ZenoFCIS
   classifies that as a proposal and returns exit code 2, because the proof
   output is not checked independently: the step is attested, not
   kernel-checked. The scope line says what else the application must check.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend cvc5 --tools zeno-fcis.tools.json
   cvc5 claim 600: UNSAT proposal retained; proof output was not independently checked
   cvc5 claim 600 assumes laws [] on every commit, [501, 502] on accepts, [503] on committed failures
   cvc5 claim 600 scope: every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also observes every value the invariant reads, checks the invariant on its exact genesis state, and enforces each assumed law on the decisions the claim assumes it on
   (exit 2)
   ```

   Z3 also answered `unsat`. RC3 has no Z3 proof checker, so the run is
   recorded as blocked evidence, with exit code 2. There is no Lean export for
   inductive claims yet, so `--backend lean` selects nothing.

   ```text
   $ zeno-fcis prove project.zeno --claim 600 --backend z3 --tools zeno-fcis.tools.json
   z3 claim 600 blocked: UnsupportedEvidence
   z3 claim 600 assumes laws [] on every commit, [501, 502] on accepts, [503] on committed failures
   (exit 2)
   $ zeno-fcis prove project.zeno --claim 600 --backend lean --tools zeno-fcis.tools.json
   claim 600 does not select compatible lean
   no compatible claim/backend pair was selected
   (exit 2)
   ```

   The step alone says nothing about this application. `tests/claims.rs`
   checks the base case, the observer, and the law manifest, which the
   library's guide to inductive claims requires.

8. **Test the application.** `cargo +1.97.1 test --locked` runs the tests
   listed under [What is checked](#what-is-checked). The library's gate,
   `python3 tools/check_generated_application.py`, repeats steps 4, 5, and 8
   on a fresh copy, with a separate Python evaluation of `rules.txt` as the
   oracle for the vectors, and compares the demonstration's summary.

9. **Draw the project.** `zeno-fcis graph project.zeno --format mermaid`
   prints one node, `c400[gateway]`: the project has a single component, so
   no diagram is included here.

## A second judge: the rule base in Tau Language

`tau/rule-base.tau` states the priority ladder of `rules.txt` once more, as a
Tau Language specification over five bitvector input streams, the features,
and three output streams: the verdict, the rule that fired, and the strikes
after the decision. `tau/check.py` runs it in the Tau REPL on every input of
the decision table in `synthesized/vectors.json`, one execution step per
input, and compares the three outputs with the table. It is a third evaluator
of the rule base, written by hand from `rules.txt`, with nothing in common
with `src/rules.rs`, `rules_to_synthesis.py`, or the synthesizer. It needs the
Tau binary from IDNI (<https://github.com/IDNI/tau-lang>), under IDNI's
license, so it is never part of the library's gates:

```sh
python3 tau/check.py --tau PATH/TO/tau --jobs 4
```

At the time of writing it had been run to completion only on a six-input
sample, with Tau 0.7.0-alpha (b647e787): `tau: 6 inputs of the decision
table, 0 disagreements`, at about half a minute per input after the REPL
starts. No agreement on the whole table is claimed here.

## Run this development candidate

This template depends on the current ZenoFCIS checkout. From that checkout,
run:

```sh
python3 tools/check_generated_application.py
```

That gate creates a fresh application using the CLI, checks its synthesis in
Rust, Python, and JavaScript against a separate evaluation of `rules.txt`,
patches its dependencies to the exact checkout, checks dependency versions
against the workspace lock, then compiles and runs its tests and demonstration
as an isolated package. With these development dependencies available in a
standalone checkout:

```sh
cargo +1.97.1 test --locked
cargo +1.97.1 run --locked -- new-gateway.sqlite
```

The SQLite shell is the `sqlite` feature, on by default. Without it,
`cargo +1.97.1 build --no-default-features` builds the core alone: the
generated bindings, the program, the law checker, the profile, the delivery
adapter, and `authority()`, with no database; the gate checks that it also
compiles for `wasm32-unknown-unknown`. `create`, `invoke`, `journey`, and the demonstration
binary need the feature.

The demonstration requires a new database path. It follows one customer
through twelve requests:

1. A verified customer (tier 2) sends under 1,000 (band 1) to an allowed
   region, low risk: allowed by `default_allow`.
2. The same customer sends under 10,000 (band 2) to a restricted region:
   held by `restricted_region`; a review ticket is queued.
3. A fully verified customer (tier 3) sends a small amount to a sanctioned
   region: blocked by `sanctioned_region`; one strike, and an alert.
4. Someone who is not a reviewer asks to reinstate: rejected, `not_reviewer`.
5. An unverified customer (tier 0) sends under 50,000 (band 3) to a high-risk
   counterparty: blocked by `unverified_high_risk`; two strikes.
6. A verified customer sends under 10,000 to an allowed region, low risk: with
   two strikes on record, held by `repeat_offender`.
7. A fully verified customer sends over 50,000 (band 4) to a restricted
   region, medium risk: blocked by `restricted_large`; three strikes, and the
   account is frozen.
8. The same customer sends under 100 (band 0) to an allowed region, low
   risk: blocked by `frozen_account`; the strikes stay at three.
9. A reviewer reinstates the account: accepted, no strikes.
10. The reviewer reinstates again: rejected, `no_strikes`.
11. An unverified customer sends over 50,000 to an allowed region, low risk:
    blocked by `unverified_large`; one strike.
12. A fully verified customer sends over 50,000 to an allowed region, medium
    risk: held by `medium_risk_large`; a review ticket is queued.

It then interrupts notice delivery, reopens the database, finishes delivery,
and prints a JSON summary: the twelve decisions in order (`Accept`,
`Reject`, or `CommittedFailure`), then `strikes` 1, `allowed` 1, `held` 3,
`blocked` 5, `bundles` 10 (the committed decisions), `pending` 0, and
`deliveries` 8 (three tickets and five alerts).

The destination keeps an in-memory idempotency ledger, which survives the
database reopen only within one process. A real destination must persist its
delivery IDs and entry hashes. The context's `identity_tier` and `reviewer`,
the transfer's band and risk rating, and the principal are trusted tutorial
inputs, not remote authentication or a real risk model: a deployment must
establish them before admission. Source hashes identify reviewed example
policy, not certified binaries or release evidence. Changing the source,
including `rules.txt` and the synthesis files, changes policy identity and
requires a new database unless a separately reviewed migration is implemented.
