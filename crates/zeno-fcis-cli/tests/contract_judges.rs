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
    assert_eq!(packet["schema"], "zeno-fcis/contract-review/3");
    assert_eq!(packet["authority"], "none");
    assert_eq!(packet["gaps"]["admissible"]["full"]["determined"], 0);
    let stderr = String::from_utf8_lossy(&first.stderr);
    assert!(
        stderr.contains("gaps: 0 of 12 tuples determined"),
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
    assert_eq!(summary["packet_schema"], "zeno-fcis/contract-review/3");
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
fn every_judge_writes_an_advisory_packet_and_exits_by_its_verdict() {
    let root = TempRoot::new("judges");
    let app = counter(&root);
    let app = text(&app);
    let profile = run(&["contract", "profile", app], 0);
    assert_eq!(profile["schema"], "zeno-fcis/contract-profile/1");
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
    assert_eq!(replayed["schema"], "zeno-fcis/contract-scenario/1");
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
    assert_eq!(explained["schema"], "zeno-fcis/contract-probe/1");
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
