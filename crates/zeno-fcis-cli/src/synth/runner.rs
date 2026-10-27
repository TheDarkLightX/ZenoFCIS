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

#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
use std::{
    ffi::OsStr,
    fs::OpenOptions,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
const MAX_OUTPUT: u64 = 32 * 1024 * 1024;

/// Starts `command`, retrying a bounded number of times while its executable
/// is busy, within the time budget that runs from `started` for `budget`.
///
/// Linux refuses to execute a file that any process holds open for writing
/// (`ETXTBSY`). A child that another thread forks while this process writes an
/// executable inherits that descriptor until it executes its own program, so
/// the condition clears quickly. No attempt starts after the budget ends,
/// including the first: an exhausted budget, or a wait that would reach its
/// end, gives an error of kind `TimedOut`. Every other error, and a file that
/// stays busy through the last attempt, fails closed.
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
pub(super) fn spawn_unless_busy(
    command: &mut Command,
    started: Instant,
    budget: Duration,
) -> std::io::Result<std::process::Child> {
    let busy = nix::errno::Errno::ETXTBSY as i32;
    let mut delay = Duration::from_millis(1);
    let timed_out = || {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "the time budget ended while the executable was busy",
        )
    };
    let mut retries = 8;
    loop {
        if started.elapsed() >= budget {
            return Err(timed_out());
        }
        let e = match command.spawn() {
            Ok(child) => return Ok(child),
            Err(e) => e,
        };
        if e.raw_os_error() != Some(busy) || retries == 0 {
            return Err(e);
        }
        if budget.saturating_sub(started.elapsed()) <= delay {
            return Err(timed_out());
        }
        std::thread::sleep(delay);
        delay = delay.saturating_mul(2);
        retries -= 1;
    }
}
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
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
    /// Exact command line, wire protocol, and per-case call this runner runs,
    /// in an owned temporary working directory with the scrubbed environment of
    /// `execute`. Discovery publishes it so an agent never guesses the harness.
    fn invocation(&self) -> &'static str;
    fn run(
        &self,
        source: &str,
        cases: &[Case],
        tool: Option<&Path>,
    ) -> Result<Observation, RunError>;
}
pub(super) struct RustRunner;
pub(super) struct PythonRunner;
pub(super) struct JavaScriptRunner;

const RUST_FIXTURE: &str = r#"mod target { include!("transition.rs"); }
fn main() {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    for line in input.lines() {
        let values: Vec<i128> = line.split_ascii_whitespace().map(|s| s.parse().unwrap()).collect();
        match target::transition(&values) {
            Some(out) => println!("{}", out.iter().map(i128::to_string).collect::<Vec<_>>().join(" ")),
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
// The harness transports bytes and never decides acceptance. It calls the
// declared string ABI with the exact input line, so the module performs its own
// admission instead of receiving a re-normalized or coerced argument.
const JAVASCRIPT_FIXTURE: &str = r#"import { readFileSync, writeFileSync } from "node:fs";
import { transition } from "./transition.mjs";
const lines = readFileSync(0, "utf8").split("\n");
if (lines.length > 0 && lines[lines.length - 1] === "") { lines.pop(); }
let out = "";
for (const line of lines) {
  const result = transition(line);
  if (result === null) { out += "trap\n"; continue; }
  if (typeof result !== "string") { throw new TypeError("non-string transition result"); }
  out += result + "\n";
}
writeFileSync(1, out);
"#;

impl TargetRunner for RustRunner {
    fn tool_requirement(&self) -> &'static str {
        "rustc 1.97.1"
    }
    fn invocation(&self) -> &'static str {
        "rustc --edition=2024 --crate-name zeno_synthesis_fixture -Dwarnings -Cdebuginfo=0 \
         fixture.rs -o fixture, then ./fixture with one whitespace-separated input tuple per \
         stdin line; fixture.rs includes transition.rs and prints the output tuple or `trap` \
         per line"
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
        write_source(&temp.0.join("transition.rs"), source)?;
        write_source(&temp.0.join("fixture.rs"), RUST_FIXTURE)?;
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
    fn invocation(&self) -> &'static str {
        "python3 -I -B fixture.py with one whitespace-separated input tuple per stdin line; \
         fixture.py is transition.py plus a reader that calls transition(tuple(int, ...)) and \
         prints the output tuple or `trap` per line"
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
        write_source(&temp.0.join("fixture.py"), &fixture)?;
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
impl TargetRunner for JavaScriptRunner {
    fn tool_requirement(&self) -> &'static str {
        "Node.js 22"
    }
    fn invocation(&self) -> &'static str {
        "node fixture.mjs, with every inherited NODE_* variable removed, and one canonical input \
         tuple per stdin line; fixture.mjs imports ./transition.mjs and calls transition(line) \
         with the exact line as a primitive string, printing the returned string or `trap` for \
         null, one line per input"
    }
    fn run(
        &self,
        source: &str,
        cases: &[Case],
        tool: Option<&Path>,
    ) -> Result<Observation, RunError> {
        let temp = Temp::new()?;
        let node = match tool {
            Some(path) => fs::canonicalize(path).map_err(|e| error("tool-missing", e))?,
            None => find("node")?,
        };
        // Identification and execution both run through `execute`, which drops
        // every inherited NODE_* switch, so a preload cannot forge either one.
        let identity = identify(&node, &temp.0, "v22.")?;
        let module = temp.0.join("transition.mjs");
        let fixture = temp.0.join("fixture.mjs");
        write_source(&module, source)?;
        write_source(&fixture, JAVASCRIPT_FIXTURE)?;
        let fixture_sha256 = sha(JAVASCRIPT_FIXTURE.as_bytes());
        let module_sha256 = sha(source.as_bytes());
        let stdout = execute(&node, &["fixture.mjs".into()], &inputs(cases), &temp.0)?;
        // The executed artifact is a two-file module graph, so its identity is
        // the digest of both exact file digests, not of a single entry point.
        if binary_hash(&fixture)? != fixture_sha256 || binary_hash(&module)? != module_sha256 {
            return Err(error(
                "execution-artifact-drift",
                "executed module graph changed",
            ));
        }
        recheck(&node, &identity)?;
        let graph = sha(
            format!(
                "zeno-fcis/synthesis-module-graph/1\nfixture.mjs {fixture_sha256}\ntransition.mjs {module_sha256}\n"
            )
            .as_bytes(),
        );
        Ok(Observation {
            stdout,
            tool: identity,
            fixture_sha256,
            execution_artifact_sha256: graph,
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
                .map(i128::to_string)
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
            .map(str::parse::<i128>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| error("output-protocol", e))?;
        if output
            .iter()
            .map(i128::to_string)
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
                json!({"ordinal":ordinal,"input":crate::report_numbers::integers(&case.input),"expected":crate::report_numbers::integers(&case.output),"actual":crate::report_numbers::integers(&output)}),
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

/// Inherited variables that could change tool identification or target
/// execution. The `NODE_` prefix covers `NODE_OPTIONS` preloads, `NODE_PATH`
/// resolution, and every analogous Node switch, including later additions.
/// Dynamic linker and library premises remain outside this scope.
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
const SCRUBBED: [&str; 7] = [
    "RUSTC_BOOTSTRAP",
    "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "PYTHONPATH",
    "PYTHONHOME",
];
#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
fn scrubbed(key: &OsStr) -> bool {
    let key = key.as_encoded_bytes();
    key.starts_with(b"NODE_") || SCRUBBED.iter().any(|name| name.as_bytes() == key)
}

fn write_source(path: &Path, source: &str) -> Result<(), RunError> {
    use std::io::Write as _;

    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?
        .write_all(source.as_bytes())
        .map_err(io)
}

pub(super) struct Temp(PathBuf);
impl Temp {
    /// Owned 0700 directory for tests that need a disposable executable.
    #[cfg(all(
        test,
        any(
            target_os = "macos",
            all(target_os = "linux", not(target_env = "uclibc"))
        )
    ))]
    pub(super) fn path(&self) -> &Path {
        &self.0
    }
    pub(super) fn new() -> Result<Self, RunError> {
        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-synth-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&root)
                .map_err(io)?;
            // Creation is already private. Restore owner permissions even under
            // a restrictive umask without ever exposing the directory to peers.
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(io)?;
        }
        #[cfg(not(unix))]
        fs::create_dir(&root).map_err(io)?;
        Ok(Self(root))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn execute(path: &Path, args: &[OsString], input: &[u8], cwd: &Path) -> Result<Vec<u8>, RunError> {
    #[cfg(not(any(
        target_os = "macos",
        all(target_os = "linux", not(target_env = "uclibc"))
    )))]
    {
        let _ = (path, args, input, cwd);
        Err(error(
            "unsupported-runtime-platform",
            "target execution requires a supported Linux or macOS process-group supervisor",
        ))
    }
    #[cfg(target_os = "macos")]
    {
        execute_macos(path, args, input, cwd, TIMEOUT, MAX_OUTPUT)
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
        for (key, _) in std::env::vars_os() {
            if scrubbed(&key) {
                command.env_remove(&key);
            }
        }
        command.env("LC_ALL", "C");
        // The operation limit covers starting the process as well as running it.
        let started = Instant::now();
        let mut child = spawn_unless_busy(&mut command, started, TIMEOUT).map_err(|e| {
            if e.kind() == std::io::ErrorKind::TimedOut {
                error("timeout", "30-second target operation limit")
            } else {
                error("tool-start", e)
            }
        })?;
        let pid = match i32::try_from(child.id()) {
            Ok(pid) => Pid::from_raw(pid),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error("tool-pid", "PID overflow"));
            }
        };
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

// Unlike Linux waitid(WNOWAIT), Child::try_wait reaps the target. A separate
// owned group leader therefore retains the group identity until cleanup.
// Keeping it live also avoids Darwin's EPERM for zombie-only groups. This is
// process supervision of regenerated code, not a sandbox for hostile tools.
#[cfg(target_os = "macos")]
struct MacGroup(std::process::Child);
#[cfg(target_os = "macos")]
impl MacGroup {
    fn new(cwd: &Path, started: Instant, budget: Duration) -> Result<Self, RunError> {
        use std::os::unix::process::CommandExt;

        let mut command = Command::new("/bin/cat");
        command
            .env_clear()
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        // The parent's held stdin keeps the helper live; EOF ends it if the
        // parent dies. Both starts use the same original operation deadline.
        spawn_unless_busy(&mut command, started, budget)
            .map(Self)
            .map_err(start_error)
    }

    fn pid(&self) -> Result<nix::unistd::Pid, RunError> {
        i32::try_from(self.0.id())
            .map(nix::unistd::Pid::from_raw)
            .map_err(|_| error("tool-pid", "PID overflow"))
    }
}
#[cfg(target_os = "macos")]
impl Drop for MacGroup {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[cfg(target_os = "macos")]
struct MacChild {
    child: std::process::Child,
    group: MacGroup,
    armed: bool,
}
#[cfg(target_os = "macos")]
impl MacChild {
    fn cleanup(&mut self) -> Result<(), RunError> {
        let result = nix::sys::signal::killpg(self.group.pid()?, nix::sys::signal::Signal::SIGKILL);
        let _ = self.child.kill();
        let reaped = self.child.wait();
        self.armed = false;
        // An owned live anchor makes ESRCH/EPERM unexpected. Neither may be
        // waived as successful cleanup on Darwin.
        result.map_err(|e| error("tool-cleanup", e))?;
        reaped.map_err(io)?;
        Ok(())
    }
}
#[cfg(target_os = "macos")]
impl Drop for MacChild {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.cleanup();
        }
    }
}
#[cfg(target_os = "macos")]
fn start_error(e: std::io::Error) -> RunError {
    if e.kind() == std::io::ErrorKind::TimedOut {
        error("timeout", "target operation deadline ended during startup")
    } else {
        error("tool-start", e)
    }
}
#[cfg(target_os = "macos")]
fn execute_macos(
    path: &Path,
    args: &[OsString],
    input: &[u8],
    cwd: &Path,
    timeout: Duration,
    max_output: u64,
) -> Result<Vec<u8>, RunError> {
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
    // Include helper and target startup/retries in the operation deadline.
    let started = Instant::now();
    let group = MacGroup::new(cwd, started, timeout)?;
    let mut command = Command::new(path);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::from(File::open(&stdin).map_err(io)?))
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .process_group(group.pid()?.as_raw());
    for (key, _) in std::env::vars_os() {
        if scrubbed(&key) {
            command.env_remove(&key);
        }
    }
    command.env("LC_ALL", "C");
    let child = spawn_unless_busy(&mut command, started, timeout).map_err(start_error)?;
    let mut child = MacChild {
        child,
        group,
        armed: true,
    };
    let terminal = loop {
        let too_large = fs::metadata(&stdout)
            .map(|m| m.len() > max_output)
            .unwrap_or(true)
            || fs::metadata(&stderr)
                .map(|m| m.len() > max_output)
                .unwrap_or(true);
        if too_large {
            break Err(error("output-limit", max_output));
        }
        if started.elapsed() > timeout {
            break Err(error("timeout", "target operation deadline ended"));
        }
        match child.child.try_wait() {
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(error("tool-wait", e)),
        }
    };
    // Retain the group leader through signalling, even if try_wait reaped the
    // target. Final reads are bounded again after killing ordinary descendants.
    child.cleanup()?;
    let status = terminal?;
    let output = read_output(&stdout, max_output)?;
    let diagnostics = read_output(&stderr, max_output)?;
    if !status.success() {
        return Err(error(
            "tool-exit",
            format!("{status:?}: {}", String::from_utf8_lossy(&diagnostics)),
        ));
    }
    Ok(output)
}

#[cfg(any(
    target_os = "macos",
    all(target_os = "linux", not(target_env = "uclibc"))
))]
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

#[cfg(all(test, target_os = "macos"))]
mod macos_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use nix::{
        errno::Errno,
        sys::{signal::kill, wait::waitpid},
        unistd::Pid,
    };
    use std::os::unix::{
        fs::{PermissionsExt, symlink},
        process::CommandExt,
    };

    fn temp() -> Temp {
        Temp::new().unwrap_or_else(|e| panic!("{}: {}", e.code, e.message))
    }
    fn run(script: &str, timeout: Duration, limit: u64, cwd: &Path) -> Result<Vec<u8>, RunError> {
        execute_macos(
            Path::new("/bin/sh"),
            &["-c".into(), script.into()],
            b"",
            cwd,
            timeout,
            limit,
        )
    }
    fn code(result: Result<Vec<u8>, RunError>) -> &'static str {
        match result {
            Err(e) => e.code,
            Ok(_) => panic!("unexpected success"),
        }
    }
    fn gone(pid: Pid) {
        for _ in 0..200 {
            if kill(pid, None) == Err(Errno::ESRCH) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("owned descendant survived cleanup: {pid}");
    }

    #[test]
    fn macos_deadlines_output_exit_and_failed_setup_refuse() {
        let temp = temp();
        assert_eq!(
            code(run("sleep 30", Duration::from_millis(30), 1024, &temp.0)),
            "timeout"
        );
        assert_eq!(
            code(run("printf 123456789", TIMEOUT, 8, &temp.0)),
            "output-limit"
        );
        assert_eq!(
            code(run("printf 123456789 >&2", TIMEOUT, 8, &temp.0)),
            "output-limit"
        );
        assert_eq!(
            code(run("exit 23", TIMEOUT, MAX_OUTPUT, &temp.0)),
            "tool-exit"
        );
        assert_eq!(
            code(run(
                "echo should-not-start",
                Duration::ZERO,
                MAX_OUTPUT,
                &temp.0
            )),
            "timeout"
        );
        assert_eq!(
            code(execute_macos(
                Path::new("/zeno-fcis-missing-tool"),
                &[],
                b"",
                &temp.0,
                TIMEOUT,
                MAX_OUTPUT
            )),
            "tool-start"
        );
        assert!(MacGroup::new(&temp.0.join("missing-directory"), Instant::now(), TIMEOUT).is_err());
        let path = temp.0.join("output");
        fs::write(&path, "123456789").unwrap();
        assert_eq!(code(read_output(&path, 8)), "output-limit");
    }

    #[test]
    fn macos_exited_leader_cannot_leave_an_ordinary_descendant_running() {
        let temp = temp();
        let bytes = run("sleep 30 & echo $!; exit 0", TIMEOUT, MAX_OUTPUT, &temp.0)
            .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
        let pid = String::from_utf8(bytes)
            .unwrap()
            .trim()
            .parse::<i32>()
            .unwrap();
        gone(Pid::from_raw(pid));
        assert_eq!(
            run("printf 12345678", TIMEOUT, 8, &temp.0).ok().unwrap(),
            b"12345678"
        );
    }

    #[test]
    fn macos_group_cleanup_survives_target_reaping_and_cancellation() {
        let temp = temp();
        for cancel in [false, true] {
            let group = MacGroup::new(&temp.0, Instant::now(), TIMEOUT)
                .unwrap_or_else(|e| panic!("{}", e.message));
            let group_pid = group.pid().unwrap_or_else(|_| unreachable!());
            let child = Command::new(if cancel { "/bin/cat" } else { "/usr/bin/true" })
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(group_pid.as_raw())
                .spawn()
                .unwrap();
            let pid = Pid::from_raw(i32::try_from(child.id()).unwrap());
            let mut guard = MacChild {
                child,
                group,
                armed: true,
            };
            if !cancel {
                assert!(guard.child.wait().unwrap().success());
                guard
                    .cleanup()
                    .unwrap_or_else(|e| panic!("{}: {}", e.code, e.message));
            }
            drop(guard);
            assert_eq!(waitpid(pid, None), Err(Errno::ECHILD));
            assert_eq!(waitpid(group_pid, None), Err(Errno::ECHILD));
        }
    }

    #[test]
    fn macos_missing_owned_group_is_a_cleanup_failure() {
        let temp = temp();
        let group = MacGroup::new(&temp.0, Instant::now(), TIMEOUT)
            .unwrap_or_else(|e| panic!("{}", e.message));
        let child = Command::new("/usr/bin/true")
            .process_group(group.pid().unwrap_or_else(|_| unreachable!()).as_raw())
            .spawn()
            .unwrap();
        let mut guard = MacChild {
            child,
            group,
            armed: true,
        };
        guard.child.wait().unwrap();
        guard.group.0.kill().unwrap();
        guard.group.0.wait().unwrap();
        let Err(failure) = guard.cleanup() else {
            panic!("missing group was accepted");
        };
        assert_eq!(failure.code, "tool-cleanup");
    }

    #[test]
    fn macos_private_sources_environment_and_tool_drift_remain_checked() {
        let temp = temp();
        assert_eq!(
            fs::metadata(&temp.0).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let original = temp.0.join("original");
        write_source(&original, "keep").unwrap_or_else(|_| unreachable!());
        assert!(write_source(&original, "replace").is_err());
        symlink(&original, temp.0.join("source")).unwrap();
        assert!(write_source(&temp.0.join("source"), "replace").is_err());
        assert_eq!(fs::read(&original).unwrap(), b"keep");
        for key in [
            "NODE_OPTIONS",
            "NODE_PATH",
            "NODE_COMPILE_CACHE",
            "RUSTC_WRAPPER",
            "PYTHONPATH",
            "PYTHONHOME",
        ] {
            assert!(scrubbed(OsStr::new(key)));
        }
        for key in ["PATH", "HOME", "RUSTUP_HOME"] {
            assert!(!scrubbed(OsStr::new(key)));
        }
        let identity =
            json!({"binary_sha256":binary_hash(&original).unwrap_or_else(|_| unreachable!())});
        fs::write(&original, "changed").unwrap();
        let Err(failure) = recheck(&original, &identity) else {
            panic!("tool drift was accepted");
        };
        assert_eq!(failure.code, "tool-drift");
    }
}

#[cfg(all(test, target_os = "linux", not(target_env = "uclibc")))]
mod process_tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    #[test]
    fn inherited_node_switches_and_toolchain_overrides_cannot_reach_a_child() {
        // A NODE_OPTIONS preload would otherwise change both `node --version`
        // identification and fixture execution; RUSTUP_HOME must survive,
        // because Rust tool discovery still has to find the pinned toolchain.
        for hostile in [
            "NODE_OPTIONS",
            "NODE_PATH",
            "NODE_V8_COVERAGE",
            "NODE_COMPILE_CACHE",
            "NODE_ICU_DATA",
            "RUSTC_BOOTSTRAP",
            "PYTHONPATH",
        ] {
            assert!(scrubbed(OsStr::new(hostile)), "{hostile} must be removed");
        }
        for inherited in [
            "PATH",
            "HOME",
            "RUSTUP_HOME",
            "NODEJS_OPTIONS",
            "node_options",
        ] {
            assert!(!scrubbed(OsStr::new(inherited)), "{inherited} was removed");
        }
    }
    #[test]
    fn source_creation_preserves_existing_files_and_symlink_targets() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let temp = Temp::new().unwrap_or_else(|e| panic!("{}", e.message));
        assert_eq!(
            fs::metadata(&temp.0).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let original = temp.0.join("original");
        write_source(&original, "keep").unwrap_or_else(|e| panic!("{}", e.message));
        assert!(write_source(&original, "replace").is_err());
        assert_eq!(fs::read(&original).unwrap(), b"keep");
        let link = temp.0.join("transition.rs");
        symlink(&original, &link).unwrap();
        assert!(write_source(&link, "replace").is_err());
        assert_eq!(fs::read(&original).unwrap(), b"keep");
    }

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
