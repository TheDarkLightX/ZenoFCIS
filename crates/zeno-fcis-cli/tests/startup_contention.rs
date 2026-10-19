//! The real POSIX guardian/worker startup path on a contended CPU.
#[cfg(target_os = "linux")]
#[test]
fn posix_startup_completes_under_same_cpu_contention() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let done = std::process::Command::new("python3")
        .args(["-I", "-B"])
        .arg(root.join("tests/test-data/startup_contention.py"))
        .arg(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg(root.join("templates/durable-counter"))
        .arg("posix")
        .output()
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        done.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(String::from_utf8_lossy(&done.stdout).contains("4 actual POSIX startup runs passed"));
}

#[cfg(target_os = "linux")]
#[test]
fn systemd_startup_completes_under_same_cpu_contention() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let done = std::process::Command::new("python3")
        .args(["-I", "-B"])
        .arg(root.join("tests/test-data/startup_contention.py"))
        .arg(env!("CARGO_BIN_EXE_zeno-fcis"))
        .arg(root.join("templates/durable-counter"))
        .arg("systemd")
        .output()
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        done.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr)
    );
    assert!(String::from_utf8_lossy(&done.stdout).contains("4 actual SYSTEMD startup runs passed"));
}
