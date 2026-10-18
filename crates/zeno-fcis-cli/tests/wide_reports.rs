//! Actual returned reports and Python consumers at the signed-128 boundaries.
#[test]
fn wide_door_judge_and_workspace_reports_round_trip_through_real_readers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let done = std::process::Command::new("python3")
        .args([
            "-B",
            "-m",
            "unittest",
            "discover",
            "-s",
            "integrations/zal",
            "-p",
            "test_wide_reports.py",
            "-v",
        ])
        .env("ZENO_WIDE_REPORT_TEST_CLI", env!("CARGO_BIN_EXE_zeno-fcis"))
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    let log = String::from_utf8_lossy(&done.stderr);
    assert!(log.contains("Ran 5 tests"), "{log}");
    assert!(!log.contains("skipped"), "{log}");
}
