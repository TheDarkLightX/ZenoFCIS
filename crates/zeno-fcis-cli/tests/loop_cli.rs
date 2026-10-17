//! `zeno-fcis loop` through the real binary: open, candidate, run with each
//! proposer, resume, encode, exit codes and the session directory.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-loop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, bytes).unwrap_or_else(|error| panic!("write {name}: {error}"));
        path
    }

    fn path(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn artifact(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/i128-programs")
        .join(format!("{name}.zcve"))
}

fn run(arguments: &[&str]) -> (Option<i32>, Value) {
    let output: Output = Command::new(CLI)
        .args(arguments)
        .env("ZENO_FCIS_TEST_SECRET", "SECRET-do-not-leak-9c1e")
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"));
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("stdout is one JSON document: {error}: {output:?}"));
    (output.status.code(), report)
}

fn open(session: &str, original: &Path, extra: &[&str]) -> (Option<i32>, Value) {
    let original = original.to_string_lossy().into_owned();
    let mut arguments = vec![
        "loop",
        "open",
        "--original",
        &original,
        "--session",
        session,
    ];
    arguments.extend_from_slice(extra);
    run(&arguments)
}

fn candidate(session: &str, candidate: &str) -> (Option<i32>, Value) {
    run(&[
        "loop",
        "candidate",
        "--session",
        session,
        "--candidate",
        candidate,
    ])
}

/// A benchmark case's program in the fixture vocabulary.
fn case_program(id: &str, side: &str) -> Value {
    let fixtures: Value = serde_json::from_str(include_str!("../../../docs/benchmarks/cases.json"))
        .unwrap_or_else(|error| panic!("cases.json: {error}"));
    let case = fixtures["cases"]
        .as_array()
        .and_then(|cases| cases.iter().find(|case| case["id"] == id))
        .unwrap_or_else(|| panic!("case {id}"));
    json!({
        "inputs": case["input_domains"],
        "outputs": case["output_domains"],
        "nodes": case[side]["nodes"],
        "roots": case[side]["roots"],
    })
}

#[test]
fn an_agent_drives_the_loop_one_candidate_at_a_time_and_resumes() {
    let directory = Directory::new();
    let session = directory.path("session");
    let (exit, report) = open(&session, &artifact("boolean-kernel-original"), &[]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["schema"], "zeno-fcis/transform-loop/1");
    assert_eq!(report["status"], "opened");
    assert_eq!(report["authority"], "none");
    let request_id = report["detail"]["request_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert_eq!(request_id.len(), 64);
    // checked-i128-v1 is the default profile (G12).
    assert_eq!(report["detail"]["request"]["profile"], "checked-i128-v1");
    assert_eq!(report["detail"]["request"]["policy"]["hosted"], "disabled");
    assert_eq!(report["detail"]["view"]["original"]["nodes"], 16);
    for name in [
        "request.json",
        "original.zcve",
        "ledger.jsonl",
        "ledger.head",
    ] {
        assert!(directory.0.join("session").join(name).is_file(), "{name}");
    }
    // Opening twice is refused: the directory is not empty.
    let (exit, report) = open(&session, &artifact("boolean-kernel-original"), &[]);
    assert_eq!(exit, Some(3), "{report}");
    assert_eq!(report["status"], "io-error");

    // A wrong candidate: a replayed counterexample, exit 1, incumbent unchanged.
    let swapped = {
        let program = json!({
            "inputs": [{"kind": "Bool"}, {"kind": "Bool"}, {"kind": "Bool"}, {"kind": "Bool"}],
            "outputs": [{"kind": "Bool"}, {"kind": "Bool"}, {"kind": "Bool"}],
            "nodes": [["Input", 0], ["Input", 1], ["Input", 2], ["Input", 3], ["Not", 0], ["Not", 2],
                      ["And", 4, 5], ["Not", 6], ["Not", 1], ["Not", 3], ["And", 8, 9], ["Not", 10],
                      ["And", 6, 10], ["Not", 12]],
            "roots": [11, 7, 13],
        });
        directory.write("swapped.json", program.to_string().as_bytes())
    };
    let (exit, report) = run(&[
        "loop",
        "candidate",
        "--session",
        &session,
        "--candidate-json",
        &swapped.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "different");
    let feedback = &report["detail"]["feedback"];
    assert_eq!(feedback["kind"], "verified-difference");
    assert_eq!(feedback["witness"]["ordinal"], 1);
    assert_eq!(feedback["witness"]["request_id"], request_id);
    assert_eq!(report["detail"]["incumbent"]["kind"], "original-admitted");
    assert_eq!(report["detail"]["status"], "no-checked-improvement");
    assert!(directory.0.join("session/witnesses/0.json").is_file());

    // The genuine candidate: exit 0, a checked improvement.
    let good = artifact("boolean-kernel-candidate");
    let (exit, report) = candidate(&session, &good.to_string_lossy());
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "checked-improvement");
    assert_eq!(
        report["detail"]["outcome"]["cost"],
        json!({"nodes": 7, "bytes": 764})
    );
    assert_eq!(
        report["detail"]["outcome"]["selection"]["selection"],
        "checked-improvement"
    );
    assert_eq!(
        report["detail"]["incumbent_program"]["nodes"]
            .as_array()
            .map(Vec::len),
        Some(7)
    );
    assert_eq!(report["detail"]["incumbent"]["kind"], "checked-replacement");
    assert_eq!(report["detail"]["status"], "best-checked-so-far");
    assert_eq!(
        report["detail"]["all_feedback"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(report["detail"]["accounting"]["attempts"], 2);
    assert_eq!(report["detail"]["accounting"]["model_calls"], 0);
    assert!(directory.0.join("session/receipts/1.json").is_file());

    // The same candidate again is a duplicate; a worse one is no improvement.
    let (exit, report) = candidate(&session, &good.to_string_lossy());
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "duplicate-of-attempt:1");
    let (exit, report) = candidate(
        &session,
        &artifact("boolean-kernel-padded").to_string_lossy(),
    );
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(
        report["status"],
        "equivalent-without-improvement:original-cost-guard"
    );
    assert_eq!(report["detail"]["incumbent"]["cost"]["nodes"], 7);

    // Resume replays the incumbent and reports true strength.
    let (exit, report) = run(&["loop", "resume", "--session", &session]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "resumed");
    let resumed = &report["detail"]["report"];
    assert_eq!(resumed["status"], "best-checked-so-far");
    assert_eq!(resumed["incumbent"]["cost"]["nodes"], 7);
    assert_eq!(resumed["accounting"]["attempts"], 4);
    assert_eq!(resumed["accounting"]["checks"], 3);
    assert!(
        resumed["accounting"]["replays"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert_eq!(resumed["session"], json!({"state": "open"}));
    assert_eq!(resumed["claims"]["not_claimed"][0], "global optimality");

    // A tampered receipt: no trusted incumbent, exit 2.
    let receipt = directory.0.join("session/receipts/1.json");
    let bytes = fs::read(&receipt).unwrap_or_default();
    let mut tampered = bytes.clone();
    tampered[20] ^= 1;
    fs::write(&receipt, &tampered).unwrap_or_else(|error| panic!("{error}"));
    let (exit, report) = run(&["loop", "resume", "--session", &session]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "resume-refused");
    assert_eq!(report["detail"]["refusal"]["reason"], "replacement-digest");
    assert_eq!(report["detail"]["trusted_incumbent"], Value::Null);
    fs::write(&receipt, &bytes).unwrap_or_else(|error| panic!("{error}"));
    // A rolled-back ledger likewise.
    let ledger = directory.0.join("session/ledger.jsonl");
    let text = fs::read_to_string(&ledger).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    fs::write(&ledger, lines[..lines.len() - 1].join("\n") + "\n")
        .unwrap_or_else(|error| panic!("{error}"));
    let (exit, report) = run(&["loop", "resume", "--session", &session]);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(
        report["detail"]["refusal"]["reason"],
        "ledger-rollback-suspected"
    );
    let (exit, report) = candidate(&session, &good.to_string_lossy());
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "resume-refused");
}

#[test]
fn the_fake_provider_improves_a_benchmark_case_and_its_claims_change_nothing() {
    let directory = Directory::new();
    let session = directory.path("session");
    let (exit, _) = open(
        &session,
        &artifact("boolean-kernel-original"),
        &["--attempts", "6"],
    );
    assert_eq!(exit, Some(0));
    let script = json!({
        "schema": "zeno-fcis/fake-provider-script/1",
        "steps": [
            {"malformed": "garbage", "claims": {"passed": true}},
            {"strategy": {"schema": "zeno-fcis/optimize-strategy/1",
                          "phases": [{"phase": "boolean", "rounds": 2}],
                          "extractor": "dag-greedy"}},
            {"candidate": artifact("boolean-kernel-candidate").to_string_lossy(),
             "claims": {"passed": true, "optimal": true}},
            {"candidate": artifact("boolean-kernel-padded").to_string_lossy()}
        ]
    });
    let script = directory.write("script.json", script.to_string().as_bytes());
    let (exit, report) = run(&[
        "loop",
        "run",
        "--session",
        &session,
        "--proposer",
        "fake",
        "--script",
        &script.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "completed");
    assert_eq!(report["detail"]["proposer"], "zeno-fcis/fake-provider/1");
    let loop_report = &report["detail"]["report"];
    let outcomes: Vec<&str> = loop_report["attempts"]
        .as_array()
        .unwrap_or_else(|| panic!("attempts"))
        .iter()
        .map(|attempt| attempt["outcome"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        outcomes,
        [
            "refused",
            // The strategy runs on the wired e-graph engine; the loop's own
            // check accepts its 7-node kernel.
            "checked-improvement",
            // The scripted candidate is byte-identical to that result.
            "duplicate-of-attempt:1",
            "equivalent-without-improvement:original-cost-guard"
        ]
    );
    assert_eq!(loop_report["status"], "best-checked-so-far");
    assert_eq!(
        loop_report["incumbent"]["cost"],
        json!({"nodes": 7, "bytes": 764})
    );
    assert_eq!(loop_report["session"]["stop_reason"], "proposer-exhausted");
    assert_eq!(loop_report["accounting"]["model_calls"], 4);
    assert_eq!(loop_report["accounting"]["tokens_reserved"], 24_576);
    let transcript =
        fs::read_to_string(directory.0.join("session/transcript.jsonl")).unwrap_or_default();
    assert!(transcript.contains("\"provider_asserted\":{\"optimal\":true,\"passed\":true}"));
    assert!(!transcript.contains("SECRET-do-not-leak"));
}

fn encoded(directory: &Directory, name: &str, program: &Value) -> String {
    let source = directory.write(&format!("{name}.json"), program.to_string().as_bytes());
    let target = directory.path(&format!("{name}.zcve"));
    let (exit, report) = run(&[
        "loop",
        "encode",
        "--program",
        &source.to_string_lossy(),
        "--out",
        &target,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    target
}

#[test]
fn work_ceilings_scale_with_the_original_unless_limits_are_given() {
    let directory = Directory::new();
    // 100,000 tuples: more check work than the compiled 1,000,000 units.
    let sum = encoded(
        &directory,
        "sum",
        &json!({
            "inputs": [{"kind": "Int", "min": "0", "max": "999"}, {"kind": "Int", "min": "0", "max": "99"}],
            "outputs": [{"kind": "Int", "min": "-9223372036854775808", "max": "9223372036854775807"}],
            "nodes": [["Input", 0], ["Input", 1], ["Add", 0, 1]],
            "roots": [2]
        }),
    );
    let scaled = directory.path("scaled");
    let (exit, report) = open(&scaled, Path::new(&sum), &[]);
    assert_eq!(exit, Some(0), "{report}");
    let limits = &report["detail"]["request"]["limits"];
    let check = limits["check_work"].as_u64().unwrap_or(0);
    assert!(check > 1_000_000, "{limits}");
    // Eight attempts in invocations of their own: two checks each plus the
    // largest intake, preparation and witness replays (4,310,016 units).
    assert_eq!(
        limits["session_work"].as_u64(),
        Some(8 * (2 * check + 4_310_016))
    );
    let fixed = directory.path("fixed");
    let (exit, report) = open(&fixed, Path::new(&sum), &["--check-work", "1000000"]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["reason"], "work-infeasible");
    assert_eq!(report["detail"]["check_work"], 1_000_000);
    assert_eq!(report["detail"]["needed"].as_u64(), Some(check));
}

#[test]
fn every_attempt_of_the_ceiling_fits_in_an_invocation_of_its_own() {
    // The MCP route: every attempt is a JSON candidate with provenance in an
    // invocation of its own, which first replays the incumbent and every stored
    // counterexample. The default session ceiling funds all eight attempts, and
    // the session still resumes afterwards.
    let directory = Directory::new();
    let program = case_program("B01", "original");
    let original = encoded(&directory, "original", &program);
    let session = directory.path("session");
    let (exit, report) = open(&session, Path::new(&original), &[]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["request"]["limits"]["attempts"], 8);
    let provenance = directory.write("provenance.json", br#"{"note": "test"}"#);
    let provenance = provenance.to_string_lossy().into_owned();
    for attempt in 0..8_u64 {
        // Distinct proposals: unused Boolean constants never trap, so they keep
        // the function; complementing the first output changes it.
        let mut proposal = program.clone();
        let nodes = proposal["nodes"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("nodes"));
        for _ in 0..=attempt / 2 {
            nodes.push(json!(["Bool", true]));
        }
        let wrong = attempt % 2 == 0;
        if wrong {
            nodes.push(json!(["Not", program["roots"][0]]));
            proposal["roots"][0] = json!(nodes.len() - 1);
        }
        let path = directory.write(
            &format!("proposal-{attempt}.json"),
            proposal.to_string().as_bytes(),
        );
        let (_, report) = run(&[
            "loop",
            "candidate",
            "--session",
            &session,
            "--candidate-json",
            &path.to_string_lossy(),
            "--provenance",
            &provenance,
        ]);
        let expected = if wrong {
            "different"
        } else {
            "equivalent-without-improvement:original-cost-guard"
        };
        assert_eq!(report["status"], expected, "{report}");
        assert_eq!(
            report["detail"]["accounting"]["attempts"],
            attempt + 1,
            "{report}"
        );
    }
    let (exit, report) = run(&["loop", "resume", "--session", &session]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["report"]["accounting"]["attempts"], 8);
}

#[test]
fn seeds_are_proposed_first_as_ordinary_attempts() {
    let directory = Directory::new();
    // Each seed simplifies one output of the original: the first is a checked
    // improvement, the second ties it, and the proposer runs after both.
    let int = |min: &str, max: &str| json!({"kind": "Int", "min": min, "max": max});
    let full = int("-9223372036854775808", "9223372036854775807");
    let program = |nodes: Value, roots: Value| {
        json!({"inputs": [int("0", "15")], "outputs": [full.clone(), full.clone()],
               "nodes": nodes, "roots": roots})
    };
    let original = encoded(
        &directory,
        "original",
        &program(
            json!([
                ["Input", 0],
                ["Int", "7"],
                ["Sub", 0, 1],
                ["Add", 2, 1],
                ["Int", "9"],
                ["Sub", 0, 4],
                ["Add", 5, 4]
            ]),
            json!([3, 6]),
        ),
    );
    let first = encoded(
        &directory,
        "first",
        &program(
            json!([["Input", 0], ["Int", "9"], ["Sub", 0, 1], ["Add", 2, 1]]),
            json!([0, 3]),
        ),
    );
    let second = encoded(
        &directory,
        "second",
        &program(
            json!([["Input", 0], ["Int", "7"], ["Sub", 0, 1], ["Add", 2, 1]]),
            json!([3, 0]),
        ),
    );
    let script = directory.write(
        "script.json",
        json!({"schema": "zeno-fcis/fake-provider-script/1", "steps": [
            {"strategy": {"schema": "zeno-fcis/optimize-strategy/1",
                          "phases": [{"phase": "share", "rounds": 1}], "extractor": "dag-greedy"}}
        ]})
        .to_string()
        .as_bytes(),
    );
    let script = script.to_string_lossy().into_owned();
    let session = directory.path("session");
    let (exit, report) = open(&session, Path::new(&original), &[]);
    assert_eq!(exit, Some(0), "{report}");
    let (exit, report) = run(&[
        "loop",
        "run",
        "--session",
        &session,
        "--proposer",
        "fake",
        "--script",
        &script,
        "--seed",
        &first,
        "--seed",
        &second,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["seeds"], 2);
    let loop_report = &report["detail"]["report"];
    let outcomes: Vec<&str> = loop_report["attempts"]
        .as_array()
        .unwrap_or_else(|| panic!("attempts"))
        .iter()
        .map(|attempt| attempt["outcome"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        &outcomes[..2],
        [
            "checked-improvement",
            "equivalent-without-improvement:tie-with-incumbent"
        ]
    );
    assert_eq!(
        outcomes.len(),
        3,
        "the proposer's attempt follows the seeds"
    );
    assert_eq!(loop_report["incumbent"]["cost"]["nodes"], 4);
    // Seeds meter no model call; the scripted strategy meters one.
    assert_eq!(loop_report["accounting"]["model_calls"], 1);
}

#[test]
fn the_local_proposer_runs_an_encoded_benchmark_case_to_exhaustion() {
    let directory = Directory::new();
    let program = directory.write(
        "b01.json",
        case_program("B01", "original").to_string().as_bytes(),
    );
    let out = directory.path("b01.zcve");
    let (exit, report) = run(&[
        "loop",
        "encode",
        "--program",
        &program.to_string_lossy(),
        "--out",
        &out,
    ]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["status"], "encoded");
    assert_eq!(report["detail"]["nodes"], 3);
    let bytes = fs::read(&out).unwrap_or_default();
    assert_eq!(report["detail"]["bytes"], bytes.len());
    // The output is never overwritten.
    let (exit, report) = run(&[
        "loop",
        "encode",
        "--program",
        &program.to_string_lossy(),
        "--out",
        &out,
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["status"], "output-exists");
    let session = directory.path("session");
    let (exit, _) = open(&session, Path::new(&out), &[]);
    assert_eq!(exit, Some(0));
    let (exit, report) = run(&["loop", "run", "--session", &session, "--proposer", "local"]);
    assert_eq!(exit, Some(0), "{report}");
    let loop_report = &report["detail"]["report"];
    assert_eq!(loop_report["status"], "best-checked-so-far");
    assert_eq!(loop_report["incumbent"]["cost"]["nodes"], 1);
    assert_eq!(loop_report["original"]["cost"]["nodes"], 3);
    assert_eq!(loop_report["session"]["stop_reason"], "proposer-exhausted");
    assert_eq!(loop_report["accounting"]["model_calls"], 0);
    // Running again on the closed session changes nothing.
    let (exit, report) = run(&["loop", "run", "--session", &session, "--proposer", "local"]);
    assert_eq!(exit, Some(0), "{report}");
    assert_eq!(report["detail"]["report"]["accounting"]["attempts"], 1);
    // A candidate on the closed session is refused as closed.
    let (exit, report) = candidate(&session, &out);
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "session-closed");
    // Closure is checked before acquiring any supplied candidate bytes.
    let (exit, report) = candidate(&session, &directory.path("absent.zcve"));
    assert_eq!(exit, Some(2), "{report}");
    assert_eq!(report["status"], "session-closed");
    assert_eq!(report["detail"]["report"]["accounting"]["attempts"], 1);
    // Garbage is not a program.
    let bad = directory.write(
        "bad.json",
        br#"{"inputs": [], "outputs": [{"kind": "Bool"}], "nodes": [["Or", 0, 0]], "roots": [0]}"#,
    );
    let (exit, report) = run(&[
        "loop",
        "encode",
        "--program",
        &bad.to_string_lossy(),
        "--out",
        &directory.path("bad.zcve"),
    ]);
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["reason"], "program-json-node");
}

#[test]
fn the_hosted_adapter_is_disabled_and_leaks_nothing() {
    let directory = Directory::new();
    let session = directory.path("session");
    let (exit, _) = open(&session, &artifact("boolean-kernel-original"), &[]);
    assert_eq!(exit, Some(0));
    let config = directory.write(
        "hosted.json",
        br#"{"schema": "zeno-fcis/hosted-provider-config/1", "enabled": false, "provider": "example", "credential_env": "ZENO_FCIS_TEST_SECRET", "money_micros": 0}"#,
    );
    let (exit, report) = run(&[
        "loop",
        "run",
        "--session",
        &session,
        "--proposer",
        "hosted",
        "--hosted-config",
        &config.to_string_lossy(),
    ]);
    assert_eq!(exit, Some(0), "{report}");
    let loop_report = &report["detail"]["report"];
    assert_eq!(
        loop_report["attempts"][0]["outcome"],
        "proposal-failed:unavailable:hosted-provider-disabled"
    );
    assert_eq!(loop_report["status"], "no-checked-improvement");
    assert_eq!(loop_report["accounting"]["money_reserved_micros"], 0);
    let text = report.to_string()
        + &fs::read_to_string(directory.0.join("session/transcript.jsonl")).unwrap_or_default();
    assert!(!text.contains("SECRET-do-not-leak"));
    // A request asking for hosted mode is refused at admission; the CLI has
    // no flag for it, so the policy stays disabled.
    let (exit, report) = open(
        &directory.path("other"),
        &artifact("boolean-kernel-original"),
        &["--attempts", "9"],
    );
    assert_eq!(exit, Some(1), "{report}");
    assert_eq!(report["detail"]["reason"], "limit-above-ceiling");
}

#[test]
fn describe_declares_loop_effects() {
    let (exit, report) = run(&["describe", "loop"]);
    assert_eq!(exit, Some(0));
    let group = &report["command"];
    assert_eq!(group["effects"]["classification"], "command-group");
    let names: Vec<_> = group["subcommands"]
        .as_array()
        .unwrap_or_else(|| panic!("subcommands"))
        .iter()
        .map(|command| command["name"].clone())
        .collect();
    assert_eq!(
        names,
        [
            json!("candidate"),
            json!("capture"),
            json!("decompose"),
            json!("encode"),
            json!("evaluate"),
            json!("family"),
            json!("learn"),
            json!("meaning"),
            json!("open"),
            json!("pipeline"),
            json!("question"),
            json!("resume"),
            json!("rules"),
            json!("run"),
            json!("sketch"),
            json!("strategy"),
            json!("teach-back"),
            json!("transfer")
        ]
    );
    fn check_effects(command: &Value) {
        let children = command["subcommands"]
            .as_array()
            .unwrap_or_else(|| panic!("subcommands"));
        if !children.is_empty() {
            assert_eq!(command["effects"]["classification"], "command-group");
            for child in children {
                check_effects(child);
            }
            return;
        }
        assert_eq!(
            command["effects"]["classification"], "declared",
            "{}",
            command["name"]
        );
        // Every loop command runs in a transient unit through `systemd-run
        // --user` and `systemctl --user`, bounded by the caller's CPU limits.
        assert_eq!(command["effects"]["executes_tools"], true);
        assert!(
            command["effects"]["reads"]
                .as_array()
                .is_some_and(|reads| reads.contains(&json!("caller-cpu-limits"))),
            "{}",
            command["name"]
        );
        assert!(
            command["effects"]["writes"]
                .as_array()
                .is_some_and(|writes| writes.contains(&json!("transient-user-unit"))),
            "{}",
            command["name"]
        );
    }
    check_effects(group);
}
