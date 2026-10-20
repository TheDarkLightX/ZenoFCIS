//! Public CLI screening cannot substitute for a transform receipt.
#![cfg(target_os = "linux")]
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zeno-fuzz-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap_or_else(|e| panic!("create scratch: {e}"));
        Self(p)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, bytes).unwrap_or_else(|e| panic!("write: {e}"));
        p
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn program(candidate: bool) -> Vec<u8> {
    let roots = if candidate { vec![1] } else { vec![0] };
    Program::try_new(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Bool(false)],
        roots,
    )
    .and_then(|p| {
        p.value()
            .map_err(|_| zeno_fcis_synthesis::finite::Error::Invalid("fixture"))
    })
    .and_then(|v| {
        v.canonical_bytes()
            .map_err(|_| zeno_fcis_synthesis::finite::Error::Invalid("fixture"))
    })
    .unwrap_or_else(|e| panic!("fixture: {e:?}"))
}

fn run(candidate: bool, request: Value) -> Value {
    let d = Directory::new();
    let r = d.write("reference.zcve", &program(false));
    let c = d.write("candidate.zcve", &program(candidate));
    let q = d.write(
        "request.json",
        &serde_json::to_vec(&request).unwrap_or_else(|e| panic!("json: {e}")),
    );
    let out = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(["--no-kb", "loop", "fuzz", "--original"])
        .arg(r)
        .arg("--candidate")
        .arg(c)
        .arg("--request")
        .arg(q)
        .output()
        .unwrap_or_else(|e| panic!("CLI: {e}"));
    assert!(out.stderr.is_empty(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("JSON: {e}: {out:?}"))
}

#[test]
fn counterexample_is_bound_replayed_and_has_no_acceptance_receipt() {
    let request = json!({"schema":"zeno-fcis/loop-fuzz/1","seeds":[[[1]]],"limits":{"trials":2}});
    let a = run(true, request.clone());
    let b = run(true, request);
    assert_eq!(a, b);
    assert_eq!(a["status"], "counterexample");
    assert_eq!(a["authority"], "none");
    assert_eq!(a["qualification"], "not-run");
    assert_eq!(a["counterexample"]["replay"], "matched");
    assert_eq!(a["counterexample"]["input"], json!([1]));
    assert_eq!(a["work"]["evaluation_pairs"], 2);
    for k in [
        "original_sha256",
        "candidate_sha256",
        "request_sha256",
        "proposer_sha256",
    ] {
        assert_eq!(a[k].as_str().map(str::len), Some(64));
    }
    assert!(a.get("receipt").is_none());
}

#[test]
fn no_hit_remains_inconclusive_and_cannot_qualify_identical_programs() {
    let r = run(
        false,
        json!({"schema":"zeno-fcis/loop-fuzz/1","limits":{"trials":4,"pairs":8}}),
    );
    assert_eq!(r["status"], "inconclusive");
    assert_eq!(r["authority"], "none");
    assert!(r["counterexample"].is_null());
    assert!(r.get("receipt").is_none());
}

#[test]
fn request_cannot_inject_authority_or_relax_domain_checks() {
    let r = run(
        true,
        json!({"schema":"zeno-fcis/loop-fuzz/1","authority":"accepted"}),
    );
    assert_ne!(r["status"], "counterexample");
    assert_ne!(r["status"], "equivalent");
    let r = run(
        true,
        json!({"schema":"zeno-fcis/loop-fuzz/1","seeds":[[[2]]]}),
    );
    assert_ne!(r["status"], "counterexample");
    assert_ne!(r["status"], "equivalent");
}
