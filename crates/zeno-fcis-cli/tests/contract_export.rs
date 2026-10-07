//! `zeno-fcis contract export-program`: a contract's current decision program
//! in the encoding `optimize`, `transform` and `loop` read, so that the
//! optimization journey needs no other tool.
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
            "zeno-fcis-export-{label}-{}-{}",
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

fn crate_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn zeno(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("run zeno-fcis: {error}"))
}

fn text(path: &Path) -> String {
    path.to_str()
        .unwrap_or_else(|| panic!("{} is not UTF-8", path.display()))
        .to_owned()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Exports `contract`'s program to `out` and returns the report.
fn export(contract: &Path, out: &Path) -> Value {
    let output = zeno(&[
        "contract",
        "export-program",
        &text(contract),
        "--out",
        &text(out),
        "--format",
        "json",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    json(&output)
}

/// Optimizes `program`, then replays the receipt of the accepted candidate:
/// the original and candidate node counts.
fn optimize_and_replay(program: &Path, directory: &Path) -> (u64, u64) {
    let candidate = directory.join("candidate.zcve");
    let receipt = directory.join("receipt.json");
    let optimized = zeno(&[
        "optimize",
        "--program",
        &text(program),
        "--candidate-out",
        &text(&candidate),
        "--receipt",
        &text(&receipt),
    ]);
    let report = json(&optimized);
    assert_eq!(optimized.status.code(), Some(0), "{report}");
    assert_eq!(report["status"], "improved");
    let replayed = zeno(&[
        "transform",
        "replay",
        "--receipt",
        &text(&receipt),
        "--original",
        &text(program),
        "--candidate",
        &text(&candidate),
    ]);
    assert_eq!(replayed.status.code(), Some(0));
    assert_eq!(json(&replayed)["status"], "replayed");
    let nodes = |side: &str| {
        report["detail"]["best"]["receipt"][side]["nodes"]
            .as_u64()
            .unwrap_or_else(|| panic!("{side} nodes in {report}"))
    };
    (nodes("original"), nodes("candidate"))
}

#[test]
fn an_exported_program_optimizes_and_its_receipt_replays() {
    let root = TempRoot::new("journey");
    for (contract, original, candidate) in [
        (crate_path("tests/fixtures/spend-approval"), 98, Some(88)),
        (crate_path("templates/durable-counter"), 19, None),
    ] {
        let directory = root.path().join(
            contract
                .file_name()
                .unwrap_or_else(|| panic!("{}", contract.display())),
        );
        fs::create_dir(&directory).unwrap_or_else(|error| panic!("{error}"));
        let program = directory.join("program.zcve");
        let report = export(&contract, &program);
        assert_eq!(report["status"], "exported");
        assert_eq!(report["version"], 1);
        assert_eq!(report["program"]["nodes"], original);
        assert_eq!(
            report["program"]["bytes"],
            u64::try_from(read(&program).len()).unwrap_or(u64::MAX)
        );
        let (before, after) = optimize_and_replay(&program, &directory);
        assert_eq!(before, original);
        assert!(after < before, "{after} >= {before}");
        if let Some(candidate) = candidate {
            assert_eq!(after, candidate);
        }
    }
}

#[test]
fn the_export_is_the_contracts_current_program_and_never_overwrites() {
    let root = TempRoot::new("current");
    // The withdrawal queue's program is the original the recorded transform
    // artifacts compare.
    let original = root.path().join("original.zcve");
    export(&crate_path("templates/withdrawal-queue"), &original);
    assert!(
        read(&original)
            == read(&crate_path(
                "../../docs/benchmarks/withdrawal-queue/artifacts/current-decision-scalars-original.zcve"
            ))
    );
    // After an adoption, the program is the adopted candidate.
    let adopted = crate_path("tests/fixtures/withdrawal-queue-adopted");
    let current = root.path().join("current.zcve");
    let report = export(&adopted, &current);
    assert_eq!(report["version"], 2);
    assert!(read(&current) == read(&adopted.join("v2/adoptions/1/program.zcve")));
    // The bytes are a function of the contract.
    let spend = crate_path("tests/fixtures/spend-approval");
    let (first, second) = (
        root.path().join("first.zcve"),
        root.path().join("second.zcve"),
    );
    export(&spend, &first);
    export(&spend, &second);
    assert!(read(&first) == read(&second));
    // An existing file is never overwritten.
    let refused = zeno(&[
        "contract",
        "export-program",
        &text(&crate_path("templates/durable-counter")),
        "--out",
        &text(&current),
        "--format",
        "json",
    ]);
    assert_eq!(refused.status.code(), Some(1));
    assert_eq!(json(&refused)["error"]["code"], "output-exists");
    assert!(read(&current) == read(&adopted.join("v2/adoptions/1/program.zcve")));
    // An invalid contract writes nothing.
    let invalid = root.path().join("invalid");
    fs::create_dir_all(invalid.join("v2")).unwrap_or_else(|error| panic!("{error}"));
    fs::copy(
        crate_path("templates/durable-counter/project.zeno"),
        invalid.join("project.zeno"),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    fs::write(invalid.join("v2/policy.json"), "{}").unwrap_or_else(|error| panic!("{error}"));
    let out = root.path().join("invalid.zcve");
    let refused = zeno(&[
        "contract",
        "export-program",
        &text(&invalid),
        "--out",
        &text(&out),
    ]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(!out.exists());
}
