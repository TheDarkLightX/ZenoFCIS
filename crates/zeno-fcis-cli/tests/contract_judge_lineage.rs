//! The judge must observe the replay-validated current Authority, including
//! its adopted program, and must refuse damaged retained lineage inputs.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);
impl Scratch {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-judge-lineage-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|e| panic!("create scratch: {e}"));
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("path is not UTF-8"))
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("parent")))
        .unwrap_or_else(|e| panic!("parent: {e}"));
    fs::write(path, bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn hash(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg("--no-kb")
        .args(args)
        .env_remove("ZENO_FCIS_Z3")
        .env_remove("ZENO_FCIS_CVC5")
        .env_remove("ZENO_FCIS_SYMBOLIC_QUERIES")
        .output()
        .unwrap_or_else(|e| panic!("run CLI: {e}"))
}

fn packet(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("JSON: {e}: {}", String::from_utf8_lossy(&output.stdout)))
}

fn succeeds(args: &[&str]) -> Value {
    let output = cli(args);
    assert!(
        output.status.success(),
        "{args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    packet(&output)
}

fn copy_contract(from: &Path, to: &Path) {
    for file in [
        "project.zeno",
        "v2/policy.json",
        "tests/decision-examples.txt",
    ] {
        write(&to.join(file), &read(&from.join(file)));
    }
}

fn optimize_and_adopt(root: &Path, app: &Path) -> (String, String) {
    let original = root.join("original.zcve");
    let candidate = root.join("candidate.zcve");
    let receipt = root.join("receipt.json");
    let strategy = root.join("strategy.json");
    write(
        &strategy,
        serde_json::to_string(&json!({
            "schema": "zeno-fcis/optimize-strategy/1",
            "phases": [
                {"phase": "fold", "rounds": 1}, {"phase": "share", "rounds": 1},
                {"phase": "select", "rounds": 2}, {"phase": "semantic-merge", "rounds": 2},
                {"phase": "select", "rounds": 2}, {"phase": "fold", "rounds": 1}
            ],
            "limits": {"max_classes": 2000, "max_enodes": 3000,
                "max_extraction_rounds": 4, "max_rewrites_per_round": 1000, "max_work": 3},
            "extractor": "dag-greedy"
        }))
        .unwrap_or_else(|e| panic!("strategy: {e}"))
        .as_bytes(),
    );
    let exported = succeeds(&[
        "contract",
        "export-program",
        text(app),
        "--out",
        text(&original),
        "--format",
        "json",
    ]);
    assert_eq!(exported["status"], "exported");
    let optimized = succeeds(&[
        "optimize",
        "--program",
        text(&original),
        "--strategy",
        text(&strategy),
        "--max-input-tuples",
        "1024",
        "--candidate-out",
        text(&candidate),
        "--receipt",
        text(&receipt),
    ]);
    assert_eq!(optimized["status"], "improved");
    let best_nodes = optimized["detail"]["best"]["nodes"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing best node count"));
    let original_nodes = optimized["detail"]["original"]["nodes"]
        .as_u64()
        .unwrap_or_else(|| panic!("missing original node count"));
    assert!(best_nodes < original_nodes);
    let replay = succeeds(&[
        "transform",
        "replay",
        "--original",
        text(&original),
        "--candidate",
        text(&candidate),
        "--receipt",
        text(&receipt),
    ]);
    assert_eq!(replay["status"], "replayed");
    let adopted = succeeds(&[
        "contract",
        "adopt",
        text(app),
        "--candidate",
        text(&candidate),
        "--receipt",
        text(&receipt),
        "--usage",
        "new-version",
        "--format",
        "json",
    ]);
    assert_eq!(adopted["status"], "adopted");
    (hash(&read(&original)), hash(&read(&candidate)))
}

fn assert_lineage_refused(app: &Path, scenario: &Path, runs: &Path) {
    let operations = [
        vec!["contract", "review", text(app), "--format", "json"],
        vec![
            "contract",
            "scenario",
            text(app),
            "--file",
            text(scenario),
            "--format",
            "json",
        ],
        vec![
            "contract",
            "probe",
            "explain",
            text(app),
            "--input",
            "0 150 0 0 0 0 0 0 0",
            "--format",
            "json",
        ],
        vec!["contract", "profile", text(app), "--format", "json"],
        vec![
            "contract",
            "lemma",
            text(app),
            "--lemma",
            "post.100.110 == 0",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "lemma",
            text(app),
            "--transition",
            "--lemma",
            "post.100.110 == 0",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "runs",
            text(app),
            "--file",
            text(runs),
            "--format",
            "json",
        ],
    ];
    for args in operations {
        let output = cli(&args);
        assert!(!output.status.success(), "lineage bypassed by {args:?}");
        assert_eq!(packet(&output)["status"], "error", "{args:?}");
    }
}

#[test]
fn owner_judges_bind_replayed_current_authority_and_refuse_stale_lineage() {
    let root = Scratch::new("owner");
    let app = root.0.join("app");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../zeno-fcis-shell-sqlite/tests/contracts/intent-owner-admission");
    copy_contract(&source, &app);
    let scenario = root.0.join("scenarios.txt");
    let runs = root.0.join("run-properties.json");
    write(&runs, br#"{"schema":"zeno-fcis/run-properties/1","properties":[{"kind":"predicate","name":"always","formula":"true"}]}"#);
    let runs_false = root.0.join("run-false.json");
    write(&runs_false, br#"{"schema":"zeno-fcis/run-properties/1","observer":{"seen":{"min":0,"max":1,"initial":0,"update":"1"}},"properties":[{"kind":"predicate","name":"never_seen","formula":"observer.seen == 0"}]}"#);
    let mut scenario_bytes = b"# scenario all-valuations\n".to_vec();
    scenario_bytes.extend_from_slice(&read(&app.join("tests/decision-examples.txt")));
    write(&scenario, &scenario_bytes);

    let before = succeeds(&["contract", "review", text(&app)]);
    assert_eq!(before["inputs"]["count"], 128);
    let before_identity = before["tool"]["library_identity_sha256"].clone();
    assert!(before["sources"].get("current_authority").is_none());
    let before_profile = succeeds(&["contract", "profile", text(&app)]);
    let before_lemma = succeeds(&[
        "contract",
        "lemma",
        text(&app),
        "--lemma",
        "post.100.110 == 0",
    ]);
    let before_transition = succeeds(&[
        "contract",
        "lemma",
        text(&app),
        "--transition",
        "--lemma",
        "post.100.110 == 0",
    ]);
    let before_runs = succeeds(&["contract", "runs", text(&app), "--file", text(&runs)]);
    let before_false = cli(&["contract", "runs", text(&app), "--file", text(&runs_false)]);
    assert_eq!(before_false.status.code(), Some(1));
    let before_false = packet(&before_false);
    assert_eq!(before_false["status"], "refuted");

    let (original_sha, candidate_sha) = optimize_and_adopt(&root.0, &app);
    assert_ne!(original_sha, candidate_sha);
    let after = succeeds(&["contract", "review", text(&app)]);
    let subject = &after["sources"]["current_authority"];
    assert_eq!(subject["program_sha256"], candidate_sha);
    assert_eq!(
        subject["retained"]["adoptions"][0]["candidate_sha256"],
        candidate_sha
    );
    assert_eq!(
        subject["retained"]["adoptions"][0]["receipt_sha256"],
        hash(&read(&app.join("v2/adoptions/1/receipt.json")))
    );
    assert_eq!(
        subject["policy_sha256"],
        hash(&read(&app.join("v2/policy.zcve")))
    );
    assert_eq!(
        subject["schema_sha256"],
        hash(&read(&app.join("v2/schema.zcve")))
    );
    assert_eq!(
        subject["authority_identity_sha256"],
        after["tool"]["library_identity_sha256"]
    );
    assert_ne!(after["tool"]["library_identity_sha256"], before_identity);
    assert_eq!(
        before["decision_table"]["rows_digest"],
        after["decision_table"]["rows_digest"]
    );
    assert_eq!(after["inputs"]["count"], 128);
    for i in 0..7 {
        assert_eq!(subject["cases"][i]["index"], i);
        assert_eq!(subject["cases"][i]["reason"], 200 + i);
        assert_eq!(subject["cases"][i]["class"], "Reject");
    }
    assert_eq!(subject["cases"][7]["class"], "Accept");
    let profile = succeeds(&["contract", "profile", text(&app)]);
    assert_eq!(profile["status"], "measured");
    assert_eq!(profile["sources"]["current_authority"], *subject);
    assert_eq!(profile["tables"], before_profile["tables"]);
    let lemma = succeeds(&[
        "contract",
        "lemma",
        text(&app),
        "--lemma",
        "post.100.110 == 0",
    ]);
    assert_eq!(lemma["status"], "inductive");
    assert_eq!(lemma["sources"]["current_authority"], *subject);
    assert_eq!(lemma["lemmas"], before_lemma["lemmas"]);
    let transition = succeeds(&[
        "contract",
        "lemma",
        text(&app),
        "--transition",
        "--lemma",
        "post.100.110 == 0",
    ]);
    assert_eq!(transition["status"], "checked");
    assert_eq!(transition["sources"]["current_authority"], *subject);
    assert_eq!(transition["lemmas"], before_transition["lemmas"]);
    let run_result = succeeds(&["contract", "runs", text(&app), "--file", text(&runs)]);
    assert_eq!(run_result["status"], "inductive");
    assert_eq!(run_result["sources"]["current_authority"], *subject);
    assert_eq!(run_result["properties"], before_runs["properties"]);
    let false_run = cli(&["contract", "runs", text(&app), "--file", text(&runs_false)]);
    assert_eq!(false_run.status.code(), Some(1));
    let false_run = packet(&false_run);
    assert_eq!(false_run["status"], "refuted");
    assert_eq!(false_run["sources"]["current_authority"], *subject);
    assert_eq!(false_run["properties"], before_false["properties"]);
    assert!(
        false_run["properties"][0]["scenario"]
            .as_array()
            .is_some_and(|lines| lines.len() > 1)
    );
    let limited = cli(&[
        "contract",
        "lemma",
        text(&app),
        "--lemma",
        "post.100.110 == 0",
        "--max-tuples",
        "1",
    ]);
    assert_eq!(limited.status.code(), Some(2));
    assert_eq!(packet(&limited)["status"], "UNKNOWN");

    let replay = succeeds(&[
        "contract",
        "scenario",
        text(&app),
        "--file",
        text(&scenario),
    ]);
    assert_eq!(replay["status"], "passed");
    assert_eq!(replay["scenarios"][0]["steps"], 128);
    assert_eq!(replay["sources"]["current_authority"], *subject);
    assert_eq!(replay["reject_everything"]["killed"], true);
    assert!(
        replay["mutants"]["list"]
            .as_array()
            .unwrap_or_else(|| panic!("missing mutants"))
            .iter()
            .any(|item| item["id"] == "m0007" && item["classification"] == "killed")
    );
    let explained = succeeds(&[
        "contract",
        "probe",
        "explain",
        text(&app),
        "--input",
        "0 150 0 0 0 0 0 0 0",
    ]);
    assert_eq!(explained["result"]["authority"]["reason"], 200);
    assert_eq!(explained["sources"]["current_authority"], *subject);
    let gaps = succeeds(&["contract", "review", text(&app), "--gaps"]);
    assert_eq!(gaps["sources"]["current_authority"], *subject);
    assert_eq!(gaps["gaps"]["status"], "measured");

    let program = app.join("v2/adoptions/1/program.zcve");
    let receipt = app.join("v2/adoptions/1/receipt.json");
    let program_good = read(&program);
    let receipt_good = read(&receipt);
    let project = app.join("project.zeno");
    let project_good = read(&project);
    let policy = app.join("v2/policy.json");
    let policy_good = read(&policy);

    let mut bad = program_good.clone();
    *bad.last_mut().unwrap_or_else(|| panic!("empty candidate")) ^= 1;
    write(&program, &bad);
    assert_lineage_refused(&app, &scenario, &runs);
    write(&program, &program_good);
    fs::remove_file(&program).unwrap_or_else(|e| panic!("remove candidate: {e}"));
    assert_lineage_refused(&app, &scenario, &runs);
    write(&program, &program_good);
    let mut bad_receipt = receipt_good.clone();
    *bad_receipt
        .last_mut()
        .unwrap_or_else(|| panic!("empty receipt")) ^= 1;
    write(&receipt, &bad_receipt);
    assert_lineage_refused(&app, &scenario, &runs);
    fs::remove_file(&receipt).unwrap_or_else(|e| panic!("remove receipt: {e}"));
    assert_lineage_refused(&app, &scenario, &runs);
    write(&receipt, &receipt_good);
    // Update the stored digest too: the receipt must be replayed, not merely
    // hash-checked against an attacker-updated rules file.
    let mut forged_receipt: Value =
        serde_json::from_slice(&receipt_good).unwrap_or_else(|e| panic!("receipt JSON: {e}"));
    forged_receipt["usage"]["usage_preserved"] = json!(true);
    let forged_bytes =
        serde_json::to_vec(&forged_receipt).unwrap_or_else(|e| panic!("receipt JSON: {e}"));
    let mut forged_rules: Value =
        serde_json::from_slice(&policy_good).unwrap_or_else(|e| panic!("policy JSON: {e}"));
    forged_rules["adoptions"][0]["receipt_sha256"] = json!(hash(&forged_bytes));
    write(&receipt, &forged_bytes);
    write(
        &policy,
        serde_json::to_string(&forged_rules)
            .unwrap_or_else(|e| panic!("policy JSON: {e}"))
            .as_bytes(),
    );
    assert_lineage_refused(&app, &scenario, &runs);
    let rejected = packet(&cli(&[
        "contract",
        "review",
        text(&app),
        "--format",
        "json",
    ]));
    assert!(
        rejected["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("usage")
    );
    write(&receipt, &receipt_good);
    write(&policy, &policy_good);
    let before_project =
        String::from_utf8(project_good.clone()).unwrap_or_else(|e| panic!("project UTF-8: {e}"));
    let changed_project = before_project.replace(
        "law 510 exact_owner_matches_refusal on reject:200 = context.102.130 == 0;",
        "law 510 exact_owner_matches_refusal on reject:200 = context.102.130 == 1;",
    );
    assert_ne!(changed_project, before_project);
    write(&project, changed_project.as_bytes());
    assert_lineage_refused(&app, &scenario, &runs);
    write(&project, &project_good);
    let mut changed_policy: Value =
        serde_json::from_slice(&policy_good).unwrap_or_else(|e| panic!("policy JSON: {e}"));
    changed_policy["cases"][0]["reason"] = json!(201);
    changed_policy["cases"][1]["reason"] = json!(200);
    write(
        &policy,
        serde_json::to_string(&changed_policy)
            .unwrap_or_else(|e| panic!("policy JSON: {e}"))
            .as_bytes(),
    );
    assert_lineage_refused(&app, &scenario, &runs);
    write(&policy, &policy_good);
    assert_eq!(
        succeeds(&[
            "contract",
            "scenario",
            text(&app),
            "--file",
            text(&scenario)
        ])["status"],
        "passed"
    );
}

#[test]
fn evolved_subjects_check_retained_review_and_migration_before_judging() {
    let root = Scratch::new("evolution");
    let template = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/durable-counter");
    let app = root.0.join("app");
    let created = cli(&["new", text(&app), "--contract", text(&template)]);
    assert!(
        created.status.success(),
        "new: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let renamed = root.0.join("renamed");
    copy_contract(&template, &renamed);
    let project = renamed.join("project.zeno");
    let original = String::from_utf8(read(&project)).unwrap_or_else(|e| panic!("project: {e}"));
    let renamed_project = original.replace("field 110 100 count 105;", "field 110 100 total 105;");
    assert_ne!(renamed_project, original);
    write(&project, renamed_project.as_bytes());
    let evolved = succeeds(&[
        "contract",
        "evolve",
        text(&app),
        "--to",
        text(&renamed),
        "--format",
        "json",
    ]);
    assert_eq!(evolved["kind"], "rename");
    let scenario = root.0.join("scenario.txt");
    write(
        &scenario,
        b"# scenario renamed-counter\n0 0 120 1 | accept - 1 0 | 300 1 0\n",
    );
    let runs = root.0.join("runs.json");
    write(&runs, br#"{"schema":"zeno-fcis/run-properties/1","properties":[{"kind":"predicate","name":"always","formula":"true"}]}"#);
    let operations = [
        vec!["contract", "review", text(&app), "--format", "json"],
        vec![
            "contract",
            "scenario",
            text(&app),
            "--file",
            text(&scenario),
            "--format",
            "json",
        ],
        vec![
            "contract",
            "probe",
            "explain",
            text(&app),
            "--input",
            "0 0 120 1",
            "--format",
            "json",
        ],
        vec!["contract", "profile", text(&app), "--format", "json"],
        vec![
            "contract",
            "lemma",
            text(&app),
            "--lemma",
            "post.100.110 >= 0",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "lemma",
            text(&app),
            "--transition",
            "--lemma",
            "post.100.110 >= 0",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "runs",
            text(&app),
            "--file",
            text(&runs),
            "--format",
            "json",
        ],
    ];
    let review = app.join("v2/evolutions/1/review.txt");
    let good_review = read(&review);
    for args in &operations {
        let observed = succeeds(args);
        assert_eq!(
            observed["sources"]["current_authority"]["retained"]["evolutions"][0]["review_sha256"],
            hash(&good_review),
            "{args:?}"
        );
    }
    write(&review, b"corrupt retained review\n");
    for args in &operations {
        let output = cli(args);
        assert!(
            !output.status.success(),
            "evolution review bypassed by {args:?}"
        );
        assert_eq!(packet(&output)["status"], "error", "{args:?}");
    }
    fs::remove_file(&review).unwrap_or_else(|e| panic!("remove review: {e}"));
    for args in &operations {
        let output = cli(args);
        assert!(
            !output.status.success(),
            "missing evolution review bypassed by {args:?}"
        );
        assert_eq!(packet(&output)["status"], "error", "{args:?}");
    }

    // Existing migrated fixture exercises retained migration loading and
    // replay. Its large domain stays deliberately below the requested judge
    // cap, so a valid lineage is inconclusive rather than a finite proof.
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spend-approval-migrated");
    let migrated = root.0.join("migrated");
    for file in [
        "project.zeno",
        "v2/policy.json",
        "tests/decision-examples.txt",
        "v2/evolutions/1/project.zeno",
        "v2/evolutions/1/v2/policy.json",
        "v2/evolutions/1/review.txt",
        "v2/evolutions/1/migration.json",
    ] {
        write(&migrated.join(file), &read(&fixture.join(file)));
    }
    let migration = migrated.join("v2/evolutions/1/migration.json");
    let good_migration = read(&migration);
    let profile = cli(&["contract", "profile", text(&migrated), "--max-tuples", "1"]);
    assert_eq!(profile.status.code(), Some(2));
    let profile = packet(&profile);
    assert_eq!(profile["status"], "inconclusive");
    assert_eq!(
        profile["sources"]["current_authority"]["retained"]["evolutions"][0]["migration_sha256"],
        hash(&good_migration)
    );
    let operations = [
        vec![
            "contract",
            "profile",
            text(&migrated),
            "--max-tuples",
            "1",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "review",
            text(&migrated),
            "--max-tuples",
            "1",
            "--format",
            "json",
        ],
        vec![
            "contract",
            "lemma",
            text(&migrated),
            "--max-tuples",
            "1",
            "--lemma",
            "post.100.121 >= 0",
            "--format",
            "json",
        ],
    ];
    write(&migration, b"corrupt retained migration\n");
    for args in &operations {
        let output = cli(args);
        assert!(!output.status.success(), "migration bypassed by {args:?}");
        assert_eq!(packet(&output)["status"], "error", "{args:?}");
    }
    fs::remove_file(&migration).unwrap_or_else(|e| panic!("remove migration: {e}"));
    for args in &operations {
        let output = cli(args);
        assert!(
            !output.status.success(),
            "missing migration bypassed by {args:?}"
        );
        assert_eq!(packet(&output)["status"], "error", "{args:?}");
    }
}

#[test]
fn mutant_seeds_compare_to_the_replayed_current_program() {
    let root = Scratch::new("seeds");
    let app = root.0.join("app");
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    copy_contract(&source, &app);
    let (_, candidate_sha) = optimize_and_adopt(&root.0, &app);
    let packet_file = root.0.join("review.json");
    let seeds_dir = root.0.join("seeds");
    let summary = succeeds(&[
        "contract",
        "review",
        text(&app),
        "--out",
        text(&packet_file),
        "--format",
        "json",
        "--mutant-programs",
        text(&seeds_dir),
    ]);
    let seeds = summary["mutant_programs"]["programs"]
        .as_array()
        .unwrap_or_else(|| panic!("missing seeds"));
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0]["id"], "m0027");
    assert_eq!(seeds[0]["compared_original_sha256"], candidate_sha);
    assert_eq!(seeds[0]["program_equivalent"], false);
    let check = cli(&[
        "transform",
        "check",
        "--original",
        text(&app.join("v2/adoptions/1/program.zcve")),
        "--candidate",
        text(&seeds_dir.join("m0027.zcve")),
        "--max-input-tuples",
        "1024",
    ]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(packet(&check)["status"], "counterexample");
}
