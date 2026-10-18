//! Inductive Lean dispatch and pinned-runtime process checks.
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use zeno_fcis_codec::CommitmentHasher as _;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_formal_tools::{LEAN_LINUX_X86_64_TREE_SHA256, LEAN_VERSION};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-lean-inductive-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create directory: {error}"));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.path());
    }
}

fn project_source(template: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(template)
        .join("project.zeno");
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read project: {error}"))
}

fn manifest(directory: &Directory, binary: &Path, runtime: &Path, sha256: &str) -> PathBuf {
    let path = directory.path().join("tools.json");
    let document = json!({
        "format": "zeno-fcis/tools/2",
        "tools": [{
            "backend": "lean",
            "path": binary,
            "version": LEAN_VERSION,
            "sha256": sha256,
            "runtime": {"root": runtime, "tree_sha256": LEAN_LINUX_X86_64_TREE_SHA256},
            "timeout_ms": 120000,
            "max_output_bytes": 1048576,
            "allowed_axioms": ["Quot.sound", "propext"]
        }]
    });
    fs::write(
        &path,
        serde_json::to_vec(&document).unwrap_or_else(|error| panic!("encode manifest: {error}")),
    )
    .unwrap_or_else(|error| panic!("write manifest: {error}"));
    path
}

fn inert_manifest(directory: &Directory) -> PathBuf {
    manifest(
        directory,
        Path::new("/definitely-absent-runtime/bin/lean"),
        Path::new("/definitely-absent-runtime"),
        &"11".repeat(32),
    )
}

fn prove(directory: &Directory, source: &str, tools: &Path) -> Output {
    let project = directory.path().join("project.zeno");
    fs::write(&project, source).unwrap_or_else(|error| panic!("write project: {error}"));
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg("prove")
        .arg(project)
        .args(["--claim", "600", "--backend", "lean", "--tools"])
        .arg(tools)
        .output()
        .unwrap_or_else(|error| panic!("run prove: {error}"))
}

#[test]
fn both_inductive_pilots_reach_lean_tool_admission() {
    for template in ["durable-counter", "account-lockout"] {
        let directory = Directory::new();
        let tools = inert_manifest(&directory);
        let output = prove(&directory, &project_source(template), &tools);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{template}: {stderr}");
        assert!(
            stderr.contains("lean claim 600 blocked: Missing"),
            "{template}: {stderr}"
        );
        assert!(!directory.path().join(".zeno-fcis").exists());
    }
}

#[test]
fn nonlinear_inductive_claim_is_refused_before_tool_execution() {
    let directory = Directory::new();
    let tools = inert_manifest(&directory);
    let source = project_source("durable-counter").replace(
        "pre.100.110 >= 0 && pre.100.111 >= 0;",
        "pre.100.110 * pre.100.110 >= 0;",
    );
    let output = prove(&directory, &source, &tools);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("claim 600 export blocked: UnsupportedMode"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("kernel checked"));
    assert!(!directory.path().join(".zeno-fcis").exists());
}

#[test]
#[ignore = "requires the pinned Lean 4.30.0 runtime in ZENO_FCIS_LEAN and ZENO_FCIS_LEAN_ROOT"]
fn pinned_lean_kernel_checks_both_inductive_pilots_through_prove() {
    let binary = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| panic!("ZENO_FCIS_LEAN")),
    );
    let runtime = PathBuf::from(
        std::env::var_os("ZENO_FCIS_LEAN_ROOT").unwrap_or_else(|| panic!("ZENO_FCIS_LEAN_ROOT")),
    );
    let binary_bytes = fs::read(&binary).unwrap_or_else(|error| panic!("read Lean: {error}"));
    let sha256 = RustCryptoSha256::hash(&binary_bytes).to_string();
    for template in ["durable-counter", "account-lockout"] {
        let directory = Directory::new();
        let tools = manifest(&directory, &binary, &runtime, &sha256);
        let output = prove(&directory, &project_source(template), &tools);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{template}: {stdout}{stderr}"
        );
        assert!(
            stdout.contains("lean claim 600: generated theorem kernel checked"),
            "{template}: {stdout}"
        );
        assert!(stdout.contains("checks the invariant on its exact genesis state"));
        let evidence = directory.path().join(".zeno-fcis/evidence");
        let records: Vec<_> = fs::read_dir(evidence)
            .unwrap_or_else(|error| panic!("retained evidence: {error}"))
            .map(|entry| {
                entry
                    .unwrap_or_else(|error| panic!("record entry: {error}"))
                    .path()
            })
            .collect();
        assert_eq!(records.len(), 1, "{template}");
        let metadata: Value = serde_json::from_slice(
            &fs::read(records[0].join("record.json"))
                .unwrap_or_else(|error| panic!("record metadata: {error}")),
        )
        .unwrap_or_else(|error| panic!("parse metadata: {error}"));
        assert_eq!(metadata["backend"], "lean");
        assert_eq!(metadata["claim_id"], 600);
        assert_eq!(metadata["status"], "kernel_checked");
        assert_eq!(metadata["toolchain_hash"], LEAN_LINUX_X86_64_TREE_SHA256);
        let source = fs::read_to_string(records[0].join("source"))
            .unwrap_or_else(|error| panic!("retained source: {error}"));
        assert!(source.contains("theorem history_holds"));
        assert!(source.contains("#print axioms history_holds"));
        assert!(!source.contains("theorem genesis_holds"));
        println!("{template}: {stdout}");
    }
}
