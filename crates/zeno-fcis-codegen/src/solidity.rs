//! Deterministic generation and fail-closed inspection for Solidity FCIS shells.
//!
//! The v1 surface deliberately supports only fixed-size ABI scalar fields and
//! local contract storage. It does not generate arbitrary external calls,
//! delegate calls, token transfers, oracle adapters, upgrade hooks, or an
//! effect interpreter.

use std::collections::BTreeSet;
use std::fmt::{self, Write as _};

/// Stable semantic identity for the Solidity scaffold generator.
pub const SOLIDITY_GENERATOR_ID: &str = "zeno-fcis-solidity/1";

/// Solidity compiler range emitted by v1.
pub const SOLIDITY_PRAGMA: &str = ">=0.8.24 <0.9.0";

/// Maximum state or command fields accepted by one scaffold.
pub const MAX_SOLIDITY_FIELDS: usize = 64;

/// Maximum stable rejection reasons accepted by one scaffold.
pub const MAX_SOLIDITY_REASONS: usize = 64;

/// Maximum generated source length.
pub const MAX_SOLIDITY_SOURCE_BYTES: usize = 512 * 1024;

/// A fixed-size ABI scalar admitted by the Solidity v1 boundary.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SolidityScalar {
    /// Solidity `bool`.
    Bool,
    /// Solidity `address`.
    Address,
    /// Solidity `bytes32`.
    Bytes32,
    /// Solidity `uint8`.
    U8,
    /// Solidity `uint16`.
    U16,
    /// Solidity `uint32`.
    U32,
    /// Solidity `uint64`.
    U64,
    /// Solidity `uint128`.
    U128,
    /// Solidity `uint256`.
    U256,
    /// Solidity `int8`.
    I8,
    /// Solidity `int16`.
    I16,
    /// Solidity `int32`.
    I32,
    /// Solidity `int64`.
    I64,
    /// Solidity `int128`.
    I128,
    /// Solidity `int256`.
    I256,
}

impl SolidityScalar {
    /// Returns the exact Solidity source spelling.
    #[must_use]
    pub const fn source_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Address => "address",
            Self::Bytes32 => "bytes32",
            Self::U8 => "uint8",
            Self::U16 => "uint16",
            Self::U32 => "uint32",
            Self::U64 => "uint64",
            Self::U128 => "uint128",
            Self::U256 => "uint256",
            Self::I8 => "int8",
            Self::I16 => "int16",
            Self::I32 => "int32",
            Self::I64 => "int64",
            Self::I128 => "int128",
            Self::I256 => "int256",
        }
    }
}

/// One validated state or command field.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SolidityField {
    name: String,
    scalar: SolidityScalar,
}

impl SolidityField {
    /// Constructs one field from a Solidity identifier and a fixed-size scalar.
    pub fn try_new(
        name: impl Into<String>,
        scalar: SolidityScalar,
    ) -> Result<Self, SolidityGenerationError> {
        let name = name.into();
        validate_identifier(&name, IdentifierRole::Field)?;
        Ok(Self { name, scalar })
    }

    /// Returns the exact field identifier.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the fixed-size scalar type.
    #[must_use]
    pub const fn scalar(&self) -> SolidityScalar {
        self.scalar
    }
}

/// Closed input for one generated local-state FCIS shell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolidityContractSpec {
    contract_name: String,
    state_fields: Vec<SolidityField>,
    command_fields: Vec<SolidityField>,
    rejection_reasons: Vec<String>,
}

impl SolidityContractSpec {
    /// Validates and constructs a closed generation request.
    pub fn try_new(
        contract_name: impl Into<String>,
        state_fields: Vec<SolidityField>,
        command_fields: Vec<SolidityField>,
        rejection_reasons: Vec<String>,
    ) -> Result<Self, SolidityGenerationError> {
        let contract_name = contract_name.into();
        validate_identifier(&contract_name, IdentifierRole::Type)?;

        validate_fields(&state_fields, SolidityListKind::StateFields)?;
        validate_fields(&command_fields, SolidityListKind::CommandFields)?;
        validate_reasons(&rejection_reasons)?;

        Ok(Self {
            contract_name,
            state_fields,
            command_fields,
            rejection_reasons,
        })
    }

    /// Returns the generated abstract contract name.
    #[must_use]
    pub fn contract_name(&self) -> &str {
        &self.contract_name
    }

    /// Returns state fields in semantic order.
    #[must_use]
    pub fn state_fields(&self) -> &[SolidityField] {
        &self.state_fields
    }

    /// Returns command fields in semantic order.
    #[must_use]
    pub fn command_fields(&self) -> &[SolidityField] {
        &self.command_fields
    }

    /// Returns stable rejection reason names in precedence order.
    #[must_use]
    pub fn rejection_reasons(&self) -> &[String] {
        &self.rejection_reasons
    }
}

/// One deterministic Solidity source artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedSolidity {
    path: String,
    source: String,
}

impl GeneratedSolidity {
    /// Returns the portable generated path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns exact UTF-8 Solidity source.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Consumes the artifact into its path and source.
    #[must_use]
    pub fn into_parts(self) -> (String, String) {
        (self.path, self.source)
    }
}

/// A closed list rejected by the generator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolidityListKind {
    /// State fields.
    StateFields,
    /// Command fields.
    CommandFields,
    /// Rejection reasons.
    RejectionReasons,
}

/// Deterministic Solidity generation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolidityGenerationError {
    /// A name was not a valid non-reserved Solidity identifier.
    InvalidIdentifier,
    /// A type or rejection name did not begin with an uppercase ASCII letter.
    InvalidTypeName,
    /// A required list was empty.
    EmptyList(SolidityListKind),
    /// A bounded list exceeded its v1 limit.
    LimitExceeded(SolidityListKind),
    /// A field or rejection reason was duplicated.
    DuplicateName,
    /// The reserved `None` rejection reason was supplied.
    ReservedReason,
    /// Rendering into the source buffer failed.
    Formatting,
    /// Generated source exceeded its hard byte limit.
    SourceTooLarge,
}

impl fmt::Display for SolidityGenerationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier => formatter.write_str("invalid Solidity identifier"),
            Self::InvalidTypeName => {
                formatter.write_str("Solidity type and reason names must begin uppercase")
            }
            Self::EmptyList(kind) => write!(formatter, "empty Solidity list: {kind:?}"),
            Self::LimitExceeded(kind) => {
                write!(formatter, "Solidity list limit exceeded: {kind:?}")
            }
            Self::DuplicateName => formatter.write_str("duplicate Solidity name"),
            Self::ReservedReason => formatter.write_str("rejection reason None is reserved"),
            Self::Formatting => formatter.write_str("Solidity source formatting failed"),
            Self::SourceTooLarge => formatter.write_str("generated Solidity source is too large"),
        }
    }
}

impl std::error::Error for SolidityGenerationError {}

impl From<fmt::Error> for SolidityGenerationError {
    fn from(_: fmt::Error) -> Self {
        Self::Formatting
    }
}

/// Generates one abstract, effect-free local-state FCIS Solidity shell.
///
/// The generated contract owns state capture, expected-root checking, command
/// admission, decision validation, invariant validation, atomic storage commit,
/// and receipt emission. Project code can implement only three `internal pure`
/// hooks: `_commandAdmissible`, `_invariant`, and `_decide`.
pub fn generate_solidity(
    spec: &SolidityContractSpec,
) -> Result<GeneratedSolidity, SolidityGenerationError> {
    let mut source = String::new();
    render_header(&mut source, spec)?;
    render_types(&mut source, spec)?;
    render_storage_and_errors(&mut source, spec)?;
    render_constructor_and_views(&mut source, spec)?;
    render_initialize(&mut source)?;
    render_execute(&mut source, spec)?;
    render_storage_helpers(&mut source, spec)?;
    render_hash_helpers(&mut source, spec)?;
    render_pure_hooks(&mut source)?;
    writeln!(source, "}}")?;

    if source.len() > MAX_SOLIDITY_SOURCE_BYTES {
        return Err(SolidityGenerationError::SourceTooLarge);
    }

    Ok(GeneratedSolidity {
        path: format!("solidity/{}.sol", spec.contract_name()),
        source,
    })
}

/// A forbidden-mechanism category found by the defense-in-depth source checker.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SoliditySafetyFindingKind {
    /// Inline assembly.
    InlineAssembly,
    /// An unchecked arithmetic block.
    UncheckedArithmetic,
    /// A low-level external call.
    LowLevelCall,
    /// A low-level static call.
    StaticCall,
    /// Delegate execution in another contract's context.
    DelegateCall,
    /// Legacy callcode execution.
    CallCode,
    /// Direct Ether transfer primitive.
    EtherTransfer,
    /// Contract destruction.
    ContractDestruction,
    /// `tx.origin` authorization surface.
    TxOrigin,
    /// Raw storage opcode reference.
    RawStorageOpcode,
    /// Raw transient-storage opcode reference.
    RawTransientStorageOpcode,
    /// Contract creation opcode or expression.
    ContractCreation,
}

/// One source-level safety finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SoliditySafetyFinding {
    kind: SoliditySafetyFindingKind,
    byte_offset: usize,
}

impl SoliditySafetyFinding {
    /// Returns the finding category.
    #[must_use]
    pub const fn kind(&self) -> SoliditySafetyFindingKind {
        self.kind
    }

    /// Returns the byte offset in the original source.
    #[must_use]
    pub const fn byte_offset(&self) -> usize {
        self.byte_offset
    }
}

/// Defense-in-depth inspection result.
///
/// This scanner is intentionally conservative and is not a Solidity parser or
/// a substitute for compiler checks, static analyzers, formal verification, or
/// an audit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoliditySafetyReport {
    findings: Vec<SoliditySafetyFinding>,
}

impl SoliditySafetyReport {
    /// Returns true when no forbidden mechanism was found.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Returns findings in byte-offset order.
    #[must_use]
    pub fn findings(&self) -> &[SoliditySafetyFinding] {
        &self.findings
    }
}

/// Conservatively scans Solidity source for mechanisms forbidden by the v1
/// effect-free FCIS profile.
///
/// Comments and quoted strings are blanked before matching so documentation and
/// error text do not create findings. The Solidity compiler remains the
/// authority for enforcing the generated `pure` hooks.
#[must_use]
pub fn inspect_solidity_source(source: &str) -> SoliditySafetyReport {
    const FORBIDDEN: &[(&str, SoliditySafetyFindingKind)] = &[
        ("assembly", SoliditySafetyFindingKind::InlineAssembly),
        ("unchecked", SoliditySafetyFindingKind::UncheckedArithmetic),
        (".delegatecall", SoliditySafetyFindingKind::DelegateCall),
        (".callcode", SoliditySafetyFindingKind::CallCode),
        (".staticcall", SoliditySafetyFindingKind::StaticCall),
        (".call", SoliditySafetyFindingKind::LowLevelCall),
        (".transfer", SoliditySafetyFindingKind::EtherTransfer),
        (".send", SoliditySafetyFindingKind::EtherTransfer),
        (
            "selfdestruct",
            SoliditySafetyFindingKind::ContractDestruction,
        ),
        ("suicide", SoliditySafetyFindingKind::ContractDestruction),
        ("tx.origin", SoliditySafetyFindingKind::TxOrigin),
        ("sstore", SoliditySafetyFindingKind::RawStorageOpcode),
        (
            "tstore",
            SoliditySafetyFindingKind::RawTransientStorageOpcode,
        ),
        ("create2", SoliditySafetyFindingKind::ContractCreation),
        ("new ", SoliditySafetyFindingKind::ContractCreation),
    ];

    let sanitized = sanitize_non_code(source);
    let lowercase = sanitized.to_ascii_lowercase();
    let mut findings = Vec::new();

    for (needle, kind) in FORBIDDEN {
        let mut start = 0;
        while let Some(relative) = lowercase[start..].find(needle) {
            let offset = start + relative;
            if token_boundary_matches(&lowercase, offset, needle) {
                findings.push(SoliditySafetyFinding {
                    kind: *kind,
                    byte_offset: offset,
                });
            }
            start = offset.saturating_add(needle.len());
        }
    }

    findings.sort_by_key(SoliditySafetyFinding::byte_offset);
    SoliditySafetyReport { findings }
}

fn render_header(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(source, "// SPDX-License-Identifier: MIT OR Apache-2.0")?;
    writeln!(source, "pragma solidity {SOLIDITY_PRAGMA};")?;
    writeln!(source)?;
    writeln!(source, "/// @notice Generated by {SOLIDITY_GENERATOR_ID}.")?;
    writeln!(
        source,
        "/// @dev Effect-free FCIS shell: no external calls, delegate calls, or upgrade hooks."
    )?;
    writeln!(source, "abstract contract {} {{", spec.contract_name())?;
    writeln!(
        source,
        "    string public constant ZENO_FCIS_GENERATOR = \"{SOLIDITY_GENERATOR_ID}\";"
    )?;
    writeln!(source)?;
    Ok(())
}

fn render_types(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(source, "    enum DecisionKind {{ Accept, Reject }}")?;
    write!(source, "    enum RejectReason {{ None")?;
    for reason in spec.rejection_reasons() {
        write!(source, ", {reason}")?;
    }
    writeln!(source, " }}")?;
    writeln!(source)?;

    writeln!(source, "    struct State {{")?;
    for field in spec.state_fields() {
        writeln!(
            source,
            "        {} {};",
            field.scalar().source_name(),
            field.name()
        )?;
    }
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(source, "    struct Command {{")?;
    for field in spec.command_fields() {
        writeln!(
            source,
            "        {} {};",
            field.scalar().source_name(),
            field.name()
        )?;
    }
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(source, "    struct Context {{")?;
    writeln!(source, "        address caller;")?;
    writeln!(source, "        uint256 blockTimestamp;")?;
    writeln!(source, "        uint256 blockNumber;")?;
    writeln!(source, "        uint256 chainId;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(source, "    struct Decision {{")?;
    writeln!(source, "        DecisionKind kind;")?;
    writeln!(source, "        RejectReason reason;")?;
    writeln!(source, "        State nextState;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_storage_and_errors(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(source, "    address private immutable _initializer;")?;
    writeln!(source, "    bool private _initialized;")?;
    writeln!(source, "    uint256 private _gate;")?;
    writeln!(source, "    bytes32 private _stateHash;")?;
    for field in spec.state_fields() {
        writeln!(
            source,
            "    {} private _state_{};",
            field.scalar().source_name(),
            field.name()
        )?;
    }
    writeln!(source)?;

    writeln!(
        source,
        "    error UnauthorizedInitializer(address expected, address actual);"
    )?;
    writeln!(source, "    error AlreadyInitialized();")?;
    writeln!(source, "    error NotInitialized();")?;
    writeln!(source, "    error Reentrancy();")?;
    writeln!(
        source,
        "    error StaleState(bytes32 expected, bytes32 actual);"
    )?;
    writeln!(
        source,
        "    error StateRootCorrupted(bytes32 committed, bytes32 actual);"
    )?;
    writeln!(source, "    error CommandNotAdmissible();")?;
    writeln!(source, "    error InvariantViolation();")?;
    writeln!(
        source,
        "    error InvalidDecision(DecisionKind kind, RejectReason reason);"
    )?;
    writeln!(source, "    error TransitionRejected(RejectReason reason);")?;
    writeln!(source)?;

    writeln!(
        source,
        "    event Initialized(bytes32 indexed stateHash, address indexed authority);"
    )?;
    writeln!(
        source,
        "    event TransitionCommitted(bytes32 indexed preStateHash, bytes32 indexed postStateHash, bytes32 indexed candidateHash, bytes32 commandHash, address caller);"
    )?;
    writeln!(source)?;
    Ok(())
}

fn render_constructor_and_views(
    source: &mut String,
    _spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(source, "    constructor() {{")?;
    writeln!(source, "        _initializer = msg.sender;")?;
    writeln!(source, "        _gate = 1;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function initializationAuthority() external view returns (address) {{"
    )?;
    writeln!(source, "        return _initializer;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function isInitialized() external view returns (bool) {{"
    )?;
    writeln!(source, "        return _initialized;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function currentStateHash() external view returns (bytes32) {{"
    )?;
    writeln!(
        source,
        "        if (!_initialized) revert NotInitialized();"
    )?;
    writeln!(source, "        return _stateHash;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function currentState() external view returns (State memory) {{"
    )?;
    writeln!(
        source,
        "        if (!_initialized) revert NotInitialized();"
    )?;
    writeln!(source, "        return _readState();")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_initialize(source: &mut String) -> Result<(), SolidityGenerationError> {
    writeln!(
        source,
        "    function initialize(State calldata initialState) external {{"
    )?;
    writeln!(source, "        if (msg.sender != _initializer) {{")?;
    writeln!(
        source,
        "            revert UnauthorizedInitializer(_initializer, msg.sender);"
    )?;
    writeln!(source, "        }}")?;
    writeln!(
        source,
        "        if (_initialized) revert AlreadyInitialized();"
    )?;
    writeln!(source, "        State memory admitted = initialState;")?;
    writeln!(
        source,
        "        if (!_invariant(admitted)) revert InvariantViolation();"
    )?;
    writeln!(source, "        _writeState(admitted);")?;
    writeln!(
        source,
        "        bytes32 admittedHash = _hashState(admitted);"
    )?;
    writeln!(source, "        _stateHash = admittedHash;")?;
    writeln!(source, "        _initialized = true;")?;
    writeln!(
        source,
        "        emit Initialized(admittedHash, msg.sender);"
    )?;
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_execute(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(
        source,
        "    function execute(Command calldata command, bytes32 expectedStateHash) external returns (bytes32 postStateHash) {{"
    )?;
    writeln!(
        source,
        "        if (!_initialized) revert NotInitialized();"
    )?;
    writeln!(source, "        if (_gate != 1) revert Reentrancy();")?;
    writeln!(source, "        _gate = 2;")?;
    writeln!(source)?;
    writeln!(source, "        State memory beforeState = _readState();")?;
    writeln!(
        source,
        "        bytes32 actualStateHash = _hashState(beforeState);"
    )?;
    writeln!(source, "        if (actualStateHash != _stateHash) {{")?;
    writeln!(
        source,
        "            revert StateRootCorrupted(_stateHash, actualStateHash);"
    )?;
    writeln!(source, "        }}")?;
    writeln!(
        source,
        "        if (actualStateHash != expectedStateHash) {{"
    )?;
    writeln!(
        source,
        "            revert StaleState(expectedStateHash, actualStateHash);"
    )?;
    writeln!(source, "        }}")?;
    writeln!(source)?;
    writeln!(source, "        Command memory admittedCommand = command;")?;
    writeln!(source, "        Context memory context = Context({{")?;
    writeln!(source, "            caller: msg.sender,")?;
    writeln!(source, "            blockTimestamp: block.timestamp,")?;
    writeln!(source, "            blockNumber: block.number,")?;
    writeln!(source, "            chainId: block.chainid")?;
    writeln!(source, "        }});")?;
    writeln!(
        source,
        "        if (!_commandAdmissible(admittedCommand, context)) revert CommandNotAdmissible();"
    )?;
    writeln!(source)?;
    writeln!(
        source,
        "        Decision memory decision = _decide(beforeState, admittedCommand, context);"
    )?;
    writeln!(
        source,
        "        if (decision.kind == DecisionKind.Reject) {{"
    )?;
    writeln!(
        source,
        "            if (decision.reason == RejectReason.None) {{"
    )?;
    writeln!(
        source,
        "                revert InvalidDecision(decision.kind, decision.reason);"
    )?;
    writeln!(source, "            }}")?;
    writeln!(
        source,
        "            revert TransitionRejected(decision.reason);"
    )?;
    writeln!(source, "        }}")?;
    writeln!(
        source,
        "        if (decision.kind != DecisionKind.Accept || decision.reason != RejectReason.None) {{"
    )?;
    writeln!(
        source,
        "            revert InvalidDecision(decision.kind, decision.reason);"
    )?;
    writeln!(source, "        }}")?;
    writeln!(
        source,
        "        if (!_invariant(decision.nextState)) revert InvariantViolation();"
    )?;
    writeln!(source)?;
    writeln!(
        source,
        "        postStateHash = _hashState(decision.nextState);"
    )?;
    writeln!(
        source,
        "        bytes32 commandHash = _hashCommand(admittedCommand);"
    )?;
    writeln!(
        source,
        "        bytes32 candidateHash = keccak256(abi.encode("
    )?;
    writeln!(
        source,
        "            \"zeno-fcis/solidity/candidate/v1/{}\",",
        spec.contract_name()
    )?;
    writeln!(source, "            actualStateHash,")?;
    writeln!(source, "            postStateHash,")?;
    writeln!(source, "            commandHash,")?;
    writeln!(source, "            context.caller,")?;
    writeln!(source, "            context.blockTimestamp,")?;
    writeln!(source, "            context.blockNumber,")?;
    writeln!(source, "            context.chainId")?;
    writeln!(source, "        ));")?;
    writeln!(source)?;
    writeln!(source, "        _writeState(decision.nextState);")?;
    writeln!(source, "        _stateHash = postStateHash;")?;
    writeln!(
        source,
        "        emit TransitionCommitted(actualStateHash, postStateHash, candidateHash, commandHash, context.caller);"
    )?;
    writeln!(source, "        _gate = 1;")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_storage_helpers(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(
        source,
        "    function _readState() private view returns (State memory current) {{"
    )?;
    for field in spec.state_fields() {
        writeln!(
            source,
            "        current.{} = _state_{};",
            field.name(),
            field.name()
        )?;
    }
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function _writeState(State memory nextState) private {{"
    )?;
    for field in spec.state_fields() {
        writeln!(
            source,
            "        _state_{} = nextState.{};",
            field.name(),
            field.name()
        )?;
    }
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_hash_helpers(
    source: &mut String,
    spec: &SolidityContractSpec,
) -> Result<(), SolidityGenerationError> {
    writeln!(
        source,
        "    function _hashState(State memory value) private pure returns (bytes32) {{"
    )?;
    writeln!(source, "        return keccak256(abi.encode(")?;
    writeln!(
        source,
        "            \"zeno-fcis/solidity/state/v1/{}\",",
        spec.contract_name()
    )?;
    for (index, field) in spec.state_fields().iter().enumerate() {
        let suffix = if index + 1 == spec.state_fields().len() {
            ""
        } else {
            ","
        };
        writeln!(source, "            value.{}{suffix}", field.name())?;
    }
    writeln!(source, "        ));")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;

    writeln!(
        source,
        "    function _hashCommand(Command memory value) private pure returns (bytes32) {{"
    )?;
    writeln!(source, "        return keccak256(abi.encode(")?;
    writeln!(
        source,
        "            \"zeno-fcis/solidity/command/v1/{}\",",
        spec.contract_name()
    )?;
    for (index, field) in spec.command_fields().iter().enumerate() {
        let suffix = if index + 1 == spec.command_fields().len() {
            ""
        } else {
            ","
        };
        writeln!(source, "            value.{}{suffix}", field.name())?;
    }
    writeln!(source, "        ));")?;
    writeln!(source, "    }}")?;
    writeln!(source)?;
    Ok(())
}

fn render_pure_hooks(source: &mut String) -> Result<(), SolidityGenerationError> {
    writeln!(
        source,
        "    function _commandAdmissible(Command memory command, Context memory context) internal pure virtual returns (bool);"
    )?;
    writeln!(source)?;
    writeln!(
        source,
        "    function _invariant(State memory stateValue) internal pure virtual returns (bool);"
    )?;
    writeln!(source)?;
    writeln!(
        source,
        "    function _decide(State memory stateValue, Command memory command, Context memory context) internal pure virtual returns (Decision memory);"
    )?;
    Ok(())
}

#[derive(Clone, Copy)]
enum IdentifierRole {
    Field,
    Type,
}

fn validate_identifier(value: &str, role: IdentifierRole) -> Result<(), SolidityGenerationError> {
    if value.is_empty() || value.len() > 96 || is_reserved_keyword(value) {
        return Err(SolidityGenerationError::InvalidIdentifier);
    }

    let mut bytes = value.bytes();
    let first = match bytes.next() {
        Some(value) => value,
        None => return Err(SolidityGenerationError::InvalidIdentifier),
    };
    if !(first.is_ascii_alphabetic() || first == b'_')
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(SolidityGenerationError::InvalidIdentifier);
    }

    if matches!(role, IdentifierRole::Type) && !first.is_ascii_uppercase() {
        return Err(SolidityGenerationError::InvalidTypeName);
    }
    Ok(())
}

fn validate_fields(
    fields: &[SolidityField],
    kind: SolidityListKind,
) -> Result<(), SolidityGenerationError> {
    if fields.is_empty() {
        return Err(SolidityGenerationError::EmptyList(kind));
    }
    if fields.len() > MAX_SOLIDITY_FIELDS {
        return Err(SolidityGenerationError::LimitExceeded(kind));
    }

    let mut names = BTreeSet::new();
    for field in fields {
        if !names.insert(field.name()) {
            return Err(SolidityGenerationError::DuplicateName);
        }
    }
    Ok(())
}

fn validate_reasons(reasons: &[String]) -> Result<(), SolidityGenerationError> {
    if reasons.is_empty() {
        return Err(SolidityGenerationError::EmptyList(
            SolidityListKind::RejectionReasons,
        ));
    }
    if reasons.len() > MAX_SOLIDITY_REASONS {
        return Err(SolidityGenerationError::LimitExceeded(
            SolidityListKind::RejectionReasons,
        ));
    }

    let mut names = BTreeSet::new();
    for reason in reasons {
        validate_identifier(reason, IdentifierRole::Type)?;
        if reason == "None" {
            return Err(SolidityGenerationError::ReservedReason);
        }
        if !names.insert(reason.as_str()) {
            return Err(SolidityGenerationError::DuplicateName);
        }
    }
    Ok(())
}

fn is_reserved_keyword(value: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "after",
        "alias",
        "anonymous",
        "apply",
        "as",
        "assembly",
        "auto",
        "byte",
        "calldata",
        "case",
        "catch",
        "constant",
        "constructor",
        "contract",
        "copyof",
        "default",
        "define",
        "delete",
        "do",
        "else",
        "emit",
        "error",
        "event",
        "external",
        "false",
        "final",
        "for",
        "from",
        "function",
        "immutable",
        "implements",
        "import",
        "in",
        "indexed",
        "inline",
        "interface",
        "internal",
        "is",
        "let",
        "library",
        "mapping",
        "match",
        "memory",
        "modifier",
        "mutable",
        "new",
        "null",
        "of",
        "override",
        "partial",
        "payable",
        "pragma",
        "private",
        "promise",
        "public",
        "pure",
        "reference",
        "relocatable",
        "return",
        "returns",
        "sealed",
        "sizeof",
        "static",
        "storage",
        "struct",
        "supports",
        "switch",
        "this",
        "throw",
        "transient",
        "true",
        "try",
        "type",
        "typedef",
        "typeof",
        "unchecked",
        "using",
        "var",
        "view",
        "virtual",
        "while",
    ];
    KEYWORDS.contains(&value)
}

fn sanitize_non_code(source: &str) -> String {
    #[derive(Clone, Copy)]
    enum Mode {
        Code,
        LineComment,
        BlockComment,
        SingleQuoted,
        DoubleQuoted,
    }

    let bytes = source.as_bytes();
    let mut output = vec![b' '; bytes.len()];
    let mut mode = Mode::Code;
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();

        match mode {
            Mode::Code => match (byte, next) {
                (b'/', Some(b'/')) => {
                    mode = Mode::LineComment;
                    index += 2;
                    continue;
                }
                (b'/', Some(b'*')) => {
                    mode = Mode::BlockComment;
                    index += 2;
                    continue;
                }
                (b'\'', _) => {
                    mode = Mode::SingleQuoted;
                    index += 1;
                    continue;
                }
                (b'"', _) => {
                    mode = Mode::DoubleQuoted;
                    index += 1;
                    continue;
                }
                _ => output[index] = byte,
            },
            Mode::LineComment => {
                if byte == b'\n' {
                    output[index] = byte;
                    mode = Mode::Code;
                }
            }
            Mode::BlockComment => {
                if byte == b'\n' {
                    output[index] = byte;
                } else if byte == b'*' && next == Some(b'/') {
                    index += 2;
                    mode = Mode::Code;
                    continue;
                }
            }
            Mode::SingleQuoted | Mode::DoubleQuoted => {
                let quote = match mode {
                    Mode::SingleQuoted => b'\'',
                    Mode::DoubleQuoted => b'"',
                    _ => unreachable!("quote mode is known"),
                };
                if byte == b'\\' && next.is_some() {
                    index += 2;
                    continue;
                }
                if byte == quote {
                    mode = Mode::Code;
                } else if byte == b'\n' {
                    output[index] = byte;
                }
            }
        }

        index += 1;
    }

    String::from_utf8(output).unwrap_or_default()
}

fn token_boundary_matches(haystack: &str, offset: usize, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let needle_bytes = needle.as_bytes();

    let begins_identifier = needle_bytes
        .first()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
    let ends_identifier = needle_bytes
        .last()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');

    let before_ok = !begins_identifier
        || offset == 0
        || !bytes[offset - 1].is_ascii_alphanumeric() && bytes[offset - 1] != b'_';
    let after = offset.saturating_add(needle.len());
    let after_ok = !ends_identifier
        || after >= bytes.len()
        || !bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_';

    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, scalar: SolidityScalar) -> SolidityField {
        match SolidityField::try_new(name, scalar) {
            Ok(value) => value,
            Err(error) => panic!("field rejected: {error}"),
        }
    }

    fn fixture_spec() -> SolidityContractSpec {
        match SolidityContractSpec::try_new(
            "CounterFcis",
            vec![
                field("owner", SolidityScalar::Address),
                field("counter", SolidityScalar::U128),
            ],
            vec![field("delta", SolidityScalar::U128)],
            vec!["Unauthorized".to_owned(), "Overflow".to_owned()],
        ) {
            Ok(value) => value,
            Err(error) => panic!("spec rejected: {error}"),
        }
    }

    #[test]
    fn repeated_generation_is_identical() {
        let spec = fixture_spec();
        assert_eq!(generate_solidity(&spec), generate_solidity(&spec));
    }

    #[test]
    fn generated_shell_has_required_fcis_boundaries() {
        let generated = match generate_solidity(&fixture_spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let source = generated.source();

        assert!(source.contains("internal pure virtual returns (Decision memory);"));
        assert!(source.contains("StateRootCorrupted"));
        assert!(source.contains("StaleState"));
        assert!(source.contains("_commandAdmissible"));
        assert!(source.contains("_invariant"));
        assert!(source.contains("_writeState(decision.nextState);"));
        assert!(source.contains("emit TransitionCommitted"));
        assert!(!source.contains("delegatecall"));
        assert!(!source.contains("selfdestruct"));
    }

    #[test]
    fn generated_shell_passes_strict_source_inspection() {
        let generated = match generate_solidity(&fixture_spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let report = inspect_solidity_source(generated.source());
        assert!(report.is_clean(), "findings: {:?}", report.findings());
    }

    #[test]
    fn duplicate_fields_are_rejected() {
        let result = SolidityContractSpec::try_new(
            "DuplicateFcis",
            vec![
                field("value", SolidityScalar::U128),
                field("value", SolidityScalar::U256),
            ],
            vec![field("delta", SolidityScalar::U128)],
            vec!["Invalid".to_owned()],
        );
        assert_eq!(result, Err(SolidityGenerationError::DuplicateName));
    }

    #[test]
    fn reserved_reason_is_rejected() {
        let result = SolidityContractSpec::try_new(
            "ReasonFcis",
            vec![field("value", SolidityScalar::U128)],
            vec![field("delta", SolidityScalar::U128)],
            vec!["None".to_owned()],
        );
        assert_eq!(result, Err(SolidityGenerationError::ReservedReason));
    }

    #[test]
    fn source_inspection_finds_dangerous_mechanisms() {
        let source = r#"
            contract Unsafe {
                function run(address target) external {
                    (bool ok,) = target.delegatecall("");
                    assembly { sstore(0, 1) }
                    if (tx.origin == msg.sender) selfdestruct(payable(msg.sender));
                }
            }
        "#;
        let report = inspect_solidity_source(source);
        let kinds: BTreeSet<_> = report
            .findings()
            .iter()
            .map(SoliditySafetyFinding::kind)
            .collect();

        assert!(kinds.contains(&SoliditySafetyFindingKind::DelegateCall));
        assert!(kinds.contains(&SoliditySafetyFindingKind::InlineAssembly));
        assert!(kinds.contains(&SoliditySafetyFindingKind::RawStorageOpcode));
        assert!(kinds.contains(&SoliditySafetyFindingKind::TxOrigin));
        assert!(kinds.contains(&SoliditySafetyFindingKind::ContractDestruction));
    }

    #[test]
    fn source_inspection_ignores_comments_and_strings() {
        let source = r#"
            // delegatecall assembly tx.origin
            contract Safe {
                string private constant NOTE = "selfdestruct .call unchecked";
            }
        "#;
        assert!(inspect_solidity_source(source).is_clean());
    }
}
