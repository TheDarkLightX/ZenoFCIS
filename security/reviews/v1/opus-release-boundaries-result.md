# ZenoFCIS V1: advisory security and API review of finite synthesis, CLI and packaging

This is a single-model advisory review. It is not an independent audit or a release approval. I only read files (Read/Glob/Grep) and ran nothing. The commit `cca3f6b3…`, the clean tree and the playbook commit `0123f8ff…` are as you stated; I did not verify them. The hotspot scanner and `security/hotspots-baseline.json` are not in the archive, so I couldn't assign EPI scores or tiers.

**Result:** I found nothing that lets bad input reach verified status or gain production authority. There are two low-severity findings: ZSEC-002 (likely) and ZSEC-001 (a hypothesis only).

Two of your paths don't exist at this commit:
- `cli/src/synthesis.rs` is actually `cli/src/synth/{mod,problem,runner}.rs`.
- There is no `codegen/src/javascript.rs`. JavaScript emission lives in `crates/zeno-fcis-synthesis/src/finite/emit.rs`.

## Answers to your questions

| Question | Result | Key evidence |
|---|---|---|
| Can malformed, out-of-domain, stale, incomplete, unsupported or mismatched input reach verified status? | **No path found** | • Target name is checked before the problem is read: `crates/zeno-fcis-cli/src/synth/mod.rs:176-182`<br>• Problem is capped at 256 KiB before parsing: `synth/problem.rs:83-95`<br>• Unknown fields rejected; atoms must be integers: `problem.rs:23-69,170-199`<br>• All limits and budgets are checked before any enumeration: `finite/mod.rs:468-505`<br>• `verify` rebuilds all five artifacts and requires the same file names and bytes: `synth/mod.rs:275-277,321-362`<br>• Target output must equal the interpreter's results, pass the relation, use canonical spelling and cover every input in order: `synth/runner.rs:286-329`<br>• Budgets, checker source and emitter source are all hashed into identities: `lib.rs:512-538`, `finite/mod.rs:556-576`, `emit.rs:60-68` |
| Can generation escape its intended paths? | **Not for names the generator picks** | • Artifact names are fixed: `synth/mod.rs:222-240`<br>• Files are opened create-exclusive: `main.rs:1071-1075`<br>• Receipts can't go inside the artifact directory: `synth/mod.rs:278-293`<br>• A symlinked `--out` supplied by the operator is followed, as with most CLIs |
| Is supplied code executed before admission? | **No, in synthesis** | • Only the regenerated in-memory source runs, and only after the artifact check: `synth/mod.rs:276-297`, `runner.rs:142,198,241`<br>• Emitters only insert integers and node indices into source: `emit.rs:83-340`<br>• Field names never reach source<br>• Exception: `rc_package.py verify-packaged` builds archive contents (build scripts, tests) by design; PACKAGING.md says archive metadata is not an attestation |
| Do subprocesses get excessive authority? | **Hardening only** | • Children run with full user authority, network access and the inherited environment minus a scrub list: `runner.rs:377-390,458-471`<br>• Limits: 30 s timeout, 32 MiB output cap, process-group kill: `runner.rs:22-25,484-515`<br>• Temp staging issue: ZSEC-001 |
| Are limits applied before costly work? | **Yes, with small ordering issues** | The empty-output check and the receipt-path check run only after full synthesis: `synth/mod.rs:256-263,278-293` |
| Can another backend grant authority or silently use a different contract? | **Synthesis targets: no. Codegen Python adapters: yes, see ZSEC-002** | • Every result and receipt says `"authority":"none"`: `synth/mod.rs:106,239,315`<br>• The template routes synthesized output through typed staging and the law checker: `templates/durable-counter/src/program.rs:60-97`, `src/lib.rs:62-71`<br>• All three languages must produce identical stdout hashes: `tools/check_synthesis.py:79-82` |
| Are canonical IDs, budgets, rejection order and determinism preserved? | **Yes (from source)** | • Alternatives and holes are sorted canonically: `finite/mod.rs:150-164`, `lib.rs:67-77,173-176,502-510`<br>• JSON key order is stable (no `indexmap` in `Cargo.lock:908-918`)<br>• First-error order is tested: `finite/choice_tests.rs:208-252`, `tests/finite.rs:291-348` |

## Findings

**ZSEC-002 (likely, low; the "strict" claim is unsupported): generated Python adapters convert values instead of checking their types**
- **Where:** `crates/zeno-fcis-codegen/src/python.rs`
  - Bool, lines 143-157: `bool(self.value)`, and `cls(v[1])` without a type check.
  - Integers, lines 159-209: `int(self.value)`, and no type check on `v[1]`.
  - Bytes and text, lines 211-281: `bytes(value)` and `str(value)`.
  - Enum ordinals, lines 307-312: accept `True` or `1.0`.
- **Contradicted claim:** "strict `to_value`/`try_from_value`" in `python.rs:3-4` and `docs/CODEGEN_TYPED_ADAPTERS.md:20-22`.
- **Examples** (fixture types from `codegen/src/fixture.rs:21-70`):
  - `Flag("false").to_value()` gives `("bool", True)`.
  - `Amount(2.9)` gives 2, and `Amount(True)` gives 1.
  - `Blob(3)` gives three zero bytes, and `Label(None)` gives `"None"`.
  - `Amount.try_from_value(("u128", 2.5))` and `Tag.try_from_value(("enum", 7, True))` are both accepted.

  All of these then encode to canonical, in-range bytes that the Rust side accepts.
- **Impact:** a Python producer can silently send a value different from what it meant. Range checks still apply and no authority is granted. The existing test `generated-code-tests/tests/python_replay.rs:43-80` only covers ranges, lengths and ASCII, and `replay()` only exercises decoding.
- **Unverified:** whether any supported deployment uses these adapters to produce data (the docs call the codec a replay-parity tool, `CODEGEN_TYPED_ADAPTERS.md:91-92`). I did not execute the examples.
- **Smallest fix:** emit exact type checks with no conversion:
  - integers: `type(x) is int`, which also rejects bool;
  - Bool: `type(v) is bool`;
  - Bytes and Text: `type(b) is bytes` and `type(s) is str`;
  - enum, sum and record ids: `type(...) is int`;
  - on failure, raise `AdapterError("type_mismatch")`.
- **Regression test:** add `rejects(..., "type_mismatch")` for each of the seven examples to `python_outgoing_schema_bounds_are_enforced`. Today those calls return values, so the test fails. Restoring `bool(self.value)` must make it fail again.

**ZSEC-001 (hypothesis, low): the conformance temp directory is only made private after it's created**
- **Where:** `crates/zeno-fcis-cli/src/synth/runner.rs`
  - Lines 399-411: the directory `zeno-fcis-synth-<pid>-<seq>` is created with `fs::create_dir` (normal permissions) and only then set to 0700.
  - Lines 142-143, 198-199 and 239-242: `transition.*` and `fixture.*` are written with `fs::write`, which follows symlinks and truncates.
  - By contrast, `execute` already uses `create_new` (lines 441-457).
- **How it could be abused:** needs a shared temp directory, a process umask that lets group or others write, and a local co-tenant who wins the race. That co-tenant plants a symlink with one of those file names. The write then overwrites the symlink's target with generated source, and conformance still passes. With any umask, pre-creating the predictable directory name just makes the run fail closed.
- **Unverified:** whether a supported deployment uses a permissive umask, and whether the race is practical. No authority or verdict changes either way.
- **Smallest fix:** create the directory with `DirBuilder` and `mode(0o700)`, and write the files with `OpenOptions::new().write(true).create_new(true)`.
- **Regression test:** move these writes into one helper. With a pre-placed symlink, the helper must return `AlreadyExists` and leave the target unchanged. This fails with `fs::write` and passes with `create_new`.

**Chains:** none. The two findings share no fact that one provides and the other needs.

## Leads I checked and ruled out (false positives)
- **Code injection into emitted target code:** only integers and node indices are inserted (`emit.rs:83-340`). Field names are restricted to `[A-Za-z0-9_.]` and only go into manifest JSON (`problem.rs:145-169`).
- **Code injection through codegen Python:**
  - Identifiers must match `SchemaName` (`[A-Za-z_][A-Za-z0-9_]{0,95}`, `zeno-fcis-schema/src/ids.rs:91-111`), and Python keywords are rejected (`python.rs:695-706`).
  - Vector names are static and bytes are rendered as hex (`vectors.rs:643-697`).
  - A schema type named like a generated helper (e.g. `decode`, `AdapterError`) fails closed.
- **Tampered source plus a matching manifest passing `verify`:** blocked by byte-for-byte regeneration (`synth/mod.rs:321-362`), and exercised in `tools/check_synthesis.py:157-168`.
- **An out-of-domain candidate output aborting or passing search:** it is rejected inside evaluation (`ir.rs:269-271`, `finite/mod.rs:668-709`). Relation traps are caught earlier by the full realizability pass (`finite/mod.rs:513-536`).
- **Non-canonical target output (`+5`, `05`, `-0`, extra or missing lines):** rejected by re-spelling, line count and coverage checks (`runner.rs:288-326`). CRLF line endings are tolerated, but values are still exact.
- **Dependency graph bypass** (internal crate falling back to the registry, local shadow package, git or path patch of an external crate): blocked in `tools/check_generated_application.py:39-53` and tested in `tools/test_generated_application.py:42-71`.
- **Crate archive with path traversal, links or duplicate normalized paths:** blocked in `rc_package.py:964-979` with `filter="data"`, and tested in `test_generated_application.py:123-139`.
- **`status: passed` despite dirty archives:** documented as intended (`PACKAGING.md:74-76`, `LLM_USAGE.md:62-65`).
- **`find()` running a binary from the current directory via an empty PATH entry** (`runner.rs:331-339`): this is normal POSIX behaviour, not a vulnerability. Skipping non-absolute PATH entries would be an easy hardening.
- **`NODE_OPTIONS` or `PYTHONPATH` reaching the child:** scrubbed (`runner.rs:377-390`), and Python runs with `-I` (line 202). Checked by `tools/check_synthesis.py:122-141`.

## Bounded memory and process risks (not findings)
- **`max_steps` counts graph nodes only** (`finite/mod.rs:484-505`). Per-candidate overhead is capped by the hard limits, not by the step budget: `Sketch::close` scans (`:189-222`), plus assignment cloning, SHA-256 and counterexample records (`lib.rs:406-452`).
  - Worst case derived from source, at `--max-assignments 1000000`: a ~30 KB problem (two 1000-alternative `int` holes, 62 single-alternative holes, a 3-node relation) declares 6.8×10⁷ steps.
  - That problem still forces 10⁶ program closes, about 2×10⁶ hashes, and 64 MB of counterexample records, plus temporary copies (`lib.rs:384-388`, `synth/mod.rs:217`).
  - The default budget of 100 000 is ten times smaller. I did not measure time or memory.
- **Largest vector corpus:** 65 536 cases × up to 32 integers, held as a JSON tree and pretty-printed (`synth/mod.rs:238`).
- **Tool hashing:** `binary_hash` reads up to 256 MiB into memory, twice per tool (`runner.rs:340-370`).
- **Output capture:** output files can overshoot 32 MiB between 5 ms polls, though the final read is capped (`runner.rs:484-540`). A Rust verify runs up to four 30-second subprocesses.
- **Ctrl-C:** the child runs in its own process group (`runner.rs:465`), so a terminal Ctrl-C to the CLI leaves the child running and the temp directory behind.
- **Crate extraction:** `extract_checked_crate` has no cap on member count or size (`rc_package.py:959-979`). This only matters if `verify-packaged` is given untrusted archives.

## Unsupported or evidence-gated deployment claims
- **Conformance runs are not a sandbox.**
  - Linux-only (`runner.rs:421-428`), with no resource limits, namespaces or network denial.
  - Tool identity is the executable's hash plus its self-reported version; no digest is pinned.
  - The linker, shared libraries, rustup resolution and Python site-packages are outside the recorded identity (`runner.rs:202,362`). Python runs with `-I` but not `-S`.
- **Receipts only cover admitted inputs.**
  - Rust and Python rejection of bad calls (wrong arity; bool, int subclass or list for Python) is supported by source only (`emit.rs:94-105,163-171`).
  - JavaScript has an integration test, but it only runs via ATDD `--ignored` (`tests/synthesis_javascript.rs:40-155`, `tools/atdd.py:88-93`).
- **`atomic_create` is create-exclusive, not crash-safe.** `run` writes the five files one at a time (`synth/mod.rs:267-269`), and a partial set fails closed later.
- **Contract adequacy is the owner's call.** A trivially true relation gets a valid "passed" receipt (`finite/mod.rs:1-5`).
- **The template compiles the checked-in `synthesized/transition.rs` directly** (`templates/durable-counter/src/lib.rs:16-17`).
  - Tying it to `synthesis.json` is a project obligation (`README.md:69-71`).
  - The app's tests catch behaviour changes (`tests/lifecycle.rs:95`), but not side effects added to that file.
- **Packaged-application receipts are not hermetic** (Cargo config files, CARGO_HOME; `PACKAGING.md:80-90`). The development path `check_generated_application.py:167` also passes the unscrubbed environment.

## Coupling with your 1.0.0 metadata change
- `crates/zeno-fcis-cli/templates/durable-counter/Cargo.toml.in:12-38` has 26 `=1.0.0-rc.3` pins. `check_generated_application.py:103-106` requires `=<workspace version>`, so both application gates will fail closed until these are updated.
- The template's `profile.rs:32-72` hashes `Cargo.toml`, so the tutorial's policy identity changes and it needs a new database (`README.md:57-58`). `README.md:28-30` still says "RC3".
- Synthesized artifacts don't contain the crate version. They only need regenerating if the bytes of `finite/mod.rs`, `finite/ir.rs` or `emit.rs` change.

## Not reviewed, and non-claims
- **Your areas:** SQLite, authorization and open-issue acceptance.
- **Crates assumed correct:** codec, crypto, value.
- **Crates not reviewed:** formal-tools, spec, bootstrap, and the codegen Rust, render and adapter modules.
- **Template files not read in full:** `laws`, `delivery`, `main`, and the test bodies.
- **`rc_package.py build()`:** SBOM, rustdoc, binary and source archives, npm checks.
- **Other files:** `release_manifest.py`; CI workflows (grep only); `tests/search.rs` and `cli_adopter_flow.rs` bodies.
- **Nothing was executed:** no tests, fuzzing, Miri, cargo-deny or cargo-audit, and no timing or memory measurement. How rustup, Node and CPython actually behave is unverified.
- **Non-claim:** a clean result here doesn't mean there are no vulnerabilities.

**Recommendation:** before 1.0.0, either fix ZSEC-002 (a generator-only change) or correct the "strict" wording. ZSEC-001 is cheap hardening that can ship in 1.0.x. There are no production-authority blockers in this slice.

**Next step:** implement both regression tests, then measure a worst-case synthesis run at the hard limits.

## Schema fragments

`epi`, `tier` and `epi_at_discovery` are deliberately `null`, which the schema rejects, until you fill them from the baseline.

```json
{"hotspot_triage":[
{"path":"crates/zeno-fcis-synthesis/src/finite/mod.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["resource-bounds-before-work","search-soundness","budget-binding","determinism"],"evidence_refs":["finite/mod.rs:468-536","finite/mod.rs:556-576","finite/mod.rs:655-729","tests/finite.rs:291-596","finite/choice_tests.rs:142-270"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-synthesis/src/finite/ir.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["type-admission","checked-arithmetic","domain-admission"],"evidence_refs":["ir.rs:86-132","ir.rs:231-311","finite/evaluation_tests.rs:16-81"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-synthesis/src/finite/emit.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["code-generation-injection","cross-language-semantics","abi-admission"],"evidence_refs":["emit.rs:60-340","tests/synthesis_javascript.rs:40-155","tools/check_synthesis.py:79-119"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-synthesis/src/lib.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["canonical-order","certificate-binding","indeterminate-handling"],"evidence_refs":["lib.rs:58-201","lib.rs:394-538"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-cli/src/synth/problem.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["json-admission","length-bounds","number-admission"],"evidence_refs":["problem.rs:23-199","synth/tests.rs:38-54"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-cli/src/synth/mod.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["artifact-integrity","path-handling","exit-classification","receipt-placement"],"evidence_refs":["synth/mod.rs:175-362","tools/check_synthesis.py:157-192"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-cli/src/synth/runner.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["subprocess-authority","output-validation","temporary-files","tool-identity","environment-scrub"],"evidence_refs":["runner.rs:101-540","synth/tests.rs:55-155","tools/check_synthesis.py:122-141"],"finding_ids":["ZSEC-001"],"deferred_reason":null},
{"path":"crates/zeno-fcis-cli/src/main.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["file-creation","generation-paths","exit-classification"],"evidence_refs":["main.rs:417-465","main.rs:499-595","main.rs:1071-1102"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-codegen/src/python.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["code-generation-injection","cross-language-contract","decoder-bounds"],"evidence_refs":["python.rs:143-325","python.rs:670-893","vectors.rs:643-697","zeno-fcis-schema/src/ids.rs:91-111","generated-code-tests/tests/python_replay.rs:43-80"],"finding_ids":["ZSEC-002"],"deferred_reason":null},
{"path":"crates/zeno-fcis-codegen/src/javascript.rs","epi":null,"tier":null,"result":"excluded","categories_checked":["existence"],"evidence_refs":["Glob crates/zeno-fcis-codegen/** shows no javascript.rs; JS emission is finite/emit.rs:207-341"],"finding_ids":[],"deferred_reason":"file absent at reviewed commit"},
{"path":"tools/check_generated_application.py","epi":null,"tier":null,"result":"reviewed","categories_checked":["dependency-identity","resolver-overrides","subprocess-invocation"],"evidence_refs":["check_generated_application.py:33-150","tools/test_generated_application.py:17-110"],"finding_ids":[],"deferred_reason":null},
{"path":"tools/rc_package.py","epi":null,"tier":null,"result":"reviewed","categories_checked":["archive-extraction","package-identity","compiler-environment","receipt-claims"],"evidence_refs":["rc_package.py:959-1147 only; build() SBOM/rustdoc/binary/source archives not analyzed","tools/test_generated_application.py:113-217","docs/PACKAGING.md:62-90"],"finding_ids":[],"deferred_reason":null},
{"path":"tools/check_synthesis.py","epi":null,"tier":null,"result":"reviewed","categories_checked":["hostile-case-coverage","cross-language-differential"],"evidence_refs":["check_synthesis.py:17-195"],"finding_ids":[],"deferred_reason":null},
{"path":"crates/zeno-fcis-cli/templates/durable-counter/src/program.rs","epi":null,"tier":null,"result":"reviewed","categories_checked":["authority-mapping","synthesized-code-consumption"],"evidence_refs":["program.rs:36-98","src/lib.rs:16-17,48-99","profile.rs:32-76","tests/lifecycle.rs:95"],"finding_ids":[],"deferred_reason":null}],
"findings":[
{"id":"ZSEC-001","title":"Conformance temp directory becomes 0700 only after creation; owned files are written with symlink-following truncating opens","status":"hypothesis","severity":"low","claim_level":"shell-enforced","epi_at_discovery":null,"categories":["temporary-files","path-handling"],"cwes":["CWE-379","CWE-59"],"cves":[],"cvss_v4":null,"epss":null,"kev":"not-applicable","ssvc_decision":null,
"locations":[{"path":"crates/zeno-fcis-cli/src/synth/runner.rs","start_line":399,"end_line":411},{"path":"crates/zeno-fcis-cli/src/synth/runner.rs","start_line":142,"end_line":143},{"path":"crates/zeno-fcis-cli/src/synth/runner.rs","start_line":198,"end_line":199},{"path":"crates/zeno-fcis-cli/src/synth/runner.rs","start_line":239,"end_line":242}],
"entry":"zeno-fcis synth verify -> TargetRunner::run -> Temp::new","preconditions":["shared temp_dir on a multi-user host","verifier umask grants group/other write","co-tenant wins create_dir->chmod race on predictable zeno-fcis-synth-<pid>-<seq>"],
"reachable_path":["runner.rs:405 fs::create_dir (0777 & ~umask)","co-tenant plants symlink named transition.rs/fixture.rs/fixture.py/transition.mjs/fixture.mjs","runner.rs:409 chmod 0700 keeps planted entry","runner.rs:142/199/241-242 fs::write follows and truncates target","hash/import through same symlink still match; verify passes"],
"impact":"Overwrite of one verifier-writable file with generated source; no authority or verdict change; predictable-name pre-creation fails closed under any umask","reliability":"schedule-dependent",
"evidence_refs":["runner.rs:441-457 execute already uses create_new","docs/LANGUAGE_NEUTRAL_SYNTHESIS.md:253-255 0700 claim"],
"reproducer":{"kind":"missing","safe_boundary":"unit test under umask 000 with a symlink to a scratch file in a private test directory","evidence_ref":"not executed; read-only review"},
"requires":["local unprivileged co-tenant","permissive verifier umask"],"provides":["overwrite of one victim-writable file with generated source"],
"mitigation":"DirBuilder with DirBuilderExt::mode(0o700); write owned files via OpenOptions write+create_new; regression: pre-placed symlink makes the owned-write helper return AlreadyExists with target unchanged (fails with fs::write)",
"residual_risk":"Same-user processes, setsid escape from process group, and disk overshoot between polls remain"},
{"id":"ZSEC-002","title":"Generated Python typed adapters coerce values instead of enforcing exact types, contrary to the documented strict conversion contract","status":"likely","severity":"low","claim_level":"unsupported-claim","epi_at_discovery":null,"categories":["cross-language-contract","input-type-validation"],"cwes":["CWE-1287"],"cves":[],"cvss_v4":null,"epss":null,"kev":"not-applicable","ssvc_decision":null,
"locations":[{"path":"crates/zeno-fcis-codegen/src/python.rs","start_line":143,"end_line":157},{"path":"crates/zeno-fcis-codegen/src/python.rs","start_line":159,"end_line":209},{"path":"crates/zeno-fcis-codegen/src/python.rs","start_line":211,"end_line":281},{"path":"crates/zeno-fcis-codegen/src/python.rs","start_line":283,"end_line":325},{"path":"docs/CODEGEN_TYPED_ADAPTERS.md","start_line":20,"end_line":22}],
"entry":"Python producer constructing generated adapter values or calling try_from_value on caller-built tuples, then zcve.encode","preconditions":["producer uses generated python/<module>.py adapters","caller supplies non-exact Python type"],
"reachable_path":["python.rs:148 Flag('false').to_value() -> ('bool', True)","python.rs:165 Amount(2.9) -> 2; Amount(True) -> 1","python.rs:217 Blob(3) -> 3 zero bytes; python.rs:248 Label(None) -> 'None'","python.rs:177-181 u128 accepts 2.5; python.rs:307-312 enum ordinal accepts True","zcve.encode emits canonical in-range bytes accepted by Rust admission"],
"impact":"Silent value substitution by Python producers; range/length bounds hold; no authority granted","reliability":"deterministic",
"evidence_refs":["codegen/src/fixture.rs:21-70","generated-code-tests/tests/python_replay.rs:43-80 covers range/length/ASCII only","python.rs:611-637 replay exercises decode only","docs/CODEGEN_TYPED_ADAPTERS.md:91-92"],
"reproducer":{"kind":"source-trace","safe_boundary":"inert python3 -c against generated codegen_fixture in generated-code-tests/python","evidence_ref":"assertions listed in mitigation; not executed"},
"requires":["Python producer using generated adapters"],"provides":["well-formed ZCVE value differing from caller intent"],
"mitigation":"Emit exact type checks (type(x) is int excluding bool; bool; bytes; str; int ordinals/field ids) raising AdapterError('type_mismatch'); regression adds rejects(...,'type_mismatch') for Flag('false'), Amount(2.9), Amount(True), Label(None), Blob(3), Amount.try_from_value(('u128',2.5)), Tag.try_from_value(('enum',7,True))",
"residual_risk":"zcve._encode still coerces raw caller tuples; Python codec remains non-production"}]}
```

`tool_runs`, for you to record:
- `tools/security_hotspots.py scan`: **unavailable** (not in the archive).
- Every fixed gate from Phase 1 of `docs/SECURITY_REVIEW_PLAYBOOK.md`: **not-run** (Read/Glob/Grep only).