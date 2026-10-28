//! Linux x86-64 native leaf supervision over dedicated, bounded IPC.
//!
//! Trusted administration supplies pinned, administrator-stable runtime files,
//! a worker image and its expected digest. The image is copied into a read-only
//! filesystem with its pinned GNU runtime. Worker requests carry no identity or
//! authority: [`Channel`] resolves them through actual retained handles.
//!
//! This profile requires an unprivileged host account, CPython 3.11+, bubblewrap,
//! libseccomp, Linux namespaces/seccomp and a responsive kernel. Runtime files
//! and their ancestors must be root-owned and not group/other writable. Hashes
//! identify supplied bytes; installation qualification must establish provenance
//! and the complete compiler/runtime/configuration closure. The bootstrap checks
//! its loaded module/cache/mapping inventory, which alone is not that argument.
//! The supervisor retains exclusive child-reaping ownership; external wait-any
//! handlers must not consume its children.
//!
//! One native leaf has no fork, threads, sockets or ambient host filesystem.
//! CPU/AS/file/FD limits are 2 s/64 MiB/16 MiB/32; wall time is 5 s. This is not an
//! aggregate resource profile for multiprocess factory build tools. Transport
//! execution remains trusted and needs its own deadline: stopping a worker does
//! not cancel or undo an admitted external action.

use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{self, Read, Write},
    net::Shutdown,
    os::{
        fd::OwnedFd,
        unix::{fs::MetadataExt, net::UnixStream},
    },
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::{
    channel::{self, Channel, Reply, Status},
    dispatcher::TrustedDispatcher,
    production::Transport,
};

const BOOTSTRAP: &str = include_str!("native_worker.py");
const MAX_FILE: u64 = 16 << 20;
const MAX_TOTAL: usize = 64 << 20;
const MAX_PINS: usize = 256;
/// Largest accepted worker ELF image, in bytes.
pub const MAX_IMAGE: usize = 8 << 20;
const PARENT_DEADLINE: Duration = Duration::from_secs(10);

/// An explicit installation-selected runtime file identity; no automatic adoption.
#[derive(Clone, Debug)]
pub struct PinnedFile {
    path: PathBuf,
    sha256: Hash32,
}

impl PinnedFile {
    /// Pin an absolute UTF-8 path of at most 512 bytes to its expected digest.
    /// Launch additionally verifies the file, permissions and canonical ancestors.
    pub fn new(path: impl Into<PathBuf>, sha256: Hash32) -> Result<Self, Error> {
        let path = path.into();
        let text = path.to_str().ok_or(Error::Configuration)?;
        if !path.is_absolute() || text.is_empty() || text.len() > 512 || text.contains('\0') {
            return Err(Error::Configuration);
        }
        Ok(Self { path, sha256 })
    }
}

/// Trusted installation configuration. All fields are checked again at launch.
/// Dependencies include the bootstrap's loaded module, bytecode-cache and mapped
/// files, with no duplicate core paths. Keep the administrator's reviewed pins;
/// a newly discovered inventory is a proposal, not authorization to update them.
#[derive(Clone, Debug)]
pub struct Runtime {
    /// Pinned CPython executable.
    pub python: PinnedFile,
    /// Pinned bubblewrap executable.
    pub bwrap: PinnedFile,
    /// Pinned util-linux prlimit executable.
    pub prlimit: PinnedFile,
    /// Pinned GNU ELF interpreter copied to the worker root.
    pub loader: PinnedFile,
    /// Pinned libc copied to the worker root.
    pub libc: PinnedFile,
    /// Pinned libgcc_s copied to the worker root.
    pub libgcc: PinnedFile,
    /// Pinned libm copied to the worker root.
    pub libm: PinnedFile,
    /// Pinned system libseccomp used by the bootstrap.
    pub seccomp: PinnedFile,
    /// Additional pinned bootstrap dependencies; at most 256 total runtime files.
    pub dependencies: Vec<PinnedFile>,
}

/// Failure never authorizes retry, acknowledgement or replacement reservation state.
#[derive(Debug)]
pub enum Error {
    /// Malformed or excessive trusted configuration.
    Configuration,
    /// Worker bytes differ from the expected digest or supported ELF header.
    Image,
    /// Runtime identity, file kind or protected ownership/permissions differ.
    Runtime,
    /// Required bootstrap/control binding was not established.
    Bootstrap,
    /// IPC, process or filesystem operation failed; an earlier action may exist.
    Io(io::Error),
    /// The supervisor's absolute I/O/cleanup deadline expired.
    Timeout,
    /// A protocol error stopped the channel before this request's dispatch.
    Protocol(channel::Error),
    /// EOF truncated a worker request. Earlier complete requests may have applied.
    Truncated,
    /// A prior failure, unwind, completion or known supervisor exit stopped this worker.
    Stopped,
    /// Reply delivery failed after dispatch. The actual observed result is retained.
    ReplyLost(Reply),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        if matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        ) {
            Self::Timeout
        } else {
            Self::Io(error)
        }
    }
}

/// The trusted bootstrap's termination classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Termination {
    /// The child exited and its bounded output was drained.
    Exited,
    /// The five-second native-worker wall deadline expired.
    WallTimeout,
    /// The requesting side closed its dedicated connection.
    RequesterClosed,
    /// A declared worker output/input bound was exceeded.
    OutputLimit,
    /// A worker pipe closed while a reply was pending.
    ClosedPeer,
    /// Configuration or an unavailable platform control refused bootstrap.
    BootstrapRefused,
    /// An unresolved trusted helper failure occurred.
    SupervisorFailure,
}

/// Bounded trusted observations. A worker exit code is not confinement proof or
/// evidence that an external operation did or did not happen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    /// Why the trusted bootstrap ended supervision.
    pub termination: Termination,
    /// Observed bwrap child exit status; negative values denote a signal.
    pub worker_exit: i32,
    /// Number of observed stderr bytes, up to the detecting 256-byte chunk.
    pub stderr_bytes: u32,
    /// SHA-256 of the captured stderr prefix, limited to 2048 bytes.
    pub stderr_prefix_sha256: Hash32,
    /// Number of observed worker stdout bytes, including a detecting chunk.
    pub output_bytes: u32,
    /// Number of observed reply bytes received from the requesting side.
    pub input_bytes: u32,
}

/// Exclusive process/endpoint custody for one native worker. Never lend this
/// administrative object or its dispatcher/channel to untrusted in-process code.
pub struct Worker<'h> {
    channel: Channel<'h>,
    child: Child,
    wire: UnixStream,
    control: UnixStream,
    deadline: Instant,
    active: bool,
    monitor: u32,
    namespace_init: u32,
}

impl<'h> Worker<'h> {
    /// Launch the checked image with explicitly worker-visible arguments.
    /// No credentials belong in these arguments. Missing controls refuse launch;
    /// the image never falls back to direct host execution.
    pub fn spawn(
        runtime: &Runtime,
        image: &[u8],
        expected: Hash32,
        arguments: &[&str],
        channel: Channel<'h>,
    ) -> Result<Self, Error> {
        if !(64..=MAX_IMAGE).contains(&image.len())
            || &image[..7] != b"\x7fELF\x02\x01\x01"
            || &image[18..20] != b"\x3e\0"
            || RustCryptoSha256::hash(image) != expected
        {
            return Err(Error::Image);
        }
        if arguments.len() > 8
            || arguments
                .iter()
                .any(|value| value.len() > 512 || value.contains('\0'))
            || runtime.dependencies.len() > MAX_PINS - 8
        {
            return Err(Error::Configuration);
        }
        let mut pins = vec![
            &runtime.python,
            &runtime.bwrap,
            &runtime.prlimit,
            &runtime.loader,
            &runtime.libc,
            &runtime.libgcc,
            &runtime.libm,
            &runtime.seccomp,
        ];
        pins.extend(runtime.dependencies.iter());
        let mut paths = Vec::with_capacity(pins.len());
        let mut seen = BTreeSet::new();
        let mut total = 0_usize;
        for pin in &pins {
            let (path, bytes) = verify(pin)?;
            total = total.checked_add(bytes).ok_or(Error::Configuration)?;
            if total > MAX_TOTAL || !seen.insert(path.clone()) {
                return Err(Error::Configuration);
            }
            paths.push(path);
        }
        let mut config = b"ZFNAT001".to_vec();
        config.extend_from_slice(&(pins.len() as u16).to_le_bytes());
        for (path, pin) in paths.iter().zip(&pins) {
            let text = path.to_str().ok_or(Error::Runtime)?;
            if text.len() > 512 {
                return Err(Error::Runtime);
            }
            config.extend_from_slice(&(text.len() as u16).to_le_bytes());
            config.extend_from_slice(text.as_bytes());
            config.extend_from_slice(pin.sha256.as_bytes());
        }
        config.push(arguments.len() as u8);
        for value in arguments {
            config.extend_from_slice(&(value.len() as u16).to_le_bytes());
            config.extend_from_slice(value.as_bytes());
        }
        config.extend_from_slice(&(image.len() as u32).to_le_bytes());
        config.extend_from_slice(expected.as_bytes());
        config.extend_from_slice(image);
        let (wire, endpoint) = UnixStream::pair()?;
        let (control, status_endpoint) = UnixStream::pair()?;
        let deadline = Instant::now() + PARENT_DEADLINE;
        let child = Command::new(&paths[0])
            .args(["-I", "-S", "-B", "-c", BOOTSTRAP])
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::from(OwnedFd::from(endpoint.try_clone()?)))
            .stdout(Stdio::from(OwnedFd::from(endpoint)))
            .stderr(Stdio::from(OwnedFd::from(status_endpoint)))
            .spawn()?;
        let mut worker = Self {
            channel,
            child,
            wire,
            control,
            deadline,
            active: false,
            monitor: 0,
            namespace_init: 0,
        };
        write_until(&mut worker.wire, &config, deadline)?;
        let mut ready = [0; 16];
        read_until(&mut worker.control, &mut ready, deadline)?;
        if &ready[..8] != b"ZFNOK001" {
            return Err(Error::Bootstrap);
        }
        worker.monitor = number(&ready[8..12]);
        worker.namespace_init = number(&ready[12..16]);
        if worker.monitor < 2
            || worker.namespace_init < 2
            || worker.monitor == worker.namespace_init
        {
            return Err(Error::Bootstrap);
        }
        worker.active = true;
        Ok(worker)
    }

    /// Host PID of the trusted bootstrap, for administrative process observations.
    #[must_use]
    pub fn supervisor_pid(&self) -> u32 {
        self.child.id()
    }

    /// Host PIDs reported by bwrap for its monitor and namespace init. These are
    /// observations, not authority or substitutes for PID-start-identity checks.
    #[must_use]
    pub fn container_pids(&self) -> [u32; 2] {
        [self.monitor, self.namespace_init]
    }

    /// Process one fixed worker request. `None` means clean request-stream EOF,
    /// not that earlier effects were absent. Revocation can occur between calls.
    /// A reply-write failure returns ReplyLost without undoing its reservation.
    /// The channel and worker are poisoned before the potentially effecting call.
    pub fn exchange<T: Transport>(
        &mut self,
        dispatcher: &mut TrustedDispatcher<'_, '_, T>,
    ) -> Result<Option<Reply>, Error> {
        if !self.active {
            self.stop();
            return Err(Error::Stopped);
        }
        self.active = false;
        let result = self.exchange_inner(dispatcher);
        match &result {
            Ok(Some(reply)) if reply.status != Status::DispatcherUnavailable => self.active = true,
            _ => self.stop(),
        }
        result
    }

    fn exchange_inner<T: Transport>(
        &mut self,
        dispatcher: &mut TrustedDispatcher<'_, '_, T>,
    ) -> Result<Option<Reply>, Error> {
        let mut frame = [0; channel::FRAME_BYTES];
        let mut used = 0;
        while used < frame.len() {
            self.wire
                .set_read_timeout(Some(remaining(self.deadline)?))?;
            match self.wire.read(&mut frame[used..]) {
                Ok(0) if used == 0 => return Ok(None),
                Ok(0) => return Err(Error::Truncated),
                Ok(count) => used += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
        }
        if self.child.try_wait()?.is_some() {
            return Err(Error::Stopped);
        }
        let reply = self
            .channel
            .dispatch(dispatcher, &frame)
            .map_err(Error::Protocol)?;
        if write_until(&mut self.wire, &reply.encode(), self.deadline).is_err() {
            return Err(Error::ReplyLost(reply));
        }
        Ok(Some(reply))
    }

    /// Close the requesting connection, collect the separate trusted status and
    /// reap the bootstrap. Closing never rolls back a possible external action.
    pub fn finish(mut self) -> Result<Completion, Error> {
        self.stop();
        let mut record = [0; 64];
        read_until(&mut self.control, &mut record, self.deadline)?;
        if &record[..8] != b"ZFNEND01" || record[9..12] != [0; 3] || record[60..] != [0; 4] {
            return Err(Error::Bootstrap);
        }
        let termination = match record[8] {
            1 => Termination::Exited,
            2 => Termination::WallTimeout,
            3 => Termination::RequesterClosed,
            4 => Termination::OutputLimit,
            5 => Termination::ClosedPeer,
            6 => Termination::BootstrapRefused,
            7 => Termination::SupervisorFailure,
            _ => return Err(Error::Bootstrap),
        };
        let mut signed = [0; 4];
        signed.copy_from_slice(&record[12..16]);
        let mut digest = [0; 32];
        digest.copy_from_slice(&record[20..52]);
        let result = Completion {
            termination,
            worker_exit: i32::from_le_bytes(signed),
            stderr_bytes: number(&record[16..20]),
            stderr_prefix_sha256: Hash32::new(digest),
            output_bytes: number(&record[52..56]),
            input_bytes: number(&record[56..60]),
        };
        if result.stderr_bytes > 2048 + 256
            || result.output_bytes > 1024 * 16 + 16
            || result.input_bytes > 1024 * 16 + 16
        {
            return Err(Error::Bootstrap);
        }
        while self.child.try_wait()?.is_none() {
            remaining(self.deadline)?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(result)
    }

    fn stop(&mut self) {
        self.active = false;
        let _ = self.wire.shutdown(Shutdown::Both);
    }
}

impl Drop for Worker<'_> {
    fn drop(&mut self) {
        self.stop();
        let until = Instant::now() + Duration::from_millis(250);
        while Instant::now() < until {
            match self.child.try_wait() {
                Ok(None) => {}
                Ok(Some(_)) | Err(_) => return,
            }
            thread::sleep(Duration::from_millis(5));
        }
        // Only this still-owned, unreaped direct child can be signalled. Bwrap's
        // parent-death/PID namespace path additionally tears down the leaf.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn verify(pin: &PinnedFile) -> Result<(PathBuf, usize), Error> {
    let path = fs::canonicalize(&pin.path).map_err(|_| Error::Runtime)?;
    for component in path.ancestors() {
        let metadata = fs::metadata(component).map_err(|_| Error::Runtime)?;
        if metadata.uid() != 0 || metadata.mode() & 0o022 != 0 {
            return Err(Error::Runtime);
        }
    }
    let file = File::open(&path).map_err(|_| Error::Runtime)?;
    if !file.metadata().map_err(|_| Error::Runtime)?.is_file() {
        return Err(Error::Runtime);
    }
    let mut data = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut data)
        .map_err(|_| Error::Runtime)?;
    if data.len() as u64 > MAX_FILE || RustCryptoSha256::hash(&data) != pin.sha256 {
        return Err(Error::Runtime);
    }
    Ok((path, data.len()))
}

fn number(bytes: &[u8]) -> u32 {
    let mut out = [0; 4];
    out.copy_from_slice(bytes);
    u32::from_le_bytes(out)
}

fn remaining(deadline: Instant) -> Result<Duration, Error> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|time| !time.is_zero())
        .ok_or(Error::Timeout)
}

fn write_until(stream: &mut UnixStream, mut bytes: &[u8], deadline: Instant) -> Result<(), Error> {
    while !bytes.is_empty() {
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        match stream.write(bytes) {
            Ok(0) => return Err(Error::Io(io::ErrorKind::WriteZero.into())),
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn read_until(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: Instant,
) -> Result<(), Error> {
    while !bytes.is_empty() {
        stream.set_read_timeout(Some(remaining(deadline)?))?;
        match stream.read(bytes) {
            Ok(0) => return Err(Error::Bootstrap),
            Ok(count) => bytes = &mut bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
