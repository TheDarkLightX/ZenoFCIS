//! Read-only program inspection through the installed command and codec.
#![forbid(unsafe_code)]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-loop-print-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("{error}"));
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("{error}"))
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap_or_else(|| panic!("UTF-8 test path"))
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn round_trip(original: &Path) {
    let directory = Directory::new();
    let bytes = read(original);
    let printed = run(&["loop", "print", "--program", text(original)]);
    assert_eq!(printed.status.code(), Some(0), "{original:?}: {printed:?}");
    let program: Value = serde_json::from_slice(&printed.stdout)
        .unwrap_or_else(|error| panic!("program JSON: {error}: {printed:?}"));
    assert_eq!(program.as_object().map(|o| o.len()), Some(4));
    // serde_json's default object map sorts keys; compact encoding plus LF is
    // the canonical JSON contract. The command must be directly encodable.
    let mut canonical = serde_json::to_vec(&program).unwrap_or_else(|error| panic!("{error}"));
    canonical.push(b'\n');
    assert_eq!(printed.stdout, canonical, "canonical JSON for {original:?}");
    let json = directory.0.join("program.json");
    fs::write(&json, &printed.stdout).unwrap_or_else(|error| panic!("{error}"));
    let encoded = directory.0.join("round-trip.zcve");
    let output = run(&[
        "loop",
        "encode",
        "--program",
        text(&json),
        "--out",
        text(&encoded),
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(read(&encoded), bytes, "round-trip bytes for {original:?}");
    assert_eq!(read(original), bytes, "input must remain unchanged");
}

#[test]
fn print_round_trips_small_and_extended_shipped_programs_through_the_binary() {
    let directory = Directory::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (index, example) in [
        "examples/guide-counter",
        "examples/contract-tables/delegation",
        "examples/arithmetic-fused",
    ]
    .iter()
    .enumerate()
    {
        let program = directory.0.join(format!("{index}.zcve"));
        let output = run(&[
            "contract",
            "export-program",
            text(&root.join(example)),
            "--out",
            text(&program),
            "--format",
            "json",
        ]);
        assert_eq!(output.status.code(), Some(0), "{example}: {output:?}");
        round_trip(&program);
    }
}

#[test]
fn print_refuses_malformed_oversized_and_missing_programs_without_writes() {
    let directory = Directory::new();
    for (name, bytes, reason) in [
        (
            "malformed.zcve",
            b"not a program".to_vec(),
            "program-not-admitted",
        ),
        ("oversized.zcve", vec![0; 524_289], "program-too-large"),
    ] {
        let path = directory.0.join(name);
        fs::write(&path, &bytes).unwrap_or_else(|error| panic!("{error}"));
        let output = run(&["loop", "print", "--program", text(&path)]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let report: Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(report["status"], "refused");
        assert_eq!(report["detail"]["reason"], reason);
        assert_eq!(read(&path), bytes);
    }
    let output = run(&[
        "loop",
        "print",
        "--program",
        text(&directory.0.join("missing")),
    ]);
    assert_eq!(output.status.code(), Some(3), "{output:?}");
    let output = run(&["loop", "print"]);
    assert_eq!(output.status.code(), Some(64), "{output:?}");
    assert_eq!(
        fs::read_dir(&directory.0)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        2
    );
}
