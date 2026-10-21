//! One locked, atomic file replacement for state + legs/outbox + replay +
//! exposure + receipt. Authentication is conditional on ReceiptVerifierV1.
//! This is an internal E18 ledger, not an external token delivery adapter.
use super::encoding::{self, R, W};
use super::*;
use crate::{Failure, Limits};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrashPoint {
    BeforeWrite,
    DuringWrite,
    AfterWrite,
    AfterFileSync,
    AfterRename,
    AfterDirectorySync,
}
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Refused(Failure),
    Conflict,
    Corrupt,
    Crash(CrashPoint),
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<Failure> for Error {
    fn from(e: Failure) -> Self {
        Self::Refused(e)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Publication {
    Committed,
    Duplicate,
}
/// A store location is mutable shell configuration, never decision authority.
/// Lock files are stable inodes and are never replaced with the ledger file.
#[derive(Clone, Debug)]
pub struct FileLedger {
    directory: PathBuf,
}
struct History {
    state: State,
    rows: Vec<Vec<u8>>,
}
impl FileLedger {
    pub fn create(path: impl AsRef<Path>, p: &Policy, genesis: &State) -> Result<Self, Error> {
        if genesis.version() != 0 || genesis.policy != p.id {
            return Err(Error::Corrupt);
        }
        fs::create_dir_all(path.as_ref())?;
        let store = Self {
            directory: path.as_ref().to_path_buf(),
        };
        let _guard = store.lock()?;
        if store.path().exists() {
            return Err(Error::Conflict);
        }
        let bytes = history(p, genesis, &[])?;
        store.replace(&bytes, None)?;
        Ok(store)
    }
    pub fn open(path: impl AsRef<Path>) -> Self {
        Self {
            directory: path.as_ref().to_path_buf(),
        }
    }
    fn path(&self) -> PathBuf {
        self.directory.join("ledger")
    }
    fn lock(&self) -> Result<File, Error> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("lock"))?;
        f.lock()?;
        Ok(f)
    }
    fn read(
        &self,
        p: &Policy,
        g: &State,
        v: &impl ReceiptVerifierV1,
        limits: Limits,
    ) -> Result<History, Error> {
        let max = p.config.bounds.history_bytes;
        let mut bytes = Vec::new();
        File::open(self.path())?
            .take(max.checked_add(1).ok_or(Error::Corrupt)?)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max || bytes.len() < 32 {
            return Err(Error::Corrupt);
        }
        let split = bytes.len() - 32;
        if encoding::hash(&bytes[..split]) != bytes[split..] {
            return Err(Error::Corrupt);
        }
        let cap = usize::try_from(max).map_err(|_| Error::Corrupt)?;
        let mut r = R::new(&bytes[..split], b"zusd.step4.ledger/1\0", cap)?;
        if r.id()? != p.id || r.id()? != g.root() || g.policy != p.id || g.version() != 0 {
            return Err(Error::Corrupt);
        }
        if r.blob()? != g.canonical_bytes() {
            return Err(Error::Corrupt);
        }
        let count = r.count()?;
        let mut state = g.clone();
        let mut rows = Vec::new();
        for _ in 0..count {
            let row = r.blob()?;
            let mut input = R::new(
                row,
                b"zusd.step4.commit/1\0",
                p.config.bounds.frame_bytes as usize,
            )?;
            if input.id()? != state.root() {
                return Err(Error::Corrupt);
            }
            let request =
                encoding::decode_request(input.blob()?, p.config.bounds.frame_bytes as usize)?;
            let checked = decide(p, &state, &request, v, limits).into_parts().0?;
            if checked.bytes != row {
                return Err(Error::Corrupt);
            }
            state = checked.post;
            rows.push(row.to_vec());
        }
        r.end()?;
        Ok(History { state, rows })
    }
    pub fn load(
        &self,
        p: &Policy,
        g: &State,
        v: &impl ReceiptVerifierV1,
        limits: Limits,
    ) -> Result<State, Error> {
        let _guard = self.lock()?;
        Ok(self.read(p, g, v, limits)?.state)
    }
    /// These are exact canonical commit bundles, including the ordered economic
    /// outbox. Reading or redelivering them cannot create another ledger commit.
    pub fn bundles(
        &self,
        p: &Policy,
        g: &State,
        v: &impl ReceiptVerifierV1,
        limits: Limits,
    ) -> Result<Vec<Vec<u8>>, Error> {
        let _guard = self.lock()?;
        Ok(self.read(p, g, v, limits)?.rows)
    }
    pub fn publish(
        &self,
        p: &Policy,
        g: &State,
        v: &impl ReceiptVerifierV1,
        prepared: Prepared,
        limits: Limits,
    ) -> Result<Publication, Error> {
        self.publish_with_crash(p, g, v, prepared, limits, None)
    }
    /// Deterministic failure injection at filesystem boundaries. A caller must
    /// reload after any error; an error after rename can mean a complete commit.
    /// Tests also terminate a subprocess after these injected failures. No
    /// partial-publication recovery or filesystem power-loss theorem is claimed.
    pub fn publish_with_crash(
        &self,
        p: &Policy,
        g: &State,
        v: &impl ReceiptVerifierV1,
        prepared: Prepared,
        limits: Limits,
        crash: Option<CrashPoint>,
    ) -> Result<Publication, Error> {
        let _guard = self.lock()?;
        let mut current = self.read(p, g, v, limits)?;
        if current.rows.iter().any(|row| *row == prepared.bytes) {
            return Ok(Publication::Duplicate);
        }
        if current.state.root() != prepared.pre
            || current.state.version().checked_add(1) != Some(prepared.post.version())
        {
            return Err(Error::Conflict);
        }
        let checked = decide(p, &current.state, &prepared.request, v, limits)
            .into_parts()
            .0?;
        if checked.bytes != prepared.bytes || checked.post != prepared.post {
            return Err(Error::Corrupt);
        }
        current.rows.push(checked.bytes);
        let bytes = history(p, g, &current.rows)?;
        self.replace(&bytes, crash)?;
        Ok(Publication::Committed)
    }
    fn replace(&self, bytes: &[u8], crash: Option<CrashPoint>) -> Result<(), Error> {
        point(crash, CrashPoint::BeforeWrite)?;
        let temp = self.directory.join("pending");
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        let middle = bytes.len() / 2;
        f.write_all(&bytes[..middle])?;
        point(crash, CrashPoint::DuringWrite)?;
        f.write_all(&bytes[middle..])?;
        point(crash, CrashPoint::AfterWrite)?;
        f.sync_all()?;
        point(crash, CrashPoint::AfterFileSync)?;
        // ATOMIC_PUBLICATION: only this rename replaces the entire authoritative file.
        fs::rename(&temp, self.path())?;
        point(crash, CrashPoint::AfterRename)?;
        File::open(&self.directory)?.sync_all()?;
        point(crash, CrashPoint::AfterDirectorySync)?;
        Ok(())
    }
}
fn point(selected: Option<CrashPoint>, at: CrashPoint) -> Result<(), Error> {
    if selected == Some(at) {
        Err(Error::Crash(at))
    } else {
        Ok(())
    }
}
fn history(p: &Policy, g: &State, rows: &[Vec<u8>]) -> Result<Vec<u8>, Error> {
    let mut w = W::new(b"zusd.step4.ledger/1\0");
    w.id(p.id);
    w.id(g.root());
    w.blob(&g.canonical_bytes());
    w.len(rows.len());
    for r in rows {
        w.blob(r)
    }
    let digest = encoding::hash(&w.0);
    w.id(digest);
    if w.0.len() as u64 > p.config.bounds.history_bytes {
        return Err(Error::Refused(Failure::Capacity));
    }
    Ok(w.0)
}
