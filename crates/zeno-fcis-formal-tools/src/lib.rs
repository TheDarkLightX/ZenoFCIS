//! Deterministic formal-tool exporters and fail-closed process adapters.
//!
//! This standard-library shell can retain tool evidence. It cannot construct
//! [`zeno_fcis_backend::BackendCertificate`]; only the independent verifier in
//! `zeno-fcis-backend` owns that constructor path.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use zeno_fcis_codec::{CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_spec::{
    BackendId, ClaimDecl, ClaimFormula, ClaimMode, CompareOp, EvalLimits, EvalOutcome,
    EvaluationContext, Identifier, MAX_FINITE_HORIZON, MAX_FORMULA_DEPTH, Observation,
    PredicateProvider, ProjectionPath, ProjectionRoot, RelExpr, StableId, TemporalEvaluation,
    TemporalFormula, TraceStep, ValueExpr, evaluate_relational, evaluate_temporal,
};

/// Formal-tools manifest format.
pub const TOOLS_MANIFEST_FORMAT: &str = "zeno-fcis/tools/1";
/// CVC5 release qualified by RC3.
pub const CVC5_VERSION: &str = "1.3.3";
/// Z3 release qualified by RC3.
pub const Z3_VERSION: &str = "4.16.0";
/// Lean release qualified by RC3.
pub const LEAN_VERSION: &str = "4.30.0";
/// Maximum tools-manifest size.
pub const MAX_TOOLS_MANIFEST_BYTES: usize = 1024 * 1024;
/// Maximum admitted executable size for hashing.
pub const MAX_TOOL_BINARY_BYTES: u64 = 512 * 1024 * 1024;
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

/// One untrusted manifest entry rechecked before every process run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    backend: ToolBackend,
    path: PathBuf,
    version: String,
    sha256: String,
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
    TooLarge,
    Json(String),
    WrongFormat,
    DuplicateBackend,
    WrongVersion {
        backend: ToolBackend,
        actual: String,
    },
    InvalidHash,
    InvalidLimit,
    InvalidAxiom,
}

/// Reads and validates one untrusted manifest without following any `.zeno` data.
pub fn load_tools_manifest(path: &Path) -> Result<ToolsManifest, ManifestError> {
    let metadata = fs::metadata(path).map_err(|error| ManifestError::Io(error.to_string()))?;
    if metadata.len() > MAX_TOOLS_MANIFEST_BYTES as u64 {
        return Err(ManifestError::TooLarge);
    }
    let bytes = fs::read(path).map_err(|error| ManifestError::Io(error.to_string()))?;
    let mut manifest: ToolsManifest =
        serde_json::from_slice(&bytes).map_err(|error| ManifestError::Json(error.to_string()))?;
    if manifest.format != TOOLS_MANIFEST_FORMAT {
        return Err(ManifestError::WrongFormat);
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
}

/// Fail-closed process or admission failure.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolFailure {
    Missing,
    NotFile,
    BinaryTooLarge,
    HashMismatch,
    VersionMismatch,
    Timeout,
    Crash(Option<i32>),
    OutputLimit,
    Unknown,
    UnsupportedEvidence,
    ModelReplayFailed,
    LeanAxiomReport,
    Io(String),
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

struct CheckedTool {
    identity: ToolIdentity,
    executable: PrivateExecutable,
}

fn check_tool(config: &ToolConfig) -> Result<CheckedTool, ToolFailure> {
    check_tool_with_max_binary_bytes(config, MAX_TOOL_BINARY_BYTES)
}

fn check_tool_with_max_binary_bytes(
    config: &ToolConfig,
    max_binary_bytes: u64,
) -> Result<CheckedTool, ToolFailure> {
    let file = File::open(&config.path).map_err(|error| {
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
    if !version_text.contains(&config.version) {
        return Err(ToolFailure::VersionMismatch);
    }
    Ok(CheckedTool {
        identity: ToolIdentity {
            backend: config.backend,
            path: config.path.clone(),
            version: config.version.clone(),
            binary_hash,
        },
        executable,
    })
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
    let (root, multiplier) = match (kind, claim.mode(), claim.formula()) {
        (ExportKind::Smt, ClaimMode::Relational, ClaimFormula::Relational(value)) => {
            (ExportNode::Rel(value), 1)
        }
        (ExportKind::Smt, ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value))
            if horizon > 0 =>
        {
            if horizon > limits.max_horizon() {
                return Err(ExportError::ResourceLimit);
            }
            let multiplier = u64::from(horizon)
                .checked_mul(u64::from(horizon))
                .ok_or(ExportError::ResourceLimit)?;
            (ExportNode::Temporal(value), multiplier)
        }
        (ExportKind::Smt, ClaimMode::UnboundedProof, _) => {
            return Err(ExportError::UnsupportedMode);
        }
        (ExportKind::Lean, ClaimMode::UnboundedProof, ClaimFormula::Temporal(value)) => {
            (ExportNode::Temporal(value), 1)
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
                TemporalFormula::Not(value)
                | TemporalFormula::Next(value)
                | TemporalFormula::Always(value)
                | TemporalFormula::Eventually(value) => push_export_node(
                    &mut stack,
                    ExportNode::Temporal(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                TemporalFormula::And(left, right)
                | TemporalFormula::Or(left, right)
                | TemporalFormula::Until(left, right) => {
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
    let (horizon, finite, formula) = match (claim.mode(), claim.formula()) {
        (ClaimMode::Relational, ClaimFormula::Relational(value)) => {
            (1, false, render_rel_smt(value, 0, &empty_environment)?)
        }
        (ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value)) if horizon > 0 => {
            let formulas = (1..=horizon)
                .map(|length| render_temporal_smt(value, 0, length, &empty_environment))
                .collect::<Result<Vec<_>, _>>()?;
            (horizon, true, select_trace_length(formulas)?)
        }
        (ClaimMode::UnboundedProof, _) => return Err(ExportError::UnsupportedMode),
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
    let formula = match claim.formula() {
        ClaimFormula::Temporal(value) => render_temporal_lean(value, "0", &empty_environment)?,
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
  simp [claim_{claim_id}, floorDiv, ceilDiv, inI128, i128Min, i128Max]\n\n\
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
        source,
        source_hash,
    })
}

/// Evidence-retention proposal classification. No variant grants production authority.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolRunStatus {
    ProposedUnsat,
    Refuted,
    Blocked(ToolFailure),
    Failed(ToolFailure),
}

/// Complete bounded process record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolRun {
    identity: ToolIdentity,
    obligation: ExportedObligation,
    status: ToolRunStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
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
        &self.stdout
    }
    /// Returns bounded standard error.
    #[must_use]
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }
    /// Returns the content-addressed record ID.
    #[must_use]
    pub const fn record_hash(&self) -> Hash32 {
        self.record_hash
    }
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
    let output = match config.backend {
        ToolBackend::Cvc5 => run_smt(config, checked.executable.path(), &obligation.source)?,
        ToolBackend::Z3 => run_smt(config, checked.executable.path(), &obligation.source)?,
        ToolBackend::Lean => run_lean(config, checked.executable.path(), &obligation.source)?,
    };
    let identity = checked.identity;
    let status = classify(config, &output, &obligation);
    let mut record = Vec::new();
    record.extend_from_slice(identity.binary_hash.as_bytes());
    record.extend_from_slice(&obligation.claim_id.get().to_be_bytes());
    record.extend_from_slice(obligation.source_hash.as_bytes());
    record.extend_from_slice(&output.stdout);
    record.extend_from_slice(&output.stderr);
    record.extend_from_slice(format!("{status:?}").as_bytes());
    let record_hash = commitment::<RustCryptoSha256>(
        Domain::new("zeno-fcis/formal-run", 1)
            .map_err(|error| ToolFailure::Io(error.to_string()))?,
        &record,
    )
    .map_err(|error| ToolFailure::Io(error.to_string()))?;
    Ok(ToolRun {
        identity,
        obligation,
        status,
        stdout: output.stdout,
        stderr: output.stderr,
        record_hash,
    })
}

fn run_smt(
    config: &ToolConfig,
    executable: &Path,
    source: &[u8],
) -> Result<ProcessOutput, ToolFailure> {
    let args: &[&str] = match config.backend {
        ToolBackend::Cvc5 => &[
            "--safe-mode=safe",
            "--lang=smt2",
            "--produce-proofs",
            "--proof-format-mode=alethe",
        ],
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
    if !first.status.success() {
        return Ok(first);
    }
    let result = String::from_utf8_lossy(&first.stdout)
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned();
    let request = match (config.backend, result.as_str()) {
        (ToolBackend::Cvc5, "unsat") => b"(get-proof)\n".as_slice(),
        (ToolBackend::Cvc5 | ToolBackend::Z3, "sat") => b"(get-model)\n".as_slice(),
        _ => return Ok(first),
    };
    let mut followup = source.to_vec();
    followup.extend_from_slice(request);
    run_fixed(
        executable,
        args,
        Some(&followup),
        config.timeout_ms,
        config.max_output_bytes,
    )
}

struct MissingPredicates;
impl PredicateProvider for MissingPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

fn replay_model(obligation: &ExportedObligation, text: &str) -> ToolRunStatus {
    let assignments = parse_model_values(text);
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
            _ => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
        },
        ClaimFormula::Temporal(formula) => match evaluate_temporal(
            formula,
            obligation.claim.mode(),
            &trace,
            &MissingPredicates,
            EvalLimits::default(),
        ) {
            TemporalEvaluation::Counterexample { .. } => ToolRunStatus::Refuted,
            _ => ToolRunStatus::Blocked(ToolFailure::ModelReplayFailed),
        },
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
        ToolBackend::Lean => match parse_lean_axioms(&text) {
            Some(axioms) if axioms == config.allowed_axioms => ToolRunStatus::ProposedUnsat,
            _ => ToolRunStatus::Blocked(ToolFailure::LeanAxiomReport),
        },
    }
}

/// Atomically retains exact source, outputs, and a normalized metadata record by content hash.
pub fn retain_run(root: &Path, run: &ToolRun) -> Result<PathBuf, ToolFailure> {
    let directory = root.join(hash_hex(run.record_hash));
    fs::create_dir_all(&directory).map_err(|error| ToolFailure::Io(error.to_string()))?;
    atomic_write(&directory.join("source"), run.obligation.source())?;
    atomic_write(&directory.join("stdout"), &run.stdout)?;
    atomic_write(&directory.join("stderr"), &run.stderr)?;
    let metadata = format!(
        "{{\"backend\":\"{}\",\"claim_id\":{},\"record_hash\":\"{}\",\"source_hash\":\"{}\",\"status\":\"{:?}\",\"tool_hash\":\"{}\",\"tool_version\":\"{}\"}}\n",
        run.identity.backend.name(),
        run.obligation.claim_id.get(),
        run.record_hash,
        run.obligation.source_hash,
        run.status,
        run.identity.binary_hash,
        run.identity.version
    );
    atomic_write(&directory.join("record.json"), metadata.as_bytes())?;
    if matches!(run.status, ToolRunStatus::Refuted) {
        let values: Vec<_> = parse_model_values(&String::from_utf8_lossy(&run.stdout))
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
        atomic_write(&directory.join("counterexample.json"), &counterexample)?;
    }
    Ok(directory)
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
    let start = Instant::now();
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| ToolFailure::Io(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolFailure::Io("missing child stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolFailure::Io("missing child stderr".into()))?;
    let stdout_reader = bounded_reader(stdout, max_output);
    let stderr_reader = bounded_reader(stderr, max_output);
    let stdin_writer = if let Some(bytes) = input {
        let Some(stdin) = child.stdin.take() else {
            return Err(ToolFailure::Io("missing child stdin".into()));
        };
        Some(bounded_writer(stdin, bytes.to_vec()))
    } else {
        None
    };
    wait_output(
        child,
        timeout_ms,
        max_output,
        stdout_reader,
        stderr_reader,
        stdin_writer,
        start,
    )
}

fn bounded_writer<W: Write + Send + 'static>(
    mut stream: W,
    bytes: Vec<u8>,
) -> JoinHandle<Result<(), ToolFailure>> {
    thread::spawn(move || {
        stream
            .write_all(&bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()))
    })
}

fn bounded_reader<R: Read + Send + 'static>(
    stream: R,
    max_output: usize,
) -> JoinHandle<Result<Vec<u8>, ToolFailure>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        stream
            .take(
                u64::try_from(max_output)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            )
            .read_to_end(&mut bytes)
            .map_err(|error| ToolFailure::Io(error.to_string()))?;
        Ok(bytes)
    })
}

fn join_writer(writer: JoinHandle<Result<(), ToolFailure>>) -> Result<(), ToolFailure> {
    writer
        .join()
        .map_err(|_| ToolFailure::Io("process input writer panicked".into()))?
}

fn join_reader(reader: JoinHandle<Result<Vec<u8>, ToolFailure>>) -> Result<Vec<u8>, ToolFailure> {
    reader
        .join()
        .map_err(|_| ToolFailure::Io("process output reader panicked".into()))?
}

fn wait_output(
    mut child: Child,
    timeout_ms: u64,
    max_output: usize,
    stdout_reader: JoinHandle<Result<Vec<u8>, ToolFailure>>,
    stderr_reader: JoinHandle<Result<Vec<u8>, ToolFailure>>,
    stdin_writer: Option<JoinHandle<Result<(), ToolFailure>>>,
    start: Instant,
) -> Result<ProcessOutput, ToolFailure> {
    let status = loop {
        match child
            .try_wait()
            .map_err(|error| ToolFailure::Io(error.to_string()))?
        {
            Some(status) => break status,
            None if start.elapsed() >= Duration::from_millis(timeout_ms) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                if let Some(writer) = stdin_writer {
                    let _ = join_writer(writer);
                }
                return Err(ToolFailure::Timeout);
            }
            None => thread::sleep(Duration::from_millis(5)),
        }
    };
    if let Some(writer) = stdin_writer {
        join_writer(writer)?;
    }
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    if stdout.len().saturating_add(stderr.len()) > max_output {
        return Err(ToolFailure::OutputLimit);
    }
    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
    })
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
    let mut arguments = Vec::new();
    if let Some(sysroot) = config
        .path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::to_str)
    {
        arguments.push(format!("--sysroot={sysroot}"));
    }
    arguments.push(
        path.to_str()
            .ok_or_else(|| ToolFailure::Io("non-UTF8 temp path".into()))?
            .to_owned(),
    );
    let argument_refs = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let result = run_fixed(
        executable,
        &argument_refs,
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

fn checked_binary_smt(operator: &str, left: SmtValue, right: SmtValue) -> SmtValue {
    let term = format!("({operator} {} {})", left.term, right.term);
    let defined = smt_and(vec![left.defined, right.defined, smt_i128_range(&term)]);
    SmtValue { term, defined }
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
) -> Result<SmtValue, ExportError> {
    Ok(match value {
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
            render_value_smt(left, step, environment)?,
            render_value_smt(right, step, environment)?,
        ),
        ValueExpr::Sub(left, right) => checked_binary_smt(
            "-",
            render_value_smt(left, step, environment)?,
            render_value_smt(right, step, environment)?,
        ),
        ValueExpr::Mul(left, right) => checked_binary_smt(
            "*",
            render_value_smt(left, step, environment)?,
            render_value_smt(right, step, environment)?,
        ),
        ValueExpr::Div(mode, left, right) => {
            let left = render_value_smt(left, step, environment)?;
            let right = render_value_smt(right, step, environment)?;
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
                total = checked_binary_smt("+", total, render_value_smt(body, step, &nested)?);
            }
            total
        }
    })
}

fn strict_bool_smt(operator: &str, left: SmtBool, right: SmtBool) -> SmtBool {
    SmtBool {
        term: format!("({operator} {} {})", left.term, right.term),
        defined: smt_and(vec![left.defined, right.defined]),
    }
}

fn render_rel_smt(
    value: &RelExpr,
    step: u32,
    environment: &BTreeMap<String, i128>,
) -> Result<SmtBool, ExportError> {
    Ok(match value {
        RelExpr::Bool(value) => SmtBool {
            term: value.to_string(),
            defined: "true".to_owned(),
        },
        RelExpr::Not(value) => {
            let value = render_rel_smt(value, step, environment)?;
            SmtBool {
                term: smt_not(&value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) => strict_bool_smt(
            "and",
            render_rel_smt(left, step, environment)?,
            render_rel_smt(right, step, environment)?,
        ),
        RelExpr::Or(left, right) => strict_bool_smt(
            "or",
            render_rel_smt(left, step, environment)?,
            render_rel_smt(right, step, environment)?,
        ),
        RelExpr::Implies(left, right) => strict_bool_smt(
            "=>",
            render_rel_smt(left, step, environment)?,
            render_rel_smt(right, step, environment)?,
        ),
        RelExpr::Compare(operation, left, right) => {
            let left = render_value_smt(left, step, environment)?;
            let right = render_value_smt(right, step, environment)?;
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
                .map(|value| render_value_smt(value, step, environment))
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
        } => render_bounded_bool(true, variable, *start, *end, body, step, environment)?,
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool(false, variable, *start, *end, body, step, environment)?,
    })
}

fn fold_all_smt(values: Vec<SmtBool>) -> SmtBool {
    let mut result = SmtBool {
        term: "true".to_owned(),
        defined: "true".to_owned(),
    };
    for value in values {
        let defined = smt_and(vec![
            result.defined,
            smt_or(vec![smt_not(&result.term), value.defined]),
        ]);
        let term = smt_and(vec![result.term, value.term]);
        result = SmtBool { term, defined };
    }
    result
}

fn fold_exists_smt(values: Vec<SmtBool>) -> SmtBool {
    let mut result = SmtBool {
        term: "false".to_owned(),
        defined: "true".to_owned(),
    };
    for value in values {
        let defined = smt_and(vec![
            result.defined,
            smt_or(vec![result.term.clone(), value.defined]),
        ]);
        let term = smt_or(vec![result.term, value.term]);
        result = SmtBool { term, defined };
    }
    result
}

fn render_bounded_bool(
    all: bool,
    variable: &Identifier,
    start: i128,
    end: i128,
    body: &RelExpr,
    step: u32,
    environment: &BTreeMap<String, i128>,
) -> Result<SmtBool, ExportError> {
    if end < start || end.saturating_sub(start) > 4096 {
        return Err(ExportError::InvalidFormula);
    }
    let mut values = Vec::new();
    for current in start..end {
        let mut nested = environment.clone();
        nested.insert(variable.as_str().into(), current);
        values.push(render_rel_smt(body, step, &nested)?);
    }
    Ok(if all {
        fold_all_smt(values)
    } else {
        fold_exists_smt(values)
    })
}

fn render_temporal_smt(
    value: &TemporalFormula,
    step: u32,
    horizon: u32,
    environment: &BTreeMap<String, i128>,
) -> Result<SmtBool, ExportError> {
    Ok(match value {
        TemporalFormula::Atom(value) => render_rel_smt(value, step, environment)?,
        TemporalFormula::Not(value) => {
            let value = render_temporal_smt(value, step, horizon, environment)?;
            SmtBool {
                term: smt_not(&value.term),
                defined: value.defined,
            }
        }
        TemporalFormula::And(left, right) => strict_bool_smt(
            "and",
            render_temporal_smt(left, step, horizon, environment)?,
            render_temporal_smt(right, step, horizon, environment)?,
        ),
        TemporalFormula::Or(left, right) => strict_bool_smt(
            "or",
            render_temporal_smt(left, step, horizon, environment)?,
            render_temporal_smt(right, step, horizon, environment)?,
        ),
        TemporalFormula::Next(value) => {
            if step + 1 < horizon {
                render_temporal_smt(value, step + 1, horizon, environment)?
            } else {
                SmtBool {
                    term: "false".to_owned(),
                    defined: "true".to_owned(),
                }
            }
        }
        TemporalFormula::Always(value) => fold_all_smt(
            (step..horizon)
                .map(|current| render_temporal_smt(value, current, horizon, environment))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        TemporalFormula::Eventually(value) => fold_exists_smt(
            (step..horizon)
                .map(|current| render_temporal_smt(value, current, horizon, environment))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        TemporalFormula::Until(left, right) => {
            let mut continuation = SmtBool {
                term: "false".to_owned(),
                defined: "true".to_owned(),
            };
            for current in (step..horizon).rev() {
                let right = render_temporal_smt(right, current, horizon, environment)?;
                let left = render_temporal_smt(left, current, horizon, environment)?;
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
                continuation = SmtBool { term, defined };
            }
            continuation
        }
    })
}

fn select_trace_length(mut formulas: Vec<SmtBool>) -> Result<SmtBool, ExportError> {
    let Some(mut selected) = formulas.pop() else {
        return Err(ExportError::InvalidFormula);
    };
    for (index, formula) in formulas.into_iter().enumerate().rev() {
        let length = index + 1;
        selected = SmtBool {
            term: format!(
                "(ite (= zeno_trace_len {length}) {} {})",
                formula.term, selected.term
            ),
            defined: format!(
                "(ite (= zeno_trace_len {length}) {} {})",
                formula.defined, selected.defined
            ),
        };
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

fn lean_checked_binary(operator: &str, left: LeanValue, right: LeanValue) -> LeanValue {
    let term = format!("({} {operator} {})", left.term, right.term);
    let defined = lean_and(vec![left.defined, right.defined, format!("inI128 {term}")]);
    LeanValue { term, defined }
}

fn render_value_lean(
    value: &ValueExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
) -> Result<LeanValue, ExportError> {
    Ok(match value {
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
        ValueExpr::Add(left, right) => lean_checked_binary(
            "+",
            render_value_lean(left, step, environment)?,
            render_value_lean(right, step, environment)?,
        ),
        ValueExpr::Sub(left, right) => lean_checked_binary(
            "-",
            render_value_lean(left, step, environment)?,
            render_value_lean(right, step, environment)?,
        ),
        ValueExpr::Mul(left, right) => lean_checked_binary(
            "*",
            render_value_lean(left, step, environment)?,
            render_value_lean(right, step, environment)?,
        ),
        ValueExpr::Div(mode, left, right) => {
            let left = render_value_lean(left, step, environment)?;
            let right = render_value_lean(right, step, environment)?;
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
                total = lean_checked_binary("+", total, render_value_lean(body, step, &nested)?);
            }
            total
        }
    })
}

fn strict_bool_lean(operator: &str, left: LeanBool, right: LeanBool) -> LeanBool {
    LeanBool {
        term: format!("({} {operator} {})", left.term, right.term),
        defined: lean_and(vec![left.defined, right.defined]),
    }
}

fn render_rel_lean(
    value: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
) -> Result<LeanBool, ExportError> {
    Ok(match value {
        RelExpr::Bool(value) => LeanBool {
            term: if *value { "True" } else { "False" }.to_owned(),
            defined: "True".to_owned(),
        },
        RelExpr::Not(value) => {
            let value = render_rel_lean(value, step, environment)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) => strict_bool_lean(
            "∧",
            render_rel_lean(left, step, environment)?,
            render_rel_lean(right, step, environment)?,
        ),
        RelExpr::Or(left, right) => strict_bool_lean(
            "∨",
            render_rel_lean(left, step, environment)?,
            render_rel_lean(right, step, environment)?,
        ),
        RelExpr::Implies(left, right) => strict_bool_lean(
            "→",
            render_rel_lean(left, step, environment)?,
            render_rel_lean(right, step, environment)?,
        ),
        RelExpr::Compare(operation, left, right) => {
            let left = render_value_lean(left, step, environment)?;
            let right = render_value_lean(right, step, environment)?;
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
                .map(|value| render_value_lean(value, step, environment))
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
        } => render_bounded_bool_lean(true, variable, *start, *end, body, step, environment)?,
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool_lean(false, variable, *start, *end, body, step, environment)?,
    })
}

fn fold_all_lean(values: Vec<LeanBool>) -> LeanBool {
    let mut result = LeanBool {
        term: "True".to_owned(),
        defined: "True".to_owned(),
    };
    for value in values {
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![lean_not(&result.term), value.defined]),
        ]);
        let term = lean_and(vec![result.term, value.term]);
        result = LeanBool { term, defined };
    }
    result
}

fn fold_exists_lean(values: Vec<LeanBool>) -> LeanBool {
    let mut result = LeanBool {
        term: "False".to_owned(),
        defined: "True".to_owned(),
    };
    for value in values {
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![result.term.clone(), value.defined]),
        ]);
        let term = lean_or(vec![result.term, value.term]);
        result = LeanBool { term, defined };
    }
    result
}

fn render_bounded_bool_lean(
    all: bool,
    variable: &Identifier,
    start: i128,
    end: i128,
    body: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
) -> Result<LeanBool, ExportError> {
    if end < start || end.saturating_sub(start) > 4096 {
        return Err(ExportError::InvalidFormula);
    }
    let mut values = Vec::new();
    for current in start..end {
        let mut nested = environment.clone();
        nested.insert(variable.as_str().into(), current);
        values.push(render_rel_lean(body, step, &nested)?);
    }
    Ok(if all {
        fold_all_lean(values)
    } else {
        fold_exists_lean(values)
    })
}

fn render_temporal_lean(
    value: &TemporalFormula,
    step: &str,
    environment: &BTreeMap<String, i128>,
) -> Result<LeanBool, ExportError> {
    Ok(match value {
        TemporalFormula::Atom(value) => render_rel_lean(value, step, environment)?,
        TemporalFormula::Not(value) => {
            let value = render_temporal_lean(value, step, environment)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        TemporalFormula::And(left, right) => strict_bool_lean(
            "∧",
            render_temporal_lean(left, step, environment)?,
            render_temporal_lean(right, step, environment)?,
        ),
        TemporalFormula::Or(left, right) => strict_bool_lean(
            "∨",
            render_temporal_lean(left, step, environment)?,
            render_temporal_lean(right, step, environment)?,
        ),
        TemporalFormula::Next(value) => {
            render_temporal_lean(value, &format!("({step} + 1)"), environment)?
        }
        TemporalFormula::Always(value) => {
            let value = render_temporal_lean(value, "n", environment)?;
            LeanBool {
                term: format!("∀ n : Nat, n >= {step} → ({})", value.term),
                defined: format!("∀ n : Nat, n >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Eventually(value) => {
            let value = render_temporal_lean(value, "n", environment)?;
            LeanBool {
                term: format!("∃ n : Nat, n >= {step} ∧ ({})", value.term),
                defined: format!("∀ n : Nat, n >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Until(left, right) => {
            let left_at_m = render_temporal_lean(left, "m", environment)?;
            let right_at_n = render_temporal_lean(right, "n", environment)?;
            LeanBool {
                term: format!(
                    "∃ n : Nat, n >= {step} ∧ ({}) ∧ \
                     ∀ m : Nat, {step} <= m → m < n → ({})",
                    right_at_n.term, left_at_m.term
                ),
                defined: format!(
                    "(∀ n : Nat, n >= {step} → ({})) ∧ \
                     (∀ m : Nat, m >= {step} → ({}))",
                    right_at_n.defined, left_at_m.defined
                ),
            }
        }
    })
}

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
    fn rc3_formal_fail_closed_and_model_replay() {
        let config = ToolConfig {
            backend: ToolBackend::Z3,
            path: "z3".into(),
            version: Z3_VERSION.into(),
            sha256: "0".repeat(64),
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
        assert!(source.contains("(observe \"pre_100\" n).val"));
        assert!(source.contains("∀ n : Nat, n >= 0"));
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
            timeout_ms: 1_000,
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
        let script = b"#!/bin/sh\nprintf tool-1.2.3\n";
        fs::write(&script_path, script).unwrap_or_else(|_| unreachable!());
        let expected_hash = RustCryptoSha256::hash(script);
        let expected_hash_hex = hash_hex(expected_hash);
        let accepted = config(script_path.clone(), expected_hash_hex.clone(), "1.2.3");
        let checked = check_tool(&accepted).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(checked.identity.binary_hash(), expected_hash);

        let wrong_version = config(script_path.clone(), expected_hash_hex, "9.9.9");
        assert!(matches!(
            check_tool(&wrong_version),
            Err(ToolFailure::VersionMismatch)
        ));
        fs::remove_file(script_path).unwrap_or_else(|_| unreachable!());
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
            ExportLimits::try_new(2, 2, 2, 8, 4096).unwrap_or_else(|| unreachable!());
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

    #[test]
    fn rc3_private_executable_preserves_the_admitted_bytes() {
        let executable = std::env::current_exe().unwrap_or_else(|_| unreachable!());
        let bytes = fs::read(executable).unwrap_or_else(|_| unreachable!());
        let expected_hash = RustCryptoSha256::hash(&bytes);
        let admitted =
            PrivateExecutable::create(ToolBackend::Z3, &bytes).unwrap_or_else(|_| unreachable!());
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
        .unwrap_or_else(|_| unreachable!());
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("checked-copy"));
        let retained_bytes = fs::read(admitted.path()).unwrap_or_else(|_| unreachable!());
        assert_eq!(RustCryptoSha256::hash(&retained_bytes), expected_hash);
    }

    #[test]
    #[ignore = "requires the workflow-pinned Lean executable"]
    fn pinned_lean_translation_kernel_checks() {
        let executable =
            PathBuf::from(std::env::var_os("ZENO_FCIS_LEAN").unwrap_or_else(|| unreachable!()));
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
        let source_path =
            std::env::temp_dir().join(format!("zeno-fcis-pinned-lean-{}.lean", std::process::id()));
        atomic_write(&source_path, obligation.source()).unwrap_or_else(|_| unreachable!());
        let output = run_fixed(
            &executable,
            &[source_path.to_str().unwrap_or_else(|| unreachable!())],
            None,
            30_000,
            1024 * 1024,
        )
        .unwrap_or_else(|_| unreachable!());
        let _ = fs::remove_file(source_path);
        assert!(output.status.success());
        assert_eq!(
            parse_lean_axioms(&String::from_utf8_lossy(&output.stdout)),
            Some(vec!["Quot.sound".to_owned(), "propext".to_owned()])
        );
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
            timeout_ms: 30_000,
            max_output_bytes: 8 * 1024 * 1024,
            allowed_axioms: Vec::new(),
        };
        let cvc5_output = run_smt(&cvc5_config, &cvc5_config.path, true_obligation.source())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            classify(&cvc5_config, &cvc5_output, &true_obligation),
            ToolRunStatus::ProposedUnsat
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
            timeout_ms: 30_000,
            max_output_bytes: 8 * 1024 * 1024,
            allowed_axioms: Vec::new(),
        };
        let z3_output = run_smt(&z3_config, &z3_config.path, false_obligation.source())
            .unwrap_or_else(|_| unreachable!());
        assert_eq!(
            classify(&z3_config, &z3_output, &false_obligation),
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
                    String::from_utf8_lossy(&output.stdout)
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
                assert_eq!(classify(config, &output, &obligation), expected);
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
