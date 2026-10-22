//! `zeno-fcis contract review --gaps`, `contract profile`, `contract lemma`,
//! `contract scenario` and `contract probe`: packets are byte-identical on
//! repeat and grant no authority, every exit code means what the reference
//! says, a limit makes a judge inconclusive rather than passing, and every
//! command is described with its effects.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

#[path = "support/cpmm.rs"]
mod cpmm;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempRoot(PathBuf);
impl TempRoot {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-judges-{label}-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create temp root: {error}"));
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A bounded counter of capacity 2 from the core-component templates.
fn counter(root: &TempRoot) -> PathBuf {
    let base =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
    let app = root.path().join("counter");
    fs::create_dir_all(app.join("v2")).unwrap_or_else(|error| panic!("{error}"));
    fs::create_dir_all(app.join("tests")).unwrap_or_else(|error| panic!("{error}"));
    for (from, to) in [
        ("project.zeno.in", "project.zeno"),
        ("policy.json.in", "v2/policy.json"),
    ] {
        let text = fs::read_to_string(base.join(from)).unwrap_or_else(|error| panic!("{error}"));
        let text = text.replace("${C}", "2").replace("${INSTANCE}", "c2");
        fs::write(app.join(to), text).unwrap_or_else(|error| panic!("{error}"));
    }
    fs::write(
        app.join("tests/decision-scenarios.txt"),
        "# scenario up\n0 | 150 | 1 | accept - 1 | -\n1 | 150 | 1 | accept - 2 | -\n",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    app
}

fn zeno(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn run(args: &[&str], expected: i32) -> Value {
    let output = zeno(args);
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: JSON: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("UTF-8 path"))
}

#[test]
fn transition_lemma_cli_checks_actual_swaps_and_refutes_a_decreasing_step() {
    let root = TempRoot::new("transition-lemma");
    let app = root.path().join("swap");
    fs::create_dir_all(app.join("v2")).unwrap_or_else(|error| panic!("{error}"));
    let write = |limit, bps, bad| {
        fs::write(app.join("project.zeno"), cpmm::project(limit))
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(
            app.join("v2/policy.json"),
            cpmm::policy(limit, bps, bad).to_string(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    };
    write(40, 30, false);
    let result = run(
        &[
            "contract",
            "lemma",
            text(&app),
            "--transition",
            "--lemma",
            cpmm::RELATION,
        ],
        0,
    );
    assert_eq!(result["schema"], "zeno-fcis/contract-transition-lemma/2");
    assert_eq!(result["status"], "checked");
    assert_eq!(result["authority"], "none");
    assert_eq!(result["lemmas"][0]["mode"], "transition-relation");
    assert_eq!(result["lemmas"][0]["admitted"], 68_921);
    assert_eq!(result["lemmas"][0]["checked"], 68_921);
    assert_eq!(result["lemmas"][0]["undefined"], 0);
    assert!(result["lemmas"][0].get("initiation").is_none());
    let stop = run(
        &[
            "contract",
            "lemma",
            text(&app),
            "--transition",
            "--lemma",
            cpmm::RELATION,
            "--max-tuples",
            "1",
        ],
        2,
    );
    assert_eq!(stop["status"], "inconclusive");
    write(3, 0, true);
    let refuted = run(
        &[
            "contract",
            "lemma",
            text(&app),
            "--transition",
            "--lemma",
            cpmm::RELATION,
        ],
        1,
    );
    assert_eq!(refuted["status"], "refuted");
    assert_eq!(
        refuted["lemmas"][0]["counterexample"]["state"],
        serde_json::json!([1, 1])
    );
    assert_eq!(
        refuted["lemmas"][0]["counterexample"]["outcome"]["post"],
        serde_json::json!([2, 0])
    );
}

#[test]
fn review_with_gaps_is_packet_version_three_and_repeats_byte_for_byte() {
    let root = TempRoot::new("gaps");
    let app = counter(&root);
    let first = zeno(&["contract", "review", text(&app), "--gaps"]);
    let second = zeno(&["contract", "review", text(&app), "--gaps"]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert_eq!(first.stdout, second.stdout);
    let packet: Value =
        serde_json::from_slice(&first.stdout).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(packet["schema"], "zeno-fcis/contract-review/4");
    assert_eq!(packet["authority"], "none");
    assert_eq!(packet["gaps"]["admissible"]["full"]["determined"], 12);
    let stderr = String::from_utf8_lossy(&first.stderr);
    assert!(
        stderr.contains("gaps: 12 of 12 tuples determined"),
        "{stderr}"
    );
    assert!(
        stderr.contains("original decisions: 0 example disagreements, 0 law refusals"),
        "{stderr}"
    );
    assert!(
        stderr.contains("scenario coverage: run contract scenario"),
        "{stderr}"
    );
    let out = root.path().join("packet.json");
    let summary = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--out",
            text(&out),
            "--format",
            "json",
        ],
        0,
    );
    assert_eq!(summary["packet_schema"], "zeno-fcis/contract-review/4");
    assert_eq!(summary["summary"]["gaps"]["status"], "measured");
    // Without --gaps the packet stays version 2.
    let plain = run(&["contract", "review", text(&app)], 0);
    assert_eq!(plain["schema"], "zeno-fcis/contract-review/2");
    assert!(plain.get("gaps").is_none());
    let laws = root.path().join("laws.zeno");
    fs::write(&laws, "law 9000 never on any = false;\n").unwrap_or_else(|error| panic!("{error}"));
    let alarm = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--gaps-laws",
            text(&laws),
        ],
        1,
    );
    assert_eq!(alarm["gaps"]["status"], "proposal-false-alarm");
    let over = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--max-work",
            "1",
        ],
        2,
    );
    assert_eq!(over["gaps"]["status"], "inconclusive");
    let bad = root.path().join("bad.zeno");
    fs::write(
        &bad,
        "law 9000 a on any = true; law 9001 b on any = true;\n",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let invalid = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--gaps-laws",
            text(&bad),
        ],
        65,
    );
    assert_eq!(invalid["gaps"]["status"], "invalid-proposals");
    // --gaps-laws needs --gaps.
    assert_eq!(
        zeno(&["contract", "review", text(&app), "--gaps-laws", text(&laws)])
            .status
            .code(),
        Some(64)
    );
}

#[test]
fn proposal_summary_retains_both_measures_and_prints_the_effect() {
    let root = TempRoot::new("proposal-summary");
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/guide-counter");
    let project_before =
        fs::read(app.join("project.zeno")).unwrap_or_else(|error| panic!("{error}"));
    let policy_before =
        fs::read(app.join("v2/policy.json")).unwrap_or_else(|error| panic!("{error}"));
    let laws = root.path().join("proposed.zeno");
    fs::write(&laws, "law 9000 rejects_only_when_needed on reject = context.102.130 == 0 || (command.101.120 == 150 && pre.100.110 == 2) || (command.101.120 == 151 && pre.100.110 == 0);\n").unwrap_or_else(|error| panic!("{error}"));
    let out = root.path().join("review.json");
    let args = [
        "contract",
        "review",
        text(&app),
        "--gaps",
        "--gaps-laws",
        text(&laws),
        "--out",
        text(&out),
    ];
    let human = zeno(&args);
    assert_eq!(human.status.code(), Some(0));
    let terminal = String::from_utf8_lossy(&human.stdout);
    assert!(
        terminal.contains("gaps: 0 of 12 tuples determined"),
        "{terminal}"
    );
    assert!(
        terminal.contains("gaps with proposals: 4 of 12 tuples determined"),
        "assertion failed: missing proposal effect\n{terminal}"
    );
    let mut json_args = args.to_vec();
    json_args.extend(["--format", "json"]);
    let summary = run(&json_args, 0);
    let packet: Value =
        serde_json::from_slice(&fs::read(&out).unwrap_or_else(|error| panic!("{error}")))
            .unwrap_or_else(|error| panic!("{error}"));
    let base = &summary["summary"]["gaps"];
    let after = &base["with_proposals"];
    assert_eq!(base["full"]["determined"], 0);
    assert_eq!(base["full"]["mutants_admitted"], 9);
    assert_eq!(after["full"]["determined"], 4);
    assert_eq!(after["full"]["mutants_admitted"], 3);
    for (counts, measure) in [
        (base, &packet["gaps"]),
        (after, &packet["gaps"]["proposals"]["after"]),
    ] {
        for domain in ["full", "valid"] {
            assert_eq!(
                counts[domain]["tuples"],
                measure["admissible"][domain]["tuples"]
            );
            assert_eq!(
                counts[domain]["determined"],
                measure["admissible"][domain]["determined"]
            );
            assert_eq!(
                counts[domain]["mutants_admitted"],
                measure["mutation"][domain]["admitted"]
            );
            assert_eq!(
                counts[domain]["mutants_caught"],
                measure["mutation"][domain]["caught"]
            );
        }
        assert_eq!(counts["inactive_laws"], measure["activation"]["inactive"]);
        assert_eq!(
            counts["reject_everything_admitted"],
            measure["reject_everything"]["admitted_where_the_contract_decides"]
        );
    }
    assert_eq!(packet["authority"], "none");
    assert_eq!(
        project_before,
        fs::read(app.join("project.zeno")).unwrap_or_else(|error| panic!("{error}"))
    );
    assert_eq!(
        policy_before,
        fs::read(app.join("v2/policy.json")).unwrap_or_else(|error| panic!("{error}"))
    );
    let plain = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--out",
            text(&out),
            "--format",
            "json",
        ],
        0,
    );
    assert!(plain["summary"]["gaps"].get("with_proposals").is_none());
    fs::write(&laws, "law 9000 false_proposal on any = false;\n")
        .unwrap_or_else(|error| panic!("{error}"));
    let alarm = run(&json_args, 1);
    assert_eq!(alarm["summary"]["gaps"]["status"], "proposal-false-alarm");
    assert!(alarm["summary"]["gaps"].get("with_proposals").is_none());
}

#[test]
fn failure_only_reasons_cannot_keep_fully_specified_rejects_ambiguous() {
    let root = TempRoot::new("reason-classes");
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    let laws = root.path().join("closing.zeno");
    // A failure-only reason remains unavailable in a rejection law. Fixing
    // the judge must not weaken the language's independent scope check.
    fs::write(&laws, "law 9000 wrong_scope on reject:202 = false;\n")
        .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
    let invalid = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--gaps-laws",
            text(&laws),
            "--format",
            "json",
        ],
        1,
    );
    assert_eq!(invalid["status"], "error");
    assert!(
        invalid["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("expected refusal message"))
            .contains("declared Reject reason")
    );
    fs::write(
        &laws,
        fs::read(app.join("proposed-laws.txt"))
            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}")),
    )
    .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
    let out = root.path().join("review.json");
    let report = run(
        &[
            "contract",
            "review",
            text(&app),
            "--gaps",
            "--gaps-laws",
            text(&laws),
            "--out",
            text(&out),
            "--format",
            "json",
        ],
        0,
    );
    let gaps = &report["summary"]["gaps"];
    assert_eq!(gaps["full"]["determined"], 0);
    // 32 non-pending states * 8 inputs, 16 pending states * 2
    // unauthorized inputs, plus 12 repeated-officer approvals: 300.
    assert_eq!(gaps["with_proposals"]["full"]["determined"], 300);
    // Ten invalid Approved states remove 10*8 closed rejects.
    assert_eq!(gaps["with_proposals"]["valid"]["determined"], 220);
    assert_eq!(report["authority"], "none");
}

#[test]
fn every_judge_writes_an_advisory_packet_and_exits_by_its_verdict() {
    let root = TempRoot::new("judges");
    let app = counter(&root);
    let app = text(&app);
    let profile = run(&["contract", "profile", app], 0);
    assert_eq!(profile["schema"], "zeno-fcis/contract-profile/2");
    assert_eq!(profile["authority"], "none");
    assert_eq!(profile["status"], "measured");
    assert_eq!(profile["states"]["reachable"], 3);
    let again = zeno(&["contract", "profile", app]);
    assert_eq!(again.stdout, zeno(&["contract", "profile", app]).stdout);
    let inductive = run(
        &["contract", "lemma", app, "--lemma", "post.100.110 <= 2"],
        0,
    );
    assert_eq!(inductive["status"], "inductive");
    let refuted = run(
        &["contract", "lemma", app, "--lemma", "post.100.110 <= 1"],
        1,
    );
    assert_eq!(refuted["status"], "refuted");
    assert_eq!(refuted["lemmas"][0]["verdict"], "false");
    assert_eq!(
        zeno(&["contract", "lemma", app, "--lemma", "pre.100.110 == 0"])
            .status
            .code(),
        Some(65)
    );
    let vacuous = run(
        &[
            "contract", "probe", "implies", app, "--if", "false", "--then", "true",
        ],
        2,
    );
    assert_eq!(vacuous["status"], "vacuous");
    let replayed = run(&["contract", "scenario", app], 0);
    assert_eq!(replayed["schema"], "zeno-fcis/contract-scenario/2");
    assert_eq!(replayed["reject_everything"]["killed"], true);
    assert!(replayed.get("max_tuples").is_none());
    let failing = PathBuf::from(app).join("failing.txt");
    fs::write(
        &failing,
        "# scenario wrong\n0 | 150 | 1 | reject 201 0 | -\n",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let failed = run(&["contract", "scenario", app, "--file", text(&failing)], 1);
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["mutants"]["status"], "withheld");
    let malformed = PathBuf::from(app).join("malformed.txt");
    fs::write(&malformed, "0 | 150 | 1 | accept - 1 | -\n")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        zeno(&["contract", "scenario", app, "--file", text(&malformed)])
            .status
            .code(),
        Some(65)
    );
    let explained = run(
        &["contract", "probe", "explain", app, "--input", "2 150 1"],
        0,
    );
    assert_eq!(explained["schema"], "zeno-fcis/contract-probe/2");
    assert_eq!(explained["result"]["case"], 1);
    assert_eq!(
        zeno(&["contract", "probe", "explain", app, "--input", "2 x 1"])
            .status
            .code(),
        Some(64)
    );
    let decisions = run(&["contract", "probe", "decisions", app], 0);
    assert_eq!(decisions["result"]["inputs"], 12);
    for args in [
        vec!["contract", "profile", app, "--max-tuples", "11"],
        vec![
            "contract",
            "lemma",
            app,
            "--lemma",
            "true",
            "--max-tuples",
            "11",
        ],
        vec![
            "contract",
            "probe",
            "count",
            app,
            "--where",
            "true",
            "--max-tuples",
            "11",
        ],
    ] {
        let over = run(&args, 2);
        assert_eq!(over["status"], "inconclusive", "{args:?}");
        assert_eq!(over["reason"], "domain-over-limit", "{args:?}");
    }
}

#[test]
fn every_judge_is_described_with_its_effects() {
    for (path, reads) in [
        (vec!["contract", "profile"], vec!["application-contract"]),
        (vec!["contract", "lemma"], vec!["application-contract"]),
        (
            vec!["contract", "scenario"],
            vec!["application-contract", "decision-scenarios"],
        ),
        (
            vec!["contract", "probe", "explain"],
            vec!["application-contract"],
        ),
        (
            vec!["contract", "probe", "depends"],
            vec!["application-contract"],
        ),
    ] {
        let mut args = vec!["describe"];
        args.extend(&path);
        let description = run(&args, 0);
        let effects = &description["command"]["effects"];
        assert_eq!(effects["classification"], "declared", "{path:?}");
        assert_eq!(effects["reads"], serde_json::json!(reads), "{path:?}");
        assert_eq!(
            effects["writes"],
            serde_json::json!(["optional-judge-packet"]),
            "{path:?}"
        );
        assert_eq!(effects["executes_tools"], false, "{path:?}");
    }
    let group = run(&["describe", "contract", "probe"], 0);
    assert_eq!(
        group["command"]["effects"]["classification"],
        "command-group"
    );
}

#[test]
fn describe_exit_inventory_includes_the_actual_invalid_judge_input_exit() {
    let root = TempRoot::new("described-exits");
    let app = counter(&root);
    let invalid = zeno(&[
        "contract",
        "lemma",
        text(&app),
        "--lemma",
        "pre.100.110 == 0",
    ]);
    assert_eq!(invalid.status.code(), Some(65));
    let description = run(&["describe", "contract", "lemma"], 0);
    assert_eq!(description["cli_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        description["source_identity"]["kind"],
        "compiled-loop-program-sources/1"
    );
    let identity = description["source_identity"]["sha256"]
        .as_str()
        .unwrap_or_else(|| panic!("source identity"));
    assert_eq!(identity.len(), 64);
    assert!(identity.bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(
        description["exit_codes"]
            .as_array()
            .is_some_and(|codes| codes.iter().any(|row| row["code"] == 65)),
        "{description}"
    );
}
