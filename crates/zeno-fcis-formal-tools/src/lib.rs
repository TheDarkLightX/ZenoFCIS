//! Deterministic formal-tool exporters and fail-closed process adapters.
//!
//! This standard-library shell can retain tool evidence. It cannot construct
//! [`zeno_fcis_backend::BackendCertificate`]; only the independent verifier in
//! `zeno-fcis-backend` owns that constructor path.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use zeno_fcis_codec::{CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_spec::{
    BackendId, ClaimDecl, ClaimFormula, ClaimMode, CompareOp, EvalLimits, EvalOutcome,
    EvaluationContext, Identifier, IndeterminateReason, LawDecl, MAX_FINITE_HORIZON,
    MAX_FORMULA_DEPTH, Observation, PredicateProvider, ProjectionPath, ProjectionRoot, RelExpr,
    StableId, TemporalEvaluation, TemporalFormula, TraceStep, ValueExpr, evaluate_relational,
    evaluate_temporal, invariant_at,
};

mod system;
pub use system::{
    SystemAnswer, SystemObligationKind, SystemObligations, SystemSolveError, SystemVerdict,
    export_system_smt, parse_system_answer, system_verdict,
};

/// Formal-tools manifest format.
pub const TOOLS_MANIFEST_FORMAT: &str = "zeno-fcis/tools/2";
/// CVC5 release qualified by RC3.
pub const CVC5_VERSION: &str = "1.3.3";
/// Z3 release qualified by RC3.
pub const Z3_VERSION: &str = "4.16.0";
/// Lean release qualified by RC3.
pub const LEAN_VERSION: &str = "4.30.0";
/// Canonical Lean 4.30.0 Linux x86-64 runtime-tree identity qualified by RC3.
pub const LEAN_LINUX_X86_64_TREE_SHA256: &str =
    "5dc9cab14b1a15fc8d6cfc3f1c1b627c0c74facb23465fb9463c42554a807f5b";
/// Maximum tools-manifest size.
pub const MAX_TOOLS_MANIFEST_BYTES: usize = 1024 * 1024;
/// Maximum admitted executable size for hashing.
pub const MAX_TOOL_BINARY_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum regular files admitted into one Lean runtime closure.
pub const MAX_LEAN_TOOLCHAIN_FILES: usize = 25_000;
/// Maximum directory nesting admitted into one Lean runtime closure.
pub const MAX_LEAN_TOOLCHAIN_DEPTH: usize = 64;
/// Maximum total regular-file bytes admitted into one Lean runtime closure.
pub const MAX_LEAN_TOOLCHAIN_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Maximum UTF-8 relative-path bytes admitted into one Lean runtime closure.
pub const MAX_LEAN_TOOLCHAIN_PATH_BYTES: usize = 4_096;
/// Maximum formula nodes rendered into one formal obligation.
pub const MAX_EXPORT_FORMULA_NODES: usize = 4_096;
/// Maximum conservative render operations for one formal obligation.
pub const MAX_EXPORT_OPERATIONS: u64 = 1_000_000;
/// Maximum generated source bytes for one formal obligation.
pub const MAX_EXPORT_SOURCE_BYTES: usize = 16 * 1024 * 1024;

/// Explicit resource envelope for deterministic formal export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportLimits {
    max_horizon: u32,
    max_formula_nodes: usize,
    max_formula_depth: usize,
    max_operations: u64,
    max_source_bytes: usize,
}
impl ExportLimits {
    /// Creates nonzero limits no larger than the RC3 hard export envelope.
    pub const fn try_new(
        max_horizon: u32,
        max_formula_nodes: usize,
        max_formula_depth: usize,
        max_operations: u64,
        max_source_bytes: usize,
    ) -> Option<Self> {
        if max_horizon == 0
            || max_horizon > MAX_FINITE_HORIZON
            || max_formula_nodes == 0
            || max_formula_nodes > MAX_EXPORT_FORMULA_NODES
            || max_formula_depth == 0
            || max_formula_depth > MAX_FORMULA_DEPTH
            || max_operations == 0
            || max_operations > MAX_EXPORT_OPERATIONS
            || max_source_bytes == 0
            || max_source_bytes > MAX_EXPORT_SOURCE_BYTES
        {
            return None;
        }
        Some(Self {
            max_horizon,
            max_formula_nodes,
            max_formula_depth,
            max_operations,
            max_source_bytes,
        })
    }
    /// Returns the finite-horizon export bound.
    #[must_use]
    pub const fn max_horizon(self) -> u32 {
        self.max_horizon
    }
    /// Returns the formula-node export bound.
    #[must_use]
    pub const fn max_formula_nodes(self) -> usize {
        self.max_formula_nodes
    }
    /// Returns the recursive render-depth bound.
    #[must_use]
    pub const fn max_formula_depth(self) -> usize {
        self.max_formula_depth
    }
    /// Returns the conservative render-operation bound.
    #[must_use]
    pub const fn max_operations(self) -> u64 {
        self.max_operations
    }
    /// Returns the generated-source byte bound.
    #[must_use]
    pub const fn max_source_bytes(self) -> usize {
        self.max_source_bytes
    }
}
impl Default for ExportLimits {
    fn default() -> Self {
        Self {
            max_horizon: MAX_FINITE_HORIZON,
            max_formula_nodes: MAX_EXPORT_FORMULA_NODES,
            max_formula_depth: MAX_FORMULA_DEPTH,
            max_operations: MAX_EXPORT_OPERATIONS,
            max_source_bytes: MAX_EXPORT_SOURCE_BYTES,
        }
    }
}

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn open_untrusted_read(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    options.open(path)
}

/// Closed process backend family.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolBackend {
    Cvc5,
    Z3,
    Lean,
}
impl ToolBackend {
    /// Returns the qualified exact release.
    #[must_use]
    pub const fn required_version(self) -> &'static str {
        match self {
            Self::Cvc5 => CVC5_VERSION,
            Self::Z3 => Z3_VERSION,
            Self::Lean => LEAN_VERSION,
        }
    }
    /// Returns the corresponding specification backend.
    #[must_use]
    pub const fn spec_backend(self) -> BackendId {
        match self {
            Self::Cvc5 => BackendId::Cvc5,
            Self::Z3 => BackendId::Z3,
            Self::Lean => BackendId::Lean,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Cvc5 => "cvc5",
            Self::Z3 => "z3",
            Self::Lean => "lean",
        }
    }
}

/// Exact runtime closure required for Lean kernel execution.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRuntimeConfig {
    root: PathBuf,
    tree_sha256: String,
}
impl ToolRuntimeConfig {
    /// Returns the source runtime root copied into the checked private snapshot.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Returns the lowercase SHA-256 commitment to the canonical runtime inventory.
    #[must_use]
    pub fn tree_sha256(&self) -> &str {
        &self.tree_sha256
    }
}

/// One untrusted manifest entry rechecked before every process run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    backend: ToolBackend,
    path: PathBuf,
    version: String,
    sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime: Option<ToolRuntimeConfig>,
    timeout_ms: u64,
    max_output_bytes: usize,
    #[serde(default)]
    allowed_axioms: Vec<String>,
}
impl ToolConfig {
    /// Returns the backend family.
    #[must_use]
    pub const fn backend(&self) -> ToolBackend {
        self.backend
    }
    /// Returns the executable path from the untrusted manifest.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the exact expected version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns the lowercase SHA-256 expectation.
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    /// Returns the exact runtime closure configuration, when required.
    #[must_use]
    pub const fn runtime(&self) -> Option<&ToolRuntimeConfig> {
        self.runtime.as_ref()
    }
    /// Returns the process timeout.
    #[must_use]
    pub const fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
    /// Returns the combined process-output bound.
    #[must_use]
    pub const fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }
    /// Returns explicitly allowed Lean axioms.
    #[must_use]
    pub fn allowed_axioms(&self) -> &[String] {
        &self.allowed_axioms
    }
}

/// Versioned tools manifest.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolsManifest {
    format: String,
    tools: Vec<ToolConfig>,
}
impl ToolsManifest {
    /// Returns entries in canonical backend order.
    #[must_use]
    pub fn tools(&self) -> &[ToolConfig] {
        &self.tools
    }
    /// Finds one backend configuration.
    #[must_use]
    pub fn tool(&self, backend: ToolBackend) -> Option<&ToolConfig> {
        self.tools
            .binary_search_by_key(&backend, ToolConfig::backend)
            .ok()
            .map(|index| &self.tools[index])
    }
    /// Serializes canonical field and backend order.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ManifestError> {
        serde_json::to_vec(self).map_err(|error| ManifestError::Json(error.to_string()))
    }
}

/// Tools-manifest admission failure.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestError {
    Io(String),
    NotFile,
    TooLarge,
    Json(String),
    WrongFormat {
        expected: &'static str,
        actual: String,
    },
    DuplicateBackend,
    WrongVersion {
        backend: ToolBackend,
        actual: String,
    },
    InvalidHash,
    InvalidRuntime,
    InvalidLimit,
    InvalidAxiom,
}

/// Reads and validates one untrusted manifest without following any `.zeno` data.
pub fn load_tools_manifest(path: &Path) -> Result<ToolsManifest, ManifestError> {
    let file = open_untrusted_read(path).map_err(|error| ManifestError::Io(error.to_string()))?;
    if !file
        .metadata()
        .map_err(|error| ManifestError::Io(error.to_string()))?
        .is_file()
    {
        return Err(ManifestError::NotFile);
    }
    let mut bytes = Vec::new();
    file.take(
        u64::try_from(MAX_TOOLS_MANIFEST_BYTES)
            .unwrap_or(u64::MAX)
            .saturating_add(1),
    )
    .read_to_end(&mut bytes)
    .map_err(|error| ManifestError::Io(error.to_string()))?;
    if bytes.len() > MAX_TOOLS_MANIFEST_BYTES {
        return Err(ManifestError::TooLarge);
    }
    let mut manifest: ToolsManifest =
        serde_json::from_slice(&bytes).map_err(|error| ManifestError::Json(error.to_string()))?;
    if manifest.format != TOOLS_MANIFEST_FORMAT {
        return Err(ManifestError::WrongFormat {
            expected: TOOLS_MANIFEST_FORMAT,
            actual: manifest.format,
        });
    }
    manifest.tools.sort_by_key(ToolConfig::backend);
    if manifest
        .tools
        .windows(2)
        .any(|pair| pair[0].backend == pair[1].backend)
    {
        return Err(ManifestError::DuplicateBackend);
    }
    for tool in &mut manifest.tools {
        if tool.version != tool.backend.required_version() {
            return Err(ManifestError::WrongVersion {
                backend: tool.backend,
                actual: tool.version.clone(),
            });
        }
        if !is_hash(&tool.sha256) {
            return Err(ManifestError::InvalidHash);
        }
        match (tool.backend, &tool.runtime) {
            (ToolBackend::Lean, Some(runtime))
                if runtime.root.is_absolute()
                    && is_hash(&runtime.tree_sha256)
                    && lean_executable_relative(tool, runtime).is_some() => {}
            (ToolBackend::Lean, _) | (_, Some(_)) => return Err(ManifestError::InvalidRuntime),
            (_, None) => {}
        }
        if tool.timeout_ms == 0
            || tool.timeout_ms > 600_000
            || tool.max_output_bytes == 0
            || tool.max_output_bytes > 64 * 1024 * 1024
        {
            return Err(ManifestError::InvalidLimit);
        }
        tool.allowed_axioms.sort();
        tool.allowed_axioms.dedup();
        if tool
            .allowed_axioms
            .iter()
            .any(|value| value.is_empty() || !value.is_ascii())
        {
            return Err(ManifestError::InvalidAxiom);
        }
        if tool.backend != ToolBackend::Lean && !tool.allowed_axioms.is_empty() {
            return Err(ManifestError::InvalidAxiom);
        }
    }
    Ok(manifest)
}

/// Rechecked executable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIdentity {
    backend: ToolBackend,
    path: PathBuf,
    version: String,
    binary_hash: Hash32,
    runtime_hash: Option<Hash32>,
}
impl ToolIdentity {
    /// Returns the backend.
    #[must_use]
    pub const fn backend(&self) -> ToolBackend {
        self.backend
    }
    /// Returns the checked executable path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Returns the checked version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns the checked executable hash.
    #[must_use]
    pub const fn binary_hash(&self) -> Hash32 {
        self.binary_hash
    }
    /// Returns the checked Lean runtime-closure hash, when one is required.
    #[must_use]
    pub const fn runtime_hash(&self) -> Option<Hash32> {
        self.runtime_hash
    }
}

/// Fail-closed process or admission failure.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolFailure {
    Missing,
    NotFile,
    BinaryTooLarge,
    ToolchainTooLarge,
    ToolchainUnsupported,
    ToolchainIncomplete,
    ToolchainHashMismatch,
    HashMismatch,
    VersionMismatch,
    Timeout,
    Crash(Option<i32>),
    OutputLimit,
    ProcessContainmentUnavailable,
    ProcessContainmentFailed,
    InconsistentResult,
    RetentionConflict,
    Unknown,
    UnsupportedEvidence,
    ModelReplayFailed,
    LeanAxiomReport,
    Io(String),
}

/// One regular file bound into a canonical Lean runtime inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolchainFileIdentity {
    relative_path: String,
    length: u64,
    executable: bool,
    sha256: Hash32,
}
impl ToolchainFileIdentity {
    /// Returns the portable slash-separated path relative to the runtime root.
    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }
    /// Returns the exact file length.
    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }
    /// Returns whether any executable bit was set in the admitted source tree.
    #[must_use]
    pub const fn executable(&self) -> bool {
        self.executable
    }
    /// Returns the exact file-content SHA-256.
    #[must_use]
    pub const fn sha256(&self) -> Hash32 {
        self.sha256
    }
}

/// Portable bounded identity of one complete Lean distribution closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolchainInventory {
    files: Vec<ToolchainFileIdentity>,
    total_bytes: u64,
    tree_sha256: Hash32,
}
impl ToolchainInventory {
    /// Returns files in canonical relative-path order.
    #[must_use]
    pub fn files(&self) -> &[ToolchainFileIdentity] {
        &self.files
    }
    /// Returns the exact total regular-file bytes.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }
    /// Returns the portable tree SHA-256.
    #[must_use]
    pub const fn tree_sha256(&self) -> Hash32 {
        self.tree_sha256
    }
    /// Serializes the canonical retained inventory.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ToolFailure> {
        #[derive(Serialize)]
        struct FileRecord<'a> {
            path: &'a str,
            length: u64,
            executable: bool,
            sha256: String,
        }
        #[derive(Serialize)]
        struct InventoryRecord<'a> {
            format: &'static str,
            tree_sha256: String,
            total_bytes: u64,
            files: Vec<FileRecord<'a>>,
        }
        let files = self
            .files
            .iter()
            .map(|file| FileRecord {
                path: &file.relative_path,
                length: file.length,
                executable: file.executable,
                sha256: hash_hex(file.sha256),
            })
            .collect();
        serde_json::to_vec(&InventoryRecord {
            format: "zeno-fcis/toolchain-inventory/1",
            tree_sha256: hash_hex(self.tree_sha256),
            total_bytes: self.total_bytes,
            files,
        })
        .map_err(|error| ToolFailure::Io(error.to_string()))
    }
}

#[derive(Clone)]
struct ToolchainSourceFile {
    #[cfg(not(unix))]
    source_path: PathBuf,
    relative_path: String,
    executable: bool,
}

/// Inspects a bounded Lean distribution without executing it.
///
/// Execution repeats this work while copying every admitted byte into a private
/// snapshot. This function is intended for preparing the separate tools
/// manifest and release checksum record.
pub fn inspect_lean_toolchain(root: &Path) -> Result<ToolchainInventory, ToolFailure> {
    snapshot_toolchain(root, None)
}

fn snapshot_toolchain(
    source_root: &Path,
    destination_root: Option<&Path>,
) -> Result<ToolchainInventory, ToolFailure> {
    let source_files = enumerate_toolchain(source_root)?;
    snapshot_toolchain_files(source_root, source_files, destination_root)
}

fn snapshot_toolchain_files(
    source_root: &Path,
    source_files: Vec<ToolchainSourceFile>,
    destination_root: Option<&Path>,
) -> Result<ToolchainInventory, ToolFailure> {
    #[cfg(unix)]
    let source_root = open_toolchain_root(source_root)?;
    let mut files = Vec::with_capacity(source_files.len());
    let mut total_bytes = 0_u64;
    for source in source_files {
        #[cfg(unix)]
        let file = open_rooted_toolchain_file(&source_root, &source.relative_path)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        #[cfg(not(unix))]
        let file = open_untrusted_read(&source.source_path)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        let metadata = file
            .metadata()
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        if !metadata.is_file() {
            return Err(ToolFailure::ToolchainUnsupported);
        }
        let mut bytes = Vec::new();
        file.take(MAX_TOOL_BINARY_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if length > MAX_TOOL_BINARY_BYTES {
            return Err(ToolFailure::BinaryTooLarge);
        }
        total_bytes = total_bytes
            .checked_add(length)
            .ok_or(ToolFailure::ToolchainTooLarge)?;
        if total_bytes > MAX_LEAN_TOOLCHAIN_BYTES {
            return Err(ToolFailure::ToolchainTooLarge);
        }
        let sha256 = RustCryptoSha256::hash(&bytes);
        if let Some(destination_root) = destination_root {
            let destination = destination_root.join(&source.relative_path);
            let parent = destination
                .parent()
                .ok_or(ToolFailure::ToolchainUnsupported)?;
            fs::create_dir_all(parent).map_err(|error| ToolFailure::Io(error.to_string()))?;
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|error| ToolFailure::Io(error.to_string()))?;
            output
                .write_all(&bytes)
                .map_err(|error| ToolFailure::Io(error.to_string()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if source.executable { 0o700 } else { 0o600 };
                output
                    .set_permissions(fs::Permissions::from_mode(mode))
                    .map_err(|error| ToolFailure::Io(error.to_string()))?;
            }
        }
        files.push(ToolchainFileIdentity {
            relative_path: source.relative_path,
            length,
            executable: source.executable,
            sha256,
        });
    }
    let tree_sha256 = hash_toolchain_inventory(&files, total_bytes);
    Ok(ToolchainInventory {
        files,
        total_bytes,
        tree_sha256,
    })
}

#[cfg(unix)]
fn nix_io_error(error: nix::errno::Errno) -> std::io::Error {
    std::io::Error::from_raw_os_error(error as i32)
}

#[cfg(unix)]
fn open_toolchain_root(root: &Path) -> Result<std::os::fd::OwnedFd, ToolFailure> {
    use nix::fcntl::{OFlag, open};
    use nix::sys::stat::Mode;

    open(
        root,
        OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| ToolFailure::Io(nix_io_error(error).to_string()))
}

#[cfg(unix)]
fn open_rooted_toolchain_file(
    root: &std::os::fd::OwnedFd,
    relative_path: &str,
) -> std::io::Result<File> {
    use nix::fcntl::{OFlag, openat};
    use nix::sys::stat::Mode;

    let mut current = root.try_clone()?;
    let mut components = relative_path.split('/').peekable();
    while let Some(component) = components.next() {
        if component.is_empty() || matches!(component, "." | "..") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "non-normal toolchain path component",
            ));
        }
        let flags = if components.peek().is_some() {
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC
        } else {
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC
        };
        current = openat(&current, component, flags, Mode::empty()).map_err(nix_io_error)?;
    }
    Ok(File::from(current))
}

fn enumerate_toolchain(root: &Path) -> Result<Vec<ToolchainSourceFile>, ToolFailure> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolFailure::Missing
        } else {
            ToolFailure::Io(error.to_string())
        }
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(ToolFailure::ToolchainUnsupported);
    }
    let mut directories = vec![(root.to_path_buf(), String::new(), 0_usize)];
    let mut files = Vec::new();
    let mut declared_bytes = 0_u64;
    while let Some((directory, prefix, depth)) = directories.pop() {
        if depth > MAX_LEAN_TOOLCHAIN_DEPTH {
            return Err(ToolFailure::ToolchainTooLarge);
        }
        let entries = fs::read_dir(directory)
            .map_err(|error| ToolFailure::Io(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        let mut entries = entries
            .into_iter()
            .map(|entry| {
                let name = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| ToolFailure::ToolchainUnsupported)?;
                Ok((name, entry.path()))
            })
            .collect::<Result<Vec<_>, ToolFailure>>()?;
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        for (name, path) in entries.into_iter().rev() {
            let relative_path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if relative_path.len() > MAX_LEAN_TOOLCHAIN_PATH_BYTES {
                return Err(ToolFailure::ToolchainTooLarge);
            }
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| ToolFailure::Io(error.to_string()))?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                return Err(ToolFailure::ToolchainUnsupported);
            }
            if file_type.is_dir() {
                directories.push((path, relative_path, depth.saturating_add(1)));
                continue;
            }
            if !file_type.is_file() {
                return Err(ToolFailure::ToolchainUnsupported);
            }
            if files.len() >= MAX_LEAN_TOOLCHAIN_FILES {
                return Err(ToolFailure::ToolchainTooLarge);
            }
            declared_bytes = declared_bytes
                .checked_add(metadata.len())
                .ok_or(ToolFailure::ToolchainTooLarge)?;
            if metadata.len() > MAX_TOOL_BINARY_BYTES || declared_bytes > MAX_LEAN_TOOLCHAIN_BYTES {
                return Err(ToolFailure::ToolchainTooLarge);
            }
            #[cfg(unix)]
            let executable = {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let executable = false;
            files.push(ToolchainSourceFile {
                #[cfg(not(unix))]
                source_path: path,
                relative_path,
                executable,
            });
        }
    }
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if files.is_empty() {
        return Err(ToolFailure::ToolchainIncomplete);
    }
    Ok(files)
}

fn hash_toolchain_inventory(files: &[ToolchainFileIdentity], total_bytes: u64) -> Hash32 {
    let mut bytes = b"zeno-fcis/toolchain-tree/1\0".to_vec();
    bytes.extend_from_slice(&u64::try_from(files.len()).unwrap_or(u64::MAX).to_be_bytes());
    bytes.extend_from_slice(&total_bytes.to_be_bytes());
    for file in files {
        bytes.extend_from_slice(
            &u32::try_from(file.relative_path.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        bytes.extend_from_slice(file.relative_path.as_bytes());
        bytes.extend_from_slice(&file.length.to_be_bytes());
        bytes.push(u8::from(file.executable));
        bytes.extend_from_slice(file.sha256.as_bytes());
    }
    RustCryptoSha256::hash(&bytes)
}

fn is_normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
}

fn lean_executable_relative(config: &ToolConfig, runtime: &ToolRuntimeConfig) -> Option<PathBuf> {
    if !is_normal_absolute(&runtime.root) || !is_normal_absolute(&config.path) {
        return None;
    }
    let relative = config.path.strip_prefix(&runtime.root).ok()?;
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }
    Some(relative.to_path_buf())
}

struct PrivateExecutable {
    path: PathBuf,
}
impl PrivateExecutable {
    fn create(backend: ToolBackend, bytes: &[u8]) -> Result<Self, ToolFailure> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let extension = if cfg!(windows) { ".exe" } else { "" };
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-checked-{}-{backend:?}-{sequence}{extension}",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        let result = (|| {
            file.write_all(bytes)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = file.metadata()?.permissions();
                permissions.set_mode(0o700);
                file.set_permissions(permissions)?;
            }
            file.sync_all()
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(&path);
            return Err(ToolFailure::Io(error.to_string()));
        }
        Ok(Self { path })
    }
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for PrivateExecutable {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct PrivateToolchain {
    root: PathBuf,
    executable: PathBuf,
    inventory: ToolchainInventory,
}
impl PrivateToolchain {
    fn create(config: &ToolConfig) -> Result<Self, ToolFailure> {
        let runtime = config
            .runtime
            .as_ref()
            .ok_or(ToolFailure::ToolchainIncomplete)?;
        let relative_executable =
            lean_executable_relative(config, runtime).ok_or(ToolFailure::ToolchainUnsupported)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-checked-Lean-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).map_err(|error| ToolFailure::Io(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
                .map_err(|error| ToolFailure::Io(error.to_string()))?;
        }
        let result = (|| {
            let inventory = snapshot_toolchain(&runtime.root, Some(&root))?;
            if hash_hex(inventory.tree_sha256) != runtime.tree_sha256 {
                return Err(ToolFailure::ToolchainHashMismatch);
            }
            let portable_executable = relative_executable
                .to_str()
                .ok_or(ToolFailure::ToolchainUnsupported)?
                .replace(std::path::MAIN_SEPARATOR, "/");
            let executable_identity = inventory
                .files
                .iter()
                .find(|file| file.relative_path == portable_executable)
                .ok_or(ToolFailure::ToolchainIncomplete)?;
            if !executable_identity.executable {
                return Err(ToolFailure::ToolchainIncomplete);
            }
            if hash_hex(executable_identity.sha256) != config.sha256 {
                return Err(ToolFailure::HashMismatch);
            }
            if !inventory
                .files
                .iter()
                .any(|file| file.relative_path == "lib/lean/Init.olean")
            {
                return Err(ToolFailure::ToolchainIncomplete);
            }
            freeze_private_toolchain(&root, &inventory)?;
            Ok(Self {
                executable: root.join(relative_executable),
                root: root.clone(),
                inventory,
            })
        })();
        if result.is_err() {
            make_private_tree_writable(&root);
            let _ = fs::remove_dir_all(&root);
        }
        result
    }

    fn verify_unchanged(&self) -> Result<(), ToolFailure> {
        if snapshot_toolchain(&self.root, None)? == self.inventory {
            Ok(())
        } else {
            Err(ToolFailure::ToolchainHashMismatch)
        }
    }

    fn refreeze(&self) -> Result<(), ToolFailure> {
        freeze_private_toolchain(&self.root, &self.inventory)
    }
}
impl Drop for PrivateToolchain {
    fn drop(&mut self) {
        make_private_tree_writable(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
fn private_tree_directories(root: &Path) -> Result<Vec<PathBuf>, ToolFailure> {
    let mut pending = vec![root.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(directory) = pending.pop() {
        directories.push(directory.clone());
        for entry in fs::read_dir(&directory).map_err(|error| ToolFailure::Io(error.to_string()))? {
            let path = entry
                .map_err(|error| ToolFailure::Io(error.to_string()))?
                .path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|error| ToolFailure::Io(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(ToolFailure::ToolchainUnsupported);
            }
            if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok(directories)
}

#[cfg(unix)]
fn freeze_private_toolchain(
    root: &Path,
    inventory: &ToolchainInventory,
) -> Result<(), ToolFailure> {
    use std::os::unix::fs::PermissionsExt as _;

    for file in &inventory.files {
        let mode = if file.executable { 0o500 } else { 0o400 };
        fs::set_permissions(
            root.join(&file.relative_path),
            fs::Permissions::from_mode(mode),
        )
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    }
    let mut directories = private_tree_directories(root)?;
    directories.sort_by_key(|path| core::cmp::Reverse(path.components().count()));
    for directory in directories {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o500))
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn freeze_private_toolchain(_: &Path, _: &ToolchainInventory) -> Result<(), ToolFailure> {
    Err(ToolFailure::ProcessContainmentUnavailable)
}

#[cfg(unix)]
fn make_private_tree_writable(root: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    let _ = fs::set_permissions(root, fs::Permissions::from_mode(0o700));
    if let Ok(directories) = private_tree_directories(root) {
        for directory in directories {
            let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o700));
        }
    }
}

#[cfg(not(unix))]
fn make_private_tree_writable(_: &Path) {}

enum CheckedExecution {
    Single(PrivateExecutable),
    Lean(PrivateToolchain),
}
impl CheckedExecution {
    fn executable(&self) -> &Path {
        match self {
            Self::Single(executable) => executable.path(),
            Self::Lean(toolchain) => &toolchain.executable,
        }
    }
    fn inventory(&self) -> Option<&ToolchainInventory> {
        match self {
            Self::Single(_) => None,
            Self::Lean(toolchain) => Some(&toolchain.inventory),
        }
    }
}

struct CheckedTool {
    identity: ToolIdentity,
    execution: CheckedExecution,
}

fn check_tool(config: &ToolConfig) -> Result<CheckedTool, ToolFailure> {
    check_tool_with_max_binary_bytes(config, MAX_TOOL_BINARY_BYTES)
}

fn check_tool_with_max_binary_bytes(
    config: &ToolConfig,
    max_binary_bytes: u64,
) -> Result<CheckedTool, ToolFailure> {
    if config.backend == ToolBackend::Lean {
        return check_lean_tool(config);
    }
    let file = open_untrusted_read(&config.path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ToolFailure::Missing
        } else {
            ToolFailure::Io(error.to_string())
        }
    })?;
    let metadata = file
        .metadata()
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    if !metadata.is_file() {
        return Err(ToolFailure::NotFile);
    }
    if metadata.len() > max_binary_bytes {
        return Err(ToolFailure::BinaryTooLarge);
    }
    let mut bytes = Vec::new();
    file.take(max_binary_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_binary_bytes {
        return Err(ToolFailure::BinaryTooLarge);
    }
    let binary_hash = RustCryptoSha256::hash(&bytes);
    if hash_hex(binary_hash) != config.sha256 {
        return Err(ToolFailure::HashMismatch);
    }
    let executable = PrivateExecutable::create(config.backend, &bytes)?;
    let output = run_fixed(
        executable.path(),
        &["--version"],
        None,
        config.timeout_ms,
        config.max_output_bytes,
    )?;
    if !output.status.success() {
        return Err(ToolFailure::Crash(output.status.code()));
    }
    let version_text = String::from_utf8_lossy(&output.stdout);
    if !version_output_matches(config.backend, &version_text, &config.version) {
        return Err(ToolFailure::VersionMismatch);
    }
    Ok(CheckedTool {
        identity: ToolIdentity {
            backend: config.backend,
            path: config.path.clone(),
            version: config.version.clone(),
            binary_hash,
            runtime_hash: None,
        },
        execution: CheckedExecution::Single(executable),
    })
}

fn check_lean_tool(config: &ToolConfig) -> Result<CheckedTool, ToolFailure> {
    let toolchain = PrivateToolchain::create(config)?;
    let executable_identity = toolchain
        .inventory
        .files
        .iter()
        .find(|file| toolchain.root.join(&file.relative_path) == toolchain.executable)
        .ok_or(ToolFailure::ToolchainIncomplete)?;
    let binary_hash = executable_identity.sha256;
    let runtime_hash = toolchain.inventory.tree_sha256;
    let output = run_fixed(
        &toolchain.executable,
        &["--version"],
        None,
        config.timeout_ms,
        config.max_output_bytes,
    )?;
    if !output.status.success() {
        return Err(ToolFailure::Crash(output.status.code()));
    }
    let version_text = String::from_utf8_lossy(&output.stdout);
    if !version_output_matches(config.backend, &version_text, &config.version) {
        return Err(ToolFailure::VersionMismatch);
    }
    toolchain.verify_unchanged()?;
    toolchain.refreeze()?;
    Ok(CheckedTool {
        identity: ToolIdentity {
            backend: config.backend,
            path: config.path.clone(),
            version: config.version.clone(),
            binary_hash,
            runtime_hash: Some(runtime_hash),
        },
        execution: CheckedExecution::Lean(toolchain),
    })
}

fn version_output_matches(backend: ToolBackend, output: &str, expected: &str) -> bool {
    let first = output.lines().next().unwrap_or("").trim();
    let actual = match backend {
        ToolBackend::Cvc5 => first
            .strip_prefix("This is cvc5 version ")
            .and_then(|rest| rest.split_whitespace().next()),
        ToolBackend::Z3 => first
            .strip_prefix("Z3 version ")
            .and_then(|rest| rest.split_whitespace().next()),
        ToolBackend::Lean => first
            .strip_prefix("Lean (version ")
            .and_then(|rest| rest.split([',', ')']).next()),
    };
    actual == Some(expected)
}

/// Rechecks file type, exact admitted bytes, hash, and version through fixed argv.
pub fn verify_tool(config: &ToolConfig) -> Result<ToolIdentity, ToolFailure> {
    Ok(check_tool(config)?.identity)
}

/// Deterministically exported claim source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedObligation {
    backend: ToolBackend,
    claim_id: StableId,
    claim: ClaimDecl,
    /// The laws an inductive claim's step assumes. Empty for every other claim.
    hypotheses: StepHypotheses,
    source: Vec<u8>,
    source_hash: Hash32,
}
impl ExportedObligation {
    /// Returns the target backend.
    #[must_use]
    pub const fn backend(&self) -> ToolBackend {
        self.backend
    }
    /// Returns the exact claim ID.
    #[must_use]
    pub const fn claim_id(&self) -> StableId {
        self.claim_id
    }
    /// Returns exact generated source bytes.
    #[must_use]
    pub fn source(&self) -> &[u8] {
        &self.source
    }
    /// Returns the content hash.
    #[must_use]
    pub const fn source_hash(&self) -> Hash32 {
        self.source_hash
    }
    /// Returns what a result for this obligation can establish about a system.
    #[must_use]
    pub const fn scope(&self) -> ObligationScope {
        if matches!(self.claim.mode(), ClaimMode::Inductive) {
            ObligationScope::InductiveStepOverLaws
        } else {
            ObligationScope::WithoutSystemModel
        }
    }
    /// Returns the laws this obligation assumes, empty unless it is an
    /// inductive step.
    #[must_use]
    pub const fn hypotheses(&self) -> &StepHypotheses {
        &self.hypotheses
    }
}

/// The laws an inductive step assumes, resolved from the project and grouped
/// by the committing decisions they are enforced on.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StepHypotheses {
    every_commit: Box<[LawDecl]>,
    accepts: Box<[LawDecl]>,
    committed_failures: Box<[LawDecl]>,
}
impl StepHypotheses {
    /// Laws assumed for every committing decision.
    #[must_use]
    pub const fn every_commit(&self) -> &[LawDecl] {
        &self.every_commit
    }
    /// Laws assumed only when the decision is an accept.
    #[must_use]
    pub const fn accepts(&self) -> &[LawDecl] {
        &self.accepts
    }
    /// Laws assumed only when the decision is a committed failure.
    #[must_use]
    pub const fn committed_failures(&self) -> &[LawDecl] {
        &self.committed_failures
    }
    /// Returns true when the step distinguishes accepts from committed failures.
    #[must_use]
    pub fn splits_decisions(&self) -> bool {
        !self.accepts.is_empty() || !self.committed_failures.is_empty()
    }
}

/// Model value of the decision kind in a split inductive step.
const ACCEPT_KIND: i128 = 1;
const COMMITTED_FAILURE_KIND: i128 = 2;
const DECISION_KIND: &str = "zeno_decision_kind";

/// What an exported obligation can establish about a system.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObligationScope {
    /// The obligation contains only the claim over bounded, otherwise
    /// unconstrained observations: no transition relation, initial state,
    /// schema bound, or law. A proof shows the claim holds for every such
    /// assignment, independent of any particular system. A counterexample
    /// assigns values that the system may never produce.
    WithoutSystemModel,
    /// The induction step of an inductive claim: every transition that
    /// satisfies the assumed laws, and starts in a state satisfying the
    /// invariant, ends in one. The laws stand in for the transition relation,
    /// because the authority refuses every decision that breaks a law enforced
    /// on it. The base case and the enforcement of the assumed laws are checked
    /// by the application, not by this obligation.
    InductiveStepOverLaws,
}

impl ObligationScope {
    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::WithoutSystemModel => "without-system-model",
            Self::InductiveStepOverLaws => "inductive-step-over-laws",
        }
    }

    /// Explains what an accepted or refuted result means under this scope.
    #[must_use]
    pub const fn meaning(self, status: &ToolRunStatus) -> Option<&'static str> {
        match (self, status) {
            (
                Self::WithoutSystemModel,
                ToolRunStatus::ProposedUnsat | ToolRunStatus::KernelChecked,
            ) => Some(
                "no transition relation was exported, so this result holds for every bounded observation assignment and says nothing specific about this system",
            ),
            (Self::WithoutSystemModel, ToolRunStatus::Refuted | ToolRunStatus::Undefined(_)) => {
                Some(
                    "no transition relation constrained the observations, so this counterexample may be unreachable in this system",
                )
            }
            (
                Self::InductiveStepOverLaws,
                ToolRunStatus::ProposedUnsat | ToolRunStatus::KernelChecked,
            ) => Some(
                "every transition that satisfies the assumed laws preserves the invariant; it holds on every committed state only when the application also checks the invariant on its exact genesis state and enforces each assumed law on the decisions the claim assumes it on",
            ),
            (Self::InductiveStepOverLaws, ToolRunStatus::Refuted | ToolRunStatus::Undefined(_)) => {
                Some(
                    "a transition that satisfies the assumed laws breaks the invariant; its starting state may be unreachable, so strengthen the invariant or assume more laws",
                )
            }
            _ => None,
        }
    }
}

/// Export failure that grants no evidence.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportError {
    BackendNotSelected,
    UnsupportedMode,
    InvalidFormula,
    ResourceLimit,
    Encode,
    /// An inductive claim assumes a law that was not supplied.
    UnknownAssumedLaw(StableId),
}
impl From<EncodeError> for ExportError {
    fn from(_: EncodeError) -> Self {
        Self::Encode
    }
}

#[derive(Clone, Copy)]
enum ExportKind {
    Smt,
    Lean,
}

enum ExportNode<'a> {
    Rel(&'a RelExpr),
    Value(&'a ValueExpr),
    Temporal(&'a TemporalFormula),
}

fn push_export_node<'a>(
    stack: &mut Vec<(ExportNode<'a>, usize, u64)>,
    node: ExportNode<'a>,
    depth: usize,
    multiplier: u64,
    limits: ExportLimits,
) -> Result<(), ExportError> {
    if stack.len() >= limits.max_formula_nodes() {
        return Err(ExportError::ResourceLimit);
    }
    stack.push((node, depth, multiplier));
    Ok(())
}

fn range_multiplier(
    start: i128,
    end: i128,
    multiplier: u64,
    limits: ExportLimits,
) -> Result<u64, ExportError> {
    let width = end.checked_sub(start).ok_or(ExportError::InvalidFormula)?;
    let width = u64::try_from(width).map_err(|_| ExportError::InvalidFormula)?;
    if width > 4_096 {
        return Err(ExportError::InvalidFormula);
    }
    let expanded = multiplier
        .checked_mul(width)
        .ok_or(ExportError::ResourceLimit)?;
    if expanded > limits.max_operations() {
        return Err(ExportError::ResourceLimit);
    }
    Ok(expanded)
}

fn preflight_export(
    claim: &ClaimDecl,
    kind: ExportKind,
    limits: ExportLimits,
) -> Result<(), ExportError> {
    let (root, multiplier, temporal_width) = match (kind, claim.mode(), claim.formula()) {
        (ExportKind::Smt, ClaimMode::Relational, ClaimFormula::Relational(value)) => {
            (ExportNode::Rel(value), 1, 1)
        }
        (ExportKind::Smt, ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value))
            if horizon > 0 =>
        {
            if horizon > limits.max_horizon() {
                return Err(ExportError::ResourceLimit);
            }
            let width = u64::from(horizon);
            (ExportNode::Temporal(value), width, width)
        }
        (ExportKind::Smt, ClaimMode::UnboundedProof | ClaimMode::Inductive, _) => {
            return Err(ExportError::UnsupportedMode);
        }
        (ExportKind::Lean, ClaimMode::UnboundedProof, ClaimFormula::Temporal(value)) => {
            (ExportNode::Temporal(value), 1, 1)
        }
        (ExportKind::Lean, _, _) => return Err(ExportError::UnsupportedMode),
        _ => return Err(ExportError::InvalidFormula),
    };

    let mut stack = Vec::new();
    stack.push((root, 1usize, multiplier));
    let mut nodes = 0usize;
    let mut operations = 0u64;
    while let Some((node, depth, render_multiplier)) = stack.pop() {
        nodes = nodes.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        if nodes > limits.max_formula_nodes() || depth > limits.max_formula_depth() {
            return Err(ExportError::ResourceLimit);
        }
        operations = operations
            .checked_add(render_multiplier)
            .ok_or(ExportError::ResourceLimit)?;
        if operations > limits.max_operations() {
            return Err(ExportError::ResourceLimit);
        }
        let next_depth = depth.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        match node {
            ExportNode::Rel(value) => match value {
                RelExpr::Bool(_) => {}
                RelExpr::Not(value) => push_export_node(
                    &mut stack,
                    ExportNode::Rel(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                RelExpr::And(left, right)
                | RelExpr::Or(left, right)
                | RelExpr::Implies(left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                RelExpr::Compare(_, left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                RelExpr::Predicate { arguments, .. } => {
                    for argument in arguments {
                        push_export_node(
                            &mut stack,
                            ExportNode::Value(argument),
                            next_depth,
                            render_multiplier,
                            limits,
                        )?;
                    }
                }
                RelExpr::ForAll {
                    start, end, body, ..
                }
                | RelExpr::Exists {
                    start, end, body, ..
                } => {
                    let expanded = range_multiplier(*start, *end, render_multiplier, limits)?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(body),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
            ExportNode::Value(value) => match value {
                ValueExpr::Int(_) | ValueExpr::Var(_) | ValueExpr::Projection(_) => {}
                ValueExpr::Add(left, right)
                | ValueExpr::Sub(left, right)
                | ValueExpr::Mul(left, right)
                | ValueExpr::Div(_, left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                ValueExpr::Sum {
                    start, end, body, ..
                } => {
                    let expanded = range_multiplier(*start, *end, render_multiplier, limits)?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(body),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
            ExportNode::Temporal(value) => match value {
                TemporalFormula::Atom(value) => push_export_node(
                    &mut stack,
                    ExportNode::Rel(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                TemporalFormula::Not(value) | TemporalFormula::Next(value) => push_export_node(
                    &mut stack,
                    ExportNode::Temporal(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                TemporalFormula::Always(value) | TemporalFormula::Eventually(value) => {
                    let expanded = render_multiplier
                        .checked_mul(temporal_width)
                        .ok_or(ExportError::ResourceLimit)?;
                    if expanded > limits.max_operations() {
                        return Err(ExportError::ResourceLimit);
                    }
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(value),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
                TemporalFormula::And(left, right) | TemporalFormula::Or(left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                TemporalFormula::Until(left, right) => {
                    let expanded = render_multiplier
                        .checked_mul(temporal_width)
                        .ok_or(ExportError::ResourceLimit)?;
                    if expanded > limits.max_operations() {
                        return Err(ExportError::ResourceLimit);
                    }
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(left),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(right),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
        }
    }
    Ok(())
}

/// Exports bounded relational or finite temporal claims to deterministic SMT-LIB.
pub fn export_smt(
    claim: &ClaimDecl,
    backend: ToolBackend,
) -> Result<ExportedObligation, ExportError> {
    export_smt_with_limits(claim, backend, ExportLimits::default())
}

/// Exports one SMT obligation within an explicit deterministic resource envelope.
pub fn export_smt_with_limits(
    claim: &ClaimDecl,
    backend: ToolBackend,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !matches!(backend, ToolBackend::Cvc5 | ToolBackend::Z3)
        || !claim.backends().contains(&backend.spec_backend())
    {
        return Err(ExportError::BackendNotSelected);
    }
    preflight_export(claim, ExportKind::Smt, limits)?;
    let empty_environment = BTreeMap::new();
    let mut budget = SmtRenderBudget::new(limits);
    let (horizon, finite, formula) = match (claim.mode(), claim.formula()) {
        (ClaimMode::Relational, ClaimFormula::Relational(value)) => (
            1,
            false,
            render_rel_smt(value, 0, &empty_environment, &mut budget)?,
        ),
        (ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value)) if horizon > 0 => {
            let mut formulas = Vec::new();
            for length in 1..=horizon {
                formulas.push(render_temporal_smt(
                    value,
                    0,
                    length,
                    &empty_environment,
                    &mut budget,
                )?);
            }
            (horizon, true, select_trace_length(formulas, &mut budget)?)
        }
        (ClaimMode::UnboundedProof | ClaimMode::Inductive, _) => {
            return Err(ExportError::UnsupportedMode);
        }
        _ => return Err(ExportError::InvalidFormula),
    };
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    collect_claim(claim, &mut paths, &mut predicates);
    let mut source = format!(
        "; zeno-fcis/smt-obligation/1\n; claim-id {}\n(set-logic ALL)\n(set-option :produce-models true)\n",
        claim.id().get()
    );
    if backend == ToolBackend::Cvc5 {
        source.push_str("(set-option :produce-proofs true)\n");
    }
    if finite {
        source.push_str("(declare-const zeno_trace_len Int)\n");
        source.push_str(&format!(
            "(assert (and (<= 1 zeno_trace_len) (<= zeno_trace_len {horizon})))\n"
        ));
    }
    for step in 0..horizon {
        for path in &paths {
            let name = smt_path(path, step);
            source.push_str(&format!("(declare-const {name} Int)\n"));
            source.push_str(&format!("(assert {})\n", smt_i128_range(&name)));
        }
    }
    for (name, arity) in predicates {
        source.push_str(&format!(
            "(declare-fun pred_{} ({}) Bool)\n",
            smt_identifier(name.as_str()),
            vec!["Int"; arity].join(" ")
        ));
    }
    source.push_str(&format!(
        "(assert (not {}))
(check-sat)
",
        smt_and(vec![formula.defined, formula.term])
    ));
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    exported(backend, claim, source.into_bytes())
}

/// Exports the induction step of an inductive claim to SMT.
///
/// `laws` must contain every law the claim assumes; other laws are ignored.
/// The obligation is satisfiable exactly when some assignment makes every
/// assumed law and the invariant over `pre.` evaluate to true, and the
/// invariant over `post.` not evaluate to true. Evaluation is strict, so each
/// part is required to be defined on its own, as the law engine requires of
/// each law it reports satisfied. An undefined part is never hidden inside a
/// larger implication.
pub fn export_inductive_smt(
    claim: &ClaimDecl,
    laws: &[LawDecl],
    backend: ToolBackend,
) -> Result<ExportedObligation, ExportError> {
    export_inductive_smt_with_limits(claim, laws, backend, ExportLimits::default())
}

/// Exports an inductive step within an explicit deterministic resource envelope.
pub fn export_inductive_smt_with_limits(
    claim: &ClaimDecl,
    laws: &[LawDecl],
    backend: ToolBackend,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !matches!(backend, ToolBackend::Cvc5 | ToolBackend::Z3)
        || !claim.backends().contains(&backend.spec_backend())
    {
        return Err(ExportError::BackendNotSelected);
    }
    let (ClaimMode::Inductive, ClaimFormula::Relational(invariant)) =
        (claim.mode(), claim.formula())
    else {
        return Err(ExportError::UnsupportedMode);
    };
    let hypotheses = step_hypotheses(claim, laws)?;
    let after = invariant_at(invariant, ProjectionRoot::Post).ok_or(ExportError::InvalidFormula)?;
    let grouped: Vec<(&LawDecl, Option<i128>)> = hypotheses
        .every_commit
        .iter()
        .map(|law| (law, None))
        .chain(
            hypotheses
                .accepts
                .iter()
                .map(|law| (law, Some(ACCEPT_KIND))),
        )
        .chain(
            hypotheses
                .committed_failures
                .iter()
                .map(|law| (law, Some(COMMITTED_FAILURE_KIND))),
        )
        .collect();
    let parts: Vec<&RelExpr> = grouped
        .iter()
        .map(|(law, _)| law.formula())
        .chain([invariant, &after])
        .collect();
    for part in &parts {
        let single = ClaimDecl::new(
            claim.id(),
            claim.name().clone(),
            claim.backends().to_vec(),
            ClaimMode::Relational,
            ClaimFormula::Relational((*part).clone()),
        );
        preflight_export(&single, ExportKind::Smt, limits)?;
    }
    let empty_environment = BTreeMap::new();
    let mut budget = SmtRenderBudget::new(limits);
    let mut rendered = Vec::with_capacity(parts.len());
    for part in &parts {
        rendered.push(render_rel_smt(part, 0, &empty_environment, &mut budget)?);
    }
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    for part in &parts {
        collect_rel(part, &mut paths, &mut predicates);
    }
    let ids = |group: &[StableId]| {
        group
            .iter()
            .map(|law| law.get().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let assumptions = claim.assumptions();
    let mut source = format!(
        "; zeno-fcis/smt-inductive-obligation/1\n; claim-id {}\n; every commit: {}\n; accepts: {}\n; committed failures: {}\n(set-logic ALL)\n(set-option :produce-models true)\n",
        claim.id().get(),
        ids(assumptions.every_commit()),
        ids(assumptions.accepts()),
        ids(assumptions.committed_failures()),
    );
    if backend == ToolBackend::Cvc5 {
        source.push_str("(set-option :produce-proofs true)\n");
    }
    for path in &paths {
        let name = smt_path(path, 0);
        source.push_str(&format!("(declare-const {name} Int)\n"));
        source.push_str(&format!("(assert {})\n", smt_i128_range(&name)));
    }
    for (name, arity) in predicates {
        source.push_str(&format!(
            "(declare-fun pred_{} ({}) Bool)\n",
            smt_identifier(name.as_str()),
            vec!["Int"; arity].join(" ")
        ));
    }
    if hypotheses.splits_decisions() {
        source.push_str(&format!(
            "; decision kind: {ACCEPT_KIND} accept, {COMMITTED_FAILURE_KIND} committed failure\n(declare-const {DECISION_KIND} Int)\n(assert (or (= {DECISION_KIND} {ACCEPT_KIND}) (= {DECISION_KIND} {COMMITTED_FAILURE_KIND})))\n"
        ));
    }
    let Some((after, before_and_laws)) = rendered.split_last() else {
        return Err(ExportError::InvalidFormula);
    };
    for (index, part) in before_and_laws.iter().enumerate() {
        let holds = smt_and(vec![part.defined.clone(), part.term.clone()]);
        let (label, assertion) = match grouped.get(index) {
            Some((law, None)) => (format!("law {}", law.id().get()), holds),
            Some((law, Some(kind))) => (
                format!(
                    "law {} on {}",
                    law.id().get(),
                    if *kind == ACCEPT_KIND {
                        "accepts"
                    } else {
                        "committed failures"
                    }
                ),
                format!("(=> (= {DECISION_KIND} {kind}) {holds})"),
            ),
            None => ("invariant before the step".to_owned(), holds),
        };
        source.push_str(&format!("; {label}\n(assert {assertion})\n"));
    }
    source.push_str(&format!(
        "; invariant after the step\n(assert (not {}))\n(check-sat)\n",
        smt_and(vec![after.defined.clone(), after.term.clone()])
    ));
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    let mut obligation = exported(backend, claim, source.into_bytes())?;
    obligation.hypotheses = hypotheses;
    Ok(obligation)
}

/// Resolves the laws an inductive claim assumes, group by group.
fn step_hypotheses(claim: &ClaimDecl, laws: &[LawDecl]) -> Result<StepHypotheses, ExportError> {
    let assumptions = claim.assumptions();
    if assumptions.is_empty() {
        return Err(ExportError::InvalidFormula);
    }
    let mut seen = BTreeSet::new();
    let mut resolve = |group: &[StableId]| -> Result<Box<[LawDecl]>, ExportError> {
        let mut resolved = Vec::with_capacity(group.len());
        for id in group {
            if !seen.insert(*id) {
                return Err(ExportError::InvalidFormula);
            }
            let law = laws
                .iter()
                .find(|law| law.id() == *id)
                .ok_or(ExportError::UnknownAssumedLaw(*id))?;
            resolved.push(law.clone());
        }
        Ok(resolved.into_boxed_slice())
    };
    Ok(StepHypotheses {
        every_commit: resolve(assumptions.every_commit())?,
        accepts: resolve(assumptions.accepts())?,
        committed_failures: resolve(assumptions.committed_failures())?,
    })
}

/// Exports an unbounded temporal obligation to Lean source.
pub fn export_lean(claim: &ClaimDecl) -> Result<ExportedObligation, ExportError> {
    export_lean_with_limits(claim, ExportLimits::default())
}

/// Exports one Lean obligation within an explicit deterministic resource envelope.
pub fn export_lean_with_limits(
    claim: &ClaimDecl,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !claim.backends().contains(&BackendId::Lean) {
        return Err(ExportError::BackendNotSelected);
    }
    if !matches!(claim.mode(), ClaimMode::UnboundedProof) {
        return Err(ExportError::UnsupportedMode);
    }
    preflight_export(claim, ExportKind::Lean, limits)?;
    let empty_environment = BTreeMap::new();
    let mut budget = LeanRenderBudget::new(limits);
    let formula = match claim.formula() {
        ClaimFormula::Temporal(value) => {
            render_temporal_lean(value, "0", &empty_environment, &mut budget)?
        }
        ClaimFormula::Relational(_) => return Err(ExportError::InvalidFormula),
    };
    let proposition = lean_and(vec![formula.defined, formula.term]);
    let claim_id = claim.id().get();
    let source = format!(
        "-- zeno-fcis/lean-obligation/1\n-- claim-id {claim_id}\nnamespace ZenoFCIS\n\n\
def i128Min : Int := {}\n\
def i128Max : Int := {}\n\
def inI128 (value : Int) : Prop := i128Min <= value ∧ value <= i128Max\n\
def I128 : Type := {{ value : Int // inI128 value }}\n\
def floorDiv (left right : Int) : Int :=\n  if right > 0 then left / right else (-left) / (-right)\n\
def ceilDiv (left right : Int) : Int :=\n  -(floorDiv (-left) right)\n\n\
variable (observe : String → Nat → I128)\n\
variable (predicate : String → List Int → Prop)\n\n\
def claim_{claim_id}\n\
    (observe : String → Nat → I128)\n\
    (predicate : String → List Int → Prop) : Prop :=\n  {proposition}\n\n\
theorem claim_{claim_id}_checked : claim_{claim_id} observe predicate := by\n\
\x20\x20simp [claim_{claim_id}, floorDiv, ceilDiv, inI128, i128Min, i128Max]\n\n\
#print axioms claim_{claim_id}_checked\n\n\
end ZenoFCIS\n",
        lean_int(i128::MIN),
        lean_int(i128::MAX),
    );
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    exported(ToolBackend::Lean, claim, source.into_bytes())
}

fn exported(
    backend: ToolBackend,
    claim: &ClaimDecl,
    source: Vec<u8>,
) -> Result<ExportedObligation, ExportError> {
    let source_hash =
        commitment::<RustCryptoSha256>(Domain::new("zeno-fcis/formal-source", 1)?, &source)?;
    Ok(ExportedObligation {
        backend,
        claim_id: claim.id(),
        claim: claim.clone(),
        hypotheses: StepHypotheses::default(),
        source,
        source_hash,
    })
}

/// Evidence-retention proposal classification. No variant grants production authority.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolRunStatus {
    ProposedUnsat,
    KernelChecked,
    Refuted,
    /// A replayed model at which the claim has no value. Evaluation is strict,
    /// so the claim does not hold there, exactly as the solver reported.
    Undefined(UndefinedReason),
    Blocked(ToolFailure),
    Failed(ToolFailure),
}

/// Why a claim has no value at a replayed assignment.
///
/// These are exactly the failures the SMT encoding models as undefinedness.
/// Evaluation limits and missing projections or predicates are not modeled,
/// so a model that reaches one of them is never confirmed by replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UndefinedReason {
    /// An arithmetic result lies outside the i128 range.
    Overflow,
    /// A division has a zero divisor.
    DivisionByZero,
    /// An exact division leaves a remainder.
    NonExactDivision,
}

impl UndefinedReason {
    /// Returns the stable machine-readable name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Overflow => "overflow",
            Self::DivisionByZero => "division_by_zero",
            Self::NonExactDivision => "non_exact_division",
        }
    }

    const fn code(self) -> u8 {
        match self {
            Self::Overflow => 1,
            Self::DivisionByZero => 2,
            Self::NonExactDivision => 3,
        }
    }

    const fn from_indeterminate(reason: IndeterminateReason) -> Option<Self> {
        match reason {
            IndeterminateReason::Overflow => Some(Self::Overflow),
            IndeterminateReason::DivisionByZero => Some(Self::DivisionByZero),
            IndeterminateReason::NonExactDivision => Some(Self::NonExactDivision),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum ProcessPhase {
    Decision = 1,
    Evidence = 2,
    Kernel = 3,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessTranscript {
    phase: ProcessPhase,
    input: Vec<u8>,
    output: ProcessOutput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ToolExecution {
    transcripts: Vec<ProcessTranscript>,
    inconsistent: bool,
}
impl ToolExecution {
    fn single(phase: ProcessPhase, input: &[u8], output: ProcessOutput) -> Self {
        Self {
            transcripts: vec![ProcessTranscript {
                phase,
                input: input.to_vec(),
                output,
            }],
            inconsistent: false,
        }
    }

    fn final_output(&self) -> &ProcessOutput {
        &self
            .transcripts
            .last()
            .unwrap_or_else(|| unreachable!("tool execution always has a transcript"))
            .output
    }
}

/// Complete bounded process record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolRun {
    identity: ToolIdentity,
    obligation: ExportedObligation,
    status: ToolRunStatus,
    transcripts: Vec<ProcessTranscript>,
    toolchain_inventory: Option<ToolchainInventory>,
    record_hash: Hash32,
}
impl ToolRun {
    /// Returns the checked identity.
    #[must_use]
    pub const fn identity(&self) -> &ToolIdentity {
        &self.identity
    }
    /// Returns the exact obligation.
    #[must_use]
    pub const fn obligation(&self) -> &ExportedObligation {
        &self.obligation
    }
    /// Returns the fail-closed status.
    #[must_use]
    pub const fn status(&self) -> &ToolRunStatus {
        &self.status
    }
    /// Returns bounded standard output.
    #[must_use]
    pub fn stdout(&self) -> &[u8] {
        &self
            .transcripts
            .last()
            .unwrap_or_else(|| unreachable!("tool run always has a transcript"))
            .output
            .stdout
    }
    /// Returns bounded standard error.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self
            .transcripts
            .last()
            .unwrap_or_else(|| unreachable!("tool run always has a transcript"))
            .output
            .stderr
    }
    /// Returns the retained Lean runtime inventory, when present.
    #[must_use]
    pub const fn toolchain_inventory(&self) -> Option<&ToolchainInventory> {
        self.toolchain_inventory.as_ref()
    }
    /// Returns the content-addressed record ID.
    #[must_use]
    pub const fn record_hash(&self) -> Hash32 {
        self.record_hash
    }
}

fn append_record_field(output: &mut Vec<u8>, tag: u8, value: &[u8]) -> Result<(), ToolFailure> {
    let length = u64::try_from(value.len())
        .map_err(|_| ToolFailure::Io("formal-run record field exceeds u64".into()))?;
    output.push(tag);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    Ok(())
}

const fn backend_record_tag(backend: ToolBackend) -> u8 {
    match backend {
        ToolBackend::Cvc5 => 1,
        ToolBackend::Z3 => 2,
        ToolBackend::Lean => 3,
    }
}

fn exit_status_record(status: &ExitStatus) -> Vec<u8> {
    if let Some(code) = status.code() {
        let mut bytes = vec![1];
        bytes.extend_from_slice(&code.to_be_bytes());
        return bytes;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            let mut bytes = vec![2];
            bytes.extend_from_slice(&signal.to_be_bytes());
            return bytes;
        }
    }
    vec![0]
}

fn failure_record(failure: &ToolFailure) -> Result<Vec<u8>, ToolFailure> {
    let (tag, detail): (u8, Option<Vec<u8>>) = match failure {
        ToolFailure::Missing => (1, None),
        ToolFailure::NotFile => (2, None),
        ToolFailure::BinaryTooLarge => (3, None),
        ToolFailure::ToolchainTooLarge => (4, None),
        ToolFailure::ToolchainUnsupported => (5, None),
        ToolFailure::ToolchainIncomplete => (6, None),
        ToolFailure::ToolchainHashMismatch => (7, None),
        ToolFailure::HashMismatch => (8, None),
        ToolFailure::VersionMismatch => (9, None),
        ToolFailure::Timeout => (10, None),
        ToolFailure::Crash(code) => {
            let mut bytes = vec![u8::from(code.is_some())];
            if let Some(code) = code {
                bytes.extend_from_slice(&code.to_be_bytes());
            }
            (11, Some(bytes))
        }
        ToolFailure::OutputLimit => (12, None),
        ToolFailure::ProcessContainmentUnavailable => (13, None),
        ToolFailure::ProcessContainmentFailed => (14, None),
        ToolFailure::InconsistentResult => (15, None),
        ToolFailure::RetentionConflict => (16, None),
        ToolFailure::Unknown => (17, None),
        ToolFailure::UnsupportedEvidence => (18, None),
        ToolFailure::ModelReplayFailed => (19, None),
        ToolFailure::LeanAxiomReport => (20, None),
        ToolFailure::Io(message) => (21, Some(message.as_bytes().to_vec())),
    };
    let mut bytes = vec![tag];
    if let Some(detail) = detail {
        append_record_field(&mut bytes, 1, &detail)?;
    }
    Ok(bytes)
}

fn status_record(status: &ToolRunStatus) -> Result<Vec<u8>, ToolFailure> {
    let (tag, failure) = match status {
        ToolRunStatus::ProposedUnsat => (1, None),
        ToolRunStatus::KernelChecked => (2, None),
        ToolRunStatus::Refuted => (3, None),
        ToolRunStatus::Blocked(failure) => (4, Some(failure)),
        ToolRunStatus::Failed(failure) => (5, Some(failure)),
        ToolRunStatus::Undefined(reason) => {
            let mut bytes = vec![6];
            append_record_field(&mut bytes, 1, &[reason.code()])?;
            return Ok(bytes);
        }
    };
    let mut bytes = vec![tag];
    if let Some(failure) = failure {
        append_record_field(&mut bytes, 1, &failure_record(failure)?)?;
    }
    Ok(bytes)
}

fn formal_run_record(
    identity: &ToolIdentity,
    obligation: &ExportedObligation,
    status: &ToolRunStatus,
    transcripts: &[ProcessTranscript],
) -> Result<Vec<u8>, ToolFailure> {
    let mut record = b"zeno-fcis/formal-run-record/3\0".to_vec();
    append_record_field(&mut record, 1, &[backend_record_tag(identity.backend)])?;
    append_record_field(&mut record, 2, identity.version.as_bytes())?;
    append_record_field(&mut record, 3, identity.binary_hash.as_bytes())?;
    let mut runtime = vec![u8::from(identity.runtime_hash.is_some())];
    if let Some(runtime_hash) = identity.runtime_hash {
        runtime.extend_from_slice(runtime_hash.as_bytes());
    }
    append_record_field(&mut record, 4, &runtime)?;
    append_record_field(&mut record, 5, &obligation.claim_id.get().to_be_bytes())?;
    append_record_field(&mut record, 6, obligation.source_hash.as_bytes())?;
    append_record_field(&mut record, 7, obligation.source())?;
    append_record_field(
        &mut record,
        8,
        &u64::try_from(transcripts.len())
            .map_err(|_| ToolFailure::Io("too many formal-run transcripts".into()))?
            .to_be_bytes(),
    )?;
    for transcript in transcripts {
        let mut phase = Vec::new();
        append_record_field(&mut phase, 1, &[transcript.phase as u8])?;
        append_record_field(&mut phase, 2, &transcript.input)?;
        append_record_field(
            &mut phase,
            3,
            &exit_status_record(&transcript.output.status),
        )?;
        append_record_field(&mut phase, 4, &transcript.output.stdout)?;
        append_record_field(&mut phase, 5, &transcript.output.stderr)?;
        append_record_field(&mut record, 9, &phase)?;
    }
    append_record_field(&mut record, 10, &status_record(status)?)?;
    Ok(record)
}

/// Executes exact generated source using only backend-fixed argv arrays.
pub fn execute_tool(
    config: &ToolConfig,
    obligation: ExportedObligation,
) -> Result<ToolRun, ToolFailure> {
    if config.backend != obligation.backend {
        return Err(ToolFailure::UnsupportedEvidence);
    }
    let checked = check_tool(config)?;
    let execution = match config.backend {
        ToolBackend::Cvc5 | ToolBackend::Z3 => {
            run_smt(config, checked.execution.executable(), &obligation.source)?
        }
        ToolBackend::Lean => ToolExecution::single(
            ProcessPhase::Kernel,
            &obligation.source,
            run_lean(config, checked.execution.executable(), &obligation.source)?,
        ),
    };
    if let CheckedExecution::Lean(toolchain) = &checked.execution {
        toolchain.verify_unchanged()?;
        toolchain.refreeze()?;
    }
    let toolchain_inventory = checked.execution.inventory().cloned();
    let identity = checked.identity;
    let status = if execution.inconsistent {
        ToolRunStatus::Blocked(ToolFailure::InconsistentResult)
    } else {
        classify(config, execution.final_output(), &obligation)
    };
    let record = formal_run_record(&identity, &obligation, &status, &execution.transcripts)?;
    let record_hash = commitment::<RustCryptoSha256>(
        Domain::new("zeno-fcis/formal-run", 3)
            .map_err(|error| ToolFailure::Io(error.to_string()))?,
        &record,
    )
    .map_err(|error| ToolFailure::Io(error.to_string()))?;
    Ok(ToolRun {
        identity,
        obligation,
        status,
        transcripts: execution.transcripts,
        toolchain_inventory,
        record_hash,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SolverResult {
    Sat,
    Unsat,
}

fn solver_result(output: &ProcessOutput) -> Option<SolverResult> {
    let text = String::from_utf8_lossy(&output.stdout);
    match text.lines().find(|line| !line.trim().is_empty())?.trim() {
        "sat" => Some(SolverResult::Sat),
        "unsat" => Some(SolverResult::Unsat),
        _ => None,
    }
}

fn run_smt(
    config: &ToolConfig,
    executable: &Path,
    source: &[u8],
) -> Result<ToolExecution, ToolFailure> {
    let args: &[&str] = match config.backend {
        ToolBackend::Cvc5 => &["--safe-mode=safe", "--lang=smt2", "--produce-proofs"],
        ToolBackend::Z3 => &["-in", "-smt2"],
        ToolBackend::Lean => return Err(ToolFailure::UnsupportedEvidence),
    };
    let first = run_fixed(
        executable,
        args,
        Some(source),
        config.timeout_ms,
        config.max_output_bytes,
    )?;
    let first_result = solver_result(&first);
    let first = ProcessTranscript {
        phase: ProcessPhase::Decision,
        input: source.to_vec(),
        output: first,
    };
    if !first.output.status.success() {
        return Ok(ToolExecution {
            transcripts: vec![first],
            inconsistent: false,
        });
    }
    let request = match (config.backend, first_result) {
        (ToolBackend::Cvc5, Some(SolverResult::Unsat)) => b"(get-proof)\n".as_slice(),
        (ToolBackend::Cvc5 | ToolBackend::Z3, Some(SolverResult::Sat)) => {
            b"(get-model)\n".as_slice()
        }
        _ => {
            return Ok(ToolExecution {
                transcripts: vec![first],
                inconsistent: false,
            });
        }
    };
    let mut followup = source.to_vec();
    followup.extend_from_slice(request);
    let second = run_fixed(
        executable,
        args,
        Some(&followup),
        config.timeout_ms,
        config.max_output_bytes,
    )?;
    let second_result = solver_result(&second);
    let inconsistent = matches!(
        (first_result, second_result),
        (Some(SolverResult::Sat), Some(SolverResult::Unsat))
            | (Some(SolverResult::Unsat), Some(SolverResult::Sat))
    );
    Ok(ToolExecution {
        transcripts: vec![
            first,
            ProcessTranscript {
                phase: ProcessPhase::Evidence,
                input: followup,
                output: second,
            },
        ],
        inconsistent,
    })
}

struct MissingPredicates;
impl PredicateProvider for MissingPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

fn replay_model(obligation: &ExportedObligation, text: &str) -> ToolRunStatus {
    let assignments = parse_model_values(text);
    if matches!(obligation.claim.mode(), ClaimMode::Inductive) {
        return replay_inductive(obligation, &assignments);
    }
    let trace_len = match obligation.claim.mode() {
        ClaimMode::Relational => 1,
        ClaimMode::Finite { horizon } if horizon > 0 => {
            let Some(length) = assignments
                .get("zeno_trace_len")
                .copied()
                .and_then(|value| u32::try_from(value).ok())
                .filter(|length| *length > 0 && *length <= horizon)
            else {
                return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
            };
            length
        }
        _ => return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
    };
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    collect_claim(&obligation.claim, &mut paths, &mut predicates);
    if !predicates.is_empty() {
        return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
    }
    let mut trace = Vec::new();
    for step in 0..trace_len {
        let mut observations = Vec::new();
        for path in &paths {
            let name = smt_path(path, step);
            let Some(value) = assignments.get(&name).copied() else {
                return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
            };
            observations.push(Observation::new(path.clone(), value));
        }
        let Some(event) = TraceStep::try_new(observations) else {
            return ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
        };
        trace.push(event);
    }
    match obligation.claim.formula() {
        ClaimFormula::Relational(formula) => match evaluate_relational(
            formula,
            EvaluationContext::new(&trace[0], &MissingPredicates, EvalLimits::default()),
        ) {
            EvalOutcome::False => ToolRunStatus::Refuted,
            EvalOutcome::Indeterminate(reason) => undefined_or_unreplayed(reason),
            EvalOutcome::True => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
        },
        ClaimFormula::Temporal(formula) => match evaluate_temporal(
            formula,
            obligation.claim.mode(),
            &trace,
            &MissingPredicates,
            EvalLimits::default(),
        ) {
            TemporalEvaluation::Counterexample { .. } => ToolRunStatus::Refuted,
            TemporalEvaluation::Indeterminate(reason) => undefined_or_unreplayed(reason),
            TemporalEvaluation::Satisfied | TemporalEvaluation::ProofObligation => {
                ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
            }
        },
    }
}

/// Replays a model of an inductive step on one transition. The model is
/// confirmed only when every assumed law and the invariant before the step
/// evaluate to true, exactly as the law engine and the induction hypothesis
/// require, and the invariant after the step does not.
fn replay_inductive(
    obligation: &ExportedObligation,
    assignments: &BTreeMap<String, i128>,
) -> ToolRunStatus {
    let unreplayed = ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed);
    let ClaimFormula::Relational(before) = obligation.claim.formula() else {
        return unreplayed;
    };
    let Some(after) = invariant_at(before, ProjectionRoot::Post) else {
        return unreplayed;
    };
    let hypotheses = &obligation.hypotheses;
    let case_laws = if hypotheses.splits_decisions() {
        match assignments.get(DECISION_KIND).copied() {
            Some(ACCEPT_KIND) => &hypotheses.accepts,
            Some(COMMITTED_FAILURE_KIND) => &hypotheses.committed_failures,
            _ => return unreplayed,
        }
    } else {
        &hypotheses.accepts
    };
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    for law in hypotheses
        .every_commit
        .iter()
        .chain(hypotheses.accepts.iter())
        .chain(hypotheses.committed_failures.iter())
    {
        collect_rel(law.formula(), &mut paths, &mut predicates);
    }
    collect_rel(before, &mut paths, &mut predicates);
    collect_rel(&after, &mut paths, &mut predicates);
    if !predicates.is_empty() {
        return unreplayed;
    }
    let mut observations = Vec::with_capacity(paths.len());
    for path in &paths {
        let Some(value) = assignments.get(&smt_path(path, 0)).copied() else {
            return unreplayed;
        };
        observations.push(Observation::new(path.clone(), value));
    }
    let Some(transition) = TraceStep::try_new(observations) else {
        return unreplayed;
    };
    let evaluate = |formula: &RelExpr| {
        evaluate_relational(
            formula,
            EvaluationContext::new(&transition, &MissingPredicates, EvalLimits::default()),
        )
    };
    let assumptions_hold = hypotheses
        .every_commit
        .iter()
        .chain(case_laws.iter())
        .all(|law| evaluate(law.formula()) == EvalOutcome::True);
    if !assumptions_hold || evaluate(before) != EvalOutcome::True {
        return unreplayed;
    }
    match evaluate(&after) {
        EvalOutcome::False => ToolRunStatus::Refuted,
        EvalOutcome::Indeterminate(reason) => undefined_or_unreplayed(reason),
        EvalOutcome::True => unreplayed,
    }
}

/// Confirms a model at which strict evaluation has no value, when the reason is
/// one the SMT encoding models as undefinedness. Any other reason means the
/// model and the evaluator disagree, so the model is not replayed.
const fn undefined_or_unreplayed(reason: IndeterminateReason) -> ToolRunStatus {
    match UndefinedReason::from_indeterminate(reason) {
        Some(reason) => ToolRunStatus::Undefined(reason),
        None => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
    }
}

fn parse_model_values(text: &str) -> BTreeMap<String, i128> {
    let normalized: String = text
        .chars()
        .map(|character| {
            if matches!(character, '(' | ')') {
                ' '
            } else {
                character
            }
        })
        .collect();
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    let mut values = BTreeMap::new();
    let mut index = 0usize;
    while index + 3 < tokens.len() {
        if tokens[index] == "define-fun" && tokens[index + 2] == "Int" {
            let parsed = if tokens[index + 3] == "-" && index + 4 < tokens.len() {
                tokens[index + 4]
                    .parse::<i128>()
                    .ok()
                    .and_then(i128::checked_neg)
            } else {
                tokens[index + 3].parse::<i128>().ok()
            };
            if let Some(value) = parsed {
                values.insert(tokens[index + 1].to_owned(), value);
            }
        }
        index = index.saturating_add(1);
    }
    values
}

fn classify(
    config: &ToolConfig,
    output: &ProcessOutput,
    obligation: &ExportedObligation,
) -> ToolRunStatus {
    if !output.status.success() {
        return ToolRunStatus::Failed(ToolFailure::Crash(output.status.code()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    match config.backend {
        ToolBackend::Cvc5 => {
            let first = text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
                .trim();
            match first {
                "unsat" if text.contains("(step") => ToolRunStatus::ProposedUnsat,
                "sat" => replay_model(obligation, &text),
                "unknown" => ToolRunStatus::Blocked(ToolFailure::Unknown),
                _ => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
            }
        }
        ToolBackend::Z3 => {
            let first = text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
                .trim();
            match first {
                "sat" => replay_model(obligation, &text),
                "unsat" => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
                "unknown" => ToolRunStatus::Blocked(ToolFailure::Unknown),
                _ => ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
            }
        }
        ToolBackend::Lean
            if config
                .runtime
                .as_ref()
                .is_none_or(|runtime| runtime.tree_sha256 != LEAN_LINUX_X86_64_TREE_SHA256) =>
        {
            ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
        }
        ToolBackend::Lean => match parse_lean_axioms(&text) {
            Some(axioms) if axioms == config.allowed_axioms => ToolRunStatus::KernelChecked,
            _ => ToolRunStatus::Blocked(ToolFailure::LeanAxiomReport),
        },
    }
}

fn run_status_name(status: &ToolRunStatus) -> &'static str {
    match status {
        ToolRunStatus::ProposedUnsat => "proposed_unsat",
        ToolRunStatus::KernelChecked => "kernel_checked",
        ToolRunStatus::Refuted => "refuted",
        ToolRunStatus::Undefined(_) => "undefined",
        ToolRunStatus::Blocked(_) => "blocked",
        ToolRunStatus::Failed(_) => "failed",
    }
}

fn failure_name(failure: &ToolFailure) -> &'static str {
    match failure {
        ToolFailure::Missing => "missing",
        ToolFailure::NotFile => "not_file",
        ToolFailure::BinaryTooLarge => "binary_too_large",
        ToolFailure::ToolchainTooLarge => "toolchain_too_large",
        ToolFailure::ToolchainUnsupported => "toolchain_unsupported",
        ToolFailure::ToolchainIncomplete => "toolchain_incomplete",
        ToolFailure::ToolchainHashMismatch => "toolchain_hash_mismatch",
        ToolFailure::HashMismatch => "hash_mismatch",
        ToolFailure::VersionMismatch => "version_mismatch",
        ToolFailure::Timeout => "timeout",
        ToolFailure::Crash(_) => "crash",
        ToolFailure::OutputLimit => "output_limit",
        ToolFailure::ProcessContainmentUnavailable => "process_containment_unavailable",
        ToolFailure::ProcessContainmentFailed => "process_containment_failed",
        ToolFailure::InconsistentResult => "inconsistent_result",
        ToolFailure::RetentionConflict => "retention_conflict",
        ToolFailure::Unknown => "unknown",
        ToolFailure::UnsupportedEvidence => "unsupported_evidence",
        ToolFailure::ModelReplayFailed => "model_replay_failed",
        ToolFailure::LeanAxiomReport => "lean_axiom_report",
        ToolFailure::Io(_) => "io",
    }
}

fn process_phase_name(phase: ProcessPhase) -> &'static str {
    match phase {
        ProcessPhase::Decision => "decision",
        ProcessPhase::Evidence => "evidence",
        ProcessPhase::Kernel => "kernel",
    }
}

fn retained_run_files(run: &ToolRun) -> Result<BTreeMap<String, Vec<u8>>, ToolFailure> {
    #[derive(Serialize)]
    struct Metadata<'a> {
        backend: &'a str,
        claim_id: u32,
        format: &'static str,
        record_hash: String,
        source_hash: String,
        status: &'static str,
        status_detail: Option<&'static str>,
        tool_hash: String,
        tool_version: &'a str,
        toolchain_hash: Option<String>,
        transcript_count: usize,
    }

    let mut files = BTreeMap::new();
    files.insert(
        "formal-run-record.bin".to_owned(),
        formal_run_record(
            &run.identity,
            &run.obligation,
            &run.status,
            &run.transcripts,
        )?,
    );
    files.insert("source".to_owned(), run.obligation.source().to_vec());
    files.insert("stdout".to_owned(), run.stdout().to_vec());
    files.insert("stderr".to_owned(), run.stderr().to_vec());
    for (index, transcript) in run.transcripts.iter().enumerate() {
        let prefix = format!(
            "transcript-{:02}-{}",
            index.saturating_add(1),
            process_phase_name(transcript.phase)
        );
        files.insert(format!("{prefix}-input"), transcript.input.clone());
        files.insert(format!("{prefix}-stdout"), transcript.output.stdout.clone());
        files.insert(format!("{prefix}-stderr"), transcript.output.stderr.clone());
    }
    if let Some(inventory) = &run.toolchain_inventory {
        files.insert("toolchain.json".to_owned(), inventory.canonical_json()?);
    }
    let status_detail = match &run.status {
        ToolRunStatus::Blocked(failure) | ToolRunStatus::Failed(failure) => {
            Some(failure_name(failure))
        }
        ToolRunStatus::Undefined(reason) => Some(reason.name()),
        _ => None,
    };
    let mut metadata = serde_json::to_vec(&Metadata {
        backend: run.identity.backend.name(),
        claim_id: run.obligation.claim_id.get(),
        format: "zeno-fcis/formal-run-record/3",
        record_hash: hash_hex(run.record_hash),
        source_hash: hash_hex(run.obligation.source_hash),
        status: run_status_name(&run.status),
        status_detail,
        tool_hash: hash_hex(run.identity.binary_hash),
        tool_version: &run.identity.version,
        toolchain_hash: run.identity.runtime_hash.map(hash_hex),
        transcript_count: run.transcripts.len(),
    })
    .map_err(|error| ToolFailure::Io(error.to_string()))?;
    metadata.push(b'\n');
    files.insert("record.json".to_owned(), metadata);
    if matches!(
        run.status,
        ToolRunStatus::Refuted | ToolRunStatus::Undefined(_)
    ) {
        let values: Vec<_> = parse_model_values(&String::from_utf8_lossy(run.stdout()))
            .into_iter()
            .map(|(projection, value)| {
                serde_json::json!({
                    "projection": projection,
                    "value": value.to_string(),
                })
            })
            .collect();
        let counterexample = serde_json::to_vec(&serde_json::json!({
            "schema": "zeno-fcis/counterexample/1",
            "claim_id": run.obligation.claim_id.get(),
            "values": values,
        }))
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
        files.insert("counterexample.json".to_owned(), counterexample);
    }
    Ok(files)
}

fn write_retained_bundle(
    path: &Path,
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ToolFailure> {
    for (name, bytes) in files {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path.join(name))
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
    }
    Ok(())
}

fn verify_retained_bundle(
    path: &Path,
    expected: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ToolFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ToolFailure::RetentionConflict)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ToolFailure::RetentionConflict);
    }
    let mut actual_names = Vec::new();
    for entry in fs::read_dir(path).map_err(|_| ToolFailure::RetentionConflict)? {
        let entry = entry.map_err(|_| ToolFailure::RetentionConflict)?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| ToolFailure::RetentionConflict)?;
        actual_names.push(name);
    }
    actual_names.sort();
    if actual_names != expected.keys().cloned().collect::<Vec<_>>() {
        return Err(ToolFailure::RetentionConflict);
    }
    for (name, expected_bytes) in expected {
        let file =
            open_untrusted_read(&path.join(name)).map_err(|_| ToolFailure::RetentionConflict)?;
        if !file
            .metadata()
            .map_err(|_| ToolFailure::RetentionConflict)?
            .is_file()
        {
            return Err(ToolFailure::RetentionConflict);
        }
        let mut actual = Vec::new();
        file.take(
            u64::try_from(expected_bytes.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
        .read_to_end(&mut actual)
        .map_err(|_| ToolFailure::RetentionConflict)?;
        if actual != *expected_bytes {
            return Err(ToolFailure::RetentionConflict);
        }
    }
    Ok(())
}

struct RetentionStage {
    path: PathBuf,
    armed: bool,
}
impl Drop for RetentionStage {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Atomically retains a complete process bundle by content hash.
pub fn retain_run(root: &Path, run: &ToolRun) -> Result<PathBuf, ToolFailure> {
    fs::create_dir_all(root).map_err(|error| ToolFailure::Io(error.to_string()))?;
    let name = hash_hex(run.record_hash);
    let directory = root.join(&name);
    let files = retained_run_files(run)?;
    if directory.exists() {
        verify_retained_bundle(&directory, &files)?;
        return Ok(directory);
    }
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stage_path = root.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    fs::create_dir(&stage_path).map_err(|error| ToolFailure::Io(error.to_string()))?;
    let mut stage = RetentionStage {
        path: stage_path,
        armed: true,
    };
    write_retained_bundle(&stage.path, &files)?;
    match fs::rename(&stage.path, &directory) {
        Ok(()) => {
            stage.armed = false;
            Ok(directory)
        }
        Err(_) if directory.exists() => {
            verify_retained_bundle(&directory, &files)?;
            Ok(directory)
        }
        Err(error) => Err(ToolFailure::Io(error.to_string())),
    }
}

/// Deterministic summary of configured and checked tools.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorEntry {
    backend: ToolBackend,
    result: Result<ToolIdentity, ToolFailure>,
}
impl DoctorEntry {
    /// Returns the backend.
    #[must_use]
    pub const fn backend(&self) -> ToolBackend {
        self.backend
    }

    /// Returns the verification result.
    pub const fn result(&self) -> &Result<ToolIdentity, ToolFailure> {
        &self.result
    }
}
/// Rechecks every configured tool in canonical backend order.
#[must_use]
pub fn doctor(manifest: &ToolsManifest) -> Vec<DoctorEntry> {
    manifest
        .tools
        .iter()
        .map(|tool| DoctorEntry {
            backend: tool.backend,
            result: verify_tool(tool),
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn run_fixed(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout_ms: u64,
    max_output: usize,
) -> Result<ProcessOutput, ToolFailure> {
    #[cfg(not(unix))]
    {
        let _ = (path, args, input, timeout_ms, max_output);
        Err(ToolFailure::ProcessContainmentUnavailable)
    }
    #[cfg(unix)]
    {
        run_fixed_unix(path, args, input, timeout_ms, max_output)
    }
}

/// Starts `command`, retrying a bounded number of times while its executable
/// is busy, within the caller's time budget.
///
/// Linux refuses to execute a file that any process holds open for writing
/// (`ETXTBSY`). A child that another thread forks while this process writes a
/// private executable copy inherits that descriptor until it executes its own
/// program, so the condition clears quickly. The copy is private and its bytes
/// are already admitted, so running it later is the same run.
///
/// The budget runs from `start` for `budget`. No attempt starts after it ends,
/// including the first: an exhausted budget, or a wait that would reach its
/// end, gives `Timeout`. Every other error, and a file that stays busy through
/// the last attempt, fails closed with `Io`.
#[cfg(unix)]
fn spawn_unless_busy(
    command: &mut Command,
    start: Instant,
    budget: Duration,
) -> Result<Child, ToolFailure> {
    let busy = nix::errno::Errno::ETXTBSY as i32;
    let mut delay = Duration::from_millis(1);
    let mut retries = 8;
    loop {
        if start.elapsed() >= budget {
            return Err(ToolFailure::Timeout);
        }
        let error = match command.spawn() {
            Ok(child) => return Ok(child),
            Err(error) => error,
        };
        if error.raw_os_error() != Some(busy) || retries == 0 {
            return Err(ToolFailure::Io(error.to_string()));
        }
        if budget.saturating_sub(start.elapsed()) <= delay {
            return Err(ToolFailure::Timeout);
        }
        thread::sleep(delay);
        delay = delay.saturating_mul(2);
        retries -= 1;
    }
}

#[cfg(unix)]
fn run_fixed_unix(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    timeout_ms: u64,
    max_output: usize,
) -> Result<ProcessOutput, ToolFailure> {
    use std::os::unix::process::CommandExt as _;

    let start = Instant::now();
    let working_directory = PrivateWorkingDirectory::create()?;
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .current_dir(working_directory.path())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.process_group(0);
    let child = spawn_unless_busy(&mut command, start, Duration::from_millis(timeout_ms))?;
    let mut child = ContainedChild::new(child)?;
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| ToolFailure::Io("missing child stdout".into()))?;
    let stderr = child
        .child
        .stderr
        .take()
        .ok_or_else(|| ToolFailure::Io("missing child stderr".into()))?;
    let (events, receiver) = mpsc::sync_channel(3);
    bounded_reader(stdout, max_output, events.clone(), IoStream::Stdout);
    bounded_reader(stderr, max_output, events.clone(), IoStream::Stderr);
    let stdin_pending = if let Some(bytes) = input {
        let Some(stdin) = child.child.stdin.take() else {
            return Err(ToolFailure::Io("missing child stdin".into()));
        };
        bounded_writer(stdin, bytes.to_vec(), events.clone());
        true
    } else {
        false
    };
    drop(events);
    wait_output(
        child,
        timeout_ms,
        max_output,
        receiver,
        stdin_pending,
        start,
    )
}

#[cfg(unix)]
struct PrivateWorkingDirectory {
    path: PathBuf,
}
#[cfg(unix)]
impl PrivateWorkingDirectory {
    fn create() -> Result<Self, ToolFailure> {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("zeno-fcis-work-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).map_err(|error| ToolFailure::Io(error.to_string()))?;
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        Ok(Self { path })
    }
    fn path(&self) -> &Path {
        &self.path
    }
}
#[cfg(unix)]
impl Drop for PrivateWorkingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

#[cfg(unix)]
struct ContainedChild {
    child: Child,
    process_group: nix::unistd::Pid,
    armed: bool,
}
#[cfg(unix)]
impl ContainedChild {
    fn new(mut child: Child) -> Result<Self, ToolFailure> {
        let raw = i32::try_from(child.id()).map_err(|_| {
            let _ = child.kill();
            let _ = child.wait();
            ToolFailure::ProcessContainmentFailed
        })?;
        Ok(Self {
            child,
            process_group: nix::unistd::Pid::from_raw(raw),
            armed: true,
        })
    }

    fn terminate(&mut self, observed_exit: bool) -> Result<(), ToolFailure> {
        use nix::errno::Errno;
        use nix::sys::signal::{Signal, killpg};

        let group_result = killpg(self.process_group, Signal::SIGKILL);
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.armed = false;
        match group_result {
            Ok(()) => Ok(()),
            Err(Errno::ESRCH) if observed_exit => Ok(()),
            Err(_) => Err(ToolFailure::ProcessContainmentFailed),
        }
    }
}
#[cfg(unix)]
impl Drop for ContainedChild {
    fn drop(&mut self) {
        if self.armed {
            let _ = nix::sys::signal::killpg(self.process_group, nix::sys::signal::Signal::SIGKILL);
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

enum IoStream {
    Stdout,
    Stderr,
}
enum IoEvent {
    Stdin(Result<(), ToolFailure>),
    Stdout(Result<Vec<u8>, ToolFailure>),
    Stderr(Result<Vec<u8>, ToolFailure>),
}

fn bounded_writer<W: Write + Send + 'static>(
    mut stream: W,
    bytes: Vec<u8>,
    events: SyncSender<IoEvent>,
) {
    thread::spawn(move || {
        let result = stream
            .write_all(&bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()));
        let _ = events.send(IoEvent::Stdin(result));
    });
}

fn bounded_reader<R: Read + Send + 'static>(
    stream: R,
    max_output: usize,
    events: SyncSender<IoEvent>,
    stream_name: IoStream,
) {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stream
            .take(
                u64::try_from(max_output)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()));
        let event = match stream_name {
            IoStream::Stdout => IoEvent::Stdout(result),
            IoStream::Stderr => IoEvent::Stderr(result),
        };
        let _ = events.send(event);
    });
}

#[cfg(unix)]
fn wait_output(
    mut child: ContainedChild,
    timeout_ms: u64,
    max_output: usize,
    receiver: mpsc::Receiver<IoEvent>,
    stdin_pending: bool,
    start: Instant,
) -> Result<ProcessOutput, ToolFailure> {
    let mut status = None;
    let mut stdin_done = !stdin_pending;
    let mut stdout = None;
    let mut stderr = None;
    loop {
        loop {
            match receiver.try_recv() {
                Ok(IoEvent::Stdin(result)) => {
                    if let Err(error) = result {
                        child.terminate(status.is_some())?;
                        return Err(error);
                    }
                    stdin_done = true;
                }
                Ok(IoEvent::Stdout(result)) => {
                    let bytes = match result {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            child.terminate(status.is_some())?;
                            return Err(error);
                        }
                    };
                    if bytes.len() > max_output {
                        child.terminate(status.is_some())?;
                        return Err(ToolFailure::OutputLimit);
                    }
                    stdout = Some(bytes);
                }
                Ok(IoEvent::Stderr(result)) => {
                    let bytes = match result {
                        Ok(bytes) => bytes,
                        Err(error) => {
                            child.terminate(status.is_some())?;
                            return Err(error);
                        }
                    };
                    if bytes.len() > max_output {
                        child.terminate(status.is_some())?;
                        return Err(ToolFailure::OutputLimit);
                    }
                    stderr = Some(bytes);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !(stdin_done && stdout.is_some() && stderr.is_some()) {
                        child.terminate(status.is_some())?;
                        return Err(ToolFailure::ProcessContainmentFailed);
                    }
                    break;
                }
            }
        }
        if status.is_none() {
            status = child
                .child
                .try_wait()
                .map_err(|error| ToolFailure::Io(error.to_string()))?;
        }
        if status.is_some() && stdin_done && stdout.is_some() && stderr.is_some() {
            let status = status.unwrap_or_else(|| unreachable!());
            let stdout = stdout.take().unwrap_or_else(|| unreachable!());
            let stderr = stderr.take().unwrap_or_else(|| unreachable!());
            if stdout.len().saturating_add(stderr.len()) > max_output {
                child.terminate(true)?;
                return Err(ToolFailure::OutputLimit);
            }
            child.terminate(true)?;
            return Ok(ProcessOutput {
                status,
                stdout,
                stderr,
            });
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            child.terminate(status.is_some())?;
            return Err(ToolFailure::Timeout);
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn run_lean(
    config: &ToolConfig,
    executable: &Path,
    source: &[u8],
) -> Result<ProcessOutput, ToolFailure> {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("zeno-fcis-{}-{sequence}.lean", std::process::id()));
    atomic_write(&path, source)?;
    let argument = path
        .to_str()
        .ok_or_else(|| ToolFailure::Io("non-UTF8 temp path".into()))?;
    let result = run_fixed(
        executable,
        &[argument],
        None,
        config.timeout_ms,
        config.max_output_bytes,
    );
    let _ = fs::remove_file(path);
    result
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), ToolFailure> {
    let mut temporary = path.to_path_buf();
    temporary.set_extension(format!(
        "tmp-{}",
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    fs::rename(&temporary, path).map_err(|error| ToolFailure::Io(error.to_string()))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
fn hash_hex(value: Hash32) -> String {
    value.to_string()
}
fn smt_identifier(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(3 + value.len().saturating_mul(2));
    encoded.push_str("id");
    encoded.push_str(&value.len().to_string());
    encoded.push('_');
    for byte in value.bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
fn smt_path(path: &ProjectionPath, step: u32) -> String {
    let root = match path.root() {
        ProjectionRoot::Pre => "pre",
        ProjectionRoot::Post => "post",
        ProjectionRoot::Command => "command",
        ProjectionRoot::Context => "context",
        ProjectionRoot::Effects => "effects",
        ProjectionRoot::Outbox => "outbox",
        ProjectionRoot::Events => "events",
    };
    let suffix = path
        .segments()
        .iter()
        .map(|id| id.get().to_string())
        .collect::<Vec<_>>()
        .join("_");
    format!("{root}_{suffix}_t{step}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SmtValue {
    term: String,
    defined: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SmtBool {
    term: String,
    defined: String,
}

struct SmtRenderBudget {
    operations: u64,
    bytes: usize,
    limits: ExportLimits,
}
impl SmtRenderBudget {
    const fn new(limits: ExportLimits) -> Self {
        Self {
            operations: 0,
            bytes: 0,
            limits,
        }
    }

    fn operation(&mut self) -> Result<(), ExportError> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(ExportError::ResourceLimit)?;
        if self.operations > self.limits.max_operations() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn bytes(&mut self, bytes: usize) -> Result<(), ExportError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(ExportError::ResourceLimit)?;
        if self.bytes > self.limits.max_source_bytes() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn value(&mut self, value: SmtValue) -> Result<SmtValue, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }

    fn boolean(&mut self, value: SmtBool) -> Result<SmtBool, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }
}

fn smt_int(value: i128) -> String {
    if value < 0 {
        format!("(- {})", value.unsigned_abs())
    } else {
        value.to_string()
    }
}

fn smt_and(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "true".to_owned(),
        [term] => term.clone(),
        _ => format!("(and {})", terms.join(" ")),
    }
}

fn smt_or(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "false".to_owned(),
        [term] => term.clone(),
        _ => format!("(or {})", terms.join(" ")),
    }
}

fn smt_not(term: &str) -> String {
    format!("(not {term})")
}

fn smt_i128_range(term: &str) -> String {
    format!(
        "(and (<= {} {term}) (<= {term} {}))",
        smt_int(i128::MIN),
        smt_int(i128::MAX)
    )
}

fn checked_binary_smt(
    operator: &str,
    left: SmtValue,
    right: SmtValue,
    budget: &mut SmtRenderBudget,
) -> Result<SmtValue, ExportError> {
    budget.operation()?;
    let term = format!("({operator} {} {})", left.term, right.term);
    let defined = smt_and(vec![left.defined, right.defined, smt_i128_range(&term)]);
    budget.value(SmtValue { term, defined })
}

fn floor_div_smt(left: &str, right: &str) -> String {
    format!("(ite (> {right} 0) (div {left} {right}) (div (- {left}) (- {right})))")
}

fn ceil_div_smt(left: &str, right: &str) -> String {
    format!("(- {})", floor_div_smt(&format!("(- {left})"), right))
}

fn render_value_smt(
    value: &ValueExpr,
    step: u32,
    environment: &BTreeMap<String, i128>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtValue, ExportError> {
    budget.operation()?;
    let rendered = match value {
        ValueExpr::Int(value) => SmtValue {
            term: smt_int(*value),
            defined: "true".to_owned(),
        },
        ValueExpr::Var(name) => {
            let Some(value) = environment.get(name.as_str()) else {
                return Err(ExportError::InvalidFormula);
            };
            SmtValue {
                term: smt_int(*value),
                defined: "true".to_owned(),
            }
        }
        ValueExpr::Projection(path) => SmtValue {
            term: smt_path(path, step),
            defined: "true".to_owned(),
        },
        ValueExpr::Add(left, right) => checked_binary_smt(
            "+",
            render_value_smt(left, step, environment, budget)?,
            render_value_smt(right, step, environment, budget)?,
            budget,
        )?,
        ValueExpr::Sub(left, right) => checked_binary_smt(
            "-",
            render_value_smt(left, step, environment, budget)?,
            render_value_smt(right, step, environment, budget)?,
            budget,
        )?,
        ValueExpr::Mul(left, right) => checked_binary_smt(
            "*",
            render_value_smt(left, step, environment, budget)?,
            render_value_smt(right, step, environment, budget)?,
            budget,
        )?,
        ValueExpr::Div(mode, left, right) => {
            let left = render_value_smt(left, step, environment, budget)?;
            let right = render_value_smt(right, step, environment, budget)?;
            let term = match mode {
                zeno_fcis_spec::DivisionMode::Exact | zeno_fcis_spec::DivisionMode::Floor => {
                    floor_div_smt(&left.term, &right.term)
                }
                zeno_fcis_spec::DivisionMode::Ceil => ceil_div_smt(&left.term, &right.term),
            };
            let mut conditions = vec![
                left.defined,
                right.defined,
                format!("(not (= {} 0))", right.term),
            ];
            if matches!(mode, zeno_fcis_spec::DivisionMode::Exact) {
                let positive_divisor = format!(
                    "(ite (< {} 0) (- {}) {})",
                    right.term, right.term, right.term
                );
                conditions.push(format!("(= (mod {} {positive_divisor}) 0)", left.term));
            }
            conditions.push(smt_i128_range(&term));
            SmtValue {
                term,
                defined: smt_and(conditions),
            }
        }
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            if end < start || end.saturating_sub(*start) > 4096 {
                return Err(ExportError::InvalidFormula);
            }
            let mut total = SmtValue {
                term: "0".to_owned(),
                defined: "true".to_owned(),
            };
            for current in *start..*end {
                let mut nested = environment.clone();
                nested.insert(variable.as_str().into(), current);
                let value = render_value_smt(body, step, &nested, budget)?;
                total = checked_binary_smt("+", total, value, budget)?;
            }
            total
        }
    };
    budget.value(rendered)
}

fn strict_bool_smt(
    operator: &str,
    left: SmtBool,
    right: SmtBool,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    budget.operation()?;
    budget.boolean(SmtBool {
        term: format!("({operator} {} {})", left.term, right.term),
        defined: smt_and(vec![left.defined, right.defined]),
    })
}

fn render_rel_smt(
    value: &RelExpr,
    step: u32,
    environment: &BTreeMap<String, i128>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        RelExpr::Bool(value) => SmtBool {
            term: value.to_string(),
            defined: "true".to_owned(),
        },
        RelExpr::Not(value) => {
            let value = render_rel_smt(value, step, environment, budget)?;
            SmtBool {
                term: smt_not(&value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) => {
            let left = render_rel_smt(left, step, environment, budget)?;
            let right = render_rel_smt(right, step, environment, budget)?;
            strict_bool_smt("and", left, right, budget)?
        }
        RelExpr::Or(left, right) => {
            let left = render_rel_smt(left, step, environment, budget)?;
            let right = render_rel_smt(right, step, environment, budget)?;
            strict_bool_smt("or", left, right, budget)?
        }
        RelExpr::Implies(left, right) => {
            let left = render_rel_smt(left, step, environment, budget)?;
            let right = render_rel_smt(right, step, environment, budget)?;
            strict_bool_smt("=>", left, right, budget)?
        }
        RelExpr::Compare(operation, left, right) => {
            let left = render_value_smt(left, step, environment, budget)?;
            let right = render_value_smt(right, step, environment, budget)?;
            SmtBool {
                term: format!(
                    "({} {} {})",
                    match operation {
                        CompareOp::Eq => "=",
                        CompareOp::NotEq => "distinct",
                        CompareOp::Less => "<",
                        CompareOp::LessEq => "<=",
                        CompareOp::Greater => ">",
                        CompareOp::GreaterEq => ">=",
                    },
                    left.term,
                    right.term
                ),
                defined: smt_and(vec![left.defined, right.defined]),
            }
        }
        RelExpr::Predicate { name, arguments } => {
            let arguments = arguments
                .iter()
                .map(|value| render_value_smt(value, step, environment, budget))
                .collect::<Result<Vec<_>, _>>()?;
            let defined = smt_and(
                arguments
                    .iter()
                    .map(|argument| argument.defined.clone())
                    .collect(),
            );
            let rendered = arguments
                .iter()
                .map(|argument| argument.term.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            SmtBool {
                term: if rendered.is_empty() {
                    format!("(pred_{})", smt_identifier(name.as_str()))
                } else {
                    format!("(pred_{} {rendered})", smt_identifier(name.as_str()))
                },
                defined,
            }
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool(
            true,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool(
            false,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
    };
    budget.boolean(rendered)
}

fn fold_all_smt(
    values: Vec<SmtBool>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    let mut result = budget.boolean(SmtBool {
        term: "true".to_owned(),
        defined: "true".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = smt_and(vec![
            result.defined,
            smt_or(vec![smt_not(&result.term), value.defined]),
        ]);
        let term = smt_and(vec![result.term, value.term]);
        result = budget.boolean(SmtBool { term, defined })?;
    }
    Ok(result)
}

fn fold_exists_smt(
    values: Vec<SmtBool>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    let mut result = budget.boolean(SmtBool {
        term: "false".to_owned(),
        defined: "true".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = smt_and(vec![
            result.defined,
            smt_or(vec![result.term.clone(), value.defined]),
        ]);
        let term = smt_or(vec![result.term, value.term]);
        result = budget.boolean(SmtBool { term, defined })?;
    }
    Ok(result)
}

fn render_bounded_bool(
    all: bool,
    variable: &Identifier,
    range: core::ops::Range<i128>,
    body: &RelExpr,
    step: u32,
    environment: &BTreeMap<String, i128>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    if range.end < range.start || range.end.saturating_sub(range.start) > 4096 {
        return Err(ExportError::InvalidFormula);
    }
    let mut values = Vec::new();
    for current in range {
        let mut nested = environment.clone();
        nested.insert(variable.as_str().into(), current);
        values.push(render_rel_smt(body, step, &nested, budget)?);
    }
    if all {
        fold_all_smt(values, budget)
    } else {
        fold_exists_smt(values, budget)
    }
}

fn render_temporal_smt(
    value: &TemporalFormula,
    step: u32,
    horizon: u32,
    environment: &BTreeMap<String, i128>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        TemporalFormula::Atom(value) => render_rel_smt(value, step, environment, budget)?,
        TemporalFormula::Not(value) => {
            let value = render_temporal_smt(value, step, horizon, environment, budget)?;
            SmtBool {
                term: smt_not(&value.term),
                defined: value.defined,
            }
        }
        TemporalFormula::And(left, right) => {
            let left = render_temporal_smt(left, step, horizon, environment, budget)?;
            let right = render_temporal_smt(right, step, horizon, environment, budget)?;
            strict_bool_smt("and", left, right, budget)?
        }
        TemporalFormula::Or(left, right) => {
            let left = render_temporal_smt(left, step, horizon, environment, budget)?;
            let right = render_temporal_smt(right, step, horizon, environment, budget)?;
            strict_bool_smt("or", left, right, budget)?
        }
        TemporalFormula::Next(value) => {
            if step + 1 < horizon {
                render_temporal_smt(value, step + 1, horizon, environment, budget)?
            } else {
                SmtBool {
                    term: "false".to_owned(),
                    defined: "true".to_owned(),
                }
            }
        }
        TemporalFormula::Always(value) => fold_all_smt(
            (step..horizon)
                .map(|current| render_temporal_smt(value, current, horizon, environment, budget))
                .collect::<Result<Vec<_>, _>>()?,
            budget,
        )?,
        TemporalFormula::Eventually(value) => fold_exists_smt(
            (step..horizon)
                .map(|current| render_temporal_smt(value, current, horizon, environment, budget))
                .collect::<Result<Vec<_>, _>>()?,
            budget,
        )?,
        TemporalFormula::Until(left, right) => {
            let mut continuation = budget.boolean(SmtBool {
                term: "false".to_owned(),
                defined: "true".to_owned(),
            })?;
            for current in (step..horizon).rev() {
                budget.operation()?;
                let right = render_temporal_smt(right, current, horizon, environment, budget)?;
                let left = render_temporal_smt(left, current, horizon, environment, budget)?;
                let defined = smt_and(vec![
                    right.defined,
                    smt_or(vec![
                        right.term.clone(),
                        smt_and(vec![
                            left.defined,
                            smt_or(vec![smt_not(&left.term), continuation.defined]),
                        ]),
                    ]),
                ]);
                let term = smt_or(vec![
                    right.term,
                    smt_and(vec![left.term, continuation.term]),
                ]);
                continuation = budget.boolean(SmtBool { term, defined })?;
            }
            continuation
        }
    };
    budget.boolean(rendered)
}

fn select_trace_length(
    mut formulas: Vec<SmtBool>,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    let Some(mut selected) = formulas.pop() else {
        return Err(ExportError::InvalidFormula);
    };
    for (index, formula) in formulas.into_iter().enumerate().rev() {
        budget.operation()?;
        let length = index + 1;
        selected = budget.boolean(SmtBool {
            term: format!(
                "(ite (= zeno_trace_len {length}) {} {})",
                formula.term, selected.term
            ),
            defined: format!(
                "(ite (= zeno_trace_len {length}) {} {})",
                formula.defined, selected.defined
            ),
        })?;
    }
    Ok(selected)
}

fn collect_claim(
    claim: &ClaimDecl,
    paths: &mut BTreeSet<ProjectionPath>,
    predicates: &mut BTreeMap<Identifier, usize>,
) {
    match claim.formula() {
        ClaimFormula::Relational(value) => collect_rel(value, paths, predicates),
        ClaimFormula::Temporal(value) => collect_temporal(value, paths, predicates),
    }
}
fn collect_rel(
    value: &RelExpr,
    paths: &mut BTreeSet<ProjectionPath>,
    predicates: &mut BTreeMap<Identifier, usize>,
) {
    match value {
        RelExpr::Not(value) => collect_rel(value, paths, predicates),
        RelExpr::And(a, b) | RelExpr::Or(a, b) | RelExpr::Implies(a, b) => {
            collect_rel(a, paths, predicates);
            collect_rel(b, paths, predicates)
        }
        RelExpr::Compare(_, a, b) => {
            collect_value(a, paths);
            collect_value(b, paths)
        }
        RelExpr::Predicate { name, arguments } => {
            predicates.insert(name.clone(), arguments.len());
            for argument in arguments.iter() {
                collect_value(argument, paths)
            }
        }
        RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => {
            collect_rel(body, paths, predicates)
        }
        RelExpr::Bool(_) => {}
    }
}
fn collect_temporal(
    value: &TemporalFormula,
    paths: &mut BTreeSet<ProjectionPath>,
    predicates: &mut BTreeMap<Identifier, usize>,
) {
    match value {
        TemporalFormula::Atom(value) => collect_rel(value, paths, predicates),
        TemporalFormula::Not(value)
        | TemporalFormula::Next(value)
        | TemporalFormula::Always(value)
        | TemporalFormula::Eventually(value) => collect_temporal(value, paths, predicates),
        TemporalFormula::And(a, b) | TemporalFormula::Or(a, b) | TemporalFormula::Until(a, b) => {
            collect_temporal(a, paths, predicates);
            collect_temporal(b, paths, predicates)
        }
    }
}
fn collect_value(value: &ValueExpr, paths: &mut BTreeSet<ProjectionPath>) {
    match value {
        ValueExpr::Projection(path) => {
            paths.insert(path.clone());
        }
        ValueExpr::Add(a, b)
        | ValueExpr::Sub(a, b)
        | ValueExpr::Mul(a, b)
        | ValueExpr::Div(_, a, b) => {
            collect_value(a, paths);
            collect_value(b, paths)
        }
        ValueExpr::Sum { body, .. } => collect_value(body, paths),
        ValueExpr::Int(_) | ValueExpr::Var(_) => {}
    }
}
fn parse_lean_axioms(text: &str) -> Option<Vec<String>> {
    let reports = text
        .lines()
        .filter_map(|line| {
            line.split_once("depends on axioms:")
                .map(|(_, value)| value.trim())
        })
        .collect::<Vec<_>>();
    let [report] = reports.as_slice() else {
        return None;
    };
    let body = report.strip_prefix('[')?.strip_suffix(']')?.trim();
    if body.is_empty() {
        return Some(Vec::new());
    }
    let mut axioms = body
        .split(',')
        .map(str::trim)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if axioms.iter().any(|axiom| {
        axiom.is_empty()
            || !axiom
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
    }) {
        return None;
    }
    axioms.sort();
    axioms.dedup();
    Some(axioms)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LeanValue {
    term: String,
    defined: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LeanBool {
    term: String,
    defined: String,
}

struct LeanRenderBudget {
    operations: u64,
    bytes: usize,
    next_time_binding: u64,
    limits: ExportLimits,
}
impl LeanRenderBudget {
    const fn new(limits: ExportLimits) -> Self {
        Self {
            operations: 0,
            bytes: 0,
            next_time_binding: 0,
            limits,
        }
    }

    // A child temporal formula must never shadow the time expression passed
    // by its parent. One supply also separates the two binders of `until`.
    fn time_binding(&mut self) -> Result<String, ExportError> {
        let binding = self.next_time_binding;
        self.next_time_binding = binding.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        Ok(format!("time_{binding}"))
    }

    fn operation(&mut self) -> Result<(), ExportError> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(ExportError::ResourceLimit)?;
        if self.operations > self.limits.max_operations() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn bytes(&mut self, bytes: usize) -> Result<(), ExportError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(ExportError::ResourceLimit)?;
        if self.bytes > self.limits.max_source_bytes() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn value(&mut self, value: LeanValue) -> Result<LeanValue, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }

    fn boolean(&mut self, value: LeanBool) -> Result<LeanBool, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }
}

fn lean_int(value: i128) -> String {
    if value < 0 {
        format!("(-{} : Int)", value.unsigned_abs())
    } else {
        format!("({value} : Int)")
    }
}

fn lean_and(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "True".to_owned(),
        [term] => term.clone(),
        _ => format!("({})", terms.join(" ∧ ")),
    }
}

fn lean_or(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "False".to_owned(),
        [term] => term.clone(),
        _ => format!("({})", terms.join(" ∨ ")),
    }
}

fn lean_not(term: &str) -> String {
    format!("¬ ({term})")
}

fn lean_path(path: &ProjectionPath, step: &str) -> String {
    let root = match path.root() {
        ProjectionRoot::Pre => "pre",
        ProjectionRoot::Post => "post",
        ProjectionRoot::Command => "command",
        ProjectionRoot::Context => "context",
        ProjectionRoot::Effects => "effects",
        ProjectionRoot::Outbox => "outbox",
        ProjectionRoot::Events => "events",
    };
    let suffix = path
        .segments()
        .iter()
        .map(|id| id.get().to_string())
        .collect::<Vec<_>>()
        .join("_");
    format!("(observe \"{root}_{suffix}\" {step}).val")
}

fn lean_checked_binary(
    operator: &str,
    left: LeanValue,
    right: LeanValue,
    budget: &mut LeanRenderBudget,
) -> Result<LeanValue, ExportError> {
    budget.operation()?;
    let term = format!("({} {operator} {})", left.term, right.term);
    let defined = lean_and(vec![left.defined, right.defined, format!("inI128 {term}")]);
    budget.value(LeanValue { term, defined })
}

fn render_value_lean(
    value: &ValueExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanValue, ExportError> {
    budget.operation()?;
    let rendered = match value {
        ValueExpr::Int(value) => LeanValue {
            term: lean_int(*value),
            defined: "True".to_owned(),
        },
        ValueExpr::Var(name) => {
            let Some(value) = environment.get(name.as_str()) else {
                return Err(ExportError::InvalidFormula);
            };
            LeanValue {
                term: lean_int(*value),
                defined: "True".to_owned(),
            }
        }
        ValueExpr::Projection(path) => LeanValue {
            term: lean_path(path, step),
            defined: "True".to_owned(),
        },
        ValueExpr::Add(left, right) | ValueExpr::Sub(left, right) | ValueExpr::Mul(left, right) => {
            let operator = match value {
                ValueExpr::Add(_, _) => "+",
                ValueExpr::Sub(_, _) => "-",
                ValueExpr::Mul(_, _) => "*",
                _ => unreachable!(),
            };
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            lean_checked_binary(operator, left, right, budget)?
        }
        ValueExpr::Div(mode, left, right) => {
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            let term = match mode {
                zeno_fcis_spec::DivisionMode::Exact | zeno_fcis_spec::DivisionMode::Floor => {
                    format!("floorDiv {} {}", left.term, right.term)
                }
                zeno_fcis_spec::DivisionMode::Ceil => {
                    format!("ceilDiv {} {}", left.term, right.term)
                }
            };
            let mut conditions = vec![left.defined, right.defined, format!("{} ≠ 0", right.term)];
            if matches!(mode, zeno_fcis_spec::DivisionMode::Exact) {
                let positive_divisor = format!(
                    "(if {} < 0 then -{} else {})",
                    right.term, right.term, right.term
                );
                conditions.push(format!("{} % {positive_divisor} = 0", left.term));
            }
            conditions.push(format!("inI128 ({term})"));
            LeanValue {
                term: format!("({term})"),
                defined: lean_and(conditions),
            }
        }
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            if end < start || end.saturating_sub(*start) > 4096 {
                return Err(ExportError::InvalidFormula);
            }
            let mut total = LeanValue {
                term: lean_int(0),
                defined: "True".to_owned(),
            };
            for current in *start..*end {
                let mut nested = environment.clone();
                nested.insert(variable.as_str().into(), current);
                let next = render_value_lean(body, step, &nested, budget)?;
                total = lean_checked_binary("+", total, next, budget)?;
            }
            total
        }
    };
    budget.value(rendered)
}

fn strict_bool_lean(
    operator: &str,
    left: LeanBool,
    right: LeanBool,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    budget.boolean(LeanBool {
        term: format!("({} {operator} {})", left.term, right.term),
        defined: lean_and(vec![left.defined, right.defined]),
    })
}

fn render_rel_lean(
    value: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        RelExpr::Bool(value) => LeanBool {
            term: if *value { "True" } else { "False" }.to_owned(),
            defined: "True".to_owned(),
        },
        RelExpr::Not(value) => {
            let value = render_rel_lean(value, step, environment, budget)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) | RelExpr::Or(left, right) | RelExpr::Implies(left, right) => {
            let operator = match value {
                RelExpr::And(_, _) => "∧",
                RelExpr::Or(_, _) => "∨",
                RelExpr::Implies(_, _) => "→",
                _ => unreachable!(),
            };
            let left = render_rel_lean(left, step, environment, budget)?;
            let right = render_rel_lean(right, step, environment, budget)?;
            strict_bool_lean(operator, left, right, budget)?
        }
        RelExpr::Compare(operation, left, right) => {
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            LeanBool {
                term: format!(
                    "({} {} {})",
                    left.term,
                    match operation {
                        CompareOp::Eq => "=",
                        CompareOp::NotEq => "≠",
                        CompareOp::Less => "<",
                        CompareOp::LessEq => "<=",
                        CompareOp::Greater => ">",
                        CompareOp::GreaterEq => ">=",
                    },
                    right.term
                ),
                defined: lean_and(vec![left.defined, right.defined]),
            }
        }
        RelExpr::Predicate { name, arguments } => {
            let arguments = arguments
                .iter()
                .map(|value| render_value_lean(value, step, environment, budget))
                .collect::<Result<Vec<_>, _>>()?;
            let defined = lean_and(
                arguments
                    .iter()
                    .map(|argument| argument.defined.clone())
                    .collect(),
            );
            let rendered = arguments
                .iter()
                .map(|argument| argument.term.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            LeanBool {
                term: format!("predicate \"{}\" [{rendered}]", name.as_str()),
                defined,
            }
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool_lean(
            true,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool_lean(
            false,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
    };
    budget.boolean(rendered)
}

fn fold_all_lean(
    values: Vec<LeanBool>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    let mut result = budget.boolean(LeanBool {
        term: "True".to_owned(),
        defined: "True".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![lean_not(&result.term), value.defined]),
        ]);
        let term = lean_and(vec![result.term, value.term]);
        result = budget.boolean(LeanBool { term, defined })?;
    }
    Ok(result)
}

fn fold_exists_lean(
    values: Vec<LeanBool>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    let mut result = budget.boolean(LeanBool {
        term: "False".to_owned(),
        defined: "True".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![result.term.clone(), value.defined]),
        ]);
        let term = lean_or(vec![result.term, value.term]);
        result = budget.boolean(LeanBool { term, defined })?;
    }
    Ok(result)
}

fn render_bounded_bool_lean(
    all: bool,
    variable: &Identifier,
    range: core::ops::Range<i128>,
    body: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    if range.end < range.start || range.end.saturating_sub(range.start) > 4096 {
        return Err(ExportError::InvalidFormula);
    }
    let mut values = Vec::new();
    for current in range {
        let mut nested = environment.clone();
        nested.insert(variable.as_str().into(), current);
        values.push(render_rel_lean(body, step, &nested, budget)?);
    }
    if all {
        fold_all_lean(values, budget)
    } else {
        fold_exists_lean(values, budget)
    }
}

fn render_temporal_lean(
    value: &TemporalFormula,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        TemporalFormula::Atom(value) => render_rel_lean(value, step, environment, budget)?,
        TemporalFormula::Not(value) => {
            let value = render_temporal_lean(value, step, environment, budget)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        TemporalFormula::And(left, right) | TemporalFormula::Or(left, right) => {
            let operator = if matches!(value, TemporalFormula::And(_, _)) {
                "∧"
            } else {
                "∨"
            };
            let left = render_temporal_lean(left, step, environment, budget)?;
            let right = render_temporal_lean(right, step, environment, budget)?;
            strict_bool_lean(operator, left, right, budget)?
        }
        TemporalFormula::Next(value) => {
            render_temporal_lean(value, &format!("({step} + 1)"), environment, budget)?
        }
        TemporalFormula::Always(value) => {
            let time = budget.time_binding()?;
            let value = render_temporal_lean(value, &time, environment, budget)?;
            LeanBool {
                term: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.term),
                defined: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Eventually(value) => {
            let time = budget.time_binding()?;
            let value = render_temporal_lean(value, &time, environment, budget)?;
            LeanBool {
                term: format!("∃ {time} : Nat, {time} >= {step} ∧ ({})", value.term),
                defined: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Until(left, right) => {
            let witness = budget.time_binding()?;
            let prefix = budget.time_binding()?;
            let left_at_prefix = render_temporal_lean(left, &prefix, environment, budget)?;
            let right_at_witness = render_temporal_lean(right, &witness, environment, budget)?;
            LeanBool {
                term: format!(
                    "∃ {witness} : Nat, {witness} >= {step} ∧ ({}) ∧ \
                     ∀ {prefix} : Nat, {step} <= {prefix} → {prefix} < {witness} → ({})",
                    right_at_witness.term, left_at_prefix.term
                ),
                defined: format!(
                    "(∀ {witness} : Nat, {witness} >= {step} → ({})) ∧ \
                     (∀ {prefix} : Nat, {prefix} >= {step} → ({}))",
                    right_at_witness.defined, left_at_prefix.defined
                ),
            }
        }
    };
    budget.boolean(rendered)
}

#[cfg(test)]
mod translation_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use zeno_fcis_spec::{ClaimFormula, ClaimMode, CompareOp, RelExpr, TemporalFormula, ValueExpr};
    fn id(value: u32) -> StableId {
        StableId::new(value).unwrap_or_else(|| unreachable!())
    }
    fn name(value: &str) -> Identifier {
        Identifier::try_new(value).unwrap_or_else(|| unreachable!())
    }

    fn pre(field: u32) -> ValueExpr {
        ValueExpr::Projection(
            ProjectionPath::try_new(ProjectionRoot::Pre, vec![id(field)])
                .unwrap_or_else(|| unreachable!()),
        )
    }

    fn relational_obligation(claim_id: u32, formula: RelExpr) -> ExportedObligation {
        let claim = ClaimDecl::new(
            id(claim_id),
            name("strict_arithmetic"),
            vec![BackendId::Cvc5],
            ClaimMode::Relational,
            ClaimFormula::Relational(formula),
        );
        export_smt(&claim, ToolBackend::Cvc5).unwrap_or_else(|_| unreachable!())
    }

    fn model(assignments: &[(&str, i128)]) -> String {
        let mut text = String::from("sat\n(\n");
        for (variable, value) in assignments {
            let rendered = if *value < 0 {
                format!("(- {})", value.unsigned_abs())
            } else {
                value.to_string()
            };
            text.push_str(&format!("(define-fun {variable} () Int {rendered})\n"));
        }
        text.push_str(")\n");
        text
    }

    #[test]
    fn replay_confirms_models_where_strict_evaluation_has_no_value() {
        let add_one = || ValueExpr::Add(Box::new(pre(100)), Box::new(ValueExpr::Int(1)));
        // Holds wherever it is defined, and overflows at the top of the i128 range.
        let increment =
            relational_obligation(1, RelExpr::Compare(CompareOp::Greater, add_one(), pre(100)));
        assert_eq!(
            replay_model(&increment, &model(&[("pre_100_t0", i128::MAX)])),
            ToolRunStatus::Undefined(UndefinedReason::Overflow)
        );
        // A model at which the claim evaluates to true is a solver error, never a verdict.
        assert_eq!(
            replay_model(&increment, &model(&[("pre_100_t0", 5)])),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );

        // Evaluation is strict: a guard does not protect the arithmetic after it.
        let guarded = relational_obligation(
            2,
            RelExpr::Implies(
                Box::new(RelExpr::Compare(
                    CompareOp::LessEq,
                    pre(100),
                    ValueExpr::Int(3),
                )),
                Box::new(RelExpr::Compare(
                    CompareOp::LessEq,
                    add_one(),
                    ValueExpr::Int(4),
                )),
            ),
        );
        assert_eq!(
            replay_model(&guarded, &model(&[("pre_100_t0", i128::MAX)])),
            ToolRunStatus::Undefined(UndefinedReason::Overflow)
        );

        let quotient = |mode| {
            RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Div(mode, Box::new(pre(100)), Box::new(pre(101))),
                ValueExpr::Int(1),
            )
        };
        let floor = relational_obligation(3, quotient(zeno_fcis_spec::DivisionMode::Floor));
        assert_eq!(
            replay_model(&floor, &model(&[("pre_100_t0", 7), ("pre_101_t0", 0)])),
            ToolRunStatus::Undefined(UndefinedReason::DivisionByZero)
        );
        let exact = relational_obligation(4, quotient(zeno_fcis_spec::DivisionMode::Exact));
        assert_eq!(
            replay_model(&exact, &model(&[("pre_100_t0", 3), ("pre_101_t0", 2)])),
            ToolRunStatus::Undefined(UndefinedReason::NonExactDivision)
        );
        // An ordinary false evaluation remains a refutation.
        assert_eq!(
            replay_model(&floor, &model(&[("pre_100_t0", 4), ("pre_101_t0", 2)])),
            ToolRunStatus::Refuted
        );
        // A model that omits an observed value is never replayed.
        assert_eq!(
            replay_model(&floor, &model(&[("pre_100_t0", 4)])),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
    }

    /// A bounded counter: `count` is field 110 of state 100, `amount` is field
    /// 120 of command 101.
    fn counter_project(claims: &str) -> zeno_fcis_spec::ProjectSpec {
        let source = format!(
            "zeno 1;\nproject 1 counter;\ntype 100 state State;\ntype 101 command Command;\n\
             type 102 context Context;\ntype 105 int Count;\nfield 110 100 count 105;\n\
             field 120 101 amount 105;\nreason 200 bad precedence 0;\n\
             component 300 machine {{ owns 100; reads pre.100; writes post.100; budget steps 10; }}\n\
             merge [300];\n\
             law 400 bounded = post.100.110 >= 0 && post.100.110 <= 9;\n\
             law 401 steps = post.100.110 == pre.100.110 + command.101.120;\n\
             law 402 small_steps = command.101.120 >= 0 && command.101.120 <= 2;\n{claims}"
        );
        let parsed =
            zeno_fcis_spec::parse_project(&source, zeno_fcis_spec::SourceLimits::default())
                .unwrap_or_else(|set| panic!("{set}"));
        zeno_fcis_spec::elaborate_project(parsed, zeno_fcis_spec::ProjectLimits::default())
            .unwrap_or_else(|set| panic!("{set}"))
    }

    fn inductive_obligation(assume: &str, invariant: &str) -> ExportedObligation {
        let spec = counter_project(&format!(
            "claim 500 invariant cvc5 inductive assume [{assume}] = {invariant};\n"
        ));
        export_inductive_smt(&spec.claims()[0], spec.laws(), ToolBackend::Cvc5)
            .unwrap_or_else(|error| panic!("{error:?}"))
    }

    fn transition(pre: i128, amount: i128, post: i128) -> String {
        model(&[
            ("pre_100_110_t0", pre),
            ("command_101_120_t0", amount),
            ("post_100_110_t0", post),
        ])
    }

    #[test]
    fn the_inductive_step_asserts_each_law_and_the_invariant_separately() {
        let obligation = inductive_obligation("401, 402", "pre.100.110 >= 0");
        assert_eq!(obligation.scope(), ObligationScope::InductiveStepOverLaws);
        assert_eq!(
            obligation
                .hypotheses()
                .every_commit()
                .iter()
                .map(|law| law.id().get())
                .collect::<Vec<_>>(),
            vec![401, 402]
        );
        let source = String::from_utf8_lossy(obligation.source()).into_owned();
        assert!(source.starts_with(
            "; zeno-fcis/smt-inductive-obligation/1\n; claim-id 500\n; every commit: 401 402\n; accepts: \n; committed failures: \n"
        ));
        // Without per-case laws there is no decision kind to choose.
        assert!(!source.contains(DECISION_KIND));
        for label in [
            "; law 401\n(assert ",
            "; law 402\n(assert ",
            "; invariant before the step\n(assert ",
        ] {
            assert_eq!(source.matches(label).count(), 1, "{label}");
        }
        // The only negated assertion is the invariant after the step.
        assert_eq!(source.matches("(assert (not ").count(), 1);
        assert!(source.contains("; invariant after the step\n(assert (not (and"));
        assert!(source.contains("post_100_110_t0"));
        assert!(source.ends_with("(check-sat)\n"));
    }

    #[test]
    fn inductive_export_refuses_what_it_cannot_ground() {
        let spec = counter_project(
            "claim 500 invariant all inductive assume [401] = pre.100.110 >= 0;\n\
             claim 501 single_step cvc5 relational = pre.100.110 >= 0;\n\
             claim 502 cvc5_only cvc5 inductive assume [401] = pre.100.110 >= 0;\n",
        );
        let inductive = spec.claim(id(500)).unwrap_or_else(|| unreachable!());
        let relational = spec.claim(id(501)).unwrap_or_else(|| unreachable!());
        let cvc5_only = spec.claim(id(502)).unwrap_or_else(|| unreachable!());
        assert_eq!(
            export_inductive_smt(inductive, &[], ToolBackend::Cvc5).map(|_| ()),
            Err(ExportError::UnknownAssumedLaw(id(401)))
        );
        assert_eq!(
            export_inductive_smt(cvc5_only, spec.laws(), ToolBackend::Z3).map(|_| ()),
            Err(ExportError::BackendNotSelected)
        );
        assert_eq!(
            export_inductive_smt(relational, spec.laws(), ToolBackend::Cvc5).map(|_| ()),
            Err(ExportError::UnsupportedMode)
        );
        // The ordinary exporters never drop the assumed laws of an inductive claim.
        assert_eq!(
            export_smt(inductive, ToolBackend::Cvc5).map(|_| ()),
            Err(ExportError::UnsupportedMode)
        );
        assert_eq!(
            export_lean(inductive).map(|_| ()),
            Err(ExportError::UnsupportedMode)
        );
    }

    #[test]
    fn inductive_replay_confirms_only_transitions_the_laws_admit() {
        let capped = inductive_obligation("401, 402", "pre.100.110 <= 9");
        // 9 + 1 satisfies both laws and leaves the invariant: a counterexample to induction.
        assert_eq!(
            replay_model(&capped, &transition(9, 1, 10)),
            ToolRunStatus::Refuted
        );
        // A model that breaks an assumed law is not a transition the authority commits.
        assert_eq!(
            replay_model(&capped, &transition(9, 3, 12)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        assert_eq!(
            replay_model(&capped, &transition(9, 1, 11)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        // A model whose starting state already breaks the invariant is not a step.
        assert_eq!(
            replay_model(&capped, &transition(10, 1, 11)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        // A model whose end state keeps the invariant refutes nothing.
        assert_eq!(
            replay_model(&capped, &transition(3, 1, 4)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        // An assumed law that overflows is not satisfied, so it cannot hide in a counterexample.
        assert_eq!(
            replay_model(&capped, &transition(i128::MAX, 1, i128::MAX)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        let square = inductive_obligation("401, 402", "pre.100.110 * pre.100.110 >= 0");
        let root: i128 = 13_043_817_825_332_782_212;
        assert_eq!(
            replay_model(&square, &transition(root, 2, root + 2)),
            ToolRunStatus::Undefined(UndefinedReason::Overflow)
        );
    }

    #[test]
    fn a_split_step_uses_each_decision_kinds_own_laws() {
        // Law 401 steps accepts up; law 403 resets committed failures to zero.
        let spec = counter_project(
            "law 403 reset = post.100.110 == 0;\n\
             claim 500 split cvc5 inductive assume [402] accept [401] failure [403] = pre.100.110 >= 0;\n\
             claim 501 accepts_only cvc5 inductive assume [402] accept [401] = pre.100.110 >= 0;\n",
        );
        let export = |claim| {
            export_inductive_smt(
                spec.claim(id(claim)).unwrap_or_else(|| unreachable!()),
                spec.laws(),
                ToolBackend::Cvc5,
            )
            .unwrap_or_else(|error| panic!("{error:?}"))
        };
        let split = export(500);
        let source = String::from_utf8_lossy(split.source()).into_owned();
        assert!(source.contains("(declare-const zeno_decision_kind Int)"));
        assert!(source.contains("; law 401 on accepts\n(assert (=> (= zeno_decision_kind 1) "));
        assert!(
            source
                .contains("; law 403 on committed failures\n(assert (=> (= zeno_decision_kind 2) ")
        );
        assert!(source.contains("; law 402\n(assert (and"));
        let with_kind = |kind, pre, amount, post| {
            let mut text = transition(pre, amount, post);
            text.insert_str(
                text.len() - 2,
                &format!("(define-fun zeno_decision_kind () Int {kind})\n"),
            );
            text
        };
        // An accept that satisfies law 401 cannot leave the invariant, so this is not a model.
        assert_eq!(
            replay_model(&split, &with_kind(1, 0, 1, -1)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        // A model without a decision kind is never replayed for a split step.
        assert_eq!(
            replay_model(&split, &transition(0, 1, 1)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        // Without failure laws, a committed failure may leave the invariant.
        let accepts_only = export(501);
        assert_eq!(
            replay_model(&accepts_only, &with_kind(2, 0, 1, -1)),
            ToolRunStatus::Refuted
        );
        // The same model as an accept breaks law 401, so it is not replayed.
        assert_eq!(
            replay_model(&accepts_only, &with_kind(1, 0, 1, -1)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
        assert_eq!(
            replay_model(&accepts_only, &with_kind(3, 0, 1, -1)),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
    }

    #[test]
    fn undefined_results_have_their_own_record_and_name() {
        let reasons = [
            UndefinedReason::Overflow,
            UndefinedReason::DivisionByZero,
            UndefinedReason::NonExactDivision,
        ];
        let records: BTreeSet<Vec<u8>> = reasons
            .iter()
            .map(|reason| {
                status_record(&ToolRunStatus::Undefined(*reason)).unwrap_or_else(|_| unreachable!())
            })
            .collect();
        assert_eq!(records.len(), reasons.len());
        let refuted = status_record(&ToolRunStatus::Refuted).unwrap_or_else(|_| unreachable!());
        assert!(!records.contains(&refuted));
        assert_eq!(
            run_status_name(&ToolRunStatus::Undefined(UndefinedReason::Overflow)),
            "undefined"
        );
        assert_eq!(
            ObligationScope::WithoutSystemModel
                .meaning(&ToolRunStatus::Undefined(UndefinedReason::Overflow)),
            ObligationScope::WithoutSystemModel.meaning(&ToolRunStatus::Refuted)
        );
    }

    fn test_obligation(claim_id: u32) -> ExportedObligation {
        let claim = ClaimDecl::new(
            id(claim_id),
            name("record_identity"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(true)),
        );
        export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!())
    }

    fn test_identity() -> ToolIdentity {
        ToolIdentity {
            backend: ToolBackend::Z3,
            path: PathBuf::from("/untrusted/tool/path"),
            version: Z3_VERSION.to_owned(),
            binary_hash: RustCryptoSha256::hash(b"tool"),
            runtime_hash: None,
        }
    }

    fn test_transcript(stdout: &[u8], stderr: &[u8]) -> ProcessTranscript {
        ProcessTranscript {
            phase: ProcessPhase::Decision,
            input: b"input".to_vec(),
            output: ProcessOutput {
                status: success_status(),
                stdout: stdout.to_vec(),
                stderr: stderr.to_vec(),
            },
        }
    }

    #[test]
    fn rc3_run_record_encoding_is_injective_and_complete() {
        let identity = test_identity();
        let obligation = test_obligation(700);
        let status = ToolRunStatus::Blocked(ToolFailure::Unknown);
        let left = formal_run_record(
            &identity,
            &obligation,
            &status,
            &[test_transcript(b"a", b"bc")],
        )
        .unwrap_or_else(|_| unreachable!());
        let right = formal_run_record(
            &identity,
            &obligation,
            &status,
            &[test_transcript(b"ab", b"c")],
        )
        .unwrap_or_else(|_| unreachable!());
        assert_ne!(left, right);
        assert!(
            left.windows(obligation.source().len())
                .any(|window| window == obligation.source())
        );

        let mut variants = Vec::new();
        let mut changed_backend = identity.clone();
        changed_backend.backend = ToolBackend::Cvc5;
        variants.push(formal_run_record(
            &changed_backend,
            &obligation,
            &status,
            &[test_transcript(b"a", b"bc")],
        ));
        let mut changed_version = identity.clone();
        changed_version.version = "4.16.0-qualified".to_owned();
        variants.push(formal_run_record(
            &changed_version,
            &obligation,
            &status,
            &[test_transcript(b"a", b"bc")],
        ));
        let mut changed_runtime = identity.clone();
        changed_runtime.runtime_hash = Some(RustCryptoSha256::hash(b"runtime"));
        variants.push(formal_run_record(
            &changed_runtime,
            &obligation,
            &status,
            &[test_transcript(b"a", b"bc")],
        ));
        variants.push(formal_run_record(
            &identity,
            &test_obligation(701),
            &status,
            &[test_transcript(b"a", b"bc")],
        ));
        variants.push(formal_run_record(
            &identity,
            &obligation,
            &ToolRunStatus::Failed(ToolFailure::Unknown),
            &[test_transcript(b"a", b"bc")],
        ));
        for variant in variants {
            assert_ne!(left, variant.unwrap_or_else(|_| unreachable!()));
        }
    }

    #[test]
    #[cfg(unix)]
    fn busy_executables_are_retried_briefly_then_fail_closed() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-busy-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("create busy root: {error}"));
        let script = root.join("tool");
        fs::write(&script, b"#!/bin/sh\nprintf ok\n")
            .unwrap_or_else(|error| panic!("write busy tool: {error}"));
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("make busy tool executable: {error}"));
        let hold = || {
            OpenOptions::new()
                .write(true)
                .open(&script)
                .unwrap_or_else(|error| panic!("hold busy tool: {error}"))
        };

        // A writer that closes soon, as an inherited descriptor does at its
        // child's exec: the bounded retry reaches the tool.
        let writer = hold();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(5));
            drop(writer);
        });
        let output = run_fixed(&script, &[], None, 5_000, 4096)
            .unwrap_or_else(|error| panic!("run briefly busy tool: {error:?}"));
        release
            .join()
            .unwrap_or_else(|_| panic!("release busy tool"));
        assert!(output.status.success());
        assert_eq!(output.stdout, b"ok");

        // A writer that stays open: every attempt fails and the run fails closed.
        let writer = hold();
        let failure = run_fixed(&script, &[], None, 5_000, 4096);
        drop(writer);
        assert!(
            matches!(&failure, Err(ToolFailure::Io(message)) if message.contains("busy")),
            "{:?}",
            failure.map(|output| output.status)
        );

        // Regression from the independent review of 562926d: a one-millisecond
        // budget took 259 ms and failed with `Io`. The retry now stops when the
        // caller's budget ends.
        let writer = hold();
        let started = Instant::now();
        let failure = run_fixed(&script, &[], None, 1, 4096);
        let elapsed = started.elapsed();
        drop(writer);
        assert!(
            matches!(failure, Err(ToolFailure::Timeout)),
            "{:?}",
            failure.map(|output| output.status)
        );
        assert!(elapsed < Duration::from_millis(100), "{elapsed:?}");

        // An exhausted budget starts nothing, even for an executable that is
        // not busy. `run_fixed` would time out either way, so this checks the
        // start itself.
        match spawn_unless_busy(&mut Command::new(&script), Instant::now(), Duration::ZERO) {
            Err(ToolFailure::Timeout) => {}
            Ok(mut child) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("an exhausted budget started a process");
            }
            Err(other) => panic!("unexpected {other:?}"),
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn rc3_smt_followup_cannot_contradict_the_decision() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-smt-consistency-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("create test root: {error}"));
        let state = root.join("called");
        let script_path = root.join("solver");
        let script = format!(
            "#!/bin/sh\nwhile IFS= read -r line; do :; done\nif [ -e '{}' ]; then\n  printf 'unsat\\n'\nelse\n  : > '{}'\n  printf 'sat\\n'\nfi\n",
            state.display(),
            state.display()
        );
        fs::write(&script_path, script.as_bytes())
            .unwrap_or_else(|error| panic!("write fake solver: {error}"));
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("make fake solver executable: {error}"));
        let config = ToolConfig {
            backend: ToolBackend::Z3,
            path: script_path.clone(),
            version: Z3_VERSION.to_owned(),
            sha256: hash_hex(RustCryptoSha256::hash(script.as_bytes())),
            runtime: None,
            timeout_ms: 1_000,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        let execution = run_smt(&config, &script_path, b"(check-sat)\n")
            .unwrap_or_else(|error| panic!("run fake solver: {error:?}"));
        assert!(execution.inconsistent);
        assert_eq!(execution.transcripts.len(), 2);
        assert_eq!(
            solver_result(&execution.transcripts[0].output),
            Some(SolverResult::Sat)
        );
        assert_eq!(
            solver_result(&execution.transcripts[1].output),
            Some(SolverResult::Unsat)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rc3_lean_render_budget_is_live() {
        let variable = name("n");
        let claim = ClaimDecl::new(
            id(702),
            name("lean_live_budget"),
            vec![BackendId::Lean],
            ClaimMode::UnboundedProof,
            ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::ForAll {
                variable,
                start: 0,
                end: 4096,
                body: Box::new(RelExpr::Bool(true)),
            })),
        );
        let limits = ExportLimits::try_new(1, 16, 16, MAX_EXPORT_OPERATIONS, 1024)
            .unwrap_or_else(|| unreachable!());
        let started = Instant::now();
        assert_eq!(
            export_lean_with_limits(&claim, limits),
            Err(ExportError::ResourceLimit)
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    #[cfg(unix)]
    fn rc3_untrusted_special_files_and_post_enumeration_swaps_are_blocked() {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-untrusted-files-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("create test root: {error}"));
        let fifo = root.join("manifest.fifo");
        mkfifo(&fifo, Mode::S_IRUSR | Mode::S_IWUSR)
            .unwrap_or_else(|error| panic!("create fifo: {error}"));
        let started = Instant::now();
        assert_eq!(load_tools_manifest(&fifo), Err(ManifestError::NotFile));
        assert!(started.elapsed() < Duration::from_secs(1));
        fs::remove_file(&fifo).unwrap_or_else(|error| panic!("remove fifo: {error}"));

        let admitted = root.join("admitted");
        let replacement = root.join("replacement");
        fs::write(&admitted, b"admitted").unwrap_or_else(|error| panic!("write file: {error}"));
        fs::write(&replacement, b"replacement")
            .unwrap_or_else(|error| panic!("write replacement: {error}"));
        let enumerated = enumerate_toolchain(&root)
            .unwrap_or_else(|error| panic!("enumerate toolchain: {error:?}"));
        fs::remove_file(&admitted).unwrap_or_else(|error| panic!("remove admitted: {error}"));
        symlink(&replacement, &admitted)
            .unwrap_or_else(|error| panic!("replace with symlink: {error}"));
        assert!(matches!(
            snapshot_toolchain_files(&root, enumerated, None),
            Err(ToolFailure::Io(_))
        ));

        fs::remove_file(&admitted).unwrap_or_else(|error| panic!("remove symlink: {error}"));
        fs::remove_file(&replacement).unwrap_or_else(|error| panic!("remove replacement: {error}"));
        let admitted_directory = root.join("admitted-directory");
        let moved_directory = root.join("moved-directory");
        let outside = root.with_extension("outside");
        fs::create_dir(&admitted_directory)
            .unwrap_or_else(|error| panic!("create admitted directory: {error}"));
        fs::write(admitted_directory.join("file"), b"inside")
            .unwrap_or_else(|error| panic!("write admitted nested file: {error}"));
        fs::create_dir(&outside)
            .unwrap_or_else(|error| panic!("create outside directory: {error}"));
        fs::write(outside.join("file"), b"outside")
            .unwrap_or_else(|error| panic!("write outside file: {error}"));
        let enumerated = enumerate_toolchain(&root)
            .unwrap_or_else(|error| panic!("enumerate nested toolchain: {error:?}"));
        fs::rename(&admitted_directory, &moved_directory)
            .unwrap_or_else(|error| panic!("move admitted directory: {error}"));
        symlink(&outside, &admitted_directory)
            .unwrap_or_else(|error| panic!("replace directory with symlink: {error}"));
        assert!(matches!(
            snapshot_toolchain_files(&root, enumerated, None),
            Err(ToolFailure::Io(_))
        ));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn rc3_retention_publishes_only_complete_bundles() {
        let obligation = test_obligation(703);
        let identity = test_identity();
        let status = ToolRunStatus::Blocked(ToolFailure::Unknown);
        let transcripts = vec![test_transcript(b"answer", b"warning")];
        let record = formal_run_record(&identity, &obligation, &status, &transcripts)
            .unwrap_or_else(|_| unreachable!());
        let record_hash = commitment::<RustCryptoSha256>(
            Domain::new("zeno-fcis/formal-run", 3).unwrap_or_else(|_| unreachable!()),
            &record,
        )
        .unwrap_or_else(|_| unreachable!());
        let run = std::sync::Arc::new(ToolRun {
            identity,
            obligation,
            status,
            transcripts,
            toolchain_inventory: None,
            record_hash,
        });
        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-retention-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut workers = Vec::new();
        for _ in 0..4 {
            let root = root.clone();
            let run = std::sync::Arc::clone(&run);
            workers.push(thread::spawn(move || retain_run(&root, &run)));
        }
        let directories = workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("retention worker panicked"))
                    .unwrap_or_else(|error| panic!("retain run: {error:?}"))
            })
            .collect::<Vec<_>>();
        assert!(directories.windows(2).all(|pair| pair[0] == pair[1]));
        let expected = retained_run_files(&run).unwrap_or_else(|_| unreachable!());
        verify_retained_bundle(&directories[0], &expected)
            .unwrap_or_else(|error| panic!("verify retained bundle: {error:?}"));
        let retained_record = fs::read(directories[0].join("formal-run-record.bin"))
            .unwrap_or_else(|error| panic!("read canonical run record: {error}"));
        let retained_hash = commitment::<RustCryptoSha256>(
            Domain::new("zeno-fcis/formal-run", 3).unwrap_or_else(|_| unreachable!()),
            &retained_record,
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(retained_hash, run.record_hash());
        let metadata = fs::read(directories[0].join("record.json"))
            .unwrap_or_else(|error| panic!("read record metadata: {error}"));
        assert!(
            !metadata
                .windows(b"/untrusted/tool/path".len())
                .any(|window| { window == b"/untrusted/tool/path" })
        );
        assert!(
            !fs::read_dir(&root)
                .unwrap_or_else(|error| panic!("read retention root: {error}"))
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
        );
        fs::write(directories[0].join("stdout"), b"tampered")
            .unwrap_or_else(|error| panic!("tamper retained bundle: {error}"));
        assert_eq!(retain_run(&root, &run), Err(ToolFailure::RetentionConflict));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn rc3_formal_tools_bind_exact_claim_and_strong_next() {
        let claim = ClaimDecl::new(
            id(1),
            name("next"),
            vec![BackendId::Z3],
            ClaimMode::Finite { horizon: 1 },
            ClaimFormula::Temporal(TemporalFormula::Next(Box::new(TemporalFormula::Atom(
                RelExpr::Bool(true),
            )))),
        );
        let export = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let source = String::from_utf8_lossy(export.source());
        assert!(source.contains("(declare-const zeno_trace_len Int)"));
        assert!(source.contains("(assert (not (and true false)))"));
    }
    #[test]
    fn unknown_and_z3_unsat_remain_blocked() {
        let config = ToolConfig {
            backend: ToolBackend::Z3,
            path: "z3".into(),
            version: Z3_VERSION.into(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 1,
            max_output_bytes: 100,
            allowed_axioms: Vec::new(),
        };
        let claim = ClaimDecl::new(
            id(1),
            name("closed"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(true)),
        );
        let obligation = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());

        let unknown = ProcessOutput {
            status: success_status(),
            stdout: b"unknown\n".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            classify(&config, &unknown, &obligation),
            ToolRunStatus::Blocked(ToolFailure::Unknown)
        );
        let unsat = ProcessOutput {
            status: success_status(),
            stdout: b"unsat\n".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            classify(&config, &unsat, &obligation),
            ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
        );
    }

    #[test]
    fn rc3_custom_lean_identity_cannot_receive_kernel_checked() {
        let claim = ClaimDecl::new(
            id(704),
            name("qualified_lean_only"),
            vec![BackendId::Lean],
            ClaimMode::UnboundedProof,
            ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::Bool(true))),
        );
        let obligation = export_lean(&claim).unwrap_or_else(|_| unreachable!());
        let output = ProcessOutput {
            status: success_status(),
            stdout: b"'claim' depends on axioms: []\n".to_vec(),
            stderr: Vec::new(),
        };
        let mut config = ToolConfig {
            backend: ToolBackend::Lean,
            path: PathBuf::from("/custom/lean/bin/lean"),
            version: LEAN_VERSION.to_owned(),
            sha256: "1".repeat(64),
            runtime: Some(ToolRuntimeConfig {
                root: PathBuf::from("/custom/lean"),
                tree_sha256: "2".repeat(64),
            }),
            timeout_ms: 1_000,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(
            classify(&config, &output, &obligation),
            ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
        );
        config
            .runtime
            .as_mut()
            .unwrap_or_else(|| unreachable!())
            .tree_sha256 = LEAN_LINUX_X86_64_TREE_SHA256.to_owned();
        assert_eq!(
            classify(&config, &output, &obligation),
            ToolRunStatus::KernelChecked
        );
    }

    #[test]
    fn rc3_formal_fail_closed_and_model_replay() {
        let config = ToolConfig {
            backend: ToolBackend::Z3,
            path: "z3".into(),
            version: Z3_VERSION.into(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 1,
            max_output_bytes: 100,
            allowed_axioms: Vec::new(),
        };
        let claim = ClaimDecl::new(
            id(9),
            name("refuted"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(false)),
        );
        let obligation = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let model = ProcessOutput {
            status: success_status(),
            stdout: b"sat\n(model)\n".to_vec(),
            stderr: Vec::new(),
        };
        assert_eq!(
            classify(&config, &model, &obligation),
            ToolRunStatus::Refuted
        );

        for (stdout, expected) in [
            (
                b"unknown\n".as_slice(),
                ToolRunStatus::Blocked(ToolFailure::Unknown),
            ),
            (
                b"unsat\n".as_slice(),
                ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence),
            ),
        ] {
            assert_eq!(
                classify(
                    &config,
                    &ProcessOutput {
                        status: success_status(),
                        stdout: stdout.to_vec(),
                        stderr: Vec::new(),
                    },
                    &obligation,
                ),
                expected
            );
        }

        let true_claim = ClaimDecl::new(
            id(11),
            name("not_refuted"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(true)),
        );
        let true_obligation =
            export_smt(&true_claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        assert_eq!(
            replay_model(&true_obligation, "sat\n(model)\n"),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );

        let missing = ToolConfig {
            backend: ToolBackend::Z3,
            path: std::env::temp_dir()
                .join(format!("zeno-fcis-missing-tool-{}", std::process::id())),
            version: Z3_VERSION.into(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 100,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(verify_tool(&missing), Err(ToolFailure::Missing));

        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let hash_mismatch = ToolConfig {
            backend: ToolBackend::Z3,
            path: executable.clone(),
            version: Z3_VERSION.into(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 100,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(verify_tool(&hash_mismatch), Err(ToolFailure::HashMismatch));

        let wrong_backend = ToolConfig {
            backend: ToolBackend::Cvc5,
            path: executable.clone(),
            version: CVC5_VERSION.into(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 100,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(
            execute_tool(&wrong_backend, obligation.clone()),
            Err(ToolFailure::UnsupportedEvidence)
        );

        assert!(matches!(
            run_fixed(
                &executable,
                &[
                    "--ignored",
                    "--exact",
                    "tests::process_helper_timeout",
                    "--nocapture",
                ],
                None,
                20,
                4096,
            ),
            Err(ToolFailure::Timeout)
        ));

        let crash = run_fixed(
            &executable,
            &[
                "--ignored",
                "--exact",
                "tests::process_helper_crash",
                "--nocapture",
            ],
            None,
            1_000,
            4096,
        )
        .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            classify(&config, &crash, &obligation),
            ToolRunStatus::Failed(ToolFailure::Crash(Some(23)))
        );

        assert!(matches!(
            run_fixed(
                &executable,
                &[
                    "--ignored",
                    "--exact",
                    "tests::process_helper_output_limit",
                    "--nocapture",
                ],
                None,
                1_000,
                64,
            ),
            Err(ToolFailure::OutputLimit)
        ));
    }

    #[test]
    fn rc3_formal_tools_translation_parity() {
        let formula = RelExpr::And(
            Box::new(RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Div(
                    zeno_fcis_spec::DivisionMode::Floor,
                    Box::new(ValueExpr::Int(5)),
                    Box::new(ValueExpr::Int(-2)),
                ),
                ValueExpr::Int(-3),
            )),
            Box::new(RelExpr::And(
                Box::new(RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Div(
                        zeno_fcis_spec::DivisionMode::Ceil,
                        Box::new(ValueExpr::Int(5)),
                        Box::new(ValueExpr::Int(-2)),
                    ),
                    ValueExpr::Int(-2),
                )),
                Box::new(RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Div(
                        zeno_fcis_spec::DivisionMode::Exact,
                        Box::new(ValueExpr::Int(6)),
                        Box::new(ValueExpr::Int(-2)),
                    ),
                    ValueExpr::Int(-3),
                )),
            )),
        );
        let claim = ClaimDecl::new(
            id(7),
            name("division"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(formula),
        );
        let first = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let second = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        assert_eq!(first.source(), second.source());
        let same_formula = ClaimDecl::new(
            id(70),
            name("division_copy"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            claim.formula().clone(),
        );
        let other = export_smt(&same_formula, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        assert_ne!(first.source_hash(), other.source_hash());
        let source = String::from_utf8_lossy(first.source());
        assert!(source.contains("; claim-id 7\n"));
        assert!(source.contains("(ite (> (- 2) 0) (div 5 (- 2)) (div (- 5) (- (- 2))))"));
        assert!(
            source.contains("(- (ite (> (- 2) 0) (div (- 5) (- 2)) (div (- (- 5)) (- (- 2)))))")
        );
        assert!(source.contains("(= (mod 6 (ite (< (- 2) 0) (- (- 2)) (- 2))) 0)"));
        assert!(source.contains("(not (= (- 2) 0))"));
        assert!(source.contains(&smt_int(i128::MIN)));
        assert!(source.contains(&smt_int(i128::MAX)));

        let finite_claim = ClaimDecl::new(
            id(71),
            name("bounded_next"),
            vec![BackendId::Z3],
            ClaimMode::Finite { horizon: 2 },
            ClaimFormula::Temporal(TemporalFormula::Next(Box::new(TemporalFormula::Atom(
                RelExpr::Bool(true),
            )))),
        );
        let finite = export_smt(&finite_claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let finite_source = String::from_utf8_lossy(finite.source());
        assert!(finite_source.contains("(declare-const zeno_trace_len Int)"));
        assert!(finite_source.contains("(ite (= zeno_trace_len 1) false true)"));
    }

    #[test]
    fn lean_export_preserves_the_exact_relational_atom_and_claim_identity() {
        let path = ProjectionPath::try_new(ProjectionRoot::Pre, vec![id(100)])
            .unwrap_or_else(|| unreachable!());
        let claim = ClaimDecl::new(
            id(501),
            name("unbounded_replay"),
            vec![BackendId::Lean],
            ClaimMode::UnboundedProof,
            ClaimFormula::Temporal(TemporalFormula::Always(Box::new(TemporalFormula::Atom(
                RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Projection(path.clone()),
                    ValueExpr::Projection(path),
                ),
            )))),
        );
        let export = export_lean(&claim).unwrap_or_else(|_| unreachable!());
        let source = String::from_utf8_lossy(export.source());
        assert!(source.contains("-- claim-id 501\n"));
        assert!(source.contains("(observe \"pre_100\" time_0).val"));
        assert!(source.contains("∀ time_0 : Nat, time_0 >= 0"));
        assert!(source.contains(":= by\n  simp [claim_501,"));
        assert!(!source.contains("relational_atom"));

        assert_eq!(
            parse_lean_axioms(
                "'ZenoFCIS.claim_501_checked' depends on axioms: [Quot.sound, propext]\n"
            ),
            Some(vec!["Quot.sound".to_owned(), "propext".to_owned()])
        );
        assert_eq!(
            parse_lean_axioms("'claim' depends on axioms: []\n'other' depends on axioms: []\n"),
            None
        );
    }

    #[test]
    fn finite_smt_checks_every_nonempty_trace_length_and_replays_the_selected_one() {
        let claim = ClaimDecl::new(
            id(8),
            name("bounded_next"),
            vec![BackendId::Z3],
            ClaimMode::Finite { horizon: 2 },
            ClaimFormula::Temporal(TemporalFormula::Next(Box::new(TemporalFormula::Atom(
                RelExpr::Bool(true),
            )))),
        );
        let obligation = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let source = String::from_utf8_lossy(obligation.source());
        assert!(source.contains("(declare-const zeno_trace_len Int)"));
        assert!(source.contains("(assert (and (<= 1 zeno_trace_len) (<= zeno_trace_len 2)))"));
        assert!(source.contains("(ite (= zeno_trace_len 1) false true)"));

        assert_eq!(
            replay_model(&obligation, "sat\n(define-fun zeno_trace_len () Int 1)\n"),
            ToolRunStatus::Refuted
        );
        assert_eq!(
            replay_model(&obligation, "sat\n(define-fun zeno_trace_len () Int 2)\n"),
            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed)
        );
    }

    #[test]
    fn smt_checked_arithmetic_makes_overflow_a_counterexample_condition() {
        let claim = ClaimDecl::new(
            id(10),
            name("overflow"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Add(
                    Box::new(ValueExpr::Int(i128::MAX)),
                    Box::new(ValueExpr::Int(1)),
                ),
                ValueExpr::Int(i128::MIN),
            )),
        );
        let obligation = export_smt(&claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let source = String::from_utf8_lossy(obligation.source());
        let sum = format!("(+ {} 1)", smt_int(i128::MAX));
        assert!(source.contains(&smt_i128_range(&sum)));
        assert!(source.contains("(assert (not "));
    }

    #[test]
    fn rc3_formal_export_limits_fail_closed_before_rendering() {
        let claim = ClaimDecl::new(
            id(81),
            name("huge_horizon"),
            vec![BackendId::Z3],
            ClaimMode::Finite {
                horizon: MAX_FINITE_HORIZON + 1,
            },
            ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::Bool(true))),
        );
        assert_eq!(
            export_smt(&claim, ToolBackend::Z3),
            Err(ExportError::ResourceLimit)
        );

        let nested = ClaimDecl::new(
            id(82),
            name("small_envelope"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Not(Box::new(RelExpr::Bool(true)))),
        );
        let limits = ExportLimits::try_new(1, 1, 1, 1, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            export_smt_with_limits(&nested, ToolBackend::Z3, limits),
            Err(ExportError::ResourceLimit)
        );

        let nested_temporal = ClaimDecl::new(
            id(83),
            name("nested_temporal_envelope"),
            vec![BackendId::Z3],
            ClaimMode::Finite {
                horizon: MAX_FINITE_HORIZON,
            },
            ClaimFormula::Temporal(TemporalFormula::Always(Box::new(TemporalFormula::Always(
                Box::new(TemporalFormula::Always(Box::new(TemporalFormula::Atom(
                    RelExpr::Bool(true),
                )))),
            )))),
        );
        let started = Instant::now();
        assert_eq!(
            export_smt(&nested_temporal, ToolBackend::Z3),
            Err(ExportError::ResourceLimit)
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    #[cfg(unix)]
    fn rc3_tool_admission_checks_exact_size_hash_status_and_version() {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let small_path = std::env::temp_dir().join(format!(
            "zeno-fcis-tool-size-{}-{sequence}",
            std::process::id()
        ));
        let oversized_path = small_path.with_extension("oversized");
        fs::write(&small_path, [0_u8; 4]).unwrap_or_else(|_| unreachable!());
        fs::write(&oversized_path, [0_u8; 5]).unwrap_or_else(|_| unreachable!());
        let config = |path: PathBuf, sha256: String, version: &str| ToolConfig {
            backend: ToolBackend::Z3,
            path,
            version: version.into(),
            sha256,
            runtime: None,
            timeout_ms: 10_000,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        let exact_size = config(small_path.clone(), "0".repeat(64), "");
        assert!(matches!(
            check_tool_with_max_binary_bytes(&exact_size, 4),
            Err(ToolFailure::HashMismatch)
        ));
        let oversized = config(oversized_path.clone(), "0".repeat(64), "");
        assert!(matches!(
            check_tool_with_max_binary_bytes(&oversized, 4),
            Err(ToolFailure::BinaryTooLarge)
        ));
        fs::remove_file(&small_path).unwrap_or_else(|_| unreachable!());
        fs::remove_file(&oversized_path).unwrap_or_else(|_| unreachable!());

        let script_path = small_path.with_extension("tool");
        let script = b"#!/bin/sh\nprintf 'Z3 version 1.2.3 - 64 bit\\n'\n";
        fs::write(&script_path, script).unwrap_or_else(|_| unreachable!());
        let expected_hash = RustCryptoSha256::hash(script);
        let expected_hash_hex = hash_hex(expected_hash);
        let accepted = config(script_path.clone(), expected_hash_hex.clone(), "1.2.3");
        let checked = check_tool(&accepted).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(checked.identity.binary_hash(), expected_hash);

        let wrong_version = config(script_path.clone(), expected_hash_hex, "9.9.9");
        assert_eq!(
            check_tool(&wrong_version).err(),
            Some(ToolFailure::VersionMismatch)
        );
        fs::remove_file(script_path).unwrap_or_else(|_| unreachable!());
    }

    #[test]
    #[cfg(unix)]
    fn rc3_lean_runtime_closure_is_portable_private_and_fail_closed() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        fn make_tree(label: &str) -> (PathBuf, Vec<u8>) {
            let root = std::env::temp_dir().join(format!(
                "zeno-fcis-lean-tree-{label}-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("bin"))
                .unwrap_or_else(|error| panic!("create bin: {error}"));
            fs::create_dir_all(root.join("lib/lean"))
                .unwrap_or_else(|error| panic!("create lib: {error}"));
            let script = b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'Lean (version 4.30.0, fake)\\n'\nelif [ \"$#\" -eq 1 ]; then\n  printf \"'claim' depends on axioms: []\\n\"\nelse\n  exit 42\nfi\n".to_vec();
            let executable = root.join("bin/lean");
            fs::write(&executable, &script)
                .unwrap_or_else(|error| panic!("write fake Lean: {error}"));
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
                .unwrap_or_else(|error| panic!("make fake Lean executable: {error}"));
            fs::write(root.join("lib/lean/Init.olean"), b"checked-init")
                .unwrap_or_else(|error| panic!("write fake Init.olean: {error}"));
            (root, script)
        }

        let (root, script) = make_tree("primary");
        let (same_root, _) = make_tree("same");
        let baseline = inspect_lean_toolchain(&root).unwrap_or_else(|error| panic!("{error:?}"));
        let same = inspect_lean_toolchain(&same_root).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(baseline.tree_sha256(), same.tree_sha256());
        assert_eq!(baseline.canonical_json(), same.canonical_json());

        let config = ToolConfig {
            backend: ToolBackend::Lean,
            path: root.join("bin/lean"),
            version: LEAN_VERSION.into(),
            sha256: hash_hex(RustCryptoSha256::hash(&script)),
            runtime: Some(ToolRuntimeConfig {
                root: root.clone(),
                tree_sha256: hash_hex(baseline.tree_sha256()),
            }),
            timeout_ms: 1_000,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        let checked = check_tool(&config).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            checked.identity.runtime_hash(),
            Some(baseline.tree_sha256())
        );
        let private_root = match &checked.execution {
            CheckedExecution::Lean(toolchain) => toolchain.root.clone(),
            CheckedExecution::Single(_) => unreachable!(),
        };
        fs::write(root.join("lib/lean/Init.olean"), b"mutated")
            .unwrap_or_else(|error| panic!("mutate source tree: {error}"));
        assert_eq!(
            fs::read(private_root.join("lib/lean/Init.olean"))
                .unwrap_or_else(|error| panic!("read private Init.olean: {error}")),
            b"checked-init"
        );
        let output = run_lean(
            &config,
            checked.execution.executable(),
            b"-- exact test source\n",
        )
        .unwrap_or_else(|error| panic!("run private fake Lean: {error:?}"));
        assert!(output.status.success());
        assert_eq!(
            parse_lean_axioms(&String::from_utf8_lossy(&output.stdout)),
            Some(Vec::new())
        );
        drop(checked);
        assert!(!private_root.exists());
        assert_eq!(
            check_tool(&config).err(),
            Some(ToolFailure::ToolchainHashMismatch)
        );

        fs::write(root.join("lib/lean/Init.olean"), b"checked-init")
            .unwrap_or_else(|error| panic!("restore Init.olean: {error}"));
        fs::write(root.join("added.olean"), b"added")
            .unwrap_or_else(|error| panic!("add runtime file: {error}"));
        assert_ne!(
            inspect_lean_toolchain(&root)
                .unwrap_or_else(|error| panic!("{error:?}"))
                .tree_sha256(),
            baseline.tree_sha256()
        );
        fs::remove_file(root.join("added.olean"))
            .unwrap_or_else(|error| panic!("remove added runtime file: {error}"));
        symlink("Init.olean", root.join("lib/lean/linked.olean"))
            .unwrap_or_else(|error| panic!("create runtime symlink: {error}"));
        assert_eq!(
            inspect_lean_toolchain(&root),
            Err(ToolFailure::ToolchainUnsupported)
        );

        assert!(version_output_matches(
            ToolBackend::Lean,
            "Lean (version 4.30.0, fake)",
            LEAN_VERSION
        ));
        assert!(!version_output_matches(
            ToolBackend::Lean,
            "Lean (version 4.30.0-modified, fake)",
            LEAN_VERSION
        ));
        assert!(!version_output_matches(
            ToolBackend::Z3,
            "Z3 version 14.16.00 - 64 bit",
            Z3_VERSION
        ));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(same_root);
    }

    #[test]
    #[cfg(unix)]
    fn rc3_tools_manifest_requires_only_lean_to_bind_a_runtime_closure() {
        let path = std::env::temp_dir().join(format!(
            "zeno-fcis-tools-manifest-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let lean = serde_json::json!({
            "format": TOOLS_MANIFEST_FORMAT,
            "tools": [{
                "backend": "lean",
                "path": "/opt/lean-4.30.0/bin/lean",
                "version": LEAN_VERSION,
                "sha256": "1".repeat(64),
                "runtime": {
                    "root": "/opt/lean-4.30.0",
                    "tree_sha256": "2".repeat(64)
                },
                "timeout_ms": 30_000,
                "max_output_bytes": 1_048_576,
                "allowed_axioms": []
            }]
        });
        fs::write(
            &path,
            serde_json::to_vec(&lean).unwrap_or_else(|_| unreachable!()),
        )
        .unwrap_or_else(|error| panic!("write valid manifest: {error}"));
        let loaded = load_tools_manifest(&path).unwrap_or_else(|error| panic!("{error:?}"));
        assert!(loaded.tool(ToolBackend::Lean).is_some());
        assert_eq!(
            String::from_utf8(loaded.canonical_json().unwrap_or_else(|_| unreachable!()))
                .unwrap_or_else(|_| unreachable!()),
            "{\"format\":\"zeno-fcis/tools/2\",\"tools\":[{\"backend\":\"lean\",\"path\":\"/opt/lean-4.30.0/bin/lean\",\"version\":\"4.30.0\",\"sha256\":\"1111111111111111111111111111111111111111111111111111111111111111\",\"runtime\":{\"root\":\"/opt/lean-4.30.0\",\"tree_sha256\":\"2222222222222222222222222222222222222222222222222222222222222222\"},\"timeout_ms\":30000,\"max_output_bytes\":1048576,\"allowed_axioms\":[]}]}"
        );

        let mut old_format = lean.clone();
        old_format["format"] = serde_json::json!("zeno-fcis/tools/1");
        fs::write(
            &path,
            serde_json::to_vec(&old_format).unwrap_or_else(|_| unreachable!()),
        )
        .unwrap_or_else(|error| panic!("write old-format manifest: {error}"));
        assert_eq!(
            load_tools_manifest(&path),
            Err(ManifestError::WrongFormat {
                expected: TOOLS_MANIFEST_FORMAT,
                actual: "zeno-fcis/tools/1".to_owned(),
            })
        );

        let mut missing_runtime = lean.clone();
        missing_runtime["tools"][0]
            .as_object_mut()
            .unwrap_or_else(|| unreachable!())
            .remove("runtime");
        fs::write(
            &path,
            serde_json::to_vec(&missing_runtime).unwrap_or_else(|_| unreachable!()),
        )
        .unwrap_or_else(|error| panic!("write missing-runtime manifest: {error}"));
        assert_eq!(
            load_tools_manifest(&path),
            Err(ManifestError::InvalidRuntime)
        );

        let mut unexpected_runtime = lean;
        unexpected_runtime["tools"][0]["backend"] = serde_json::json!("z3");
        unexpected_runtime["tools"][0]["version"] = serde_json::json!(Z3_VERSION);
        fs::write(
            &path,
            serde_json::to_vec(&unexpected_runtime).unwrap_or_else(|_| unreachable!()),
        )
        .unwrap_or_else(|error| panic!("write unexpected-runtime manifest: {error}"));
        assert_eq!(
            load_tools_manifest(&path),
            Err(ManifestError::InvalidRuntime)
        );

        fs::write(&path, vec![b' '; MAX_TOOLS_MANIFEST_BYTES + 1])
            .unwrap_or_else(|error| panic!("write oversized manifest: {error}"));
        assert_eq!(load_tools_manifest(&path), Err(ManifestError::TooLarge));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rc3_export_preflight_has_exact_mode_and_resource_boundaries() {
        let finite = |horizon| {
            ClaimDecl::new(
                id(91),
                name("finite_boundary"),
                vec![BackendId::Z3],
                ClaimMode::Finite { horizon },
                ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::Bool(true))),
            )
        };
        assert_eq!(
            preflight_export(&finite(0), ExportKind::Smt, ExportLimits::default()),
            Err(ExportError::InvalidFormula)
        );
        assert_eq!(
            preflight_export(
                &finite(MAX_FINITE_HORIZON),
                ExportKind::Smt,
                ExportLimits::default(),
            ),
            Ok(())
        );
        assert_eq!(
            preflight_export(
                &finite(MAX_FINITE_HORIZON + 1),
                ExportKind::Smt,
                ExportLimits::default(),
            ),
            Err(ExportError::ResourceLimit)
        );

        let unbounded = ClaimDecl::new(
            id(92),
            name("unbounded_boundary"),
            vec![BackendId::Z3],
            ClaimMode::UnboundedProof,
            ClaimFormula::Temporal(TemporalFormula::Atom(RelExpr::Bool(true))),
        );
        assert_eq!(
            preflight_export(&unbounded, ExportKind::Smt, ExportLimits::default()),
            Err(ExportError::UnsupportedMode)
        );
        let relational = ClaimDecl::new(
            id(93),
            name("relational_boundary"),
            vec![BackendId::Lean],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Not(Box::new(RelExpr::Bool(true)))),
        );
        assert_eq!(
            preflight_export(&relational, ExportKind::Lean, ExportLimits::default()),
            Err(ExportError::UnsupportedMode)
        );

        let exact = ExportLimits::try_new(2, 2, 2, 2, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            preflight_export(&relational, ExportKind::Smt, exact),
            Ok(())
        );
        let node_only = ClaimDecl::new(
            id(94),
            name("node_boundary"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::And(
                Box::new(RelExpr::Bool(true)),
                Box::new(RelExpr::Bool(true)),
            )),
        );
        let node_limit = ExportLimits::try_new(2, 2, 3, 3, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            preflight_export(&node_only, ExportKind::Smt, node_limit),
            Err(ExportError::ResourceLimit)
        );
        let depth_limit = ExportLimits::try_new(2, 2, 1, 2, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            preflight_export(&relational, ExportKind::Smt, depth_limit),
            Err(ExportError::ResourceLimit)
        );
        let exact_operations =
            ExportLimits::try_new(2, 2, 2, 4, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            preflight_export(&finite(2), ExportKind::Smt, exact_operations),
            Ok(())
        );
        let operation_limit =
            ExportLimits::try_new(2, 2, 2, 3, 4096).unwrap_or_else(|| unreachable!());
        assert_eq!(
            preflight_export(&finite(2), ExportKind::Smt, operation_limit),
            Err(ExportError::ResourceLimit)
        );
    }

    #[test]
    fn rc3_process_output_accepts_the_exact_byte_limit() {
        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let arguments = [
            "--ignored",
            "--exact",
            "tests::process_helper_checked_copy",
            "--nocapture",
        ];
        let probe = run_fixed(&executable, &arguments, None, 1_000, 4096)
            .unwrap_or_else(|_| unreachable!());
        let exact_limit = probe.stdout.len().saturating_add(probe.stderr.len());
        let exact = run_fixed(&executable, &arguments, None, 1_000, exact_limit)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            exact.stdout.len().saturating_add(exact.stderr.len()),
            exact_limit
        );
    }

    #[test]
    fn rc3_smt_predicate_symbols_are_injective() {
        let claim = |predicate: &str| {
            ClaimDecl::new(
                id(83),
                name("symbol_identity"),
                vec![BackendId::Z3],
                ClaimMode::Relational,
                ClaimFormula::Relational(RelExpr::Predicate {
                    name: name(predicate),
                    arguments: Box::new([]),
                }),
            )
        };
        let hyphen =
            export_smt(&claim("foo-bar"), ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let underscore =
            export_smt(&claim("foo_bar"), ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        assert_ne!(hyphen.source(), underscore.source());
        assert_ne!(hyphen.source_hash(), underscore.source_hash());
        assert!(String::from_utf8_lossy(hyphen.source()).contains("pred_id7_666f6f2d626172"));
        assert!(String::from_utf8_lossy(underscore.source()).contains("pred_id7_666f6f5f626172"));
    }

    #[test]
    fn rc3_process_timeout_includes_blocked_stdin_delivery() {
        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let input = vec![b'x'; 1024 * 1024];
        let start = Instant::now();
        assert!(matches!(
            run_fixed(
                &executable,
                &[
                    "--ignored",
                    "--exact",
                    "tests::process_helper_timeout",
                    "--nocapture",
                ],
                Some(&input),
                20,
                4096,
            ),
            Err(ToolFailure::Timeout)
        ));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn rc3_process_timeout_kills_descendant_holding_solver_pipes() {
        use nix::errno::Errno;
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-pipe-holder-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("create helper root: {error}"));
        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let root_bytes = root
            .to_str()
            .unwrap_or_else(|| unreachable!())
            .as_bytes()
            .to_vec();
        let start = Instant::now();
        assert_eq!(
            run_fixed(
                &executable,
                &[
                    "--ignored",
                    "--exact",
                    "tests::process_helper_spawns_pipe_holder",
                    "--nocapture",
                ],
                Some(&root_bytes),
                2_000,
                4096,
            )
            .err(),
            Some(ToolFailure::Timeout)
        );
        assert!(start.elapsed() < Duration::from_secs(5));

        let parent = fs::read_to_string(root.join("parent"))
            .unwrap_or_else(|error| panic!("read parent identity: {error}"));
        let descendant = fs::read_to_string(root.join("descendant"))
            .unwrap_or_else(|error| panic!("read descendant identity: {error}"));
        let parse_identity = |value: &str| {
            let mut fields = value.split_whitespace();
            let pid = fields
                .next()
                .and_then(|field| field.parse::<i32>().ok())
                .unwrap_or_else(|| panic!("missing helper pid"));
            let group = fields
                .next()
                .and_then(|field| field.parse::<i32>().ok())
                .unwrap_or_else(|| panic!("missing helper process group"));
            assert!(fields.next().is_none());
            (pid, group)
        };
        let (_, parent_group) = parse_identity(&parent);
        let (descendant_pid, descendant_group) = parse_identity(&descendant);
        assert_eq!(descendant_group, parent_group);

        let descendant_pid = Pid::from_raw(descendant_pid);
        let reaped = (0..200).any(|_| match kill(descendant_pid, None) {
            Err(Errno::ESRCH) => true,
            Ok(()) => {
                thread::sleep(Duration::from_millis(10));
                false
            }
            Err(error) => panic!("check descendant process: {error}"),
        });
        if !reaped {
            let _ = kill(descendant_pid, Signal::SIGKILL);
        }
        let _ = fs::remove_dir_all(&root);
        assert!(reaped, "descendant process survived process-group timeout");
    }

    #[cfg(unix)]
    #[test]
    fn rc3_process_success_kills_descendants_after_collecting_output() {
        use nix::errno::Errno;
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-success-descendant-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap_or_else(|error| panic!("create helper root: {error}"));
        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let root_bytes = root
            .to_str()
            .unwrap_or_else(|| unreachable!())
            .as_bytes()
            .to_vec();
        let output = run_fixed(
            &executable,
            &[
                "--ignored",
                "--exact",
                "tests::process_helper_spawns_detached_pipe_child",
                "--nocapture",
            ],
            Some(&root_bytes),
            2_000,
            4096,
        )
        .unwrap_or_else(|error| panic!("run success helper: {error:?}"));
        assert!(output.status.success());
        let descendant_pid = fs::read_to_string(root.join("descendant"))
            .unwrap_or_else(|error| panic!("read descendant pid: {error}"))
            .trim()
            .parse::<i32>()
            .unwrap_or_else(|error| panic!("parse descendant pid: {error}"));
        let descendant_pid = Pid::from_raw(descendant_pid);
        let reaped = (0..200).any(|_| match kill(descendant_pid, None) {
            Err(Errno::ESRCH) => true,
            Ok(()) => {
                thread::sleep(Duration::from_millis(10));
                false
            }
            Err(error) => panic!("check descendant process: {error}"),
        });
        if !reaped {
            let _ = kill(descendant_pid, Signal::SIGKILL);
        }
        let _ = fs::remove_dir_all(root);
        assert!(reaped, "descendant process survived successful tool run");
    }

    #[cfg(unix)]
    #[test]
    fn rc3_lean_runtime_mutation_during_version_probe_is_blocked() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "zeno-fcis-mutating-lean-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("bin"))
            .unwrap_or_else(|error| panic!("create fake Lean bin: {error}"));
        fs::create_dir_all(root.join("lib/lean"))
            .unwrap_or_else(|error| panic!("create fake Lean lib: {error}"));
        let script = b"#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  root=${0%/bin/lean}\n  /bin/chmod 600 \"$root/lib/lean/Init.olean\"\n  printf 'mutated' > \"$root/lib/lean/Init.olean\"\n  printf 'Lean (version 4.30.0, fake)\\n'\nelse\n  printf \"'claim' depends on axioms: []\\n\"\nfi\n";
        let executable = root.join("bin/lean");
        fs::write(&executable, script).unwrap_or_else(|error| panic!("write fake Lean: {error}"));
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .unwrap_or_else(|error| panic!("make fake Lean executable: {error}"));
        fs::write(root.join("lib/lean/Init.olean"), b"checked-init")
            .unwrap_or_else(|error| panic!("write fake Lean runtime: {error}"));
        let inventory = inspect_lean_toolchain(&root)
            .unwrap_or_else(|error| panic!("inventory fake Lean: {error:?}"));
        let config = ToolConfig {
            backend: ToolBackend::Lean,
            path: executable,
            version: LEAN_VERSION.to_owned(),
            sha256: hash_hex(RustCryptoSha256::hash(script)),
            runtime: Some(ToolRuntimeConfig {
                root: root.clone(),
                tree_sha256: hash_hex(inventory.tree_sha256()),
            }),
            timeout_ms: 1_000,
            max_output_bytes: 4096,
            allowed_axioms: Vec::new(),
        };
        assert_eq!(
            check_tool(&config).err(),
            Some(ToolFailure::ToolchainHashMismatch)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rc3_private_executable_preserves_the_admitted_bytes() {
        let executable = std::env::current_exe()
            .unwrap_or_else(|error| panic!("locate test executable: {error}"));
        let bytes =
            fs::read(executable).unwrap_or_else(|error| panic!("read test executable: {error}"));
        let expected_hash = RustCryptoSha256::hash(&bytes);
        let admitted = PrivateExecutable::create(ToolBackend::Z3, &bytes)
            .unwrap_or_else(|error| panic!("admit test executable: {error:?}"));
        let output = run_fixed(
            admitted.path(),
            &[
                "--ignored",
                "--exact",
                "tests::process_helper_checked_copy",
                "--nocapture",
            ],
            None,
            1_000,
            4096,
        )
        .unwrap_or_else(|error| panic!("run admitted test executable: {error:?}"));
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("checked-copy"));
        let retained_bytes = fs::read(admitted.path())
            .unwrap_or_else(|error| panic!("read admitted test executable: {error}"));
        assert_eq!(RustCryptoSha256::hash(&retained_bytes), expected_hash);
    }

    #[test]
    #[ignore = "requires the workflow-pinned Lean distribution"]
    fn pinned_lean_translation_kernel_checks() {
        let executable =
            PathBuf::from(std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| unreachable!()));
        let runtime_root = PathBuf::from(
            std::env::var_os("ZENO_FCIS_LEAN_ROOT").unwrap_or_else(|| unreachable!()),
        );
        let expected_runtime_hash = LEAN_LINUX_X86_64_TREE_SHA256;
        let executable_bytes =
            fs::read(&executable).unwrap_or_else(|error| panic!("read pinned Lean: {error}"));
        let config = ToolConfig {
            backend: ToolBackend::Lean,
            path: executable,
            version: LEAN_VERSION.to_owned(),
            sha256: hash_hex(RustCryptoSha256::hash(&executable_bytes)),
            runtime: Some(ToolRuntimeConfig {
                root: runtime_root,
                tree_sha256: expected_runtime_hash.to_owned(),
            }),
            timeout_ms: 30_000,
            max_output_bytes: 1024 * 1024,
            allowed_axioms: vec!["Quot.sound".to_owned(), "propext".to_owned()],
        };
        let path = ProjectionPath::try_new(ProjectionRoot::Pre, vec![id(100)])
            .unwrap_or_else(|| unreachable!());
        let claim = ClaimDecl::new(
            id(501),
            name("unbounded_state_reflexivity"),
            vec![BackendId::Lean],
            ClaimMode::UnboundedProof,
            ClaimFormula::Temporal(TemporalFormula::Always(Box::new(TemporalFormula::Atom(
                RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Projection(path.clone()),
                    ValueExpr::Projection(path),
                ),
            )))),
        );
        let obligation = export_lean(&claim).unwrap_or_else(|_| unreachable!());
        let run = execute_tool(&config, obligation).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(run.status(), &ToolRunStatus::KernelChecked);
        assert_eq!(
            run.identity().runtime_hash().map(hash_hex),
            Some(expected_runtime_hash.to_owned())
        );
        assert_eq!(
            run.toolchain_inventory()
                .map(ToolchainInventory::tree_sha256)
                .map(hash_hex),
            Some(expected_runtime_hash.to_owned())
        );
    }

    /// Every counterexample to an inductive step in a box that the assumed
    /// laws bound, found by replaying each transition in the box. Assignments
    /// outside the box break an assumed law, so they are never counterexamples.
    fn exhaustive_step_counterexamples(obligation: &ExportedObligation) -> BTreeSet<String> {
        let kinds: &[Option<i128>] = if obligation.hypotheses().splits_decisions() {
            &[Some(ACCEPT_KIND), Some(COMMITTED_FAILURE_KIND)]
        } else {
            &[None]
        };
        let mut found = BTreeSet::new();
        for kind in kinds {
            for pre in 0..=4 {
                for amount in 0..=2 {
                    for post in 0..=4 {
                        let mut text = transition(pre, amount, post);
                        if let Some(kind) = kind {
                            text.insert_str(
                                text.len() - 2,
                                &format!("(define-fun {DECISION_KIND} () Int {kind})\n"),
                            );
                        }
                        match replay_model(obligation, &text) {
                            ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed) => {}
                            status => {
                                found.insert(run_status_label(&status));
                            }
                        }
                    }
                }
            }
        }
        found
    }

    fn run_status_label(status: &ToolRunStatus) -> String {
        match status {
            ToolRunStatus::Undefined(reason) => format!("undefined:{}", reason.name()),
            other => run_status_name(other).to_owned(),
        }
    }

    #[test]
    #[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
    fn pinned_inductive_steps_agree_with_exhaustive_replay() {
        let solver = |backend, variable, version: &str| ToolConfig {
            backend,
            path: PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| unreachable!())),
            version: version.to_owned(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 60_000,
            max_output_bytes: 8 * 1024 * 1024,
            allowed_axioms: Vec::new(),
        };
        let solvers = [
            solver(ToolBackend::Cvc5, "ZENO_FCIS_CVC5", CVC5_VERSION),
            solver(ToolBackend::Z3, "ZENO_FCIS_Z3", Z3_VERSION),
        ];
        // Law 400 bounds every observed value, so the box 0..=4 by 0..=2 by
        // 0..=4 holds every transition the laws admit.
        let spec = counter_project(
            "law 403 reset = post.100.110 == 0;\n\
             law 404 in_box = pre.100.110 >= 0 && pre.100.110 <= 4 && post.100.110 >= 0 && post.100.110 <= 4;\n\
             claim 600 nonnegative all inductive assume [404, 402] accept [401] failure [403] = pre.100.110 >= 0;\n\
             claim 601 at_most_three all inductive assume [404, 402] accept [401] failure [403] = pre.100.110 <= 3;\n\
             claim 602 not_two all inductive assume [404, 402] accept [401] failure [403] = pre.100.110 != 2;\n\
             claim 603 doubled all inductive assume [404, 402] accept [401] failure [403] = pre.100.110 * 2 <= 8;\n\
             claim 604 quotient all inductive assume [404, 402] accept [401] failure [403] = div_floor(pre.100.110, pre.100.110 - 2) >= 0;\n\
             claim 605 scaled all inductive assume [404, 402] accept [401] failure [403] = pre.100.110 * 10000000000000000000 * 5000000000000000000 >= 0;\n\
             claim 606 accepts_only all inductive assume [404, 402] accept [401] = pre.100.110 <= 2;\n\
             claim 607 unsplit all inductive assume [404, 402, 401] = pre.100.110 >= 0;\n",
        );
        let mut verdicts = BTreeMap::new();
        for claim in spec.claims() {
            for config in &solvers {
                let obligation = export_inductive_smt(claim, spec.laws(), config.backend)
                    .unwrap_or_else(|error| panic!("claim {}: {error:?}", claim.id().get()));
                let expected = exhaustive_step_counterexamples(&obligation);
                let output = run_smt(config, &config.path, obligation.source())
                    .unwrap_or_else(|error| panic!("claim {}: {error:?}", claim.id().get()));
                let status = classify(config, output.final_output(), &obligation);
                let holds = matches!(
                    (config.backend, &status),
                    (ToolBackend::Cvc5, ToolRunStatus::ProposedUnsat)
                        | (
                            ToolBackend::Z3,
                            ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
                        )
                );
                if expected.is_empty() {
                    assert!(holds, "claim {} {config:?}: {status:?}", claim.id().get());
                } else {
                    assert!(
                        expected.contains(&run_status_label(&status)),
                        "claim {} {:?}: {status:?} not in {expected:?}",
                        claim.id().get(),
                        config.backend
                    );
                }
                verdicts.insert((claim.id().get(), config.backend.name()), expected);
            }
        }
        // The collection covers every outcome: holds, refuted, undefined, and both.
        let outcomes: BTreeSet<_> = verdicts.values().cloned().collect();
        assert!(outcomes.contains(&BTreeSet::new()));
        assert!(
            outcomes
                .iter()
                .any(|set| set.len() == 1 && set.contains("refuted"))
        );
        assert!(
            outcomes
                .iter()
                .any(|set| set.len() == 1 && set.contains("undefined:overflow"))
        );
        assert!(outcomes.iter().any(|set| set.len() > 1));
    }

    #[test]
    #[ignore = "requires the workflow-pinned CVC5 and Z3 executables"]
    fn pinned_smt_translation_differential_check() {
        let cvc5 =
            PathBuf::from(std::env::var_os("ZENO_FCIS_CVC5").unwrap_or_else(|| unreachable!()));
        let z3 = PathBuf::from(std::env::var_os("ZENO_FCIS_Z3").unwrap_or_else(|| unreachable!()));
        let true_claim = ClaimDecl::new(
            id(601),
            name("solver_true"),
            vec![BackendId::Cvc5],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(true)),
        );
        let true_obligation =
            export_smt(&true_claim, ToolBackend::Cvc5).unwrap_or_else(|_| unreachable!());
        let cvc5_config = ToolConfig {
            backend: ToolBackend::Cvc5,
            path: cvc5,
            version: CVC5_VERSION.to_owned(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 30_000,
            max_output_bytes: 8 * 1024 * 1024,
            allowed_axioms: Vec::new(),
        };
        let cvc5_output = run_smt(&cvc5_config, &cvc5_config.path, true_obligation.source())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            classify(&cvc5_config, cvc5_output.final_output(), &true_obligation),
            ToolRunStatus::ProposedUnsat,
            "CVC5 stdout:\n{}\nCVC5 stderr:\n{}",
            String::from_utf8_lossy(&cvc5_output.final_output().stdout),
            String::from_utf8_lossy(&cvc5_output.final_output().stderr)
        );

        let false_claim = ClaimDecl::new(
            id(602),
            name("solver_false"),
            vec![BackendId::Z3],
            ClaimMode::Relational,
            ClaimFormula::Relational(RelExpr::Bool(false)),
        );
        let false_obligation =
            export_smt(&false_claim, ToolBackend::Z3).unwrap_or_else(|_| unreachable!());
        let z3_config = ToolConfig {
            backend: ToolBackend::Z3,
            path: z3,
            version: Z3_VERSION.to_owned(),
            sha256: "0".repeat(64),
            runtime: None,
            timeout_ms: 30_000,
            max_output_bytes: 8 * 1024 * 1024,
            allowed_axioms: Vec::new(),
        };
        let z3_output = run_smt(&z3_config, &z3_config.path, false_obligation.source())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            classify(&z3_config, z3_output.final_output(), &false_obligation),
            ToolRunStatus::Refuted
        );

        let variable = name("x");
        let boundary_claims = [
            ClaimDecl::new(
                id(603),
                name("signed_floor"),
                vec![BackendId::Cvc5, BackendId::Z3],
                ClaimMode::Relational,
                ClaimFormula::Relational(RelExpr::Compare(
                    CompareOp::Eq,
                    ValueExpr::Div(
                        zeno_fcis_spec::DivisionMode::Floor,
                        Box::new(ValueExpr::Int(5)),
                        Box::new(ValueExpr::Int(-2)),
                    ),
                    ValueExpr::Int(-3),
                )),
            ),
            ClaimDecl::new(
                id(604),
                name("bounded_squares"),
                vec![BackendId::Cvc5, BackendId::Z3],
                ClaimMode::Relational,
                ClaimFormula::Relational(RelExpr::ForAll {
                    variable: variable.clone(),
                    start: -3,
                    end: 4,
                    body: Box::new(RelExpr::Compare(
                        CompareOp::GreaterEq,
                        ValueExpr::Mul(
                            Box::new(ValueExpr::Var(variable.clone())),
                            Box::new(ValueExpr::Var(variable)),
                        ),
                        ValueExpr::Int(0),
                    )),
                }),
            ),
            ClaimDecl::new(
                id(605),
                name("finite_always"),
                vec![BackendId::Cvc5, BackendId::Z3],
                ClaimMode::Finite { horizon: 2 },
                ClaimFormula::Temporal(TemporalFormula::Always(Box::new(TemporalFormula::Atom(
                    RelExpr::Bool(true),
                )))),
            ),
        ];
        for claim in boundary_claims {
            for (backend, config) in [
                (ToolBackend::Cvc5, &cvc5_config),
                (ToolBackend::Z3, &z3_config),
            ] {
                let obligation = export_smt(&claim, backend).unwrap_or_else(|_| unreachable!());
                let output = run_smt(config, &config.path, obligation.source())
                    .unwrap_or_else(|_| unreachable!());
                assert_eq!(
                    String::from_utf8_lossy(&output.final_output().stdout)
                        .lines()
                        .next()
                        .unwrap_or(""),
                    "unsat"
                );
                let expected = if backend == ToolBackend::Cvc5 {
                    ToolRunStatus::ProposedUnsat
                } else {
                    ToolRunStatus::Blocked(ToolFailure::UnsupportedEvidence)
                };
                assert_eq!(
                    classify(config, output.final_output(), &obligation),
                    expected
                );
            }
        }
    }

    #[test]
    #[ignore = "spawned by the checked-executable regression test"]
    fn process_helper_checked_copy() {
        println!("checked-copy");
    }

    #[test]
    #[ignore = "spawned by the fail-closed process-adapter test"]
    fn process_helper_timeout() {
        std::thread::sleep(Duration::from_millis(500));
    }

    #[test]
    #[ignore = "spawned by the fail-closed process-adapter test"]
    fn process_helper_crash() {
        std::process::exit(23);
    }

    #[test]
    #[ignore = "spawned by the fail-closed process-adapter test"]
    fn process_helper_output_limit() {
        println!("{}", "x".repeat(4096));
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "spawned by the process-tree containment regression test"]
    fn process_helper_spawns_pipe_holder() {
        let mut root = String::new();
        std::io::stdin()
            .read_to_string(&mut root)
            .unwrap_or_else(|error| panic!("read helper root: {error}"));
        let root = PathBuf::from(root.trim());
        fs::write(
            root.join("parent"),
            format!("{} {}\n", std::process::id(), nix::unistd::getpgrp()),
        )
        .unwrap_or_else(|error| panic!("write parent identity: {error}"));
        let descendant = Command::new(std::env::current_exe().unwrap_or_else(|_| unreachable!()))
            .args([
                "--ignored",
                "--exact",
                "tests::process_helper_holds_inherited_pipes",
                "--nocapture",
            ])
            .env("ZENO_FCIS_PIPE_HOLDER_ROOT", &root)
            .spawn()
            .unwrap_or_else(|error| panic!("spawn descendant helper: {error}"));
        std::mem::forget(descendant);
        let marker = root.join("descendant");
        for _ in 0..200 {
            if marker.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("descendant helper did not start");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "spawned by the process-tree containment regression test"]
    fn process_helper_holds_inherited_pipes() {
        let root = PathBuf::from(
            std::env::var_os("ZENO_FCIS_PIPE_HOLDER_ROOT")
                .unwrap_or_else(|| panic!("missing helper root")),
        );
        fs::write(
            root.join("descendant"),
            format!("{} {}\n", std::process::id(), nix::unistd::getpgrp()),
        )
        .unwrap_or_else(|error| panic!("write descendant identity: {error}"));
        thread::sleep(Duration::from_secs(30));
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "spawned by the successful process-tree containment regression test"]
    fn process_helper_spawns_detached_pipe_child() {
        let mut root = String::new();
        std::io::stdin()
            .read_to_string(&mut root)
            .unwrap_or_else(|error| panic!("read helper root: {error}"));
        let root = PathBuf::from(root.trim());
        let descendant = Command::new(std::env::current_exe().unwrap_or_else(|_| unreachable!()))
            .args([
                "--ignored",
                "--exact",
                "tests::process_helper_sleeps_without_solver_pipes",
                "--nocapture",
            ])
            .env("ZENO_FCIS_SUCCESS_DESCENDANT_ROOT", &root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("spawn descendant helper: {error}"));
        std::mem::forget(descendant);
        let marker = root.join("descendant");
        for _ in 0..200 {
            if marker.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("descendant helper did not start");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "spawned by the successful process-tree containment regression test"]
    fn process_helper_sleeps_without_solver_pipes() {
        let root = PathBuf::from(
            std::env::var_os("ZENO_FCIS_SUCCESS_DESCENDANT_ROOT")
                .unwrap_or_else(|| panic!("missing helper root")),
        );
        fs::write(root.join("descendant"), std::process::id().to_string())
            .unwrap_or_else(|error| panic!("write descendant pid: {error}"));
        thread::sleep(Duration::from_secs(30));
    }

    #[cfg(unix)]
    fn success_status() -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
    #[cfg(windows)]
    fn success_status() -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(0)
    }
}
