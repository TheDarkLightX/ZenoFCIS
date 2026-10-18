# Task packet P3: machine-check the composed-domain theorem, plus a shared proved outcome resolver

**Branch:** `agent/2.6-composed-domain-proof` **Size:** large **Evidence level of the result:** proved (Verus) for
the abstract theorem and the resolver; every other bridge is named as tested or as a premise

## Why (the user's story)
A user composes several checked components: for example a procurement flow, where one component reserves budget, one
records an order and one tracks a guard. The user states coupling invariants across them, such as "reserved equals
ordered". `zeno-fcis contract check-composition` (profile `/3`, "composed domain") checks the local, assume and couple
obligations exhaustively over finite tables. From those checks, the guide derives that every reachable global state
satisfies every component and coupling invariant. That last step, the induction, is argued in prose and is not
machine-checked. Both the code and the guide say so.

This task proves that induction in Verus over an abstract model of the joint step. It also extracts the small pure
function that decides a joint step's outcome and committed prefix, so the runtime and the proof share one source.

## Facts about the current code (verified on the base commit)
- **Location.** The code is in `crates/zeno-fcis-contract/src/checked_composition.rs` and
  `checked_composition/{domain,joint,obligations}.rs`. Other composition crates (`zeno-fcis-compose`,
  `zeno-fcis-composed-program`) are not this profile.
- **Model types** (`domain.rs`):
  - `Component` (:605-631) has these fields:
    - `owns` (false for a read-only component);
    - `participates`;
    - `bindings`, and `state_bindings` for read-only state;
    - `assume`, `invariant`;
    - `committed_failure`;
    - `providers`, `cone`;
    - `genesis`.
  - `Coupling { name, members /* merge order */, invariant }` is at :673-678, and `Composition` at :682.
- **Outcomes** (`joint.rs`):
  - `OutcomeKind { Accept, Reject{component, reason}, Refused{component, class}, CommittedFailure{component, reason} }`
    is at :131-156.
  - `RefusedClass` is at :52-71.
  - The per-component `local(...) -> Local {Skip, Reject, Refused, Commit{failure, reason, post}}` is at :713-750:
    participation gives Skip (:722-727), and a read-only state change gives `ReadOnlyStateChanged` (:740-741).
  - The joint step and prefix, `step(...) -> (Ended, Vec<Vec<_>>)`, is at :763-790. Reject and Refused return the
    pre-state. The first CommittedFailure commits the successors up to and including its own, and later components
    do not run. Read-only components are never written (:780).
  - The public wrapper is `JointComposition::joint_step` (:986-994).
- **No shared resolver exists today.** `check_couple` (`obligations.rs:962-1056`) re-implements the member loop. It
  builds the `cutters`, which are outside components with a CommittedFailure case between members (:972-982). It
  checks the coupling at those cut points (:1018-1027) and handles a member's own failure (:1034-1039). The
  obligations are summarised at `obligations.rs:1-42`, and `check_domain` is at :470.
- **The prose claim and argument.**
  - The rules: `docs/COMPOSED_DOMAIN_COMPOSITION.md:27-45`.
  - Outside components: :160-165.
  - The claim: :239-242.
  - The induction: :244-270.
  - The statement that it is not machine-checked: :272.
  - The trusted base: :273-283. The non-claims: :285-292.
  - The same in code: `DOMAIN_CLAIM` (`checked_composition.rs:215`), `DOMAIN_TRUSTED_BASE` (:218-225) and
    `DOMAIN_NONCLAIMS` (:228ff).
- **Finite evidence that exists.**
  - The fault fixtures are in `crates/zeno-fcis-contract/tests/fixtures/checked-composition-3/faults/`:
    invariant-not-preserved, coupling-broken-by-step, coupling-false-at-genesis, binding-reads-later,
    binding-out-of-domain, read-only-changes, two-owners and ticker-tripwire.
  - The fault tests are in `obligations_tests.rs`, and the joint-step tests in `joint_tests.rs` (:109-265).
  - The independent Python reference is `tools/composition_domain_reference.py`, and the soundness generator is
    `tools/composition_domain_soundness.py`. Its planted control, "a CommittedFailure that commits only its own
    successor", is at :23-25.
- **Verus.**
  - The pin is release `0.2026.09.27.3cf1832`, recorded in `verification/verus/toolchain.json`. It runs under Rust
    1.98.1; the runtime stays on 1.97.1.
  - To install: `rustup toolchain install 1.97.1 --profile minimal`, `rustup toolchain install 1.98.1 --profile
    minimal`, then `python3 tools/check_verus.py --install --out <file>` (`verification/verus/README.md:106-124`).
  - Command shape: `--crate-type=lib --edition=2024 --no-cheating --no-external-by-default --num-threads N -V
    spinoff-all --output-json --log vir` (`tools/check_checked_assembly.py:248-252`). Output matching rlimit,
    timeout or unknown is a failure (`tools/check_verus.py:41`).
  - **The idiom to reuse** is `verification/verus/checked_assembly_joint.rs`:
    - spec functions `joint` (:11), `local_check` (:22), `routed_step` (:30) and `history(states: Seq<Seq<S>>,
      ...)` (:37);
    - lemmas `joint_genesis` (:45), `joint_step` (:55) and `joint_induction` (:89).

    It is an abstract model with one selected component per step. Yours is all participants with a prefix and
    couplings.
  - Harnesses `#[path]`-include the production files (`verification/verus/checked_assembly.rs:19-43`). Profiles
    record reviewed counts and VIR inventories (`checked-assembly.json`).
  - Note: the README's checked-assembly counts are stale against `checked-assembly.json`. Do not copy them.

## What to build
1. **The shared resolver.** Add `crates/zeno-fcis-contract/src/checked_composition/resolve.rs`: a pure function with
   no state values, so that it is independent of the integer width:
   ```rust
   pub(crate) enum Local { Skip, Commit, Reject, Refused, CommitFailure }   // decision kinds only
   pub(crate) enum End { Accept, Reject(usize), Refused(usize), CommittedFailure(usize) }
   pub(crate) struct Resolution { pub end: End, pub committed: usize }       // a prefix length: commit components 0..committed
   pub(crate) fn resolve(n: usize, decide: impl FnMut(usize) -> Local) -> Resolution
   ```
   - It scans in component order and calls `decide` only until the step ends, so later components do not run.
   - Then `joint::step` builds its result from `Resolution`. Keep reasons and classes on the caller side.
   - Use `resolve` in `check_couple` too, if its member loop has the same semantics. If the cut-point
     over-approximation cannot use it directly, keep that loop, explain why in a comment and in the report, and
     prove instead that the abstract `couple` obligation is implied by what `check_couple` checks (item 3).
   - `joint_step`'s observable behaviour must not change: identical outcomes on every existing test and fixture,
     and identical output on the Python reference corpus. Add a differential test that runs the old and new `step`
     on every fixture tuple before you delete the old body.
2. **The Verus resolver proof.**
   - Add `verification/verus/composed_domain.rs`, which `#[path]`-includes `resolve.rs` and is annotated with
     `#[cfg_attr(verus_keep_ghost, verus_spec(...))]` in the house style.
   - Prove that `resolve` equals a `spec fn resolve_spec(n, decisions: Seq<Local>)`, and that `committed` is
     exactly the prefix the rules in the guide (:32-45) define.
   - Prove termination and the "later components do not run" property: `decide` is never called past the
     terminating index.
3. **The abstract theorem** (in a new file `verification/verus/composed_domain_theorem.rs`).
   - **The model:** n components and couplings.
     - Each component has a participation predicate, an ownership flag, a domain predicate, `assume_i`, `inv_i`,
       a decision function from (its pre-state and its delivered input) to a `Local` and a post-state, and a
       committed-failure flag.
     - Inputs are delivered by bindings that read the request and, for read-only components, the state of an
       earlier provider.
     - All components decide from the same pre-state.
   - **The obligations, as spec predicates:**
     - **local:** in-domain inputs satisfying `assume_i` and `inv_i` give a Commit or CommitFailure post that
       satisfies `inv_i` and its domain;
     - **assume:** every delivered input is in domain and satisfies `assume_i`, given the global invariants;
     - **couple:** each coupling invariant holds after any committed prefix, including the cut points where an
       outside component can end the step with a CommittedFailure between members.
   - **The theorem:** genesis satisfies all invariants. If the obligations hold, every finite history of joint steps
     keeps every component invariant and every coupling invariant. Every delivered input, including a read-only
     component's delivered state, is in its domain and satisfies its assumption. The theorem text must match the
     guide's claim at :239-242. Reuse the `history` / `joint_induction` idiom.
   - **The smallest pilot first:** three components, coupled on members 0 and 2, with a possible committed failure
     outside the coupling at member 1, one read-only guard and one non-participant. Then the general n-component
     theorem. Report both, labelled with their scope.
   - Connect the abstract step to `resolve_spec`: the abstract joint step must be defined through `resolve_spec`,
     so that the proved resolver and the theorem share the same outcome rules.
4. **The gate.**
   - Add `tools/check_composed_domain.py` and `tools/test_check_composed_domain.py`, following
     `tools/check_checked_assembly.py` and its test.
   - Add the profile `verification/verus/composed-domain.json`, with the reviewed counts and the VIR inventory, and
     a README section with the command pair.
   - `--no-cheating`; nothing assumed; no `external_body`.
   - Do not edit `.github/workflows/verus.yml`; maintainers will wire the gate in.
5. **Bridges.** List each one with its status in `docs/COMPOSED_DOMAIN_PROOF.md` (new; at most 120 lines):

   | Bridge | Status |
   |---|---|
   | Joint-step resolver | Proved, shared source |
   | Local decision tables (Authority) | Tested premise |
   | Manifest parsing | Tested premise |
   | Formula lowering | Tested premise |
   | Validator / planner coverage | Tested premise |
   | `check_couple` cut points vs the abstract couple obligation | Proved, or a tested premise with the reason |

   The document separates the conditional abstract theorem from the proved resolver. It never says "verified
   composition checker".

## Controls
- **A missing outside-failure cut.** In the abstract model, remove the cut-point condition from the couple
  obligation. The theorem proof must fail. Also show a realizable witness from
  `tools/composition_domain_soundness.py`, or from a small new fixture, where the coupling breaks.
- **A CommittedFailure that commits only its own successor,** in `resolve_spec` or in `resolve`. The resolver proof
  must fail, and the existing native test (`joint_tests.rs:179`) and the Python reference must disagree with it.
- **A read-only component written by the step.** The theorem or resolver proof must fail. `joint_tests.rs:210` must
  catch it natively.
- **A resolver that calls `decide` past a Reject.** The "later components do not run" proof must fail.
- **A non-participant treated as Commit.** A proof or a native test must fail.
- **An actual native case that reaches CommittedFailure:** the `documented/lockstep-failure.json` fixture through
  `joint_step`. Keep it as regression evidence.

Run each proof control with the gate's restricted `--verify-module` mode, as `check_checked_assembly.py` does. Record
the failing obligation's name.

## Done when
- The Verus gate passes with nothing assumed, and every control fails at its named obligation.
- These pass, unchanged except for the new tests:
  - `cargo +1.97.1 test -p zeno-fcis-contract --locked`
  - `cargo +1.97.1 test -p zeno-fcis-cli --locked --test contract_check_composition_domain`
  - `python3 tools/test_composition_domain_reference.py`
- `DOMAIN_CLAIM`, `DOMAIN_TRUSTED_BASE` and `DOMAIN_NONCLAIMS`, and the CHANGELOG and release-note sentences, are
  **not** edited here: they feed receipts and release text. Instead, list in the PR the exact replacement text you
  propose for each.

## Out of scope
- Proving the Authority tables, the parser, the lowering or the planner;
- the `/1` assembly profile;
- performance work;
- changing any composition behaviour.

## Common rules (part of every packet)

**Base.** Start from `main` of github.com/TheDarkLightX/ZenoFCIS at commit `c951cd957a831326373c710efe6165ec0f6bb8a3`
(release 2.4.0 plus its status notes). Work on one new branch named in the packet. A later release line (2.5) will
be merged under this work, so keep changes additive:
- prefer new files and new modules;
- keep edits to existing files small and local;
- do not reformat files you did not change (run `rustfmt` on changed files only, never `cargo fmt --all`);
- avoid these paths unless the packet names them: `crates/zeno-fcis-cli/src/`, `crates/zeno-fcis-synthesis/src/finite/execution_v2/`,
  `crates/zeno-fcis-contract/src/`, `crates/zeno-fcis-loop-doors/`, `core-apps/`, `verification/kernel-laws/`, `tools/atdd.py`.

**Where this packet lives.** The packets are on the branch `tasks/2.6-agent-packets` under `docs/agent-tasks/2.6/`.
Start your work from `main` (or from that branch, which adds only the packets). Do not edit the packet files.

**Before editing.** Read `AGENTS.md` and `skills/handoff-completion-gate/SKILL.md` and follow them. State the
definition of done and the checks you will run, at the top of your final report.

**Toolchain.** Rust 1.97.1, as pinned in `rust-toolchain.toml`. Build and test with `cargo +1.97.1 ... --locked`. Use
`CARGO_PROFILE_DEV_DEBUG=0` and at most `-j 2` if the container is small.
- Add no new crates to `Cargo.lock` unless the packet allows it. Never run `cargo update`.
- External tools (Lean, Verus, solvers) use only the exact versions and hashes the repository pins. Verify the
  hashes; never substitute a newer release.

**Forbidden commands.** `git reset --hard`, `git clean -f`, `git checkout --`, `cargo update`, `cargo publish`,
`git tag`, `gh pr merge` and `gh release create`. Do not edit release files, version numbers, `CHANGELOG.md`,
the top-level `README.md` or `docs/RELEASE_NOTES.md` (template READMEs named in a packet are fine). Do not run the full acceptance suite (`python3 tools/atdd.py run --all`);
it is the release gate and runs later on the maintainer's host.

**Proof hygiene.**
- Verus: no `assume`, `admit`, `#[verifier::external_body]`, `#[verifier::external]` or trusted specifications on
  the proved path. Run with `--no-cheating` where the repository does. Do not raise an rlimit beyond the
  repository's recorded convention.
- Lean: no `sorry`, `admit`, `axiom` or `unsafe`. The axiom report may list only the repository's allowed axioms.
- No `unsafe` Rust in new code.

**Controls.** Every new check ships with at least one planted defect: a small, realistic mutation that must make a
named test fail. Show that the mutation builds and that the named test fails for the stated reason. Then show that
the test passes on the real code. A build failure is not a caught defect.

**Evidence words.** State each claim at its true strength, with one of these levels:
- **tested**;
- **finite checked** (every case in a stated finite domain);
- **solver attested**;
- **kernel checked** (Lean kernel);
- **proved** (Verus, nothing assumed).

Name the scope, premises and trust basis next to each claim. A timeout, an incomplete run or a skipped case is never
a pass. Say "exhaustively checked over the stated domain", never "correct by construction".

**Public wording.** This repository is public. In code comments and documents:
- write plain English;
- describe generated code as "LLM-generated" or "synthesized", never "hand-written";
- never use the word "authoring";
- name no AI model, agent or vendor;
- include no local machine paths and no private project names.

**Deliverable.**
- One branch with focused commits. Each commit message ends with a line saying which checks were run.
- The pull request description is the final report (at most 60 lines):
  - definition of done;
  - what was built;
  - each claim with its evidence level;
  - every command run, with its exit code;
  - the controls table (mutation, named test, result);
  - what is not done, and why.
- List every new public document in `release/public-documents.txt` and link it from `docs/INDEX.md`; then
  `python3 tools/check_public_docs.py` and `python3 tools/check_library_docs.py` must pass.
- Open the pull request as a draft. Do not merge it.

