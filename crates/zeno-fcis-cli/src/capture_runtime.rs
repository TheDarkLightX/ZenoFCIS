//! Bounded interpreter adapter inside checker_process's contained unit.
//! LLM-generated. Kernel confinement precedes owner code; no candidate judge.

use serde_json::{Value, json};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(super) const PROTOCOL_LIMIT: usize = 2 << 20;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const WORKER: &str = include_str!("capture_worker.py");
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const CONTAINED_MARKER: &[u8] = b"zeno-capture-contained-v1\n";

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
struct Scratch(PathBuf);
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Scratch {
    fn new() -> Result<Self, String> {
        let dir = std::env::temp_dir().join(format!(
            "zeno-capture-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|_| "capture-scratch-unavailable")?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&dir).map_err(|_| "capture-scratch-unavailable")?;
        let scratch = Self(dir);
        crate::atomic_create(&scratch.0.join("scratch.txt"), b"")
            .map_err(|_| "capture-scratch-unavailable")?;
        Ok(scratch)
    }

    fn cleanup(&mut self) -> Result<(), String> {
        std::fs::remove_file(self.0.join("scratch.txt")).map_err(|_| "capture-scratch-cleanup")?;
        std::fs::remove_dir(&self.0).map_err(|_| "capture-scratch-cleanup")?;
        self.0 = PathBuf::new();
        Ok(())
    }
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
impl Drop for Scratch {
    fn drop(&mut self) {
        // Scratch is exclusively created here. Submitted code cannot create
        // entries or mutate directory metadata after confinement.
        if !self.0.as_os_str().is_empty() {
            let _ = std::fs::remove_file(self.0.join("scratch.txt"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }
}

/// Fresh process per observation schedule; its only input is source and points.
pub(super) fn invoke(source: &str, points: &[Vec<i64>]) -> Result<Value, String> {
    if !crate::checker_process::contained() {
        return Err("capture-containment-unavailable".into());
    }
    if source.len() > 65536 {
        return Err("capture-source-bound".into());
    }
    let request = serde_json::to_vec(&json!({"source":source,"points":points}))
        .map_err(|_| "capture-request-json")?;
    if request.len() > PROTOCOL_LIMIT {
        return Err("capture-request-bound".into());
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let mut scratch = Scratch::new()?;
        let result = invoke_linux(&scratch.0, &request);
        scratch.cleanup()?;
        result
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = request;
        Err("capture-containment-unavailable:platform".into())
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn invoke_linux(scratch: &Path, request: &[u8]) -> Result<Value, String> {
    use std::{
        net::Shutdown,
        os::{fd::OwnedFd, unix::net::UnixStream},
        process::{Command, Stdio},
    };
    let pair = || -> Result<(UnixStream, Stdio), String> {
        let (a, b) = UnixStream::pair().map_err(|_| "capture-pipe")?;
        a.set_nonblocking(true).map_err(|_| "capture-pipe")?;
        let descriptor: OwnedFd = b.into();
        Ok((a, Stdio::from(descriptor)))
    };
    let (mut input, stdin) = pair()?;
    let (mut output, stdout) = pair()?;
    let (mut errors, stderr) = pair()?;
    // Absolute interpreter and empty environment: no PATH, PYTHONPATH, caller
    // secrets, reference path, session path or inherited judge handles.
    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-S", "-B", "-c", WORKER])
        .env_clear()
        .env("LC_ALL", "C")
        .current_dir(scratch)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);
    let mut child = command
        .spawn()
        .map_err(|_| "capture-interpreter-unavailable")?;
    drop(command); // close this parent's copies of every child socket endpoint
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::new();
    let mut marker = Vec::new();
    let mut used = 0usize;
    let mut sent = 0usize;
    let mut input_closed = false;
    let mut output_closed = false;
    let mut errors_closed = false;
    let result = (|| {
        loop {
            if Instant::now() >= deadline {
                return Err("capture-worker-timeout".into());
            }
            if !input_closed {
                match input.write(&request[sent..request.len().min(sent + 8192)]) {
                    Ok(0) => return Err("capture-worker-input".into()),
                    Ok(n) => sent += n,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        input_closed = true;
                    }
                }
                if sent == request.len() {
                    input
                        .shutdown(Shutdown::Write)
                        .map_err(|_| "capture-worker-input")?;
                    input_closed = true;
                }
            }
            for (stream, closed, retain) in [
                (&mut output, &mut output_closed, true),
                (&mut errors, &mut errors_closed, false),
            ] {
                let mut chunk = [0; 8192];
                // One read per pipe per iteration: flood output cannot prevent
                // checking the deadline or draining the other protocol pipe.
                if !*closed {
                    match stream.read(&mut chunk) {
                        Ok(0) => *closed = true,
                        Ok(n) => {
                            used += n;
                            if used > PROTOCOL_LIMIT {
                                return Err("capture-worker-output-bound".into());
                            }
                            if retain {
                                bytes.extend_from_slice(&chunk[..n]);
                            } else {
                                marker.extend_from_slice(&chunk[..n]);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => return Err("capture-worker-output".into()),
                    }
                }
            }
            if let Some(status) = child.try_wait().map_err(|_| "capture-worker-wait")?
                && output_closed
                && errors_closed
            {
                return worker_reply(status.code(), &marker, &bytes);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    })();
    // The outer checker_process supervisor also kills the complete cgroup on
    // interruption. This local deadline reaps a worker before replying.
    if result.is_err() {
        let _ = child.kill();
    }
    child.wait().map_err(|_| "capture-worker-cleanup")?;
    result
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn worker_reply(exit: Option<i32>, marker: &[u8], bytes: &[u8]) -> Result<Value, String> {
    if marker != CONTAINED_MARKER {
        return Err("capture-containment-unavailable:bootstrap".into());
    }
    if exit != Some(0) {
        return Err("capture-worker-failed".into());
    }
    super::strict_json(bytes).map_err(|_| "capture-worker-malformed-output".into())
}

#[cfg(all(test, target_os = "linux", target_arch = "x86_64"))]
mod tests {
    use super::*;

    #[test]
    fn capture_worker_exit_code_is_not_a_containment_marker() {
        assert_eq!(
            worker_reply(Some(78), CONTAINED_MARKER, b"")
                .err()
                .as_deref(),
            Some("capture-worker-failed")
        );
        for exit in [Some(0), Some(1), Some(78), None] {
            assert_eq!(
                worker_reply(exit, b"", b"{}").err().as_deref(),
                Some("capture-containment-unavailable:bootstrap")
            );
        }
        assert!(worker_reply(Some(0), CONTAINED_MARKER, b"{\"rows\":[[0]]}").is_ok());
        assert_eq!(
            worker_reply(Some(0), CONTAINED_MARKER, b"bad")
                .err()
                .as_deref(),
            Some("capture-worker-malformed-output")
        );
    }
}
