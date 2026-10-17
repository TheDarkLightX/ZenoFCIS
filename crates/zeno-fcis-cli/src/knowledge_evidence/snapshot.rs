//! Invocation-owned filesystem preparation, never decision or publication authority.
use super::{Case, Input, bind_checked_subjects, bound_input, error, sha256_hex};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_BYTES: u64 = 64 * 1024 * 1024;
// Count all files and directories, including parents made for declared inputs.
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 128;
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Shell preparation with exclusive ownership. No mutable aliases escape;
/// scratch is removed on success and every refusal, after the child is reaped.
pub(super) struct Snapshot {
    root: PathBuf,
    entries: BTreeMap<String, Option<String>>,
    bytes: u64,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn relative(path: &Path) -> Result<PathBuf, Value> {
    let mut result = PathBuf::new();
    if path.as_os_str().is_empty() || path.components().count() > MAX_DEPTH {
        return Err(error("kb-evidence-snapshot-path"));
    }
    for part in path.components() {
        match part {
            Component::Normal(name) => result.push(name),
            Component::CurDir => (),
            _ => return Err(error("kb-evidence-snapshot-path")),
        }
    }
    Ok(result)
}

impl Snapshot {
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn manifest(&self) -> &BTreeMap<String, Option<String>> {
        &self.entries
    }

    #[cfg(unix)]
    pub(super) fn new(root: &Path, case: &Case, dependencies: &[Input]) -> Result<Self, Value> {
        use nix::fcntl::{OFlag, open};
        use nix::sys::stat::Mode;
        use std::os::unix::fs::DirBuilderExt;
        let mut directory = fs::DirBuilder::new();
        directory.mode(0o700);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| error("kb-evidence-clock"))?
            .as_nanos();
        let mut created = None;
        for _ in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "zeno-fcis-kb-{}-{timestamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match directory.create(&path) {
                Ok(()) => {
                    created = Some(path);
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(_) => return Err(error("kb-evidence-snapshot-create")),
            }
        }
        let mut snapshot = Self {
            root: created.ok_or_else(|| error("kb-evidence-snapshot-create"))?,
            entries: BTreeMap::new(),
            bytes: 0,
        };
        let source = File::from(
            open(
                root,
                OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| error("kb-evidence-snapshot-source"))?,
        );
        for input in dependencies.iter().chain(&case.inputs) {
            snapshot.copy_path(&source, &input.path, false)?;
        }
        if case.arguments[0] == "transform" {
            for i in [3, 5] {
                snapshot.copy_path(&source, Path::new(&case.arguments[i]), false)?;
            }
        } else {
            snapshot.copy_path(&source, Path::new(&case.arguments[2]), true)?;
        }
        // Verify the copies only after the complete subject has been acquired.
        // The child will read these owned bytes, rather than reopening the source.
        for input in dependencies.iter().chain(&case.inputs) {
            bound_input(&snapshot.root, input, 1_048_576)?;
        }
        bind_checked_subjects(&snapshot.root, case)?;
        Ok(snapshot)
    }

    #[cfg(not(unix))]
    pub(super) fn new(_: &Path, _: &Case, _: &[Input]) -> Result<Self, Value> {
        // Do not silently substitute a path-check/open race on other platforms.
        Err(error("kb-evidence-snapshot-platform"))
    }

    #[cfg(unix)]
    fn copy_path(&mut self, root: &File, path: &Path, directory: bool) -> Result<(), Value> {
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;
        let path = relative(path)?;
        let mut source = root
            .try_clone()
            .map_err(|_| error("kb-evidence-snapshot-read"))?;
        let parts = path.components().collect::<Vec<_>>();
        for (i, part) in parts.iter().enumerate() {
            let mut flags =
                OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK;
            if i + 1 < parts.len() {
                flags |= OFlag::O_DIRECTORY;
            }
            source = File::from(
                openat(&source, part.as_os_str(), flags, Mode::empty())
                    .map_err(|_| error("kb-evidence-snapshot-source"))?,
            );
        }
        if !directory && !source.metadata().is_ok_and(|m| m.is_file()) {
            return Err(error("kb-evidence-snapshot-type"));
        }
        if directory && !source.metadata().is_ok_and(|m| m.is_dir()) {
            return Err(error("kb-evidence-snapshot-type"));
        }
        self.copy_entry(source, &path)
    }

    #[cfg(unix)]
    fn create_directories(&mut self, path: &Path) -> Result<(), Value> {
        let mut parent = PathBuf::new();
        for part in path.components() {
            parent.push(part);
            let key = parent
                .to_str()
                .ok_or_else(|| error("kb-evidence-snapshot-path"))?;
            if !self.entries.contains_key(key) {
                if self.entries.len() >= MAX_ENTRIES {
                    return Err(error("kb-evidence-snapshot-bound"));
                }
                fs::create_dir(self.root.join(&parent))
                    .map_err(|_| error("kb-evidence-snapshot-write"))?;
                self.entries.insert(key.to_owned(), None);
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    fn copy_entry(&mut self, source: File, path: &Path) -> Result<(), Value> {
        use nix::dir::Dir;
        use nix::fcntl::{OFlag, openat};
        use nix::sys::stat::Mode;
        use std::os::unix::ffi::OsStrExt;
        let key = path
            .to_str()
            .ok_or_else(|| error("kb-evidence-snapshot-path"))?
            .to_owned();
        let metadata = source
            .metadata()
            .map_err(|_| error("kb-evidence-snapshot-read"))?;
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(error("kb-evidence-snapshot-type"));
        }
        if metadata.is_file() && self.entries.contains_key(&key) {
            return Ok(());
        }
        if path.components().count() > MAX_DEPTH {
            return Err(error("kb-evidence-snapshot-bound"));
        }
        let target = self.root.join(path);
        if metadata.is_dir() {
            // A parent made for a declared file is counted, but its subtree
            // has not been copied. Still traverse it with the pinned source fd.
            self.create_directories(path)?;
            let mut directory =
                Dir::from_fd(source.into()).map_err(|_| error("kb-evidence-snapshot-read"))?;
            // Clone the descriptor before iterating; each open is relative to
            // this pinned directory, even if its source name is replaced.
            let parent = File::from(
                nix::unistd::dup(&directory).map_err(|_| error("kb-evidence-snapshot-read"))?,
            );
            for entry in directory.iter() {
                let entry = entry.map_err(|_| error("kb-evidence-snapshot-read"))?;
                let name = entry.file_name().to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                let name = std::ffi::OsStr::from_bytes(name);
                let child = openat(
                    &parent,
                    name,
                    OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|_| error("kb-evidence-snapshot-source"))?;
                self.copy_entry(File::from(child), &path.join(name))?;
            }
        } else {
            let remaining = MAX_BYTES - self.bytes;
            if metadata.len() > remaining {
                return Err(error("kb-evidence-snapshot-bound"));
            }
            let mut bytes = Vec::new();
            source
                .take(remaining + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| error("kb-evidence-snapshot-read"))?;
            if bytes.len() as u64 > remaining {
                return Err(error("kb-evidence-snapshot-bound"));
            }
            if let Some(parent) = path.parent() {
                self.create_directories(parent)?;
            }
            if self.entries.len() >= MAX_ENTRIES {
                return Err(error("kb-evidence-snapshot-bound"));
            }
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|_| error("kb-evidence-snapshot-write"))?;
            output
                .write_all(&bytes)
                .map_err(|_| error("kb-evidence-snapshot-write"))?;
            self.bytes += bytes.len() as u64;
            // Digest the installed copy, never a live source pathname.
            let staged = super::read(&target, remaining)?;
            self.entries.insert(key, Some(sha256_hex(&staged)));
        }
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn kb_snapshot_fifo_socket_and_device_descriptors_refuse_without_reading() {
        // mknod/mkfifo is prohibited in some development sandboxes. Owned
        // descriptors exercise the same fstat classification without mknod.
        let root = std::env::temp_dir().join(format!(
            "fcis-kb-types-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|e| panic!("{e}"));
        let mut snapshot = Snapshot {
            root,
            entries: BTreeMap::new(),
            bytes: 0,
        };
        let (reader, _writer) = nix::unistd::pipe().unwrap_or_else(|e| panic!("{e}"));
        let (socket, _peer) =
            std::os::unix::net::UnixStream::pair().unwrap_or_else(|e| panic!("{e}"));
        let files = [
            File::from(reader),
            File::from(std::os::fd::OwnedFd::from(socket)),
            File::open("/dev/null").unwrap_or_else(|e| panic!("{e}")),
        ];
        for file in files {
            let result = snapshot.copy_entry(file, Path::new("special"));
            assert_eq!(
                result.err().map(|v| v["reason"].clone()),
                Some(serde_json::json!("kb-evidence-snapshot-type"))
            );
        }
    }
}
