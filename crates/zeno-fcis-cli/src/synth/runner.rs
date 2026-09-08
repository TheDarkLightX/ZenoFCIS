//! Target execution belongs to this shell. Acceptance is shared across runners.
use super::sha;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};
use zeno_fcis_synthesis::finite::{Case, Contract};

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
use std::{
    fs::OpenOptions,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
const MAX_OUTPUT: u64 = 32 * 1024 * 1024;
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
const TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct RunError {
    pub code: &'static str,
    pub message: String,
}
fn error(code: &'static str, message: impl ToString) -> RunError {
    RunError {
        code,
        message: message.to_string(),
    }
}
fn io(error: std::io::Error) -> RunError {
    self::error("tool-io", error)
}
pub(super) struct Observation {
    pub stdout: Vec<u8>,
    pub tool: Value,
    pub fixture_sha256: String,
    pub execution_artifact_sha256: String,
}
pub(super) trait TargetRunner {
    fn tool_requirement(&self) -> &'static str;
    fn run(
        &self,
        source: &str,
        cases: &[Case],
        tool: Option<&Path>,
    ) -> Result<Observation, RunError>;
}
pub(super) struct RustRunner;
pub(super) struct PythonRunner;

const RUST_FIXTURE: &str = r#"mod target { include!("transition.rs"); }
fn main() {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    for line in input.lines() {
        let values: Vec<i64> = line.split_ascii_whitespace().map(|s| s.parse().unwrap()).collect();
        match target::transition(&values) {
            Some(out) => println!("{}", out.iter().map(i64::to_string).collect::<Vec<_>>().join(" ")),
            None => println!("trap"),
        }
    }
}
"#;
const PYTHON_FIXTURE: &str = r#"
if __name__ == "__main__":
    import sys
    for line in sys.stdin:
        result = transition(tuple(int(v) for v in line.split()))
        print("trap" if result is None else " ".join(str(v) for v in result))
"#;

impl TargetRunner for RustRunner {
    fn tool_requirement(&self) -> &'static str {
        "rustc 1.97.1"
    }
    fn run(
        &self,
        source: &str,
        cases: &[Case],
        tool: Option<&Path>,
    ) -> Result<Observation, RunError> {
        let temp = Temp::new()?;
        let compiler = match tool {
            Some(path) => fs::canonicalize(path).map_err(|e| error("tool-missing", e))?,
            None => {
                let rustup = find("rustup")?;
                let path = execute(
                    &rustup,
                    &[
                        "which".into(),
                        "--toolchain".into(),
                        "1.97.1".into(),
                        "rustc".into(),
                    ],
                    b"",
                    &temp.0,
                )?;
                fs::canonicalize(
                    String::from_utf8(path)
                        .map_err(|e| error("tool-protocol", e))?
                        .trim(),
                )
                .map_err(|e| error("tool-missing", e))?
            }
        };
        let identity = identify(&compiler, &temp.0, "rustc 1.97.1 ")?;
        fs::write(temp.0.join("transition.rs"), source).map_err(io)?;
        fs::write(temp.0.join("fixture.rs"), RUST_FIXTURE).map_err(io)?;
        execute(
            &compiler,
            &[
                "--edition=2024".into(),
                "--crate-name".into(),
                "zeno_synthesis_fixture".into(),
                "-Dwarnings".into(),
                "-Cdebuginfo=0".into(),
                "fixture.rs".into(),
                "-o".into(),
                "fixture".into(),
            ],
            b"",
            &temp.0,
        )?;
        let executable = temp.0.join("fixture");
        let execution_artifact_sha256 = binary_hash(&executable)?;
        let stdout = execute(&executable, &[], &inputs(cases), &temp.0)?;
        if binary_hash(&executable)? != execution_artifact_sha256 {
            return Err(error(
                "execution-artifact-drift",
                "compiled fixture changed",
            ));
        }
        recheck(&compiler, &identity)?;
        Ok(Observation {
            stdout,
            tool: identity,
            fixture_sha256: sha(RUST_FIXTURE.as_bytes()),
            execution_artifact_sha256,
        })
    }
}
impl TargetRunner for PythonRunner {
    fn tool_requirement(&self) -> &'static str {
        "Python 3"
    }
    fn run(
        &self,
        source: &str,
        cases: &[Case],
        tool: Option<&Path>,
    ) -> Result<Observation, RunError> {
        let temp = Temp::new()?;
        let interpreter = match tool {
            Some(path) => fs::canonicalize(path).map_err(|e| error("tool-missing", e))?,
            None => find("python3")?,
        };
        let identity = identify(&interpreter, &temp.0, "Python 3.")?;
        let fixture = format!("{source}{PYTHON_FIXTURE}");
        fs::write(temp.0.join("fixture.py"), &fixture).map_err(io)?;
        let stdout = execute(
            &interpreter,
            &["-I".into(), "-B".into(), "fixture.py".into()],
            &inputs(cases),
            &temp.0,
        )?;
        recheck(&interpreter, &identity)?;
        Ok(Observation {
            stdout,
            tool: identity,
            fixture_sha256: sha(fixture.as_bytes()),
            execution_artifact_sha256: sha(fixture.as_bytes()),
        })
    }
}
fn inputs(cases: &[Case]) -> Vec<u8> {
    let mut wire = String::new();
    for case in cases {
        wire.push_str(
            &case
                .input
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(" "),
        );
        wire.push('\n');
    }
    wire.into_bytes()
}

/// Runners supply bytes, never a verdict. Recheck shape, exact result and relation.
pub(super) fn validate(contract: &Contract, cases: &[Case], stdout: &[u8]) -> Result<(), RunError> {
    let text = std::str::from_utf8(stdout).map_err(|e| error("output-protocol", e))?;
    if !text.ends_with('\n') || text.lines().count() != cases.len() {
        return Err(error(
            "output-protocol",
            "expected exactly one newline-terminated output per input",
        ));
    }
    // Verify that the corpus itself covers the original space, in the right order.
    let mut space = contract.input_space().map_err(|e| error("coverage", e))?;
    for (ordinal, (line, case)) in text.lines().zip(cases).enumerate() {
        if space.next().as_deref() != Some(case.input.as_slice()) {
            return Err(error("coverage", ordinal));
        }
        let output = line
            .split(' ')
            .map(str::parse::<i64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| error("output-protocol", e))?;
        if output
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(" ")
            != line
        {
            return Err(error("output-protocol", "noncanonical integer output"));
        }
        if output != case.output
            || !contract
                .holds(&case.input, &output)
                .map_err(|e| error("contract-recheck", e))?
        {
            return Err(error(
                "counterexample",
                json!({"ordinal":ordinal,"input":case.input,"expected":case.output,"actual":output}),
            ));
        }
    }
    if space.next().is_some() {
        return Err(error("coverage", "incomplete corpus"));
    }
    Ok(())
}

fn find(name: &str) -> Result<PathBuf, RunError> {
    for root in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let path = root.join(name);
        if path.is_file() {
            return fs::canonicalize(path).map_err(io);
        }
    }
    Err(error("tool-missing", name))
}
fn binary_hash(path: &Path) -> Result<String, RunError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() > 256 * 1024 * 1024 {
        return Err(error("tool-size", "binary exceeds identity limit"));
    }
    Ok(sha(&bytes))
}
fn identify(path: &Path, temp: &Path, prefix: &str) -> Result<Value, RunError> {
    let digest = binary_hash(path)?;
    let version = execute(path, &["--version".into()], b"", temp)?;
    let version = std::str::from_utf8(&version)
        .map_err(|e| error("tool-version", e))?
        .trim();
    if !version.starts_with(prefix) {
        return Err(error("tool-version", version));
    }
    Ok(
        json!({"path":path,"binary_sha256":digest,"version":version,"identity_scope":"executable-and-version; dynamic dependencies and compiler correctness are external premises"}),
    )
}
fn recheck(path: &Path, identity: &Value) -> Result<(), RunError> {
    if identity["binary_sha256"] != binary_hash(path)? {
        return Err(error("tool-drift", path.display()));
    }
    Ok(())
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self, RunError> {
        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-synth-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io)?;
        }
        Ok(Self(root))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn execute(path: &Path, args: &[OsString], input: &[u8], cwd: &Path) -> Result<Vec<u8>, RunError> {
    #[cfg(not(all(target_os = "linux", not(target_env = "uclibc"))))]
    {
        let _ = (path, args, input, cwd);
        Err(error(
            "unsupported-runtime-platform",
            "target execution currently requires Linux waitid and process-group cleanup",
        ))
    }
    #[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
    {
        use nix::sys::wait::{Id, WaitPidFlag, WaitStatus, waitid};
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        use std::os::unix::process::CommandExt;
        let sequence = crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let stdin = cwd.join(format!("stdin-{sequence}"));
        let stdout = cwd.join(format!("stdout-{sequence}"));
        let stderr = cwd.join(format!("stderr-{sequence}"));
        let mut stream = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdin)
            .map_err(io)?;
        stream.write_all(input).map_err(io)?;
        drop(stream);
        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout)
            .map_err(io)?;
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr)
            .map_err(io)?;
        let mut command = Command::new(path);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::from(File::open(&stdin).map_err(io)?))
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .process_group(0);
        for key in [
            "RUSTC_BOOTSTRAP",
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "PYTHONPATH",
            "PYTHONHOME",
        ] {
            command.env_remove(key);
        }
        command.env("LC_ALL", "C");
        let mut child = command.spawn().map_err(|e| error("tool-start", e))?;
        let pid = match i32::try_from(child.id()) {
            Ok(pid) => Pid::from_raw(pid),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("tool-pid", "PID overflow"));
            }
        };
        let started = Instant::now();
        // Observe exit without reaping, so the owned group ID cannot be reused
        // before descendants are terminated. Reap only after group cleanup.
        let terminal = loop {
            let too_large = fs::metadata(&stdout)
                .map(|m| m.len() > MAX_OUTPUT)
                .unwrap_or(true)
                || fs::metadata(&stderr)
                    .map(|m| m.len() > MAX_OUTPUT)
                    .unwrap_or(true);
            if too_large {
                break Err(error("output-limit", MAX_OUTPUT));
            }
            if started.elapsed() > TIMEOUT {
                break Err(error("timeout", "30-second target operation limit"));
            }
            match waitid(
                Id::Pid(pid),
                WaitPidFlag::WEXITED | WaitPidFlag::WNOHANG | WaitPidFlag::WNOWAIT,
            ) {
                Ok(WaitStatus::StillAlive) => std::thread::sleep(Duration::from_millis(5)),
                Ok(status) => break Ok(status),
                Err(err) => break Err(error("tool-wait", err)),
            }
        };
        let cleanup = killpg(pid, Signal::SIGKILL);
        let _ = child.kill();
        let reaped = child.wait();
        let status = terminal?;
        if let Err(err) = cleanup
            && err != nix::errno::Errno::ESRCH
        {
            return Err(error("tool-cleanup", err));
        }
        reaped.map_err(io)?;
        let output = read_output(&stdout, MAX_OUTPUT)?;
        let diagnostics = read_output(&stderr, MAX_OUTPUT)?;
        if !matches!(status, WaitStatus::Exited(_, 0)) {
            return Err(error(
                "tool-exit",
                format!("{status:?}: {}", String::from_utf8_lossy(&diagnostics)),
            ));
        }
        Ok(output)
    }
}

#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
fn read_output(path: &Path, limit: u64) -> Result<Vec<u8>, RunError> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io)?;
    if bytes.len() as u64 > limit {
        return Err(error("output-limit", limit));
    }
    Ok(bytes)
}

#[cfg(all(test, target_os = "linux", not(target_env = "uclibc")))]
mod process_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn final_capture_is_bounded_and_exited_parents_do_not_leave_running_children() {
        let temp = Temp::new().unwrap_or_else(|e| panic!("{}", e.message));
        let path = temp.0.join("capture");
        fs::write(&path, b"1234").unwrap();
        assert_eq!(read_output(&path, 3).err().unwrap().code, "output-limit");
        let bytes = execute(
            Path::new("/bin/sh"),
            &["-c".into(), "sleep 30 & echo $!; exit 0".into()],
            b"",
            &temp.0,
        )
        .unwrap_or_else(|e| panic!("{}", e.message));
        let pid = std::str::from_utf8(&bytes).unwrap().trim();
        let mut running = true;
        for _ in 0..100 {
            running = fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .is_some_and(|stat| {
                    stat.rsplit_once(") ")
                        .is_some_and(|(_, tail)| !tail.starts_with('Z'))
                });
            if !running {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!running, "descendant survived parent exit");
        let result = execute(
            Path::new("/bin/sh"),
            &[
                "-c".into(),
                format!("truncate -s {} /proc/self/fd/1", MAX_OUTPUT + 1).into(),
            ],
            b"",
            &temp.0,
        );
        assert_eq!(result.err().unwrap().code, "output-limit");
    }
}
