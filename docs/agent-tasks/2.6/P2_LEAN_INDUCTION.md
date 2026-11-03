# Task packet P2: a Lean-kernel-checked inductive claim (restricted linear fragment)

**Revision 2 (2026-10-18).** Corrects acceptance requirements in revision 1 (commit `a38a8ee`). If you started from
revision 1, apply the changes listed for this packet in [README.md](README.md#revision-2) before continuing.

**Branch:** `agent/2.6-lean-induction` **Size:** large **Evidence level of the result:** kernel checked, for the
supported fragment only

## Why (the user's story)
A ZenoFCIS project states invariants such as "counters are never negative" as inductive claims. Today
`zeno-fcis prove` sends them only to SMT solvers:
- CVC5's `unsat` is kept as an attested proposal ("proof output was not independently checked");
- Z3's `unsat` is blocked as unsupported evidence;
- the Lean backend refuses inductive claims outright.

Users who need the strongest evidence level want the step argument (and the history argument built on it)
checked by a small proof kernel. This task adds a Lean exporter for inductive claims in a restricted linear integer
fragment, so that `prove --backend lean` can return `KernelChecked` for claims like the durable counter's claim 600.
Everything outside the fragment stays refused.

## Facts about the current code (verified on the base commit)
- **The pilot claims.**
  - `crates/zeno-fcis-cli/templates/durable-counter/project.zeno`:
    - :9 `type 105 int CounterValue;` has no range, so the full i128 range applies;
    - :28 law 500 `bounded_state on commit, genesis`;
    - :29 law 501 `accepted_increment on accept`;
    - :30 law 502 `committed_failure on failure`;
    - :31 law 503 `reject_no_authority on reject = true`;
    - :32 `claim 600 counters_never_negative all inductive accept [501] failure [502] = pre.100.110 >= 0 && pre.100.111 >= 0;`
  - The second pilot is `crates/zeno-fcis-cli/templates/account-lockout/project.zeno`:
    - :14-16 the declared ranges (Attempts 0..=2, UnixTime 0..=4102444800, LockDeadline 0..=4102445700);
    - :47 law 500;
    - :58 claim 600 `lock_state_stays_consistent all inductive accept [501, 502] failure [503]`.
  - Its README records the current results at :268-287:
    - cvc5: `UNSAT proposal retained`, exit 2;
    - z3: `blocked: UnsupportedEvidence`;
    - lean: `claim 600 does not select compatible lean`.
- **The SMT inductive exporter** is the semantic reference: `export_inductive_smt[_with_limits]` in
  `crates/zeno-fcis-formal-tools/src/lib.rs:1725-1880`.
  - `step_hypotheses` (:1883-1909) resolves the every-commit, accept and committed-failure law groups, refusing
    duplicates and unknown laws (`UnknownAssumedLaw`).
  - Each observed path is an Int with an i128 range assertion and a declared-domain assertion (:1810-1828).
  - The decision kind is split as 1 = accept, 2 = committed failure (:1291-1293, :1837-1841). Each kind's laws are
    implications on that kind (:1845-1864).
  - The invariant is asserted on the pre-state, and its negation on the post-state (:1866-1869).
- **Lean infrastructure that exists.**
  - The tools manifest is `zeno-fcis/tools/2` (:63). `LEAN_VERSION = "4.30.0"` (:69), and
    `LEAN_LINUX_X86_64_TREE_SHA256` is at :71-72.
  - `execute_tool` and `run_lean` are at :2222-2266 and :3344-3364.
  - `classify` (:2686-2720) gives `KernelChecked` only when the runtime tree hash is pinned and the single
    `depends on axioms: [...]` line equals the manifest's `allowed_axioms` exactly (`parse_lean_axioms` :4008-4038).
  - The run record is `formal_run_record` (:2179-2220, `zeno-fcis/formal-run-record/3`). It binds the source hash.
    The theorem identity is bound through the source text (`-- claim-id N`).
- **What refuses Lean today.**
  - The only Lean exporter is test-only: lib.rs:53-54 includes
    `verification/kernel-laws/src/oracle/formal_tools/lean_items.rs`, which accepts only unbounded temporal claims.
    The test at lib.rs:4269-4272 asserts that inductive claims are refused.
  - The normal CLI pairs claim kinds and backends in `crates/zeno-fcis-cli/src/main.rs:1339-1345`, and hard-refuses
    Lean at :1371 (`UnsupportedMode`).
  - `crates/zeno-fcis-cli/tests/cli_adopter_flow.rs:1190` asserts that the retired shallow producer stays refused.
    Keep that test true for the temporal producer.
- **Docs.** `docs/INDUCTIVE_CLAIMS.md:44-45` and :257 say there is no Lean export for inductive claims yet.
  `docs/FORMAL_TOOLS_RC3.md` describes the manifest and the Lean admission.

## Environment setup (as in `.github/workflows/formal-tools.yml`)
1. Download `https://github.com/leanprover/lean4/releases/download/v4.30.0/lean-4.30.0-linux.zip`.
2. Check its SHA-256 against `release/formal-tools-linux-x86_64.sha256`, then unzip it.
3. Check that `bin/lean --version` reports 4.30.0.
4. Check that `cargo +1.97.1 run -p zeno-fcis-cli --locked -- backend inventory-lean <root>` prints the tree hash in
   `release/lean-4.30.0-tree.sha256`.
5. Set `ZENO_FCIS_LEAN` and `ZENO_FCIS_LEAN_ROOT` for the ignored kernel tests.

Use only this distribution. Add no Mathlib and no Lake packages: the emitted file must check with plain `lean` and
core tactics (`omega`, `decide`, `simp`, `intro`, `cases`, `induction`).

## What to build
1. **A new module, `crates/zeno-fcis-formal-tools/src/lean_inductive.rs`,** providing
   `export_inductive_lean(...)`. It has the same inputs and limits as `export_inductive_smt_with_limits`, and the
   same `Exported` result shape, with the backend set to Lean.
2. **The supported fragment** (everything else returns an existing `ExportError`, preferably `UnsupportedMode`,
   with no partial output):
   - claim mode `Inductive` with a relational formula;
   - integer and boolean projections of `pre`, `post`, `command` and `context`, and integer constants;
   - `+`, `-`, unary minus, and multiplication by a constant;
   - `==`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `||`, `!` and `->`;
   - the same in every assumed law.

   Division, remainder, non-constant multiplication, sums, quantifiers, host predicates, text and bytes are
   refused, as are any value forms added later. Match on the expression enums **exhaustively, with no wildcard
   arm**, so that a future expression form fails to compile until someone decides how to handle it.
3. **The semantics must equal the SMT exporter's,** item by item:
   - the i128 range of each observed path;
   - the declared domains (a range or a finite list);
   - strict checked-arithmetic definedness, with the same rule the SMT exporter and `lean_checked_binary` use: an
     arithmetic result outside i128 makes the enclosing law or claim atom undefined, and so not satisfied;
   - boolean encoding;
   - law-group resolution, including the every-commit group;
   - the decision-kind split.

   Write a test that, for every claim in the corpus below, enumerates the set of (path, domain, law, kind) facts
   each exporter emits, in a structured form rather than text, and asserts the two sets are equal.
4. **The emitted Lean file** (one file per claim; no imports beyond core):
   - Header comments: `-- claim-id N`, the project SHA-256, the law-manifest SHA-256 (the hash of the resolved law
     texts and kinds), the exporter identity `zeno-fcis/lean-inductive/1`, and the theorem name.
   - `structure Pre`, `structure Post` (or one `State` structure with the state fields), and the command and context
     fields as `Int` variables, with booleans as `Int` in `{0,1}`, as in the SMT exporter.
   - `def Inv (s : State) : Prop`: the claim formula over the state.
   - `def Admitted (pre post : State) (cmd ctx ...) (kind : Nat) : Prop`. It is the conjunction of the ranges,
     domains and definedness, plus `kind = 1 → accept laws`, `kind = 2 → failure laws`, the every-commit laws and
     `kind = 1 ∨ kind = 2`.
   - `theorem step_preserves : Inv pre → Admitted pre post ... kind → Inv post`, proved by `omega`, after `intro` and
     `cases` on the kind if needed.
   - `def Step (s s' : State) : Prop := (∃ cmd ctx kind, Admitted s s' cmd ctx kind) ∨ s' = s`. The `s' = s`
     disjunct covers rejects and technical refusals; this is a stated runtime premise (see below).
   - **The base case is conditional.** The project definition (`ProjectSpec`) carries no genesis value. Each
     application defines its own initial state (for example `genesis_state()` in
     `crates/zeno-fcis-cli/templates/durable-counter/src/lib.rs:53`), and the library checks it at genesis. So the
     required theorem takes the base case as a hypothesis:
     `theorem history_holds (g : State) (hg : Inv g) : ∀ (h : List State), Chain Step g h → ∀ s ∈ h, Inv s`.
     Prove it by induction on `h` from `step_preserves`. Define `Chain` in the file; do not depend on library
     lemmas that may move.
   - **Optional, only if you also bind the value: a concrete genesis.** The exporter may accept an explicit genesis
     state supplied by the caller, for example the template's reviewed initial state passed as canonical values. It
     then also emits `theorem genesis_holds : Inv genesis` (by `decide` or `omega`) and a corollary
     `history_from_genesis`. The genesis value's canonical bytes and their SHA-256 go in the file header and the run
     record. Without that binding, never emit a concrete genesis theorem.
   - **The claim this evidence supports.** With the conditional form: "if the initial state satisfies the invariant,
     every finite history of admitted steps keeps it". The base case is discharged by the application's existing
     genesis check, for example `the_invariant_holds_on_the_exact_genesis_state` in the template tests
     (docs/INDUCTIVE_CLAIMS.md:284). Name that test as a premise.
   - `#print axioms history_holds` as the only axioms line.
5. **CLI.** The minimal change in `main.rs:1339-1345` and :1371: an `Inductive` claim with the Lean backend calls
   `export_inductive_lean`. A claim outside the fragment still exits 2 with a clear refusal. The temporal shallow
   producer stays refused.
6. **Axioms.** Prefer proofs whose axiom report is a subset of `propext` and `Quot.sound`. If `omega` or `decide`
   pulls in `Classical.choice`, record exactly which theorem does. Do not widen any default allowed list; that is
   a maintainer decision. Test manifests list exactly the reported axioms. `classify` requires equality.
7. **The premises,** stated in the docs and in the emitted header:
   - the runtime refuses a post-state outside its declared domains;
   - a reject or refusal leaves the state unchanged and commits nothing;
   - the observation projection used by the exporter is the one the runtime's law checker reads.

   Point to the existing tests that check each of these, or say that none exists.

## The corpus and controls
- **Positive:** durable-counter claim 600 and account-lockout claim 600. Both must give `KernelChecked` (exit 0)
  through `zeno-fcis prove --backend lean` in an ignored test that uses the pinned Lean, and through a direct
  `execute_tool` test.
- **Control fixtures** go in `crates/zeno-fcis-formal-tools/tests/fixtures/lean-inductive/`. Each is a minimal
  project with a stated **normal result** on the correct exporter, plus one exporter mutation that must **flip**
  that result. A control is valid only if the normal result holds on the unmutated exporter and the mutated
  exporter changes it.

  | # | Fixture | Normal result | Exporter mutation | Result under the mutation |
  |---|---|---|---|---|
  | 1 | A non-inductive invariant: the accept law decrements the counter | Not checked | Assume the invariant on the post-state (pre-to-post substitution) | Wrongly checked |
  | 2 | A failure-kind law that breaks the invariant, with accept and failure laws on different command values | Not checked | Drop the decision-kind split, so all laws are assumed at once and contradict each other | Wrongly checked |
  | 3a | An invariant that holds only because the runtime refuses a post-state outside a declared range (for example `x in 0..=3`, accept `post.x == pre.x + 1`, claim `pre.x <= 3`) | **Checked** | Drop the declared-domain hypotheses | Not checked |
  | 3b | The same law, with the range declared on a different field than the claim reads | Not checked | Attach the domain to the wrong path | Wrongly checked |
  | 4 | A claim whose **own** arithmetic can overflow at a reachable state (for example claim `pre.x * 2 >= 0`, accept `post.x == pre.x + 1`) | Not checked: strict definedness makes the post-state goal undefined near the i128 bound | Drop the definedness conjuncts | Wrongly checked |
  | 5 | A claim that holds only if an out-of-group law (for example the reject law) were assumed | Not checked | Assume every declared law regardless of group | Wrongly checked |
  | 6 | Only if the optional concrete genesis is built: a supplied genesis that violates the invariant | `genesis_holds` fails | Substitute a different genesis value | Wrongly checked |
  | 7 | A claim that reads field 110, where only field 111 is constrained | Not checked | Map path 110 to field 111 in the projection | Wrongly checked |

- **Exporter mutations:** `tools/check_lean_inductive_mutations.py`, following `tools/check_bounded_mutations.py`.
  Apply each mutation in the table to `lean_inductive.rs`, and show that the fixture's result flips. Also show the
  unmutated result first. This needs the pinned Lean; mark it as the ignored lane and run it in your container.
- **Evidence tampering** must give no `KernelChecked`:
  - a changed theorem name;
  - a changed source byte after export;
  - an axiom line listing an extra axiom;
  - a runtime tree hash that is not pinned;
  - a replayed record from an older source.

  Reuse the existing `classify` and retention tests where they apply.

## Done when
- These pass:
  - `cargo +1.97.1 test -p zeno-fcis-formal-tools --locked`
  - `cargo +1.97.1 test -p zeno-fcis-cli --locked --test <the prove test you add>`
  - `clippy -D warnings` on the touched crates
  - the ignored Lean lane, with the pinned runtime

  Copy the exact commands and outputs into the PR.
- Both positive claims, and control 3a, are `KernelChecked`. Every other control fixture and every tampering case
  is refused, and each exporter mutation flips its fixture's result.
- `docs/INDUCTIVE_CLAIMS.md` gets a short new section (at most 60 lines). It covers:
  - the fragment;
  - the theorem chain;
  - the premises;
  - the evidence level ("kernel checked, for this fragment, under these premises");
  - what is still refused.

  The two README statements that there is no Lean export are updated for the two templates only, with their new
  recorded output.

## Out of scope
- Nonlinear arithmetic, division, sums and quantifiers;
- temporal claims;
- changes to SMT behaviour;
- Verus work;
- any change to how existing claims are classified.

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

