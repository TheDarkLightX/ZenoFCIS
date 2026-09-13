//! Real filesystem failures must keep the CLI bounded and retryable.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CLI: &str = env!("CARGO_BIN_EXE_zeno-fcis");

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-cli-file-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap_or_else(|error| panic!("create test directory: {error}"));
        Self(path)
    }

    fn project(&self) -> PathBuf {
        let path = self.0.join("project");
        assert!(
            Command::new(CLI)
                .arg("new")
                .arg(&path)
                .output()
                .unwrap_or_else(|error| panic!("{error}"))
                .status
                .success()
        );
        path.join("project.zeno")
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn generate(project: &Path, out: &Path) -> Command {
    let mut command = Command::new(CLI);
    command
        .arg("generate")
        .arg(project)
        .arg("--out")
        .arg(out)
        .args(["--format", "json"]);
    command
}

fn limited_write(command: &mut Command) -> Output {
    Command::new("python3")
        .args(["-c", "import os,resource,signal,sys; resource.setrlimit(resource.RLIMIT_FSIZE,(128,128)); signal.signal(signal.SIGXFSZ,signal.SIG_IGN); os.execv(sys.argv[1],sys.argv[1:])"])
        .arg(command.get_program())
        .args(command.get_args())
        .output()
        .unwrap_or_else(|error| panic!("run child with a file-size limit: {error}"))
}

#[test]
fn named_pipe_artifacts_return_json_instead_of_blocking() {
    let root = Directory::new();
    let project = root.project();
    let out = root.0.join("generated");
    fs::create_dir(&out).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        Command::new("mkfifo")
            .arg(out.join("generated.rs"))
            .status()
            .unwrap_or_else(|error| panic!("{error}"))
            .success()
    );
    for check in [false, true] {
        let mut command = generate(&project, &out);
        if check {
            command.arg("--check");
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        let start = Instant::now();
        while child
            .try_wait()
            .unwrap_or_else(|error| panic!("{error}"))
            .is_none()
        {
            if start.elapsed() > Duration::from_secs(3) {
                child.kill().unwrap_or_else(|error| panic!("{error}"));
                child.wait().unwrap_or_else(|error| panic!("{error}"));
                panic!("opening a named-pipe artifact blocked the CLI");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stderr.is_empty());
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            json["error"]["code"],
            if check {
                "artifact-read-failed"
            } else {
                "artifact-write-failed"
            }
        );
    }
}

#[test]
fn failed_replacement_keeps_destination_and_removes_temporary_file() {
    let root = Directory::new();
    let project = root.project();
    let out = root.0.join("generated");
    fs::create_dir(&out).unwrap_or_else(|error| panic!("{error}"));
    fs::write(out.join("generated.rs"), b"keep this version")
        .unwrap_or_else(|error| panic!("{error}"));
    let output = limited_write(&mut generate(&project, &out));
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        fs::read(out.join("generated.rs")).unwrap_or_else(|error| panic!("{error}")),
        b"keep this version"
    );
    assert_eq!(
        fs::read_dir(&out)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        1
    );
    assert!(
        generate(&project, &out)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
            .status
            .success()
    );
}

#[test]
fn failed_first_project_file_can_be_retried() {
    let root = Directory::new();
    let out = root.0.join("project");
    let output = limited_write(Command::new(CLI).arg("new").arg(&out));
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        fs::read_dir(&out)
            .unwrap_or_else(|error| panic!("{error}"))
            .count(),
        0
    );
    assert!(
        Command::new(CLI)
            .arg("new")
            .arg(&out)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
            .status
            .success()
    );
}

#[test]
fn regular_file_symlinks_keep_their_existing_behavior() {
    let root = Directory::new();
    let project = root.project();
    let out = root.0.join("generated");
    assert!(
        generate(&project, &out)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
            .status
            .success()
    );
    let retained = root.0.join("retained.rs");
    fs::rename(out.join("generated.rs"), &retained).unwrap_or_else(|error| panic!("{error}"));
    std::os::unix::fs::symlink(&retained, out.join("generated.rs"))
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        generate(&project, &out)
            .arg("--check")
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
            .status
            .success()
    );
    assert!(
        generate(&project, &out)
            .output()
            .unwrap_or_else(|error| panic!("{error}"))
            .status
            .success()
    );
    assert!(
        fs::symlink_metadata(out.join("generated.rs"))
            .unwrap_or_else(|error| panic!("{error}"))
            .is_symlink()
    );
}
