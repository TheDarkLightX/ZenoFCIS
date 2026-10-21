//! Shell-only artifact inventory and bounded subprocess capture.

use super::common::*;
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub(super) const NATIVE_BINARY_LIMIT: usize = 1 << 28;
pub(super) fn binary_sha(path: &Path) -> Result<String> {
    Ok(sha(&read(path, NATIVE_BINARY_LIMIT)?))
}
pub(super) fn phase_report(path: &Path, phase: &str) -> Result<Value> {
    let value = strict(&read(path, LOG_LIMIT)?)?;
    closed(
        &value,
        &[
            "schema",
            "revision",
            "phase",
            "status",
            "stop_reason",
            "inputs",
            "identities",
            "counts",
            "detail",
            "elapsed_ms",
            "evidence_paths",
        ],
    )?;
    if value["schema"] != "zeno-fcis/priority-construction-run/1"
        || value["revision"] != REVISION
        || value["phase"] != phase
    {
        return fail(65, "phase-report-schema");
    }
    closed(
        &value["identities"],
        &[
            "source_manifest_sha256",
            "driver_sha256",
            "cargo_lock_sha256",
            "rust_toolchain_sha256",
            "native_cli_sha256",
            "checker_identity_sha256",
            "evaluator_identity_sha256",
        ],
    )?;
    closed(
        &value["counts"],
        &[
            "authority_evaluations",
            "checked_rows",
            "proposal_work",
            "transform_evaluations",
            "transform_accounting_complete",
            "scalar_evaluations",
            "scalar_steps",
            "authority_usage",
        ],
    )?;
    for key in [
        "authority_evaluations",
        "checked_rows",
        "proposal_work",
        "transform_evaluations",
        "scalar_evaluations",
    ] {
        if value["counts"][key].as_u64().is_none() {
            return fail(65, "phase-count-kind");
        }
    }
    if value["counts"]["transform_accounting_complete"]
        .as_bool()
        .is_none()
    {
        return fail(65, "phase-accounting-kind");
    }
    value["counts"]["scalar_steps"]
        .as_str()
        .ok_or_else(|| malformed("scalar-step-kind"))?
        .parse::<u128>()
        .map_err(|e| malformed(e.to_string()))?;
    let usage = value["counts"]["authority_usage"]
        .as_array()
        .filter(|v| v.len() == 8)
        .ok_or_else(|| malformed("phase-usage-count"))?;
    for n in usage {
        n.as_str()
            .ok_or_else(|| malformed("phase-usage-kind"))?
            .parse::<u128>()
            .map_err(|e| malformed(e.to_string()))?;
    }
    Ok(value)
}
pub(super) fn repository() -> Result<PathBuf> {
    super::build_tree::BUILD_TREE
        .map(PathBuf::from)
        .ok_or_else(|| malformed("construction-build-tree-unrecorded"))
}

fn visit(root: &Path, path: &Path, rows: &mut Vec<[String; 2]>) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return fail(65, "artifact-symlink");
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            visit(root, &entry?.path(), rows)?;
        }
    } else if metadata.is_file() {
        let name = path
            .strip_prefix(root)
            .map_err(|e| malformed(e.to_string()))?
            .to_str()
            .ok_or_else(|| malformed("artifact-path-utf8"))?
            .replace('\\', "/");
        if name != "manifest.json" {
            rows.push([name, sha(&read(path, LOG_LIMIT)?)]);
        }
    } else {
        return fail(65, "artifact-not-regular-file");
    }
    Ok(())
}
pub(super) fn manifest(root: &Path) -> Result<Value> {
    let mut rows = Vec::new();
    visit(root, root, &mut rows)?;
    rows.sort();
    Ok(json!({"schema":"zeno-fcis/priority-artifact-manifest/1","revision":REVISION,"files":rows}))
}
pub(super) fn seal(root: &Path) -> Result<Value> {
    let value = manifest(root)?;
    write_json(&root.join("manifest.json"), &value)?;
    Ok(value)
}
pub(super) fn verify(root: &Path) -> Result<Value> {
    let bytes = read(&root.join("manifest.json"), LOG_LIMIT)?;
    let value = strict(&bytes)?;
    closed(&value, &["schema", "revision", "files"])?;
    if value != manifest(root)? || canonical(&value)? != bytes {
        return fail(65, "artifact-manifest-mismatch");
    }
    Ok(value)
}
pub(super) fn reference(root: &Path, path: &Path) -> Result<Value> {
    let name = path
        .strip_prefix(root)
        .map_err(|e| malformed(e.to_string()))?
        .to_str()
        .ok_or_else(|| malformed("reference-path-utf8"))?;
    relative(name)?;
    Ok(json!({"path":name,"sha256":sha(&read(path,LOG_LIMIT)?)}))
}

pub(super) struct Process {
    pub(super) exit: Option<i32>,
    pub(super) truncated: bool,
    pub(super) timed_out: bool,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    pub(super) millis: u128,
}
impl Process {
    pub(super) fn save(&self, dir: &Path, name: &str) -> Result<Value> {
        write(&dir.join(format!("{name}.stdout")), &self.stdout)?;
        write(&dir.join(format!("{name}.stderr")), &self.stderr)?;
        write_json(
            &dir.join(format!("{name}.exit.json")),
            &json!({"exit":self.exit,"truncated":self.truncated,"timed_out":self.timed_out,"elapsed_ms":self.millis.to_string()}),
        )?;
        Ok(
            json!({"exit":self.exit,"truncated":self.truncated,"timed_out":self.timed_out,
            "stdout_sha256":sha(&self.stdout),"stderr_sha256":sha(&self.stderr),"elapsed_ms":self.millis.to_string()}),
        )
    }
    pub(super) fn complete(&self) -> Result<()> {
        if self.truncated || self.timed_out || self.exit.is_none() {
            return fail(3, "subprocess-incomplete");
        }
        Ok(())
    }
}
fn drain(
    mut reader: impl Read,
    limit: usize,
    overflow: mpsc::Sender<()>,
) -> std::io::Result<Vec<u8>> {
    let mut prefix = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut signaled = false;
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let retained = n.min(limit.saturating_sub(prefix.len()));
        prefix.extend_from_slice(&chunk[..retained]);
        if retained < n && !signaled {
            let _ = overflow.send(());
            signaled = true;
        }
    }
    Ok(prefix)
}
/// Production children are the pinned CLI's nonspawning transform commands
/// (and read-only git inventory). Kill/reap only the owned direct child before
/// joining its streams. This helper is not a sandbox for descendant processes.
pub(super) fn process(program: &Path, args: &[String], cwd: &Path) -> Result<Process> {
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| malformed("child-stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| malformed("child-stderr"))?;
    let (tx, rx) = mpsc::channel();
    let out_tx = tx.clone();
    let out = std::thread::spawn(move || drain(stdout, LOG_LIMIT, out_tx));
    let err = std::thread::spawn(move || drain(stderr, LOG_LIMIT, tx));
    let mut truncated = false;
    let mut timed_out = false;
    let status = loop {
        if rx.try_recv().is_ok() {
            truncated = true;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => break Err(error),
        }
        timed_out = started.elapsed() > Duration::from_secs(120);
        if truncated || timed_out {
            break child.kill().and_then(|()| child.wait());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let stdout = out.join().map_err(|_| gate("stdout-reader-panicked"))??;
    let stderr = err.join().map_err(|_| gate("stderr-reader-panicked"))??;
    // A final signal can arrive between the last poll and EOF.
    truncated |= rx.try_recv().is_ok();
    Ok(Process {
        exit: status?.code(),
        truncated,
        timed_out,
        stdout,
        stderr,
        millis: started.elapsed().as_millis(),
    })
}

pub(super) fn source_inventory() -> Result<Vec<u8>> {
    let root = repository()?;
    let args = [
        "ls-files",
        "-z",
        "--cached",
        "--others",
        "--exclude-standard",
        "--",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "build.rs",
        ".cargo",
        "crates",
        "tools",
        "acceptance",
        "docs",
        "examples",
        "verification",
    ]
    .map(str::to_string);
    let result = process(Path::new("git"), &args, &root)?;
    result.complete()?;
    if result.exit != Some(0) {
        return fail(3, "source-inventory-git-failed");
    }
    let mut names: Vec<_> = result
        .stdout
        .split(|c| *c == 0)
        .filter(|v| !v.is_empty())
        .collect();
    names.sort();
    names.dedup();
    let mut inventory = Vec::new();
    for name in names {
        let name = std::str::from_utf8(name).map_err(|e| malformed(e.to_string()))?;
        relative(name)?;
        inventory.extend_from_slice(sha(&read(&root.join(name), LOG_LIMIT)?).as_bytes());
        inventory.extend_from_slice(b"  ");
        inventory.extend_from_slice(name.as_bytes());
        inventory.push(b'\n');
    }
    Ok(inventory)
}
