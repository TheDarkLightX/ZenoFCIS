//! Capture through the real binary. Runtime qualification is intentionally
//! deferred to the lead; a missing Linux contained runtime fails these tests.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zeno-capture-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("fixture directory: {e}"));
        Self(p)
    }
    fn json(&self, name: &str, v: &Value) -> String {
        self.text(name, &v.to_string())
    }
    fn text(&self, name: &str, text: &str) -> String {
        let p = self.0.join(name);
        fs::write(&p, text).unwrap_or_else(|e| panic!("fixture write: {e}"));
        p.to_string_lossy().into_owned()
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
fn run(args: &[&str]) -> (i32, Value) {
    let done = Command::new(CLI)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("CLI unavailable: {e}"));
    let report = serde_json::from_slice(&done.stdout)
        .unwrap_or_else(|e| panic!("JSON reply: {e}; {done:?}"));
    (done.status.code().unwrap_or(-1), report)
}
fn spec() -> Value {
    json!({"inputs":[{"kind":"Int","min":"0","max":"3"}],"outputs":[{"kind":"Bool"}]})
}
fn open(d: &Directory, limits: &[&str]) -> (i32, Value) {
    let spec = d.json("spec.json", &spec());
    let reference = d.text("reference.py", "def reference(p): return [int(p[0] < 2)]\n");
    let session = d.path("session");
    let mut args = vec![
        "loop",
        "capture",
        "open",
        "--spec",
        &spec,
        "--reference",
        &reference,
        "--session",
        &session,
    ];
    args.extend_from_slice(limits);
    run(&args)
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn wrong() -> Value {
    json!({"inputs":spec()["inputs"],"outputs":spec()["outputs"],"nodes":[["Bool",false]],"roots":[0]})
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn right() -> Value {
    json!({"inputs":spec()["inputs"],"outputs":spec()["outputs"],"nodes":[["Input",0],["Int","2"],["Lt",0,1]],"roots":[2]})
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn candidate(d: &Directory, v: &Value) -> (i32, Value) {
    let p = d.json("candidate.json", v);
    run(&[
        "loop",
        "capture",
        "candidate",
        "--session",
        &d.path("session"),
        "--candidate-json",
        &p,
    ])
}

#[test]
fn capture_platform_without_containment_is_typed_refusal() {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        return;
    }
    let d = Directory::new();
    let (code, r) = open(&d, &[]);
    assert_eq!(code, 1);
    assert_eq!(r["status"], "refused");
    assert!(
        r["detail"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("capture-containment-unavailable"))
    );
    assert!(!d.0.join("session").exists());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_examples_never_accept_and_the_exact_receipt_replays_separately() {
    let d = Directory::new();
    let (code, opened) = open(&d, &[]);
    assert_eq!(code, 0, "{opened}");
    let (_, bad) = candidate(&d, &wrong());
    assert_eq!(bad["detail"]["precheck"], "examples-equivalent");
    assert_eq!(bad["status"], "refuted");
    assert_eq!(bad["detail"]["example"]["input"], json!([0]));
    let (_, repeat) = candidate(&d, &wrong());
    assert_eq!(repeat["status"], "precheck-refuted");
    assert_eq!(repeat["detail"]["exact"], "not-run");
    let (code, captured) = candidate(&d, &right());
    assert_eq!(code, 0, "{captured}");
    assert_eq!(captured["status"], "captured-exact");
    assert_eq!(captured["detail"]["check"]["replay"], "matched");
    let (code, new_wrong) = candidate(&d, &wrong());
    assert_ne!(code, 0);
    assert_eq!(new_wrong["status"], "refused");
    assert_eq!(
        new_wrong["detail"]["reason"],
        "capture-session-already-captured"
    );
    let (code, status) = run(&[
        "loop",
        "capture",
        "status",
        "--full",
        "--session",
        &d.path("session"),
    ]);
    assert_eq!(code, 0, "{status}");
    let original = d.json("table.json", &status["detail"]["table_program"]);
    let candidate = d.json("captured.json", &right());
    let a = d.path("table.zcve");
    let b = d.path("candidate.zcve");
    for (input, out) in [(&original, &a), (&candidate, &b)] {
        let (code, encoded) = run(&["loop", "encode", "--program", input, "--out", out]);
        assert_eq!(code, 0, "{encoded}");
    }
    let receipt = d.path("receipt.json");
    let (code, checked) = run(&[
        "transform",
        "check",
        "--original",
        &a,
        "--candidate",
        &b,
        "--max-input-tuples",
        "4096",
        "--receipt",
        &receipt,
    ]);
    assert_eq!(code, 0, "{checked}");
    let value = &status["detail"]["session"]["receipt"];
    let expected = format!(
        "{}\n",
        serde_json::to_string(value).unwrap_or_else(|e| panic!("receipt: {e}"))
    );
    assert_eq!(
        fs::read(&receipt).unwrap_or_else(|e| panic!("receipt: {e}")),
        expected.as_bytes()
    );
    let (code, replayed) = run(&[
        "transform",
        "replay",
        "--original",
        &a,
        "--candidate",
        &b,
        "--max-input-tuples",
        "4096",
        "--receipt",
        &receipt,
    ]);
    assert_eq!(code, 0, "{replayed}");
    assert_eq!(replayed["status"], "replayed");
    let (_, packet) = run(&["loop", "capture", "packet", "--session", &d.path("session")]);
    assert_eq!(
        packet["detail"]["packet"].as_object().map(|m| m.len()),
        Some(4)
    );
    for public in [opened, bad, repeat, captured, packet] {
        let text = public.to_string();
        assert!(!text.contains("source_sha256"));
        assert!(!text.contains("table_sha256"));
        assert!(!text.contains("receipt_sha256"));
        assert!(!text.contains("exact_reserved_visits"));
        assert!(!text.contains("precheck_reserved_visits"));
        assert!(!text.contains("def reference"));
    }
    // Cached status cannot replace candidate binding or a freshly replayed
    // receipt. Each altered private record must fail before reporting capture.
    let state_path = d.0.join("session/capture.json");
    let saved: Value =
        serde_json::from_slice(&fs::read(&state_path).unwrap_or_else(|e| panic!("state: {e}")))
            .unwrap_or_else(|e| panic!("state: {e}"));
    for field in ["receipt", "candidate", "status", "receipt-count"] {
        let mut altered = saved.clone();
        match field {
            "receipt" => altered["receipt"] = Value::Null,
            "candidate" => altered["candidate"] = wrong(),
            "status" => altered["status"] = json!("ready"),
            _ => altered["receipt"]["inputs_checked"] = json!(0),
        }
        fs::write(&state_path, altered.to_string()).unwrap_or_else(|e| panic!("state: {e}"));
        let (code, result) = run(&["loop", "capture", "status", "--session", &d.path("session")]);
        assert_ne!(code, 0, "{field}: {result}");
        assert_eq!(result["status"], "refused", "{field}: {result}");
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_budget_stops_spend_no_attempt_and_preserve_invocation_accounting() {
    for limits in [vec!["--node-visits", "0"], vec!["--rounds", "0"]] {
        let d = Directory::new();
        let (code, opened) = open(&d, &limits);
        assert_eq!(code, 0, "{opened}");
        let (code, result) = candidate(&d, &wrong());
        assert_eq!(code, 2, "{result}");
        assert_eq!(result["status"], "inconclusive");
        assert_eq!(result["detail"]["attempt_spent"], false);
        let state: Value = serde_json::from_slice(
            &fs::read(d.0.join("session/capture.json")).unwrap_or_else(|e| panic!("state: {e}")),
        )
        .unwrap_or_else(|e| panic!("state: {e}"));
        assert_eq!(state["attempts"], 0);
        if limits[0] != "--rounds" {
            assert_eq!(state["invocations"], 1);
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_reference_failure_nondeterminism_and_malformed_labels_publish_no_session() {
    for source in [
        "def reference(p): raise ValueError('hidden')",
        "def reference(p): return [True]",
        "def reference(p): return [0.0]",
        "def reference(p): return [2]",
        "def reference(p): return [0,1]",
        "n=0\ndef reference(p):\n global n\n n+=1\n return [int(n>4)]",
        "def reference(p):\n if p[0]==3: raise ValueError()\n return [0]",
    ] {
        let d = Directory::new();
        let spec = d.json("spec.json", &spec());
        let reference = d.text("reference.py", source);
        let (code, result) = run(&[
            "loop",
            "capture",
            "open",
            "--spec",
            &spec,
            "--reference",
            &reference,
            "--session",
            &d.path("session"),
        ]);
        assert_ne!(code, 0);
        assert_eq!(result["status"], "refused", "{result}");
        assert!(!d.0.join("session").exists());
        assert!(!result.to_string().contains("hidden"));
    }
}

// BEGIN capture review regressions
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn open_reference(d: &Directory, abi: &Value, source: &str, limits: &[&str]) -> (i32, Value) {
    let spec = d.json("spec.json", abi);
    let reference = d.text("reference.py", source);
    let session = d.path("session");
    let mut args = vec![
        "loop",
        "capture",
        "open",
        "--spec",
        &spec,
        "--reference",
        &reference,
        "--session",
        &session,
    ];
    args.extend_from_slice(limits);
    run(&args)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_padded_candidate_refutation_spends_attempt_despite_coverage_budget() {
    let d = Directory::new();
    let abi: Value = serde_json::from_str(include_str!("fixtures/capture/identity-spec.json"))
        .unwrap_or_else(|e| panic!("fixture: {e}"));
    let (code, opened) = open_reference(
        &d,
        &abi,
        include_str!("fixtures/capture/identity-reference.py"),
        &[],
    );
    assert_eq!(code, 0, "{opened}");
    let padded: Value =
        serde_json::from_str(include_str!("fixtures/capture/padded-candidate.json"))
            .unwrap_or_else(|e| panic!("fixture: {e}"));
    let (code, refuted) = candidate(&d, &padded);
    assert_eq!(code, 1, "{refuted}");
    assert_eq!(refuted["status"], "refuted");
    assert_eq!(refuted["detail"]["attempt_spent"], true);
    assert_eq!(refuted["detail"]["attempts"], 1);
    assert_eq!(refuted["detail"]["coverage"]["status"], "inconclusive");
    assert!(
        refuted["detail"]["packet"]["examples"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    );
    let state: Value = serde_json::from_slice(
        &fs::read(d.0.join("session/capture.json")).unwrap_or_else(|e| panic!("state: {e}")),
    )
    .unwrap_or_else(|e| panic!("state: {e}"));
    assert_eq!(state["attempts"], 1);
    assert_eq!(state["events"].as_array().map(Vec::len), Some(1));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_decided_refutation_spends_attempt_when_example_budget_is_zero() {
    let d = Directory::new();
    let (code, opened) = open(&d, &["--max-examples", "0"]);
    assert_eq!(code, 0, "{opened}");
    let (code, refuted) = candidate(&d, &wrong());
    assert_eq!(code, 1, "{refuted}");
    assert_eq!(refuted["status"], "refuted");
    assert_eq!(refuted["detail"]["attempt_spent"], true);
    assert_eq!(refuted["detail"]["attempts"], 1);
    assert_eq!(refuted["detail"]["feedback"], "example-budget");
    assert_eq!(refuted["detail"]["packet"]["examples"], json!([]));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn nullary_int(max: i64) -> Value {
    json!({"inputs":[],"outputs":[{"kind":"Int","min":"0","max":max.to_string()}]})
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn rows(d: &Directory) -> Value {
    let (code, result) = run(&[
        "loop",
        "capture",
        "status",
        "--full",
        "--session",
        &d.path("session"),
    ]);
    assert_eq!(code, 0, "{result}");
    result["detail"]["session"]["rows"].clone()
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_confinement_denial_rows_fail_if_containment_is_noop() {
    let d = Directory::new();
    let secret = d.text("secret.txt", "outside-content");
    let outside = d.path("outside-write.txt");
    let secret_literal = serde_json::to_string(&secret).unwrap_or_else(|e| panic!("fixture: {e}"));
    let outside_literal =
        serde_json::to_string(&outside).unwrap_or_else(|e| panic!("fixture: {e}"));
    let source = format!(
        "import ctypes\ndef reference(p):\n denied=0\n for path,mode in [({secret_literal},'r'),({outside_literal},'w'),('/proc/self/environ','r')]:\n  try: open(path,mode)\n  except PermissionError: denied+=1\n libc=ctypes.CDLL(None,use_errno=True)\n for number,args in [(41,(2,1,0)),(57,()),(59,(0,0,0)),(101,(0,0,0,0)),(310,(0,0,0,0,0,0)),(72,(1,8,1))]:\n  if libc.syscall(number,*args)==-1 and ctypes.get_errno()==1: denied+=1\n return [denied]\n"
    );
    let (code, opened) = open_reference(&d, &nullary_int(9), &source, &[]);
    assert_eq!(code, 0, "{opened}");
    assert_eq!(
        rows(&d),
        json!([[9]]),
        "every denial is required; an opened table alone is insufficient"
    );
    assert!(!PathBuf::from(outside).exists());
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_confinement_allows_positive_scratch_read_write() {
    let d = Directory::new();
    let source = "def reference(p):\n with open('scratch.txt','w') as f: f.write('safe')\n with open('scratch.txt') as f: return [len(f.read())]\n";
    let (code, opened) = open_reference(&d, &nullary_int(4), source, &[]);
    assert_eq!(code, 0, "{opened}");
    assert_eq!(rows(&d), json!([[4]]));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_confinement_closes_injected_parent_descriptor_and_environment() {
    let d = Directory::new();
    let abi = d.json("spec.json", &nullary_int(1));
    let source = d.text("reference.py", "import os\ndef reference(p):\n for fd in range(3,4096):\n  try: os.fstat(fd)\n  except OSError: continue\n  return [0]\n return [int('CAPTURE_CONTROL_SECRET' not in os.environ)]\n");
    let secret = d.text("parent-only.txt", "descriptor-control");
    // Python explicitly passes an open parent fd; Rust's normal files have
    // CLOEXEC and would make an inheritance control vacuous.
    let helper = "import os,subprocess,sys\nwith open(sys.argv[2],'rb') as f:\n r=subprocess.run([sys.argv[1],*sys.argv[3:]],pass_fds=(f.fileno(),),env={**os.environ,'CAPTURE_CONTROL_SECRET':'parent-control'},capture_output=True,timeout=35)\n sys.stdout.buffer.write(r.stdout)\n sys.stderr.buffer.write(r.stderr)\n sys.exit(r.returncode)\n";
    let done = Command::new("python3")
        .args([
            "-B",
            "-c",
            helper,
            CLI,
            &secret,
            "loop",
            "capture",
            "open",
            "--spec",
            &abi,
            "--reference",
            &source,
            "--session",
            &d.path("session"),
        ])
        .output()
        .unwrap_or_else(|e| panic!("inheritance probe: {e}"));
    assert!(done.status.success(), "{done:?}");
    assert_eq!(rows(&d), json!([[1]]));
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_confinement_reference_caps_and_exit78_are_reference_refusals() {
    for source in [
        "import ctypes\ndef reference(p): return [ctypes.string_at(0)]",
        "def reference(p): return [len(bytearray(256<<20))]",
        "def reference(p):\n while True: pass",
        "def reference(p):\n print('x'*(3<<20))\n return [0]",
        "def reference(p):\n with open('scratch.txt','w') as f: f.write('x'*65537)\n return [0]",
        "import os\ndef reference(p): os._exit(78)",
    ] {
        let d = Directory::new();
        let (code, result) = open_reference(&d, &nullary_int(1), source, &[]);
        assert_eq!(code, 1, "{result}");
        assert_eq!(result["status"], "refused");
        assert!(!d.0.join("session").exists());
        assert!(
            !result["detail"]["reason"]
                .as_str()
                .is_some_and(|s| s.contains("containment-unavailable")),
            "{result}"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_large_session_uses_delta_events_and_bounded_default_status() {
    let d = Directory::new();
    let int = json!({"kind":"Int","min":"0","max":"9223372036854775807"});
    let mut inputs = vec![json!({"kind":"Int","min":"0","max":"4095"})];
    inputs
        .extend((0..7).map(
            |_| json!({"kind":"Int","min":"9223372036854775807","max":"9223372036854775807"}),
        ));
    let abi = json!({"inputs":inputs,"outputs":vec![int;8]});
    let (code, opened) = open_reference(
        &d,
        &abi,
        "def reference(p): return [9223372036854775807]*8",
        &["--node-visits", "10000000"],
    );
    assert_eq!(code, 0, "{opened}");
    for ordinal in 2..34 {
        let fixture = json!({"inputs":abi["inputs"],"outputs":abi["outputs"],
            "nodes":[["Input",0],["Int",ordinal.to_string()],["Eq",0,1],["Int","0"],
                     ["Int","9223372036854775807"],["Select",2,3,4]],"roots":[5,4,4,4,4,4,4,4]});
        let (code, result) = candidate(&d, &fixture);
        assert_eq!(code, 1, "{result}");
        assert_eq!(result["status"], "refuted");
    }
    let state: Value = serde_json::from_slice(
        &fs::read(d.0.join("session/capture.json")).unwrap_or_else(|e| panic!("state: {e}")),
    )
    .unwrap_or_else(|e| panic!("state: {e}"));
    let events = state["events"]
        .as_array()
        .unwrap_or_else(|| panic!("events"));
    assert_eq!(events.len(), 32);
    let mut added = 0;
    for event in events {
        assert!(event["detail"].get("packet").is_none());
        assert!(event["detail"]["coverage"].get("outcomes").is_none());
        added += event["new_example_ordinals"].as_array().map_or(0, Vec::len);
    }
    assert_eq!(added, state["examples"].as_array().map_or(0, Vec::len));
    let (code, summary) = run(&["loop", "capture", "status", "--session", &d.path("session")]);
    assert_eq!(code, 1, "{summary}"); // the latest decided candidate is refuted
    assert!(summary["detail"].get("session").is_none());
    assert!(summary["detail"].get("table_program").is_none());
    assert_eq!(summary["detail"]["events"], 32);
    assert!(summary.to_string().len() < 65536);
    let (code, full) = run(&[
        "loop",
        "capture",
        "status",
        "--full",
        "--session",
        &d.path("session"),
    ]);
    assert_eq!(code, 1, "{full}");
    assert_eq!(full["status"], "refused");
    assert_eq!(full["detail"]["reason"], "capture-full-status-output-bound");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn capture_zal_real_cli_expression_grammar_round_trips() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or_else(|| panic!("repository root"))
        .to_path_buf();
    let done = Command::new("python3")
        .args([
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            "integrations/zal",
            "-p",
            "test_capture_cli.py",
            "-v",
        ])
        .current_dir(root)
        .env("ZENO_CAPTURE_TEST_CLI", CLI)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .output()
        .unwrap_or_else(|e| panic!("paired CLI probe: {e}"));
    assert!(done.status.success(), "{done:?}");
    assert!(
        String::from_utf8_lossy(&done.stderr).contains("Ran 1 test"),
        "no real grammar test: {done:?}"
    );
    assert!(!String::from_utf8_lossy(&done.stderr).contains("skipped"));
}
// END capture review regressions
