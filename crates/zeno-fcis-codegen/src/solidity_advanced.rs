//! Advanced Solidity generation from the shared on-chain FCIS machine model.
//!
//! The generated shell owns storage, context capture, decision validation,
//! bounded observation plans, catalogued fungible-transfer authority, atomic
//! commit, receipts, and effect interpretation. Derived contracts implement
//! only compiler-enforced `internal pure` hooks.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::onchain::{
    GeneratedOnchainBundle, GeneratedOnchainFile, ObservationPolicy, OnchainCapability,
    OnchainCapabilityKind, OnchainField, OnchainMachineSpec, OnchainModelError, OnchainScalar,
    RecipientPolicy,
};
use crate::solidity::inspect_solidity_source;

/// Stable semantic identity for the advanced Solidity backend.
pub const SOLIDITY_ADVANCED_GENERATOR_ID: &str = "zeno-fcis-solidity/2";
/// Exact Solidity compiler version required by generated v2 source.
pub const SOLIDITY_PINNED_VERSION: &str = "0.8.35";
/// Reviewed OpenZeppelin Contracts release expected by generated imports.
pub const OPENZEPPELIN_CONTRACTS_VERSION: &str = "5.6.1";

/// Exact EVM binding for one shared fungible-transfer capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolidityFungibleBinding {
    capability_code: u16,
    token_address: [u8; 20],
    runtime_code_hash: [u8; 32],
}

impl SolidityFungibleBinding {
    /// Constructs one nonzero token-address and runtime-code-hash binding.
    pub fn try_new(
        capability_code: u16,
        token_address: [u8; 20],
        runtime_code_hash: [u8; 32],
    ) -> Result<Self, OnchainModelError> {
        if capability_code == 0 || token_address == [0_u8; 20] || runtime_code_hash == [0_u8; 32] {
            return Err(OnchainModelError::InvalidBinding);
        }
        Ok(Self {
            capability_code,
            token_address,
            runtime_code_hash,
        })
    }

    /// Returns the shared capability code.
    #[must_use]
    pub const fn capability_code(&self) -> u16 {
        self.capability_code
    }

    /// Returns the exact token address.
    #[must_use]
    pub const fn token_address(&self) -> [u8; 20] {
        self.token_address
    }

    /// Returns the required runtime code hash.
    #[must_use]
    pub const fn runtime_code_hash(&self) -> [u8; 32] {
        self.runtime_code_hash
    }
}

/// Closed request for one advanced Solidity FCIS bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolidityAdvancedSpec {
    machine: OnchainMachineSpec,
    fungible_bindings: Vec<SolidityFungibleBinding>,
}

impl SolidityAdvancedSpec {
    /// Validates exact one-to-one bindings for every fungible capability.
    pub fn try_new(
        machine: OnchainMachineSpec,
        mut fungible_bindings: Vec<SolidityFungibleBinding>,
    ) -> Result<Self, OnchainModelError> {
        fungible_bindings.sort_by_key(SolidityFungibleBinding::capability_code);
        if fungible_bindings
            .windows(2)
            .any(|pair| pair[0].capability_code == pair[1].capability_code)
        {
            return Err(OnchainModelError::InvalidBinding);
        }

        let expected: Vec<u16> = machine
            .capabilities()
            .iter()
            .filter(|capability| capability.kind() == OnchainCapabilityKind::FungibleTransfer)
            .map(OnchainCapability::code)
            .collect();
        let actual: Vec<u16> = fungible_bindings
            .iter()
            .map(SolidityFungibleBinding::capability_code)
            .collect();
        if expected != actual {
            return Err(OnchainModelError::InvalidBinding);
        }

        Ok(Self {
            machine,
            fungible_bindings,
        })
    }

    /// Returns the shared semantic machine.
    #[must_use]
    pub const fn machine(&self) -> &OnchainMachineSpec {
        &self.machine
    }

    /// Returns exact fungible bindings in capability-code order.
    #[must_use]
    pub fn fungible_bindings(&self) -> &[SolidityFungibleBinding] {
        &self.fungible_bindings
    }
}

/// Generates an abstract Solidity shell, manifest, and agent-editing policy.
pub fn generate_advanced_solidity(
    spec: &SolidityAdvancedSpec,
) -> Result<GeneratedOnchainBundle, OnchainModelError> {
    let source = render_contract(spec)?;
    let safety = inspect_solidity_source(&source);
    if !safety.is_clean() {
        return Err(OnchainModelError::InvalidBinding);
    }

    let machine_hash = spec.machine().machine_hash();
    let base = to_lower_snake(spec.machine().name());
    let manifest = render_manifest(spec);
    let policy = render_agent_policy(spec);
    GeneratedOnchainBundle::try_new(
        SOLIDITY_ADVANCED_GENERATOR_ID,
        machine_hash,
        vec![
            GeneratedOnchainFile::new(
                format!("solidity/{base}/{}Fcis.sol", spec.machine().name()),
                source,
            ),
            GeneratedOnchainFile::new(format!("solidity/{base}/MANIFEST.zfcis"), manifest),
            GeneratedOnchainFile::new(format!("solidity/{base}/AGENT_POLICY.md"), policy),
        ],
    )
}

fn render_contract(spec: &SolidityAdvancedSpec) -> Result<String, OnchainModelError> {
    let machine = spec.machine();
    let event_capacity = usize::from(machine.max_event_slots().max(1));
    let effect_capacity = usize::from(machine.max_effect_slots().max(1));
    let machine_hash = hex32(machine.machine_hash().into_bytes());
    let contract_name = format!("{}Fcis", machine.name());
    let mut output = String::new();

    output.push_str("// SPDX-License-Identifier: MIT OR Apache-2.0\n");
    writeln!(output, "pragma solidity {SOLIDITY_PINNED_VERSION};")?;
    output.push('\n');
    output.push_str("import {IERC20} from \"@openzeppelin/contracts/token/ERC20/IERC20.sol\";\n");
    output.push_str(
        "import {SafeERC20} from \"@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol\";\n\n",
    );
    writeln!(
        output,
        "/// @notice Generated by {SOLIDITY_ADVANCED_GENERATOR_ID}."
    )?;
    output.push_str(
        "/// @dev Regenerate this base. Derived contracts implement only the pure hooks.\n",
    );
    writeln!(output, "abstract contract {contract_name} {{")?;
    output.push_str("    using SafeERC20 for IERC20;\n\n");
    writeln!(
        output,
        "    bytes32 public constant MACHINE_HASH = {machine_hash};"
    )?;
    writeln!(
        output,
        "    uint16 public constant MACHINE_VERSION = {};",
        machine.version()
    )?;
    writeln!(
        output,
        "    uint8 private constant MAX_EVENT_SLOTS = {};",
        machine.max_event_slots()
    )?;
    writeln!(
        output,
        "    uint8 private constant MAX_EFFECT_SLOTS = {};",
        machine.max_effect_slots()
    )?;
    output.push('\n');

    output.push_str("    enum DecisionKind { Accept, Reject }\n\n");
    render_struct(&mut output, "State", machine.state_fields())?;
    render_struct(&mut output, "Command", machine.command_fields())?;
    output.push_str("    struct Context {\n");
    output.push_str("        bytes32 actor;\n");
    output.push_str("        bytes32 chainDomain;\n");
    output.push_str("        uint64 sequence;\n");
    output.push_str("        uint64 blockNumber;\n");
    output.push_str("        uint64 timestamp;\n");
    output.push_str("    }\n\n");
    output.push_str("    struct EventPlan {\n");
    output.push_str("        uint16 code;\n");
    output.push_str("        uint8 fieldCount;\n");
    output.push_str("        bytes32[8] data;\n");
    output.push_str("    }\n\n");
    output.push_str("    struct EffectPlan {\n");
    output.push_str("        uint16 capability;\n");
    output.push_str("        bytes32 assetId;\n");
    output.push_str("        bytes32 recipient;\n");
    output.push_str("        uint128 amount;\n");
    output.push_str("    }\n\n");
    output.push_str("    struct Decision {\n");
    output.push_str("        DecisionKind kind;\n");
    output.push_str("        uint16 reasonCode;\n");
    output.push_str("        State nextState;\n");
    output.push_str("        uint8 eventCount;\n");
    writeln!(output, "        EventPlan[{event_capacity}] events;")?;
    output.push_str("        uint8 effectCount;\n");
    writeln!(output, "        EffectPlan[{effect_capacity}] effects;")?;
    output.push_str("    }\n\n");

    output.push_str("    address private immutable _initializationAuthority;\n");
    output.push_str("    bool private _initialized;\n");
    output.push_str("    uint256 private _gate;\n");
    output.push_str("    uint64 private _sequence;\n");
    output.push_str("    bytes32 private _stateHash;\n");
    for field in machine.state_fields() {
        writeln!(
            output,
            "    {} private _state_{};",
            solidity_type(field.scalar()),
            field.name()
        )?;
    }
    output.push('\n');

    output.push_str("    error UnauthorizedInitializer(address expected, address actual);\n");
    output.push_str("    error AlreadyInitialized();\n");
    output.push_str("    error NotInitialized();\n");
    output.push_str("    error Reentrancy();\n");
    output.push_str("    error StaleState(bytes32 expectedHash, bytes32 actualHash, uint64 expectedSequence, uint64 actualSequence);\n");
    output.push_str("    error StateRootCorrupted(bytes32 committed, bytes32 actual);\n");
    output.push_str("    error CommandNotAdmissible();\n");
    output.push_str("    error InvariantViolation();\n");
    output.push_str("    error InvalidDecision(uint16 reasonCode);\n");
    output.push_str("    error TransitionRejected(uint16 reasonCode);\n");
    output.push_str("    error InvalidPlan();\n");
    output.push_str("    error InvalidCapability(uint16 capability);\n");
    output.push_str(
        "    error AssetBindingChanged(uint16 capability, bytes32 expected, bytes32 actual);\n\n",
    );

    output
        .push_str("    event Initialized(bytes32 indexed stateHash, address indexed authority);\n");
    output.push_str("    event DomainEvent(uint16 indexed code, bytes32 indexed payloadHash, bytes32[8] data, uint8 fieldCount);\n");
    output.push_str("    event TransitionCommitted(uint64 indexed sequence, bytes32 indexed preStateHash, bytes32 indexed postStateHash, bytes32 candidateHash, bytes32 commandHash, bytes32 contextHash);\n\n");

    output.push_str("    constructor() {\n");
    output.push_str("        _initializationAuthority = msg.sender;\n");
    output.push_str("        _gate = 1;\n");
    output.push_str("    }\n\n");
    output.push_str("    function initializationAuthority() external view returns (address) { return _initializationAuthority; }\n");
    output.push_str("    function currentStateHash() external view returns (bytes32) { if (!_initialized) revert NotInitialized(); return _stateHash; }\n");
    output.push_str("    function currentSequence() external view returns (uint64) { if (!_initialized) revert NotInitialized(); return _sequence; }\n");
    output.push_str("    function currentState() external view returns (State memory) { if (!_initialized) revert NotInitialized(); return _readState(); }\n\n");

    output.push_str("    function initialize(State calldata initialState) external {\n");
    output.push_str("        if (msg.sender != _initializationAuthority) revert UnauthorizedInitializer(_initializationAuthority, msg.sender);\n");
    output.push_str("        if (_initialized) revert AlreadyInitialized();\n");
    output.push_str("        State memory admitted = initialState;\n");
    output.push_str("        if (!_invariant(admitted)) revert InvariantViolation();\n");
    output.push_str("        _writeState(admitted);\n");
    output.push_str("        _stateHash = _hashState(admitted);\n");
    output.push_str("        _initialized = true;\n");
    output.push_str("        emit Initialized(_stateHash, msg.sender);\n");
    output.push_str("    }\n\n");

    render_execute(&mut output, machine, event_capacity, effect_capacity)?;
    render_storage_helpers(&mut output, machine)?;
    render_hash_helpers(&mut output, machine)?;
    render_validation_helpers(&mut output, spec, event_capacity, effect_capacity)?;
    render_typed_builders(&mut output, machine)?;
    render_catalog_helpers(&mut output, spec)?;

    output.push_str("    function _commandAdmissible(Command memory command, Context memory context) internal pure virtual returns (bool);\n\n");
    output.push_str("    function _invariant(State memory stateValue) internal pure virtual returns (bool);\n\n");
    output.push_str("    function _decide(State memory stateValue, Command memory command, Context memory context) internal pure virtual returns (Decision memory);\n");
    output.push_str("}\n");
    Ok(output)
}

fn render_execute(
    output: &mut String,
    machine: &OnchainMachineSpec,
    event_capacity: usize,
    effect_capacity: usize,
) -> Result<(), OnchainModelError> {
    output.push_str("    function execute(Command calldata command, bytes32 expectedStateHash, uint64 expectedSequence) external returns (bytes32 postStateHash) {\n");
    output.push_str("        if (!_initialized) revert NotInitialized();\n");
    output.push_str("        if (_gate != 1) revert Reentrancy();\n");
    output.push_str("        _gate = 2;\n");
    output.push_str("        State memory beforeState = _readState();\n");
    output.push_str("        bytes32 actualStateHash = _hashState(beforeState);\n");
    output.push_str("        if (actualStateHash != _stateHash) revert StateRootCorrupted(_stateHash, actualStateHash);\n");
    output.push_str("        if (actualStateHash != expectedStateHash || _sequence != expectedSequence) revert StaleState(expectedStateHash, actualStateHash, expectedSequence, _sequence);\n");
    output.push_str("        Command memory admittedCommand = command;\n");
    output.push_str("        Context memory context = Context({ actor: bytes32(uint256(uint160(msg.sender))), chainDomain: sha256(abi.encode(block.chainid, address(this), MACHINE_HASH)), sequence: _sequence, blockNumber: uint64(block.number), timestamp: uint64(block.timestamp) });\n");
    output.push_str("        if (!_commandAdmissible(admittedCommand, context)) revert CommandNotAdmissible();\n");
    output.push_str(
        "        Decision memory decision = _decide(beforeState, admittedCommand, context);\n",
    );
    output.push_str("        if (decision.kind == DecisionKind.Reject) {\n");
    output.push_str("            if (!_knownReason(decision.reasonCode)) revert InvalidDecision(decision.reasonCode);\n");
    output.push_str("            revert TransitionRejected(decision.reasonCode);\n");
    output.push_str("        }\n");
    output.push_str(
        "        if (decision.reasonCode != 0) revert InvalidDecision(decision.reasonCode);\n",
    );
    output.push_str("        if (!_invariant(decision.nextState)) revert InvariantViolation();\n");
    output.push_str("        _validatePlans(decision, beforeState, admittedCommand, context);\n");
    output.push_str("        postStateHash = _hashState(decision.nextState);\n");
    output.push_str("        bytes32 commandHash = _hashCommand(admittedCommand);\n");
    output.push_str("        bytes32 contextHash = _hashContext(context);\n");
    output.push_str("        bytes32 eventPlanHash = _hashEventPlan(decision);\n");
    output.push_str("        bytes32 effectPlanHash = _hashEffectPlan(decision);\n");
    output.push_str("        bytes32 candidateHash = sha256(abi.encode(MACHINE_HASH, actualStateHash, postStateHash, commandHash, contextHash, eventPlanHash, effectPlanHash));\n");
    output.push_str("        _writeState(decision.nextState);\n");
    output.push_str("        _stateHash = postStateHash;\n");
    output.push_str("        _sequence += 1;\n");
    output.push_str("        for (uint256 index = 0; index < decision.eventCount; ++index) {\n");
    output.push_str("            EventPlan memory plannedEvent = decision.events[index];\n");
    output.push_str("            emit DomainEvent(plannedEvent.code, _hashEvent(plannedEvent), plannedEvent.data, plannedEvent.fieldCount);\n");
    output.push_str("        }\n");
    output.push_str("        for (uint256 index = 0; index < decision.effectCount; ++index) { _applyEffect(decision.effects[index]); }\n");
    output.push_str("        emit TransitionCommitted(_sequence, actualStateHash, postStateHash, candidateHash, commandHash, contextHash);\n");
    output.push_str("        _gate = 1;\n");
    output.push_str("    }\n\n");

    let _ = (machine, event_capacity, effect_capacity);
    Ok(())
}

fn render_storage_helpers(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    output.push_str("    function _readState() private view returns (State memory current) {\n");
    for field in machine.state_fields() {
        writeln!(
            output,
            "        current.{} = _state_{};",
            field.name(),
            field.name()
        )?;
    }
    output.push_str("    }\n\n");
    output.push_str("    function _writeState(State memory nextState) private {\n");
    for field in machine.state_fields() {
        writeln!(
            output,
            "        _state_{} = nextState.{};",
            field.name(),
            field.name()
        )?;
    }
    output.push_str("    }\n\n");
    Ok(())
}

fn render_hash_helpers(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    output.push_str("    function _hashState(State memory value) private pure returns (bytes32) { return sha256(abi.encode(MACHINE_HASH");
    for field in machine.state_fields() {
        write!(output, ", uint16({}), value.{}", field.id(), field.name())?;
    }
    output.push_str(")); }\n");
    output.push_str("    function _hashCommand(Command memory value) private pure returns (bytes32) { return sha256(abi.encode(MACHINE_HASH");
    for field in machine.command_fields() {
        write!(output, ", uint16({}), value.{}", field.id(), field.name())?;
    }
    output.push_str(")); }\n");
    output.push_str("    function _hashContext(Context memory value) private pure returns (bytes32) { return sha256(abi.encode(MACHINE_HASH, value.actor, value.chainDomain, value.sequence, value.blockNumber, value.timestamp)); }\n");
    output.push_str("    function _hashEvent(EventPlan memory value) private pure returns (bytes32) { return sha256(abi.encode(MACHINE_HASH, value.code, value.fieldCount, value.data)); }\n");
    output.push_str("    function _hashEffect(EffectPlan memory value) private pure returns (bytes32) { return sha256(abi.encode(MACHINE_HASH, value.capability, value.assetId, value.recipient, value.amount)); }\n");
    output.push_str("    function _hashEventPlan(Decision memory value) private pure returns (bytes32 result) { result = sha256(abi.encode(MACHINE_HASH, value.eventCount)); for (uint256 index = 0; index < value.eventCount; ++index) result = sha256(abi.encode(result, _hashEvent(value.events[index]))); }\n");
    output.push_str("    function _hashEffectPlan(Decision memory value) private pure returns (bytes32 result) { result = sha256(abi.encode(MACHINE_HASH, value.effectCount)); for (uint256 index = 0; index < value.effectCount; ++index) result = sha256(abi.encode(result, _hashEffect(value.effects[index]))); }\n\n");
    Ok(())
}

fn render_validation_helpers(
    output: &mut String,
    spec: &SolidityAdvancedSpec,
    event_capacity: usize,
    effect_capacity: usize,
) -> Result<(), OnchainModelError> {
    let machine = spec.machine();
    output.push_str("    function _validatePlans(Decision memory decision, State memory beforeState, Command memory command, Context memory context) private view {\n");
    output.push_str("        if (decision.eventCount > MAX_EVENT_SLOTS || decision.effectCount > MAX_EFFECT_SLOTS) revert InvalidPlan();\n");
    if machine.observation_policy() == ObservationPolicy::FixedShape {
        output.push_str("        if (decision.eventCount != MAX_EVENT_SLOTS || decision.effectCount != MAX_EFFECT_SLOTS) revert InvalidPlan();\n");
    }
    output.push_str("        bytes32 prior;\n");
    output.push_str("        for (uint256 index = 0; index < decision.eventCount; ++index) { EventPlan memory planned = decision.events[index]; if (_eventFieldCount(planned.code) != planned.fieldCount) revert InvalidPlan(); bytes32 digest = _hashEvent(planned); if (index != 0 && uint256(digest) < uint256(prior)) revert InvalidPlan(); prior = digest; }\n");
    writeln!(
        output,
        "        for (uint256 index = decision.eventCount; index < {event_capacity}; ++index) if (!_zeroEvent(decision.events[index])) revert InvalidPlan();"
    )?;
    output.push_str("        prior = bytes32(0);\n");
    output.push_str("        for (uint256 index = 0; index < decision.effectCount; ++index) { EffectPlan memory planned = decision.effects[index]; _validateEffect(planned, beforeState, command, context); bytes32 digest = _hashEffect(planned); if (index != 0 && uint256(digest) < uint256(prior)) revert InvalidPlan(); prior = digest; uint8 uses; for (uint256 inner = 0; inner <= index; ++inner) if (decision.effects[inner].capability == planned.capability) ++uses; if (uses > _capabilityMaxUses(planned.capability)) revert InvalidPlan(); }\n");
    writeln!(
        output,
        "        for (uint256 index = decision.effectCount; index < {effect_capacity}; ++index) if (!_zeroEffect(decision.effects[index])) revert InvalidPlan();"
    )?;
    output.push_str("    }\n\n");
    output.push_str("    function _validateEffect(EffectPlan memory planned, State memory beforeState, Command memory command, Context memory context) private view {\n");
    output.push_str("        if (!_knownCapability(planned.capability) || planned.amount == 0 || planned.amount > _capabilityMaxAmount(planned.capability)) revert InvalidCapability(planned.capability);\n");
    output.push_str("        if (planned.assetId != _capabilityAsset(planned.capability) || planned.recipient != _expectedRecipient(planned.capability, beforeState, command, context) || uint256(planned.recipient) > type(uint160).max) revert InvalidCapability(planned.capability);\n");
    output.push_str("        address token = _tokenAddress(planned.capability); bytes32 actualCodeHash = token.codehash; bytes32 expectedCodeHash = _tokenCodeHash(planned.capability); if (actualCodeHash != expectedCodeHash) revert AssetBindingChanged(planned.capability, expectedCodeHash, actualCodeHash);\n");
    output.push_str("    }\n\n");
    output.push_str("    function _zeroEvent(EventPlan memory value) private pure returns (bool) { if (value.code != 0 || value.fieldCount != 0) return false; for (uint256 index = 0; index < 8; ++index) if (value.data[index] != bytes32(0)) return false; return true; }\n");
    output.push_str("    function _zeroEffect(EffectPlan memory value) private pure returns (bool) { return value.capability == 0 && value.assetId == bytes32(0) && value.recipient == bytes32(0) && value.amount == 0; }\n");
    output.push_str("    function _applyEffect(EffectPlan memory planned) private { IERC20(_tokenAddress(planned.capability)).safeTransfer(address(uint160(uint256(planned.recipient))), uint256(planned.amount)); }\n\n");
    Ok(())
}

fn render_typed_builders(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    for event in machine.events() {
        write!(output, "    function _event{}(", event.name())?;
        render_parameter_list(output, event.fields())?;
        output.push_str(") internal pure returns (EventPlan memory planned) {\n");
        writeln!(output, "        planned.code = {};", event.code())?;
        writeln!(
            output,
            "        planned.fieldCount = {};",
            event.fields().len()
        )?;
        for (index, field) in event.fields().iter().enumerate() {
            writeln!(
                output,
                "        planned.data[{index}] = {};",
                encode_to_bytes32(field.scalar(), field.name())
            )?;
        }
        output.push_str("    }\n\n");
    }
    for capability in machine.capabilities() {
        writeln!(
            output,
            "    function _effect{}(uint128 amount, State memory beforeState, Command memory command, Context memory context) internal pure returns (EffectPlan memory planned) {{",
            capability.name()
        )?;
        writeln!(
            output,
            "        planned.capability = {};",
            capability.code()
        )?;
        writeln!(
            output,
            "        planned.assetId = {};",
            hex32(capability.asset_id())
        )?;
        writeln!(
            output,
            "        planned.recipient = {};",
            recipient_expression(capability.recipient(), machine)
        )?;
        output.push_str("        planned.amount = amount;\n");
        output.push_str("    }\n\n");
    }
    Ok(())
}

fn render_catalog_helpers(
    output: &mut String,
    spec: &SolidityAdvancedSpec,
) -> Result<(), OnchainModelError> {
    let machine = spec.machine();
    output.push_str("    function _knownReason(uint16 code) private pure returns (bool) { return ");
    render_code_disjunction(output, machine.reasons().iter().map(|reason| reason.code()))?;
    output.push_str("; }\n");
    output.push_str(
        "    function _knownCapability(uint16 code) private pure returns (bool) { return ",
    );
    render_code_disjunction(
        output,
        machine.capabilities().iter().map(OnchainCapability::code),
    )?;
    output.push_str("; }\n");

    output.push_str("    function _eventFieldCount(uint16 code) private pure returns (uint8) {\n");
    for event in machine.events() {
        writeln!(
            output,
            "        if (code == {}) return {};",
            event.code(),
            event.fields().len()
        )?;
    }
    output.push_str("        revert InvalidPlan();\n    }\n");

    output
        .push_str("    function _capabilityAsset(uint16 code) private pure returns (bytes32) {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            hex32(capability.asset_id())
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n");

    output.push_str(
        "    function _capabilityMaxAmount(uint16 code) private pure returns (uint128) {\n",
    );
    for capability in machine.capabilities() {
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            capability.max_amount()
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n");

    output
        .push_str("    function _capabilityMaxUses(uint16 code) private pure returns (uint8) {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            capability.max_uses()
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n");

    output.push_str("    function _expectedRecipient(uint16 code, State memory beforeState, Command memory command, Context memory context) private pure returns (bytes32) {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            recipient_expression(capability.recipient(), machine)
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n");

    let bindings: BTreeMap<u16, SolidityFungibleBinding> = spec
        .fungible_bindings()
        .iter()
        .map(|binding| (binding.capability_code(), *binding))
        .collect();
    output.push_str("    function _tokenAddress(uint16 code) private pure returns (address) {\n");
    for capability in machine.capabilities() {
        let binding = bindings
            .get(&capability.code())
            .ok_or(OnchainModelError::InvalidBinding)?;
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            hex20(binding.token_address())
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n");
    output.push_str("    function _tokenCodeHash(uint16 code) private pure returns (bytes32) {\n");
    for capability in machine.capabilities() {
        let binding = bindings
            .get(&capability.code())
            .ok_or(OnchainModelError::InvalidBinding)?;
        writeln!(
            output,
            "        if (code == {}) return {};",
            capability.code(),
            hex32(binding.runtime_code_hash())
        )?;
    }
    output.push_str("        revert InvalidCapability(code);\n    }\n\n");
    Ok(())
}

fn render_struct(
    output: &mut String,
    name: &str,
    fields: &[OnchainField],
) -> Result<(), OnchainModelError> {
    writeln!(output, "    struct {name} {{")?;
    for field in fields {
        writeln!(
            output,
            "        {} {};",
            solidity_type(field.scalar()),
            field.name()
        )?;
    }
    output.push_str("    }\n\n");
    Ok(())
}

fn render_parameter_list(
    output: &mut String,
    fields: &[OnchainField],
) -> Result<(), OnchainModelError> {
    for (index, field) in fields.iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        write!(output, "{} {}", solidity_type(field.scalar()), field.name())?;
    }
    Ok(())
}

fn render_code_disjunction(
    output: &mut String,
    codes: impl Iterator<Item = u16>,
) -> Result<(), OnchainModelError> {
    let collected: Vec<u16> = codes.collect();
    if collected.is_empty() {
        output.push_str("false");
        return Ok(());
    }
    for (index, code) in collected.iter().enumerate() {
        if index != 0 {
            output.push_str(" || ");
        }
        write!(output, "code == {code}")?;
    }
    Ok(())
}

fn recipient_expression(policy: RecipientPolicy, machine: &OnchainMachineSpec) -> String {
    match policy {
        RecipientPolicy::Caller => "context.actor".to_owned(),
        RecipientPolicy::Fixed(value) => hex32(value),
        RecipientPolicy::CommandField(id) => {
            let name = machine
                .command_fields()
                .iter()
                .find(|field| field.id() == id)
                .map(OnchainField::name)
                .unwrap_or("invalid_recipient");
            format!("command.{name}")
        }
        RecipientPolicy::StateField(id) => {
            let name = machine
                .state_fields()
                .iter()
                .find(|field| field.id() == id)
                .map(OnchainField::name)
                .unwrap_or("invalid_recipient");
            format!("beforeState.{name}")
        }
    }
}

fn solidity_type(scalar: OnchainScalar) -> &'static str {
    match scalar {
        OnchainScalar::Bool => "bool",
        OnchainScalar::U8 => "uint8",
        OnchainScalar::U16 => "uint16",
        OnchainScalar::U32 => "uint32",
        OnchainScalar::U64 => "uint64",
        OnchainScalar::U128 => "uint128",
        OnchainScalar::I8 => "int8",
        OnchainScalar::I16 => "int16",
        OnchainScalar::I32 => "int32",
        OnchainScalar::I64 => "int64",
        OnchainScalar::I128 => "int128",
        OnchainScalar::Bytes32 => "bytes32",
    }
}

fn encode_to_bytes32(scalar: OnchainScalar, name: &str) -> String {
    match scalar {
        OnchainScalar::Bool => format!("{name} ? bytes32(uint256(1)) : bytes32(0)"),
        OnchainScalar::Bytes32 => name.to_owned(),
        OnchainScalar::I8
        | OnchainScalar::I16
        | OnchainScalar::I32
        | OnchainScalar::I64
        | OnchainScalar::I128 => format!("bytes32(uint256(int256({name})))"),
        OnchainScalar::U8
        | OnchainScalar::U16
        | OnchainScalar::U32
        | OnchainScalar::U64
        | OnchainScalar::U128 => format!("bytes32(uint256({name}))"),
    }
}

fn render_manifest(spec: &SolidityAdvancedSpec) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "generator={SOLIDITY_ADVANCED_GENERATOR_ID}");
    let _ = writeln!(output, "machine_hash={}", spec.machine().machine_hash());
    let _ = writeln!(output, "solc={SOLIDITY_PINNED_VERSION}");
    let _ = writeln!(
        output,
        "openzeppelin-contracts={OPENZEPPELIN_CONTRACTS_VERSION}"
    );
    output.push_str("optimizer=true\noptimizer_runs=200\nvia_ir=false\n");
    for binding in spec.fungible_bindings() {
        let _ = writeln!(
            output,
            "capability.{}.token={}",
            binding.capability_code(),
            hex20(binding.token_address())
        );
        let _ = writeln!(
            output,
            "capability.{}.runtime_code_hash={}",
            binding.capability_code(),
            hex32(binding.runtime_code_hash())
        );
    }
    output
}

fn render_agent_policy(spec: &SolidityAdvancedSpec) -> String {
    format!(
        "# Agent policy for {}\n\nMachine hash: `{}`\n\nAn agent may implement only `_commandAdmissible`, `_invariant`, `_decide`, and tests in a derived contract. Regenerate the base instead of editing it. Use generated `_event<Name>` and `_effect<Name>` builders.\n\nForbidden without a new reviewed generator profile: raw `.call`, `.delegatecall`, arbitrary calldata, assembly, unchecked arithmetic in the pure core, upgrade hooks, new token bindings, dynamic storage, and direct writes to generated storage.\n\nRequired before production authorization: exact solc compilation, compiler-known-bug review, source digest retention, unit/property/invariant tests, static analysis, formal analysis proportional to value at risk, independent review, deployment binding verification, and post-deployment code-hash verification.\n",
        spec.machine().name(),
        spec.machine().machine_hash()
    )
}

fn hex20(bytes: [u8; 20]) -> String {
    let mut output = String::from("0x");
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn hex32(bytes: [u8; 32]) -> String {
    let mut output = String::from("0x");
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn to_lower_snake(value: &str) -> String {
    let mut output = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() && index != 0 {
            output.push('_');
        }
        output.push(character.to_ascii_lowercase());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onchain::{OnchainCapability, OnchainField, OnchainReason};

    fn field(id: u16, name: &str, scalar: OnchainScalar) -> OnchainField {
        match OnchainField::try_new(id, name, scalar) {
            Ok(value) => value,
            Err(error) => panic!("field rejected: {error}"),
        }
    }

    fn machine() -> OnchainMachineSpec {
        let capability = match OnchainCapability::try_new(
            7,
            "Payout",
            OnchainCapabilityKind::FungibleTransfer,
            [8_u8; 32],
            RecipientPolicy::CommandField(1),
            1_000,
            1,
        ) {
            Ok(value) => value,
            Err(error) => panic!("capability rejected: {error}"),
        };
        let reason = match OnchainReason::try_new(1, "Unauthorized") {
            Ok(value) => value,
            Err(error) => panic!("reason rejected: {error}"),
        };
        match OnchainMachineSpec::try_new(
            "TreasuryMachine",
            1,
            vec![field(1, "balance", OnchainScalar::U128)],
            vec![
                field(1, "recipient", OnchainScalar::Bytes32),
                field(2, "amount", OnchainScalar::U128),
            ],
            vec![reason],
            Vec::new(),
            vec![capability],
            0,
            1,
            ObservationPolicy::PublicVariableShape,
        ) {
            Ok(value) => value,
            Err(error) => panic!("machine rejected: {error}"),
        }
    }

    fn binding() -> SolidityFungibleBinding {
        match SolidityFungibleBinding::try_new(7, [3_u8; 20], [4_u8; 32]) {
            Ok(value) => value,
            Err(error) => panic!("binding rejected: {error}"),
        }
    }

    #[test]
    fn generation_is_deterministic_and_policy_clean() {
        let spec = match SolidityAdvancedSpec::try_new(machine(), vec![binding()]) {
            Ok(value) => value,
            Err(error) => panic!("spec rejected: {error}"),
        };
        let left = generate_advanced_solidity(&spec);
        let right = generate_advanced_solidity(&spec);
        assert_eq!(left, right);
        let bundle = match left {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let source = bundle
            .files()
            .iter()
            .find(|file| file.path().ends_with(".sol"))
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        assert!(source.contains("internal pure virtual"));
        assert!(source.contains("SafeERC20"));
        assert!(source.contains("token.codehash"));
        assert!(source.contains("_effectPayout"));
        assert!(inspect_solidity_source(source).is_clean());
    }

    #[test]
    fn bindings_must_be_exact() {
        assert_eq!(
            SolidityAdvancedSpec::try_new(machine(), Vec::new()),
            Err(OnchainModelError::InvalidBinding)
        );
    }

    #[test]
    fn generated_files_are_canonically_ordered() {
        let spec = match SolidityAdvancedSpec::try_new(machine(), vec![binding()]) {
            Ok(value) => value,
            Err(error) => panic!("spec rejected: {error}"),
        };
        let bundle = match generate_advanced_solidity(&spec) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        assert!(
            bundle
                .files()
                .windows(2)
                .all(|pair| pair[0].path() < pair[1].path())
        );
    }
}
