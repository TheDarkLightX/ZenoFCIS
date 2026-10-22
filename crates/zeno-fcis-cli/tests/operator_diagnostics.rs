//! Refusal details stay in private operator records, outside capture packets.
#[cfg(target_os = "linux")]
#[test]
fn capture_refusal_keeps_exact_operator_reason_out_of_public_json() {
    use std::{
        fs,
        path::Path,
        process::{Command, Stdio},
    };
    let child = Command::new(env!("CARGO_BIN_EXE_zeno-fcis"))
        .args([
            "--no-kb",
            "--containment-backend",
            "posix",
            "loop",
            "capture",
            "open",
            "--spec",
            "/does-not-exist-spec",
            "--reference",
            "/does-not-exist-reference",
            "--session",
            "/does-not-exist-session",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let pid = child.id();
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_else(|e| panic!("{e}"));
    let ticks: u64 = stat
        .rsplit_once(") ")
        .unwrap_or_else(|| panic!("stat command field"))
        .1
        .split_whitespace()
        .nth(19)
        .unwrap_or_else(|| panic!("stat start-time field"))
        .parse()
        .unwrap_or_else(|e| panic!("{e}"));
    let out = child.wait_with_output().unwrap_or_else(|e| panic!("{e}"));
    assert!(!out.status.success(), "{out:?}");
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(report["authority"], "none");
    assert_eq!(
        report["detail"]["reason"],
        "capture-containment-unavailable"
    );
    let root = Path::new("/tmp").join(format!("zeno-fcis-operator-{}", nix::unistd::geteuid()));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&root)
            .unwrap_or_else(|e| panic!("{e}"))
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let records: Vec<_> = fs::read_dir(root)
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|e| e.unwrap_or_else(|e| panic!("{e}")).path())
        .filter(|p| {
            p.file_name()
                .unwrap_or_else(|| panic!("record file name"))
                .to_string_lossy()
                .starts_with(&format!("{pid}-"))
        })
        .map(|p| {
            assert_eq!(
                fs::metadata(&p)
                    .unwrap_or_else(|e| panic!("{e}"))
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            serde_json::from_slice::<serde_json::Value>(
                &fs::read(p).unwrap_or_else(|e| panic!("{e}")),
            )
            .unwrap_or_else(|e| panic!("{e}"))
        })
        .filter(|r| r["observer"] == "refusal" && r["process_start_ticks"] == ticks)
        .collect();
    assert_eq!(records.len(), 1, "{records:?}; {out:?}");
    let reason = records[0]["supervisor_reason"]
        .as_str()
        .unwrap_or_else(|| panic!("supervisor reason"));
    assert!(
        reason.starts_with("containment-unavailable:posix:"),
        "{reason}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains(reason));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("operator-containment"));
    assert!(
        out.stderr.is_empty(),
        "normal diagnostic persistence does not add public stderr"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn unrelated_partial_records_do_not_break_startup_but_matching_corruption_refuses() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/test-data/operator_records_race.py");
    let output = std::process::Command::new("python3")
        .args(["-I", "-B"])
        .arg(script)
        .output()
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("matching-corruption refusals passed")
    );
}
