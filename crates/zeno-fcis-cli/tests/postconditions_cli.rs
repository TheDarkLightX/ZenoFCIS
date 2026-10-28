//! End-to-end opt-in postcondition proposal/check workflow and file effects.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct App(PathBuf);
impl Drop for App {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn app() -> App {
    let root = std::env::temp_dir().join(format!(
        "postconditions-cli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("v2")).unwrap_or_else(|e| panic!("{e}"));
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
    let project =
        fs::read_to_string(source.join("project.zeno.in")).unwrap_or_else(|e| panic!("{e}"));
    let project = project
        .replace("${C}", "2")
        .lines()
        .filter(|s| !s.starts_with("law 502 "))
        .collect::<Vec<_>>()
        .join("\n");
    let policy = fs::read_to_string(source.join("policy.json.in"))
        .unwrap_or_else(|e| panic!("{e}"))
        .replace("${C}", "2")
        .replace("${INSTANCE}", "test");
    let mut policy: Value = serde_json::from_str(&policy).unwrap_or_else(|e| panic!("{e}"));
    policy["law_kinds"]
        .as_object_mut()
        .unwrap_or_else(|| panic!("object"))
        .remove("502");
    fs::write(root.join("project.zeno"), project).unwrap_or_else(|e| panic!("{e}"));
    fs::write(root.join("v2/policy.json"), policy.to_string()).unwrap_or_else(|e| panic!("{e}"));
    policy["cases"][3]["post"]["110"] = json!("value");
    fs::write(root.join("mutant.json"), policy.to_string()).unwrap_or_else(|e| panic!("{e}"));
    fs::write(
        root.join("intent.txt"),
        "Increment adds exactly one; decrement subtracts exactly one.",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    App(root)
}

fn invoke(app: &App, operation: &str, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(&app.0)
        .args([
            "contract",
            "postconditions",
            operation,
            ".",
            "--intent",
            "intent.txt",
        ])
        .args(extra)
        .output()
        .unwrap_or_else(|e| panic!("{e}"))
}

fn json_output(output: &Output, expected: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn postconditions_cli_round_trip_statuses_bindings_and_no_contract_writes() {
    let app = app();
    let before = fs::read(app.0.join("project.zeno")).unwrap_or_else(|e| panic!("{e}"));
    let policy_before = fs::read(app.0.join("v2/policy.json")).unwrap_or_else(|e| panic!("{e}"));
    let output = invoke(&app, "prompt", &[]);
    let request = json_output(&output, 0);
    assert_eq!(output.stdout, invoke(&app, "prompt", &[]).stdout);
    let mut proposals = json!({"schema":"zeno-fcis/postcondition-proposals/1","request_sha256":request["request_sha256"],"proposer":"assistant-example",
        "candidates":[{"id":"increment","formula":"command.101.120 == 150 -> post.100.110 == pre.100.110 + 1","rationale":"increment adds exactly one"}]});
    let save = |value: &Value| {
        fs::write(app.0.join("proposals.json"), value.to_string()).unwrap_or_else(|e| panic!("{e}"))
    };
    save(&proposals);
    let args = ["--proposals", "proposals.json", "--mutant", "mutant.json"];
    let output = invoke(&app, "check", &args);
    let report = json_output(&output, 0);
    assert_eq!(report["candidates"][0]["status"], "checked-discriminating");
    assert_eq!(report["candidates"][0]["reference"]["tuples"], 12);
    assert_eq!(report["candidates"][0]["mutants"][0]["detail"]["failed"], 2);
    assert_eq!(output.stdout, invoke(&app, "check", &args).stdout);
    assert_eq!(
        json_output(
            &invoke(&app, "check", &["--proposals", "proposals.json"]),
            2
        )["candidates"][0]["status"],
        "checked-no-discrimination"
    );
    proposals["candidates"][0]["formula"] = json!("true");
    save(&proposals);
    assert_eq!(
        json_output(&invoke(&app, "check", &args), 2)["candidates"][0]["status"],
        "checked-trivial"
    );
    proposals["candidates"][0]["formula"] = json!("post.100.110 == pre.100.110");
    save(&proposals);
    assert_eq!(
        json_output(&invoke(&app, "check", &args), 1)["candidates"][0]["status"],
        "refuted"
    );
    proposals["candidates"][0]["precondition"] = json!("false");
    save(&proposals);
    assert_eq!(
        json_output(&invoke(&app, "check", &args), 1)["status"],
        "refused"
    );
    proposals["candidates"][0]
        .as_object_mut()
        .unwrap_or_else(|| panic!("object"))
        .remove("precondition");
    save(&proposals);
    fs::write(app.0.join("intent.txt"), "Changed intent").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        json_output(&invoke(&app, "check", &args), 1)["status"],
        "refused"
    );
    assert_eq!(
        before,
        fs::read(app.0.join("project.zeno")).unwrap_or_else(|e| panic!("{e}"))
    );
    assert_eq!(
        policy_before,
        fs::read(app.0.join("v2/policy.json")).unwrap_or_else(|e| panic!("{e}"))
    );
}

#[test]
fn postconditions_cli_declares_read_only_effects_and_refuses_oversized_files() {
    let app = app();
    for sub in ["prompt", "check"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
            .args(["describe", "contract", "postconditions", sub])
            .output()
            .unwrap_or_else(|e| panic!("{e}"));
        let report = json_output(&output, 0);
        let text = report.to_string();
        assert!(text.contains("written-intent"), "{text}");
    }
    fs::write(app.0.join("intent.txt"), "x".repeat(65_537)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        json_output(&invoke(&app, "prompt", &[]), 1)["status"],
        "refused"
    );
}

#[cfg(unix)]
#[test]
fn postconditions_cli_fifo_input_refuses_without_waiting_for_a_writer() {
    let app = app();
    fs::remove_file(app.0.join("intent.txt")).unwrap_or_else(|e| panic!("{e}"));
    nix::unistd::mkfifo(&app.0.join("intent.txt"), nix::sys::stat::Mode::S_IRUSR)
        .unwrap_or_else(|e| panic!("{e}"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .current_dir(&app.0)
        .args([
            "contract",
            "postconditions",
            "prompt",
            ".",
            "--intent",
            "intent.txt",
        ])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
            assert_eq!(status.code(), Some(1));
            break;
        }
        if started.elapsed() > std::time::Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("blocked on FIFO");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
