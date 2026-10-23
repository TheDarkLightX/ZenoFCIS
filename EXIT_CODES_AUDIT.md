# Verdict exit audit

Baseline: `6b3cb417a67c5a0a76714cbdaf805d3f7d7983c3`; locations below refer to that source.
Existing convention: `main.rs:78` defines 0 positive, 1 invalid/refuted, 2 blocked;
`judge_command.rs:48,63,76` documents checked/refuted/UNKNOWN as 0/1/2.
Rust locations are relative to `crates/zeno-fcis-cli/src/`.
Input/usage/IO errors retain their existing codes (including 3, 64 and 65).

| Command(s) | Baseline verdict -> exit | Mapping location |
| --- | --- | --- |
| check; explain | valid 0; invalid/vacuous 1 | main.rs:1006,1393 |
| check --expand | expanded 0; invalid 1; read/expansion failure 3 | table_expansion.rs:25 |
| prove | checked 0; refuted/undefined 1; unknown 2; execution failure 3 | main.rs:1942 |
| counterexample | refuted/undefined 0; checked/unknown 2; failure 3 | main.rs:1942 |
| transform check; replay | equivalent/replayed 0; counterexample/refused 1; inconclusive 2 | transform_command.rs:595,644 |
| transform check-certificates; replay-certificates | checked/replayed 0; counterexample/refused 1; incomplete 2 | transform_command.rs:426,479 |
| optimize | improved 0; refuted/refused 1; no improvement/incomplete 2 | optimize_command.rs:164,211 |
| contract review; --gaps; --gaps-laws | reviewed/measured 0; findings/false-alarm 1; inconclusive/out-of-scope 2 | review_command.rs:291 |
| contract collections; distribution | checked 0; refuted 1; UNKNOWN 2 | judge_command.rs:178,238 |
| contract profile | measured 0; inconclusive 2 | judge_command.rs:258 |
| contract runs; lemma [--transition] | inductive/checked 0; false 1; not-inductive/undefined/vacuous 2 | judge_command.rs:207,297 |
| contract scenario | passed 0; failed 1 | judge_command.rs:326 |
| contract probe implies; reachable; depends | yes/no 0; vacuous/inconclusive 2 | judge_command.rs:356 |
| contract probe explain; count; decisions | answered 0 (counts include undefined/refused); incomplete 2 | judge_command.rs:363 |
| contract refine-check | refines 0; counterexample 1; inconclusive 2 | refine_command.rs:86 |
| contract check-symbolic | proved 0; refuted 1; attested/inconclusive 2 | symbolic_command.rs:186 |
| contract check-composition [--replay] | checked/proved 0; violated/refused 1 | composition_command.rs:290,382,469 |
| contract diff | classified 0 for every change kind; invalid 1 (classification query) | contract_diff.rs:56 |
| contract draft check; finalize | ready 0; not ready 1 | draft_command.rs:224,239 |
| contract draft start/propose/questions/label | workflow completed/assessment returned 0; refused 1 (not a readiness verdict) | draft_command.rs:144 |
| contract adopt; evolve; refresh-receipts; export-program; import; export | completed 0; refused 1; IO 3 (artifact operations) | contract_adopt.rs:225; contract_evolve.rs:387; contract_files.rs:538; tau_command.rs:313 |
| loop candidate; sketch | checked equivalent 0; different/refused 1; limits/inconclusive 2 | loop_command.rs:900; loop_doors_command.rs:1423 |
| loop open; encode; resume; run | workflow/replay completed 0; refused 1; incomplete 2 (attempts remain in report) | loop_command.rs:500,605,752,952 |
| loop pipeline; rules; strategy | qualified/evaluated/packet 0; refuted/refused 1; inconclusive 2 | loop_pipeline.rs:981 |
| loop strategy --kb-op; kb * | query/plan completed or consistent 0; inconsistent/invalid 1; incomplete 2 | loop_strategy_knowledge.rs:124; knowledge_command.rs:354 |
| loop family; transfer | accepted/transferred 0; refuted/failed 1; partial 2 | loop_doors_command.rs:2043,2668 |
| loop meaning | keyed 0 even when same_as_original=false; domain budget refusal 1; stage stop 2 | loop_doors_command.rs:2195 |
| loop evaluate | evaluated 0; refused 1; stage stop 2 (single-input query) | loop_doors_command.rs:2065 |
| loop question --contract | open/determined 0, including answer disagreement; inconsistent/conflict 1; incomplete 2 | loop_doors_command.rs:2334,2415 |
| loop question --specification | determined/question/no-completion 0; refused 1; inconclusive 2 | loop_version_space.rs:581,663 |
| loop teach-back | judged 0, including wrong/vacuous/refused rules and incomplete coverage; inconclusive 2 | loop_doors_command.rs:2458 |
| loop decompose [--compose] | accepted/completed/meets-requirements/realizable 0; other verdicts 1; stage stop 2 | loop_doors_command.rs:2543 |
| loop learn | determined/qualified 0; outside-vocabulary/ambiguous/refuted 1; inconclusive 2; question 3 | loop_learn.rs:369 |
| loop capture open/packet/status/candidate/whole | opened/packet/ready/captured-exact 0; refuted/refused 1; inconclusive 2 | loop_capture.rs:151 |
| synth run/verify; completion find/verify/replay | checked 0; refuted/unrealizable 1; incomplete/unsupported 2 | synth/mod.rs:122,158; synth/completion.rs:243,362 |
| purity; backend verify; doctor | confined/verified/available 0; violations 1; unreadable/unavailable 2 | main.rs:1700,1713,1731 |
| generate [contract] --check | unchanged 0; drift/invalid 1 | main.rs:1323; contract_files.rs |
| trace_game/game.py check/certify/select-guard | checked/certified/counterexample/no guard in class 0; UNKNOWN 2 | tools/trace_game/game.py:141 |
| trace_game/certificate_check.py | any replayed evidence (including counterexample/UNKNOWN/unrealizability) 0; refused 2 | tools/trace_game/certificate_check.py:329 |
| trace_game/run.py; run_v11.py; check.py | harness completed 0; assertion/exception nonzero (test runners, not claim verdicts) | tools/trace_game/run.py:349; run_v11.py:79; check.py:405 |
| ZAL check/qualify; typed/effect check | pass 0; counterexample/fail 1; unfinished 1; unsupported 3 | integrations/zal/run.py:114,155 |
| ZAL diff | same/different/domain-changed/invalid-overlap 0 (comparison field only) | integrations/zal/run.py:114 |
| ZAL why-not | already/found/none-in-scope 0 | integrations/zal/run.py:114 |
| ZAL story/what-if/explain/guide/legend/render/lower/export/ui | query/render/export completed 0; refused 2 (story retains check_status) | integrations/zal/run.py:68,114,117,130,155 |

Commands without verdicts: describe, guide, new, graph, discovery, backend listings,
Commons records/leases/status and workflow/query operations identified above.
