//! Reconcile only private app scratch; admit no new external package identity.
use super::{FILE_LIMIT, Parser, Path, PathBuf, Value, read_bounded};
use std::collections::BTreeSet;
use std::fs;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

type External = (String, String, String, Option<String>);

fn external_packages(bytes: &[u8]) -> Result<BTreeSet<External>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut packages = BTreeSet::new();
    for (table, entries) in Parser::new(text).document()? {
        if table.as_slice() != ["package"] {
            continue;
        }
        let mut fields = std::collections::BTreeMap::new();
        for (key, value) in entries {
            if key.len() != 1 || fields.insert(key[0].clone(), value).is_some() {
                return Err("repeated or unsupported locked package key".to_owned());
            }
        }
        let text = |key| match fields.get(key) {
            Some(Value::Text(value)) => Ok(Some(value.clone())),
            None => Ok(None),
            _ => Err(format!("locked {key} is not text")),
        };
        if let Some(source) = text("source")? {
            let identity = (
                text("name")?.ok_or("locked package omits name")?,
                text("version")?.ok_or("locked package omits version")?,
                source,
                text("checksum")?,
            );
            if !packages.insert(identity) {
                return Err("duplicate external locked package".to_owned());
            }
        }
    }
    Ok(packages)
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "zeno-app-lock-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut directory = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            directory.mode(0o700);
        }
        directory
            .create(&path)
            .map_err(|error| format!("create private lock workspace: {error}"))?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        #[cfg(unix)]
        if let Ok(id) = i32::try_from(self.0.id()) {
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            let _ = killpg(Pid::from_raw(id), Signal::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(super) fn for_application(bytes: &[u8], manifest: &str) -> Result<Vec<u8>, String> {
    if manifest.len() as u64 > FILE_LIMIT {
        return Err("application manifest is too large".to_owned());
    }
    let admitted = external_packages(bytes)?;
    let scratch = Scratch::new()?;
    let write = |path: &Path, bytes| fs::write(path, bytes).map_err(|error| error.to_string());
    write(&scratch.0.join("Cargo.toml"), manifest.as_bytes())?;
    write(&scratch.0.join("Cargo.lock"), bytes)?;
    fs::create_dir(scratch.0.join("src")).map_err(|error| error.to_string())?;
    // Metadata reads target declarations, but never compiles or runs these files.
    write(&scratch.0.join("src/lib.rs"), b"")?;
    write(&scratch.0.join("src/main.rs"), b"fn main() {}\n")?;
    let stderr_path = scratch.0.join("cargo.stderr");
    let stderr = fs::File::create(&stderr_path).map_err(|error| error.to_string())?;
    let mut command = Command::new("cargo");
    command
        .args(["+1.97.1", "metadata", "--offline", "--format-version", "1"])
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut child = Process(
        command
            .spawn()
            .map_err(|error| format!("start pinned offline Cargo: {error}"))?,
    );
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if start.elapsed() >= Duration::from_secs(60) {
            return Err("offline Cargo lock reconciliation timed out".to_owned());
        }
        if fs::metadata(&stderr_path)
            .map_err(|error| error.to_string())?
            .len()
            > FILE_LIMIT
        {
            return Err("offline Cargo diagnostics exceed 4 MiB".to_owned());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        let diagnostic = read_bounded(&stderr_path).map_err(|error| error.to_string())?;
        return Err(format!(
            "offline Cargo lock reconciliation refused: {}",
            String::from_utf8_lossy(&diagnostic)
        ));
    }
    let locked = read_bounded(&scratch.0.join("Cargo.lock")).map_err(|error| error.to_string())?;
    if !external_packages(&locked)?.is_subset(&admitted) {
        return Err(
            "reconciled lock selects an external package identity outside the source lock"
                .to_owned(),
        );
    }
    Ok(locked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_identity_includes_version_source_and_checksum() {
        let source = b"version = 4\n[[package]]\nname = \"library\"\nversion = \"1\"\nsource = \"registry+https://example.invalid\"\nchecksum = \"a\"\n";
        let original = external_packages(source).unwrap_or_else(|error| panic!("{error}"));
        for (before, after) in [
            ("version = \"1\"", "version = \"2\""),
            ("checksum = \"a\"", "checksum = \"b\""),
            ("example.invalid", "other.invalid"),
        ] {
            let changed = std::str::from_utf8(source)
                .unwrap_or_else(|error| panic!("{error}"))
                .replace(before, after);
            assert!(
                !external_packages(changed.as_bytes())
                    .unwrap_or_else(|error| panic!("{error}"))
                    .is_subset(&original)
            );
        }
    }
}
