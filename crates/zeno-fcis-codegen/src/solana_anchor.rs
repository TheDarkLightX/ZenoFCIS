//! Solana/Anchor generation from the shared on-chain FCIS machine model.
//!
//! The generated workspace separates a dependency-free `#![no_std]` functional
//! core crate from an Anchor imperative shell. The shell owns accounts, PDAs,
//! clocks, hashing, validation, token CPIs, events, and receipts. Agent-authored
//! project code remains confined to the pure core implementation module.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::onchain::{
    GeneratedOnchainBundle, GeneratedOnchainFile, ObservationPolicy, OnchainCapability,
    OnchainCapabilityKind, OnchainField, OnchainMachineSpec, OnchainModelError, OnchainScalar,
    RecipientPolicy,
};

/// Stable semantic identity for the Solana Anchor backend.
pub const SOLANA_ANCHOR_GENERATOR_ID: &str = "zeno-fcis-solana-anchor/3";
/// Exact Anchor release recorded by generated manifests.
pub const ANCHOR_VERSION: &str = "1.0.2";
/// Exact Solana/Agave toolchain family recorded by generated manifests.
pub const SOLANA_TOOLCHAIN_VERSION: &str = "3.1.10";
/// Exact Solana SHA-256 adapter used by generated programs.
pub const SOLANA_SHA256_HASHER_VERSION: &str = "3.1.0";
/// Hard maximum for one generated UTF-8 file.
pub const MAX_SOLANA_GENERATED_FILE_BYTES: usize = 768 * 1024;

/// Closed token-program family admitted by the initial Solana effect profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolanaTokenProgram {
    /// The original SPL Token program.
    Legacy,
}

impl SolanaTokenProgram {
    /// Returns the exact canonical program identifier.
    #[must_use]
    pub const fn program_id(self) -> [u8; 32] {
        match self {
            Self::Legacy => [
                6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28,
                180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
            ],
        }
    }

    /// Admits only the legacy SPL Token program identifier.
    ///
    /// Token-2022 requires a separate reviewed profile because transfer fees,
    /// hooks, and additional account requirements change effect semantics.
    pub const fn try_from_program_id(program_id: [u8; 32]) -> Result<Self, OnchainModelError> {
        if bytes32_equal(program_id, Self::Legacy.program_id()) {
            Ok(Self::Legacy)
        } else {
            Err(OnchainModelError::InvalidBinding)
        }
    }
}

const fn bytes32_equal(left: [u8; 32], right: [u8; 32]) -> bool {
    let mut index = 0_usize;
    while index < 32 {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

/// Exact Solana binding for one shared fungible-transfer capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolanaFungibleBinding {
    capability_code: u16,
    mint: [u8; 32],
    token_program: SolanaTokenProgram,
    vault: [u8; 32],
}

impl SolanaFungibleBinding {
    /// Constructs one exact mint, token-program, and vault binding.
    pub fn try_new(
        capability_code: u16,
        mint: [u8; 32],
        token_program: SolanaTokenProgram,
        vault: [u8; 32],
    ) -> Result<Self, OnchainModelError> {
        if capability_code == 0 || mint == [0_u8; 32] || vault == [0_u8; 32] {
            return Err(OnchainModelError::InvalidBinding);
        }
        Ok(Self {
            capability_code,
            mint,
            token_program,
            vault,
        })
    }

    /// Returns the shared capability code.
    #[must_use]
    pub const fn capability_code(&self) -> u16 {
        self.capability_code
    }

    /// Returns the exact mint public key bytes.
    #[must_use]
    pub const fn mint(&self) -> [u8; 32] {
        self.mint
    }

    /// Returns the closed token-program profile.
    #[must_use]
    pub const fn token_program(&self) -> SolanaTokenProgram {
        self.token_program
    }

    /// Returns the exact source vault token-account public key bytes.
    #[must_use]
    pub const fn vault(&self) -> [u8; 32] {
        self.vault
    }
}

/// Closed input for one generated Anchor workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolanaAnchorSpec {
    machine: OnchainMachineSpec,
    program_id: [u8; 32],
    state_seed: String,
    fungible_bindings: Vec<SolanaFungibleBinding>,
}

impl SolanaAnchorSpec {
    /// Validates exact one-to-one capability bindings and a bounded PDA seed.
    pub fn try_new(
        machine: OnchainMachineSpec,
        program_id: [u8; 32],
        state_seed: impl Into<String>,
        mut fungible_bindings: Vec<SolanaFungibleBinding>,
    ) -> Result<Self, OnchainModelError> {
        if program_id == [0_u8; 32] {
            return Err(OnchainModelError::InvalidBinding);
        }
        let state_seed = state_seed.into();
        if !valid_seed(&state_seed) {
            return Err(OnchainModelError::InvalidIdentifier);
        }

        fungible_bindings.sort_by_key(SolanaFungibleBinding::capability_code);
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
            .map(SolanaFungibleBinding::capability_code)
            .collect();
        if expected != actual
            || machine
                .capabilities()
                .iter()
                .any(|capability| capability.max_amount() > u128::from(u64::MAX))
        {
            return Err(OnchainModelError::InvalidBinding);
        }

        let mut vaults = BTreeSet::new();
        for binding in &fungible_bindings {
            if !vaults.insert(binding.vault()) {
                return Err(OnchainModelError::InvalidBinding);
            }
        }

        Ok(Self {
            machine,
            program_id,
            state_seed,
            fungible_bindings,
        })
    }

    /// Returns the shared semantic machine.
    #[must_use]
    pub const fn machine(&self) -> &OnchainMachineSpec {
        &self.machine
    }

    /// Returns the exact program ID bytes.
    #[must_use]
    pub const fn program_id(&self) -> [u8; 32] {
        self.program_id
    }

    /// Returns the state-PDA seed.
    #[must_use]
    pub fn state_seed(&self) -> &str {
        &self.state_seed
    }

    /// Returns fungible bindings in capability-code order.
    #[must_use]
    pub fn fungible_bindings(&self) -> &[SolanaFungibleBinding] {
        &self.fungible_bindings
    }
}

/// Forbidden-source category found in an agent-authored Solana core.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SolanaCoreSafetyFindingKind {
    /// Anchor framework access inside the pure crate.
    AnchorDependency,
    /// Solana runtime or account access inside the pure crate.
    SolanaRuntime,
    /// Cross-program invocation authority inside the pure crate.
    CrossProgramInvocation,
    /// Dynamic remaining-account authority.
    RemainingAccounts,
    /// Unsafe Rust.
    UnsafeCode,
    /// Panic or incomplete implementation path.
    PanicPath,
    /// Nondeterministic or unavailable host facility.
    HostFacility,
    /// Floating-point operation or type.
    FloatingPoint,
}

/// Forbidden-source category found in an Anchor shell or extension.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SolanaShellSafetyFindingKind {
    /// Raw unchecked account authority.
    UncheckedAccount,
    /// Dynamic remaining-account authority.
    RemainingAccounts,
    /// Raw invoke or invoke-signed call outside typed adapters.
    RawInvocation,
    /// Unsafe Rust.
    UnsafeCode,
    /// Dynamic account resizing or closure.
    DynamicAccountLifecycle,
    /// Upgrade-loader or authority mutation surface.
    UpgradeAuthority,
}

/// One source-level safety finding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolanaSafetyFinding<K> {
    kind: K,
    byte_offset: usize,
}

impl<K: Copy> SolanaSafetyFinding<K> {
    /// Returns the finding kind.
    #[must_use]
    pub const fn kind(&self) -> K {
        self.kind
    }

    /// Returns the byte offset in the original source.
    #[must_use]
    pub const fn byte_offset(&self) -> usize {
        self.byte_offset
    }
}

/// Conservative source-policy report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolanaSafetyReport<K> {
    findings: Vec<SolanaSafetyFinding<K>>,
}

impl<K> SolanaSafetyReport<K> {
    /// Returns true when no forbidden mechanism was found.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    /// Returns findings in byte-offset order.
    #[must_use]
    pub fn findings(&self) -> &[SolanaSafetyFinding<K>] {
        &self.findings
    }
}

/// Scans a pure-core source file for runtime, CPI, host, panic, and float access.
#[must_use]
pub fn inspect_solana_core_source(source: &str) -> SolanaSafetyReport<SolanaCoreSafetyFindingKind> {
    const FORBIDDEN: &[(&str, SolanaCoreSafetyFindingKind)] = &[
        ("anchor_lang", SolanaCoreSafetyFindingKind::AnchorDependency),
        ("anchor_spl", SolanaCoreSafetyFindingKind::AnchorDependency),
        ("solana_program", SolanaCoreSafetyFindingKind::SolanaRuntime),
        ("accountinfo", SolanaCoreSafetyFindingKind::SolanaRuntime),
        ("context<", SolanaCoreSafetyFindingKind::SolanaRuntime),
        ("clock::", SolanaCoreSafetyFindingKind::SolanaRuntime),
        (
            "invoke(",
            SolanaCoreSafetyFindingKind::CrossProgramInvocation,
        ),
        (
            "invoke_signed",
            SolanaCoreSafetyFindingKind::CrossProgramInvocation,
        ),
        (
            "cpicontext",
            SolanaCoreSafetyFindingKind::CrossProgramInvocation,
        ),
        (
            "remaining_accounts",
            SolanaCoreSafetyFindingKind::RemainingAccounts,
        ),
        ("unsafe {", SolanaCoreSafetyFindingKind::UnsafeCode),
        ("unwrap(", SolanaCoreSafetyFindingKind::PanicPath),
        ("expect(", SolanaCoreSafetyFindingKind::PanicPath),
        ("panic!", SolanaCoreSafetyFindingKind::PanicPath),
        ("todo!", SolanaCoreSafetyFindingKind::PanicPath),
        ("unimplemented!", SolanaCoreSafetyFindingKind::PanicPath),
        ("std::fs", SolanaCoreSafetyFindingKind::HostFacility),
        ("std::net", SolanaCoreSafetyFindingKind::HostFacility),
        ("std::thread", SolanaCoreSafetyFindingKind::HostFacility),
        ("std::time", SolanaCoreSafetyFindingKind::HostFacility),
        ("rand", SolanaCoreSafetyFindingKind::HostFacility),
        ("f32", SolanaCoreSafetyFindingKind::FloatingPoint),
        ("f64", SolanaCoreSafetyFindingKind::FloatingPoint),
    ];
    inspect_source(source, FORBIDDEN)
}

/// Scans a shell extension for raw account, CPI, lifecycle, and upgrade authority.
#[must_use]
pub fn inspect_solana_shell_source(
    source: &str,
) -> SolanaSafetyReport<SolanaShellSafetyFindingKind> {
    const FORBIDDEN: &[(&str, SolanaShellSafetyFindingKind)] = &[
        (
            "uncheckedaccount",
            SolanaShellSafetyFindingKind::UncheckedAccount,
        ),
        (
            "remaining_accounts",
            SolanaShellSafetyFindingKind::RemainingAccounts,
        ),
        ("invoke(", SolanaShellSafetyFindingKind::RawInvocation),
        ("invoke_signed", SolanaShellSafetyFindingKind::RawInvocation),
        ("unsafe {", SolanaShellSafetyFindingKind::UnsafeCode),
        (
            "realloc",
            SolanaShellSafetyFindingKind::DynamicAccountLifecycle,
        ),
        (
            "init_if_needed",
            SolanaShellSafetyFindingKind::DynamicAccountLifecycle,
        ),
        (
            "close =",
            SolanaShellSafetyFindingKind::DynamicAccountLifecycle,
        ),
        (
            "loader_upgradeable",
            SolanaShellSafetyFindingKind::UpgradeAuthority,
        ),
        (
            "set_upgrade_authority",
            SolanaShellSafetyFindingKind::UpgradeAuthority,
        ),
    ];
    inspect_source(source, FORBIDDEN)
}

/// Generates one modular Anchor workspace and fail-closed agent policy.
pub fn generate_solana_anchor(
    spec: &SolanaAnchorSpec,
) -> Result<GeneratedOnchainBundle, OnchainModelError> {
    let base = to_lower_snake(spec.machine().name());
    let core_crate = format!("{base}_core");
    let files = vec![
        GeneratedOnchainFile::new("Anchor.toml".to_owned(), render_anchor_toml(spec, &base)),
        GeneratedOnchainFile::new("Cargo.toml".to_owned(), render_workspace_cargo(&base)),
        GeneratedOnchainFile::new(
            format!("crates/{base}-core/Cargo.toml"),
            render_core_cargo(&base),
        ),
        GeneratedOnchainFile::new(
            format!("crates/{base}-core/src/lib.rs"),
            render_core_lib(spec)?,
        ),
        GeneratedOnchainFile::new(
            format!("crates/{base}-core/src/project.rs"),
            render_inert_project(spec),
        ),
        GeneratedOnchainFile::new(
            format!("programs/{base}/Cargo.toml"),
            render_program_cargo(&base),
        ),
        GeneratedOnchainFile::new(
            format!("programs/{base}/src/lib.rs"),
            render_program(spec, &base, &core_crate)?,
        ),
        GeneratedOnchainFile::new("MANIFEST.zfcis".to_owned(), render_manifest(spec)),
        GeneratedOnchainFile::new("AGENT_POLICY.md".to_owned(), render_agent_policy(spec)),
    ];
    if files
        .iter()
        .any(|file| file.content().len() > MAX_SOLANA_GENERATED_FILE_BYTES)
    {
        return Err(OnchainModelError::LimitExceeded(
            crate::onchain::OnchainListKind::PlanSlots,
        ));
    }

    let core_source = files
        .iter()
        .find(|file| file.path().ends_with("core/src/project.rs"))
        .map(GeneratedOnchainFile::content)
        .ok_or(OnchainModelError::InvalidBinding)?;
    if !inspect_solana_core_source(core_source).is_clean() {
        return Err(OnchainModelError::InvalidBinding);
    }
    let shell_source = files
        .iter()
        .find(|file| file.path().contains("programs/") && file.path().ends_with("src/lib.rs"))
        .map(GeneratedOnchainFile::content)
        .ok_or(OnchainModelError::InvalidBinding)?;
    if !inspect_solana_shell_source(shell_source).is_clean() {
        return Err(OnchainModelError::InvalidBinding);
    }

    GeneratedOnchainBundle::try_new(
        SOLANA_ANCHOR_GENERATOR_ID,
        spec.machine().machine_hash(),
        files,
    )
}

fn render_core_lib(spec: &SolanaAnchorSpec) -> Result<String, OnchainModelError> {
    let machine = spec.machine();
    let event_capacity = usize::from(machine.max_event_slots().max(1));
    let effect_capacity = usize::from(machine.max_effect_slots().max(1));
    let mut output = String::new();
    output.push_str(
        "#![no_std]\n#![forbid(unsafe_code)]\n\nmod project;\npub use project::ProjectCore;\n\n",
    );
    writeln!(
        output,
        "pub const MACHINE_HASH: [u8; 32] = {};",
        rust_bytes(machine.machine_hash().into_bytes())
    )?;
    writeln!(
        output,
        "pub const MACHINE_VERSION: u16 = {};",
        machine.version()
    )?;
    for reason in machine.reasons() {
        writeln!(
            output,
            "pub const REASON_{}: u16 = {};",
            to_upper_snake(reason.name()),
            reason.code()
        )?;
    }
    for event in machine.events() {
        writeln!(
            output,
            "pub const EVENT_{}: u16 = {};",
            to_upper_snake(event.name()),
            event.code()
        )?;
    }
    for capability in machine.capabilities() {
        writeln!(
            output,
            "pub const CAPABILITY_{}: u16 = {};",
            to_upper_snake(capability.name()),
            capability.code()
        )?;
    }
    output.push('\n');

    output.push_str("#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub enum DecisionKind {\n    #[default]\n    Accept,\n    Reject,\n}\n\n");
    render_rust_struct(&mut output, "State", machine.state_fields())?;
    render_rust_struct(&mut output, "Command", machine.command_fields())?;
    output.push_str("#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub struct Context {\n    pub actor: [u8; 32],\n    pub chain_domain: [u8; 32],\n    pub sequence: u64,\n    pub slot: u64,\n    pub unix_timestamp: i64,\n}\n\n");
    output.push_str("#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub struct EventPlan {\n    pub code: u16,\n    pub field_count: u8,\n    pub data: [[u8; 32]; 8],\n}\n\n");
    output.push_str("#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub struct EffectPlan {\n    pub capability: u16,\n    pub asset_id: [u8; 32],\n    pub recipient: [u8; 32],\n    pub amount: u128,\n}\n\n");
    writeln!(
        output,
        "#[derive(Clone, Copy, Debug, Eq, PartialEq)]\npub struct Decision {{\n    pub kind: DecisionKind,\n    pub reason_code: u16,\n    pub next_state: State,\n    pub event_count: u8,\n    pub events: [EventPlan; {event_capacity}],\n    pub effect_count: u8,\n    pub effects: [EffectPlan; {effect_capacity}],\n}}\n"
    )?;
    writeln!(
        output,
        "impl Default for Decision {{\n    fn default() -> Self {{\n        Self {{ kind: DecisionKind::Accept, reason_code: 0, next_state: State::default(), event_count: 0, events: [EventPlan::default(); {event_capacity}], effect_count: 0, effects: [EffectPlan::default(); {effect_capacity}] }}\n    }}\n}}\n"
    )?;
    output.push_str("impl Decision {\n    pub fn rejected(reason_code: u16) -> Self { Self { kind: DecisionKind::Reject, reason_code, ..Self::default() } }\n    pub fn accepted(next_state: State) -> Self { Self { next_state, ..Self::default() } }\n}\n\n");
    output.push_str("pub trait Core {\n    fn command_admissible(command: &Command, context: &Context) -> bool;\n    fn invariant(state: &State) -> bool;\n    fn decide(state: &State, command: &Command, context: &Context) -> Decision;\n}\n\n");
    render_core_builders(&mut output, machine)?;
    Ok(output)
}

fn render_inert_project(spec: &SolanaAnchorSpec) -> String {
    let first_reason = spec.machine().reasons()[0].code();
    format!(
        "use crate::{{Command, Context, Core, Decision, State}};\n\n/// Fail-closed starter. Replace only this implementation and keep it pure.\npub struct ProjectCore;\n\nimpl Core for ProjectCore {{\n    fn command_admissible(_command: &Command, _context: &Context) -> bool {{ false }}\n    fn invariant(_state: &State) -> bool {{ false }}\n    fn decide(_state: &State, _command: &Command, _context: &Context) -> Decision {{ Decision::rejected({first_reason}) }}\n}}\n"
    )
}

fn render_program(
    spec: &SolanaAnchorSpec,
    base: &str,
    core_crate: &str,
) -> Result<String, OnchainModelError> {
    let machine = spec.machine();
    let bindings: BTreeMap<u16, SolanaFungibleBinding> = spec
        .fungible_bindings()
        .iter()
        .map(|binding| (binding.capability_code(), *binding))
        .collect();
    let state_space = 8_usize
        + 2
        + 1
        + 32
        + 8
        + 32
        + machine
            .state_fields()
            .iter()
            .map(|field| usize::from(field.scalar().byte_width()))
            .sum::<usize>();
    let mut output = String::new();
    output.push_str("#![forbid(unsafe_code)]\n\nuse anchor_lang::prelude::*;\nuse anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};\nuse solana_sha256_hasher::{hash, hashv};\n");
    writeln!(
        output,
        "use {core_crate}::{{Command, Context as CoreContext, Core, Decision, DecisionKind, EffectPlan, EventPlan, ProjectCore, State, MACHINE_HASH, MACHINE_VERSION}};"
    )?;
    writeln!(
        output,
        "\ndeclare_id!(\"{}\");\n",
        base58_encode(spec.program_id())
    )?;
    writeln!(
        output,
        "const STATE_SEED: &[u8] = b\"{}\";",
        spec.state_seed()
    )?;
    writeln!(
        output,
        "const MAX_EVENT_SLOTS: u8 = {};",
        machine.max_event_slots()
    )?;
    writeln!(
        output,
        "const MAX_EFFECT_SLOTS: u8 = {};\n",
        machine.max_effect_slots()
    )?;

    writeln!(output, "#[program]\npub mod {base} {{\n    use super::*;\n")?;
    output.push_str("    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {\n        let initial_state = args.into_core();\n        require!(ProjectCore::invariant(&initial_state), FcisError::InvariantViolation);\n        let state = &mut ctx.accounts.state;\n        state.version = MACHINE_VERSION;\n        state.bump = ctx.bumps.state;\n        state.authority = ctx.accounts.authority.key();\n        state.sequence = 0;\n        state.apply_core(initial_state);\n        state.state_hash = hash_state(&initial_state);\n        emit!(Initialized { state: state.key(), state_hash: state.state_hash, authority: state.authority });\n        Ok(())\n    }\n\n");
    output.push_str("    pub fn execute(ctx: Context<Execute>, args: ExecuteArgs) -> Result<()> {\n        let before_state = ctx.accounts.state.to_core();\n        let actual_state_hash = hash_state(&before_state);\n        require!(actual_state_hash == ctx.accounts.state.state_hash, FcisError::StateRootCorrupted);\n        require!(actual_state_hash == args.expected_state_hash && ctx.accounts.state.sequence == args.expected_sequence, FcisError::StaleState);\n        let command = args.command();\n        let clock = Clock::get()?;\n        let context = CoreContext { actor: ctx.accounts.actor.key().to_bytes(), chain_domain: hash_chain_domain(&ctx.accounts.state.key()), sequence: ctx.accounts.state.sequence, slot: clock.slot, unix_timestamp: clock.unix_timestamp };\n        require!(ProjectCore::command_admissible(&command, &context), FcisError::CommandNotAdmissible);\n        let decision = ProjectCore::decide(&before_state, &command, &context);\n        if decision.kind == DecisionKind::Reject { return Err(rejection_error(decision.reason_code)); }\n        require!(decision.reason_code == 0, FcisError::InvalidDecision);\n        require!(ProjectCore::invariant(&decision.next_state), FcisError::InvariantViolation);\n        validate_plans(&decision, &before_state, &command, &context)?;\n        let post_state_hash = hash_state(&decision.next_state);\n        let command_hash = hash_command(&command);\n        let context_hash = hash_context(&context);\n        let event_plan_hash = hash_event_plan(&decision);\n        let effect_plan_hash = hash_effect_plan(&decision);\n        let candidate_hash = hashv_owned(&[&MACHINE_HASH, &actual_state_hash, &post_state_hash, &command_hash, &context_hash, &event_plan_hash, &effect_plan_hash]);\n        {\n            let state = &mut ctx.accounts.state;\n            state.apply_core(decision.next_state);\n            state.state_hash = post_state_hash;\n            state.sequence = state.sequence.checked_add(1).ok_or(FcisError::SequenceOverflow)?;\n        }\n        apply_effects(&ctx, &decision)?;\n        for index in 0..usize::from(decision.event_count) { let planned = decision.events[index]; emit!(DomainEvent { code: planned.code, field_count: planned.field_count, data: planned.data, payload_hash: hash_event(&planned) }); }\n        emit!(TransitionCommitted { state: ctx.accounts.state.key(), sequence: ctx.accounts.state.sequence, pre_state_hash: actual_state_hash, post_state_hash, candidate_hash, command_hash, context_hash });\n        Ok(())\n    }\n}\n\n");

    output.push_str(
        "#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]\npub struct InitializeArgs {\n",
    );
    for field in machine.state_fields() {
        writeln!(
            output,
            "    pub {}: {},",
            field.name(),
            rust_type(field.scalar())
        )?;
    }
    output.push_str("}\n\nimpl InitializeArgs { fn into_core(self) -> State { State {\n");
    for field in machine.state_fields() {
        writeln!(output, "    {}: self.{},", field.name(), field.name())?;
    }
    output.push_str("} } }\n\n");

    output.push_str("#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]\npub struct ExecuteArgs {\n    pub expected_state_hash: [u8; 32],\n    pub expected_sequence: u64,\n");
    for field in machine.command_fields() {
        writeln!(
            output,
            "    pub {}: {},",
            field.name(),
            rust_type(field.scalar())
        )?;
    }
    output.push_str("}\n\nimpl ExecuteArgs { fn command(&self) -> Command { Command {\n");
    for field in machine.command_fields() {
        writeln!(output, "    {}: self.{},", field.name(), field.name())?;
    }
    output.push_str("} } }\n\n");

    output.push_str("#[derive(Accounts)]\npub struct Initialize<'info> {\n    #[account(init, payer = authority, space = MachineState::SPACE, seeds = [STATE_SEED, authority.key().as_ref()], bump)]\n    pub state: Account<'info, MachineState>,\n    #[account(mut, signer)]\n    pub authority: SystemAccount<'info>,\n    pub system_program: Program<'info, System>,\n}\n\n");
    output.push_str("#[derive(Accounts)]\npub struct Execute<'info> {\n    #[account(mut, seeds = [STATE_SEED, authority.key().as_ref()], bump = state.bump, has_one = authority)]\n    pub state: Account<'info, MachineState>,\n    #[account(address = state.authority)]\n    pub authority: SystemAccount<'info>,\n    pub actor: Signer<'info>,\n");
    for capability in machine.capabilities() {
        let name = to_lower_snake(capability.name());
        let binding = bindings
            .get(&capability.code())
            .ok_or(OnchainModelError::InvalidBinding)?;
        writeln!(
            output,
            "    #[account(address = Pubkey::new_from_array({}))]\n    pub {name}_mint: InterfaceAccount<'info, Mint>,",
            rust_bytes(binding.mint())
        )?;
        writeln!(
            output,
            "    #[account(mut, address = Pubkey::new_from_array({}), token::mint = {name}_mint, token::authority = state, token::token_program = {name}_token_program)]\n    pub {name}_vault: InterfaceAccount<'info, TokenAccount>,",
            rust_bytes(binding.vault())
        )?;
        writeln!(
            output,
            "    #[account(mut, token::mint = {name}_mint, token::token_program = {name}_token_program)]\n    pub {name}_destination: InterfaceAccount<'info, TokenAccount>,"
        )?;
        writeln!(
            output,
            "    #[account(address = Pubkey::new_from_array({}))]\n    pub {name}_token_program: Interface<'info, TokenInterface>,",
            rust_bytes(binding.token_program().program_id())
        )?;
    }
    output.push_str("}\n\n");

    writeln!(
        output,
        "#[account]\npub struct MachineState {{\n    pub version: u16,\n    pub bump: u8,\n    pub authority: Pubkey,\n    pub sequence: u64,\n    pub state_hash: [u8; 32],"
    )?;
    for field in machine.state_fields() {
        writeln!(
            output,
            "    pub {}: {},",
            field.name(),
            rust_type(field.scalar())
        )?;
    }
    writeln!(
        output,
        "}}\n\nimpl MachineState {{\n    pub const SPACE: usize = {state_space};\n    fn to_core(&self) -> State {{ State {{"
    )?;
    for field in machine.state_fields() {
        writeln!(output, "        {}: self.{},", field.name(), field.name())?;
    }
    output.push_str("    } }\n    fn apply_core(&mut self, value: State) {\n");
    for field in machine.state_fields() {
        writeln!(
            output,
            "        self.{} = value.{};",
            field.name(),
            field.name()
        )?;
    }
    output.push_str("    }\n}\n\n");

    output.push_str("#[event]\npub struct Initialized { pub state: Pubkey, pub state_hash: [u8; 32], pub authority: Pubkey }\n#[event]\npub struct DomainEvent { pub code: u16, pub field_count: u8, pub data: [[u8; 32]; 8], pub payload_hash: [u8; 32] }\n#[event]\npub struct TransitionCommitted { pub state: Pubkey, pub sequence: u64, pub pre_state_hash: [u8; 32], pub post_state_hash: [u8; 32], pub candidate_hash: [u8; 32], pub command_hash: [u8; 32], pub context_hash: [u8; 32] }\n\n");
    render_error_enum(&mut output, machine)?;
    render_program_helpers(&mut output, spec, &bindings)?;
    Ok(output)
}

fn render_program_helpers(
    output: &mut String,
    spec: &SolanaAnchorSpec,
    bindings: &BTreeMap<u16, SolanaFungibleBinding>,
) -> Result<(), OnchainModelError> {
    let machine = spec.machine();
    output.push_str("fn validate_plans(decision: &Decision, before_state: &State, command: &Command, context: &CoreContext) -> Result<()> {\n    require!(decision.event_count <= MAX_EVENT_SLOTS && decision.effect_count <= MAX_EFFECT_SLOTS, FcisError::InvalidPlan);\n");
    if machine.observation_policy() == ObservationPolicy::FixedShape {
        output.push_str("    require!(decision.event_count == MAX_EVENT_SLOTS && decision.effect_count == MAX_EFFECT_SLOTS, FcisError::InvalidPlan);\n");
    }
    output.push_str("    let mut prior = [0_u8; 32];\n    for index in 0..usize::from(decision.event_count) { let planned = decision.events[index]; require!(event_field_count(planned.code)? == planned.field_count, FcisError::InvalidPlan); let digest = hash_event(&planned); if index != 0 { require!(digest >= prior, FcisError::InvalidPlan); } prior = digest; }\n    for index in usize::from(decision.event_count)..decision.events.len() { require!(decision.events[index] == EventPlan::default(), FcisError::InvalidPlan); }\n    prior = [0_u8; 32];\n    for index in 0..usize::from(decision.effect_count) { let planned = decision.effects[index]; validate_effect(&planned, before_state, command, context)?; let digest = hash_effect(&planned); if index != 0 { require!(digest >= prior, FcisError::InvalidPlan); } prior = digest; let mut uses = 0_u8; for inner in 0..=index { if decision.effects[inner].capability == planned.capability { uses = uses.checked_add(1).ok_or(FcisError::InvalidPlan)?; } } require!(uses <= capability_max_uses(planned.capability)?, FcisError::InvalidPlan); }\n    for index in usize::from(decision.effect_count)..decision.effects.len() { require!(decision.effects[index] == EffectPlan::default(), FcisError::InvalidPlan); }\n    Ok(())\n}\n\n");
    output.push_str("fn validate_effect(planned: &EffectPlan, before_state: &State, command: &Command, context: &CoreContext) -> Result<()> { require!(planned.capability != 0 && planned.amount != 0, FcisError::InvalidCapability); require!(planned.asset_id == capability_asset(planned.capability)? && planned.recipient == expected_recipient(planned.capability, before_state, command, context) && planned.amount <= capability_max_amount(planned.capability)?, FcisError::InvalidCapability); Ok(()) }\n\n");

    output.push_str("fn apply_effects(ctx: &Context<Execute>, decision: &Decision) -> Result<()> {\n    for index in 0..usize::from(decision.effect_count) { let planned = decision.effects[index]; match planned.capability {\n");
    for capability in machine.capabilities() {
        let name = to_lower_snake(capability.name());
        writeln!(output, "        {} => {{", capability.code())?;
        output.push_str("            require!(ctx.accounts.");
        output.push_str(&name);
        output.push_str(
            "_destination.owner.to_bytes() == planned.recipient, FcisError::InvalidRecipient);\n",
        );
        output.push_str("            let amount = u64::try_from(planned.amount).map_err(|_| error!(FcisError::InvalidPlan))?;\n");
        output.push_str("            let authority_key = ctx.accounts.authority.key();\n            let bump = [ctx.accounts.state.bump];\n            let signer_seeds: &[&[u8]] = &[STATE_SEED, authority_key.as_ref(), &bump];\n            let signer = &[signer_seeds];\n");
        writeln!(
            output,
            "            let cpi_accounts = TransferChecked {{ from: ctx.accounts.{name}_vault.to_account_info(), mint: ctx.accounts.{name}_mint.to_account_info(), to: ctx.accounts.{name}_destination.to_account_info(), authority: ctx.accounts.state.to_account_info() }};"
        )?;
        writeln!(
            output,
            "            token_interface::transfer_checked(CpiContext::new_with_signer(ctx.accounts.{name}_token_program.key(), cpi_accounts, signer), amount, ctx.accounts.{name}_mint.decimals)?;"
        )?;
        output.push_str("        }\n");
    }
    output.push_str(
        "        _ => return err!(FcisError::InvalidCapability),\n    } }\n    Ok(())\n}\n\n",
    );

    output.push_str("fn event_field_count(code: u16) -> Result<u8> { match code {\n");
    for event in machine.events() {
        writeln!(
            output,
            "    {} => Ok({}),",
            event.code(),
            event.fields().len()
        )?;
    }
    output.push_str("    _ => err!(FcisError::InvalidPlan),\n} }\n\n");
    output.push_str("fn capability_asset(code: u16) -> Result<[u8; 32]> { match code {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "    {} => Ok({}),",
            capability.code(),
            rust_bytes(capability.asset_id())
        )?;
    }
    output.push_str("    _ => err!(FcisError::InvalidCapability),\n} }\n");
    output.push_str("fn capability_max_amount(code: u16) -> Result<u128> { match code {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "    {} => Ok({}),",
            capability.code(),
            capability.max_amount()
        )?;
    }
    output.push_str("    _ => err!(FcisError::InvalidCapability),\n} }\n");
    output.push_str("fn capability_max_uses(code: u16) -> Result<u8> { match code {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "    {} => Ok({}),",
            capability.code(),
            capability.max_uses()
        )?;
    }
    output.push_str("    _ => err!(FcisError::InvalidCapability),\n} }\n");
    output.push_str("fn expected_recipient(code: u16, before_state: &State, command: &Command, context: &CoreContext) -> [u8; 32] { let _ = (before_state, command, context); match code {\n");
    for capability in machine.capabilities() {
        writeln!(
            output,
            "    {} => {},",
            capability.code(),
            recipient_expression(capability.recipient(), machine)
        )?;
    }
    output.push_str("    _ => [0_u8; 32],\n} }\n\n");

    output.push_str("fn rejection_error(code: u16) -> anchor_lang::error::Error { match code {\n");
    for reason in machine.reasons() {
        writeln!(
            output,
            "    {} => FcisError::Reject{}.into(),",
            reason.code(),
            reason.name()
        )?;
    }
    output.push_str("    _ => FcisError::InvalidDecision.into(),\n} }\n\n");

    render_hash_helpers(output, machine)?;
    let _ = bindings;
    Ok(())
}

fn render_hash_helpers(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    output.push_str("fn hash_state(value: &State) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b\"zeno-fcis/solana/state/v1\\0\"); bytes.extend_from_slice(&MACHINE_HASH);\n");
    for field in machine.state_fields() {
        writeln!(
            output,
            "    bytes.extend_from_slice(&{}u16.to_be_bytes()); {}",
            field.id(),
            append_expression("value", field)
        )?;
    }
    output.push_str("    hash(&bytes).to_bytes() }\n");
    output.push_str("fn hash_command(value: &Command) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b\"zeno-fcis/solana/command/v1\\0\"); bytes.extend_from_slice(&MACHINE_HASH);\n");
    for field in machine.command_fields() {
        writeln!(
            output,
            "    bytes.extend_from_slice(&{}u16.to_be_bytes()); {}",
            field.id(),
            append_expression("value", field)
        )?;
    }
    output.push_str("    hash(&bytes).to_bytes() }\n");
    output.push_str("fn hash_context(value: &CoreContext) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b\"zeno-fcis/solana/context/v1\\0\"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.actor); bytes.extend_from_slice(&value.chain_domain); bytes.extend_from_slice(&value.sequence.to_be_bytes()); bytes.extend_from_slice(&value.slot.to_be_bytes()); bytes.extend_from_slice(&value.unix_timestamp.to_be_bytes()); hash(&bytes).to_bytes() }\n");
    output.push_str("fn hash_chain_domain(state: &Pubkey) -> [u8; 32] { hashv_owned(&[b\"zeno-fcis/solana/domain/v1\\0\", &crate::ID.to_bytes(), &state.to_bytes(), &MACHINE_HASH]) }\n");
    output.push_str("fn hash_event(value: &EventPlan) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b\"zeno-fcis/solana/event/v1\\0\"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.code.to_be_bytes()); bytes.push(value.field_count); for item in value.data { bytes.extend_from_slice(&item); } hash(&bytes).to_bytes() }\n");
    output.push_str("fn hash_effect(value: &EffectPlan) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b\"zeno-fcis/solana/effect/v1\\0\"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.capability.to_be_bytes()); bytes.extend_from_slice(&value.asset_id); bytes.extend_from_slice(&value.recipient); bytes.extend_from_slice(&value.amount.to_be_bytes()); hash(&bytes).to_bytes() }\n");
    output.push_str("fn hash_event_plan(value: &Decision) -> [u8; 32] { let mut result = hashv_owned(&[b\"zeno-fcis/solana/event-plan/v1\\0\", &MACHINE_HASH, &[value.event_count]]); for index in 0..usize::from(value.event_count) { result = hashv_owned(&[&result, &hash_event(&value.events[index])]); } result }\n");
    output.push_str("fn hash_effect_plan(value: &Decision) -> [u8; 32] { let mut result = hashv_owned(&[b\"zeno-fcis/solana/effect-plan/v1\\0\", &MACHINE_HASH, &[value.effect_count]]); for index in 0..usize::from(value.effect_count) { result = hashv_owned(&[&result, &hash_effect(&value.effects[index])]); } result }\n");
    output
        .push_str("fn hashv_owned(values: &[&[u8]]) -> [u8; 32] { hashv(values).to_bytes() }\n\n");
    Ok(())
}

fn render_core_builders(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    for event in machine.events() {
        write!(output, "pub fn event_{}(", to_lower_snake(event.name()))?;
        for (index, field) in event.fields().iter().enumerate() {
            if index != 0 {
                output.push_str(", ");
            }
            write!(output, "{}: {}", field.name(), rust_type(field.scalar()))?;
        }
        output.push_str(") -> EventPlan {\n    let mut planned = EventPlan { code: ");
        writeln!(
            output,
            "{}, field_count: {}, ..EventPlan::default() }};",
            event.code(),
            event.fields().len()
        )?;
        for (index, field) in event.fields().iter().enumerate() {
            writeln!(
                output,
                "    planned.data[{index}] = {};",
                encode_bytes32(field.scalar(), field.name())
            )?;
        }
        output.push_str("    planned\n}\n\n");
    }
    for capability in machine.capabilities() {
        writeln!(
            output,
            "pub fn effect_{}(amount: u128, before_state: &State, command: &Command, context: &Context) -> EffectPlan {{\n    let _ = (before_state, command, context);\n    EffectPlan {{ capability: {}, asset_id: {}, recipient: {}, amount }}\n}}\n",
            to_lower_snake(capability.name()),
            capability.code(),
            rust_bytes(capability.asset_id()),
            recipient_expression(capability.recipient(), machine)
        )?;
    }
    Ok(())
}

fn render_error_enum(
    output: &mut String,
    machine: &OnchainMachineSpec,
) -> Result<(), OnchainModelError> {
    output.push_str("#[error_code]\npub enum FcisError {\n    #[msg(\"State invariant rejected\")] InvariantViolation,\n    #[msg(\"Command admission rejected\")] CommandNotAdmissible,\n    #[msg(\"State root differs from committed root\")] StateRootCorrupted,\n    #[msg(\"Expected state root or sequence is stale\")] StaleState,\n    #[msg(\"Decision encoding is inconsistent\")] InvalidDecision,\n    #[msg(\"Plan exceeds its closed authority\")] InvalidPlan,\n    #[msg(\"Unknown or malformed capability\")] InvalidCapability,\n    #[msg(\"Destination token owner does not match the planned recipient\")] InvalidRecipient,\n    #[msg(\"State sequence overflow\")] SequenceOverflow,\n");
    for reason in machine.reasons() {
        writeln!(
            output,
            "    #[msg(\"Transition rejected: {}\")] Reject{},",
            reason.name(),
            reason.name()
        )?;
    }
    output.push_str("}\n\n");
    Ok(())
}

fn render_rust_struct(
    output: &mut String,
    name: &str,
    fields: &[OnchainField],
) -> Result<(), OnchainModelError> {
    writeln!(
        output,
        "#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]\npub struct {name} {{"
    )?;
    for field in fields {
        writeln!(
            output,
            "    pub {}: {},",
            field.name(),
            rust_type(field.scalar())
        )?;
    }
    output.push_str("}\n\n");
    Ok(())
}

fn render_anchor_toml(spec: &SolanaAnchorSpec, base: &str) -> String {
    format!(
        "[toolchain]\nanchor_version = \"{ANCHOR_VERSION}\"\n\n[features]\nresolution = true\nskip-lint = false\n\n[programs.localnet]\n{base} = \"{}\"\n\n[provider]\ncluster = \"localnet\"\nwallet = \"~/.config/solana/id.json\"\n",
        base58_encode(spec.program_id())
    )
}

fn render_workspace_cargo(base: &str) -> String {
    format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/{base}-core\", \"programs/{base}\"]\n\n[profile.release]\noverflow-checks = true\nlto = \"fat\"\ncodegen-units = 1\n"
    )
}

fn render_core_cargo(base: &str) -> String {
    format!(
        "[package]\nname = \"{base}-core\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n"
    )
}

fn render_program_cargo(base: &str) -> String {
    format!(
        "[package]\nname = \"{base}\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[lib]\ncrate-type = [\"cdylib\", \"lib\"]\nname = \"{base}\"\n\n[features]\ndefault = []\nanchor-debug = [\"anchor-lang/anchor-debug\"]\ncpi = [\"no-entrypoint\"]\ncustom-heap = []\ncustom-panic = []\nno-entrypoint = []\nno-idl = []\nno-log-ix-name = []\nidl-build = [\"anchor-lang/idl-build\", \"anchor-spl/idl-build\"]\n\n[dependencies]\nanchor-lang = \"={ANCHOR_VERSION}\"\nanchor-spl = {{ version = \"={ANCHOR_VERSION}\", features = [\"token\", \"token_2022\"] }}\nsolana-sha256-hasher = {{ version = \"={SOLANA_SHA256_HASHER_VERSION}\", features = [\"sha2\"] }}\n{base}-core = {{ path = \"../../crates/{base}-core\" }}\n\n[lints.rust]\nunexpected_cfgs = {{ level = \"warn\", check-cfg = ['cfg(target_os, values(\"solana\"))'] }}\n"
    )
}

fn render_manifest(spec: &SolanaAnchorSpec) -> String {
    let mut output = String::new();
    let _ = writeln!(output, "generator={SOLANA_ANCHOR_GENERATOR_ID}");
    let _ = writeln!(output, "machine_hash={}", spec.machine().machine_hash());
    let _ = writeln!(output, "anchor={ANCHOR_VERSION}");
    let _ = writeln!(output, "solana={SOLANA_TOOLCHAIN_VERSION}");
    let _ = writeln!(
        output,
        "solana-sha256-hasher={SOLANA_SHA256_HASHER_VERSION}"
    );
    let _ = writeln!(output, "program_id={}", base58_encode(spec.program_id()));
    let _ = writeln!(output, "state_seed={}", spec.state_seed());
    output.push_str("core_dependencies=0\ncore_no_std=true\noverflow_checks=true\nverified_build=required\nupgrade_authority_review=required\n");
    for binding in spec.fungible_bindings() {
        let _ = writeln!(
            output,
            "capability.{}.mint={}",
            binding.capability_code(),
            base58_encode(binding.mint())
        );
        let _ = writeln!(
            output,
            "capability.{}.token_program={}",
            binding.capability_code(),
            base58_encode(binding.token_program().program_id())
        );
        let _ = writeln!(
            output,
            "capability.{}.vault={}",
            binding.capability_code(),
            base58_encode(binding.vault())
        );
    }
    output
}

fn render_agent_policy(spec: &SolanaAnchorSpec) -> String {
    format!(
        "# Solana agent policy for {}\n\nMachine hash: `{}`\nProgram ID: `{}`\n\nAn agent may edit only `crates/{}-core/src/project.rs` and tests. The pure crate has no dependencies, is `#![no_std]`, and forbids unsafe Rust. Regenerate all shell, account, PDA, token-binding, hashing, receipt, and manifest files.\n\nForbidden without a new reviewed generator profile: `UncheckedAccount`, `remaining_accounts`, raw `invoke` or `invoke_signed`, arbitrary CPI program IDs, dynamic account resizing or closure, upgrade-loader instructions, host I/O, randomness, floats, panic paths, and direct shell edits.\n\nRequired before production authorization: Anchor/Solana exact-version compilation, LiteSVM or Mollusk tests, program-test integration tests, compute ceilings, mutation and property tests, verified builds, deployed-program hash verification, explicit upgrade-authority disposition, independent review, and deployment-specific token-account checks.\n",
        spec.machine().name(),
        spec.machine().machine_hash(),
        base58_encode(spec.program_id()),
        to_lower_snake(spec.machine().name())
    )
}

fn inspect_source<K: Copy>(source: &str, forbidden: &[(&str, K)]) -> SolanaSafetyReport<K> {
    let sanitized = sanitize_non_code(source).to_ascii_lowercase();
    let mut findings = Vec::new();
    for (needle, kind) in forbidden {
        let mut start = 0;
        while let Some(relative) = sanitized[start..].find(needle) {
            let offset = start + relative;
            findings.push(SolanaSafetyFinding {
                kind: *kind,
                byte_offset: offset,
            });
            start = offset.saturating_add(needle.len());
        }
    }
    findings.sort_by_key(SolanaSafetyFinding::byte_offset);
    SolanaSafetyReport { findings }
}

fn sanitize_non_code(source: &str) -> String {
    #[derive(Clone, Copy)]
    enum Mode {
        Code,
        LineComment,
        BlockComment,
        Quoted(u8),
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
                (b'\'', _) | (b'"', _) => {
                    mode = Mode::Quoted(byte);
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
            Mode::Quoted(quote) => {
                if byte == b'\\' && next.is_some() {
                    index += 2;
                    continue;
                }
                if byte == quote {
                    mode = Mode::Code;
                }
            }
        }
        index += 1;
    }
    String::from_utf8(output).unwrap_or_default()
}

fn recipient_expression(policy: RecipientPolicy, machine: &OnchainMachineSpec) -> String {
    match policy {
        RecipientPolicy::Caller => "context.actor".to_owned(),
        RecipientPolicy::Fixed(value) => rust_bytes(value),
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
            format!("before_state.{name}")
        }
    }
}

fn append_expression(prefix: &str, field: &OnchainField) -> String {
    match field.scalar() {
        OnchainScalar::Bool => format!("bytes.push(u8::from({prefix}.{}));", field.name()),
        OnchainScalar::Bytes32 => format!("bytes.extend_from_slice(&{prefix}.{});", field.name()),
        _ => format!(
            "bytes.extend_from_slice(&{prefix}.{}.to_be_bytes());",
            field.name()
        ),
    }
}

fn encode_bytes32(scalar: OnchainScalar, name: &str) -> String {
    match scalar {
        OnchainScalar::Bool => {
            format!("{{ let mut output = [0_u8; 32]; output[31] = u8::from({name}); output }}")
        }
        OnchainScalar::Bytes32 => name.to_owned(),
        OnchainScalar::U8 => encode_unsigned_bytes32(name, 1),
        OnchainScalar::U16 => encode_unsigned_bytes32(name, 2),
        OnchainScalar::U32 => encode_unsigned_bytes32(name, 4),
        OnchainScalar::U64 => encode_unsigned_bytes32(name, 8),
        OnchainScalar::U128 => encode_unsigned_bytes32(name, 16),
        OnchainScalar::I8 => encode_signed_bytes32(name, 1),
        OnchainScalar::I16 => encode_signed_bytes32(name, 2),
        OnchainScalar::I32 => encode_signed_bytes32(name, 4),
        OnchainScalar::I64 => encode_signed_bytes32(name, 8),
        OnchainScalar::I128 => encode_signed_bytes32(name, 16),
    }
}

fn encode_unsigned_bytes32(name: &str, width: usize) -> String {
    let start = 32_usize.saturating_sub(width);
    format!(
        "{{ let mut output = [0_u8; 32]; let bytes = {name}.to_be_bytes(); output[{start}..].copy_from_slice(&bytes); output }}"
    )
}

fn encode_signed_bytes32(name: &str, width: usize) -> String {
    let start = 32_usize.saturating_sub(width);
    format!(
        "{{ let mut output = if {name} < 0 {{ [0xff_u8; 32] }} else {{ [0_u8; 32] }}; let bytes = {name}.to_be_bytes(); output[{start}..].copy_from_slice(&bytes); output }}"
    )
}

fn rust_type(scalar: OnchainScalar) -> &'static str {
    match scalar {
        OnchainScalar::Bool => "bool",
        OnchainScalar::U8 => "u8",
        OnchainScalar::U16 => "u16",
        OnchainScalar::U32 => "u32",
        OnchainScalar::U64 => "u64",
        OnchainScalar::U128 => "u128",
        OnchainScalar::I8 => "i8",
        OnchainScalar::I16 => "i16",
        OnchainScalar::I32 => "i32",
        OnchainScalar::I64 => "i64",
        OnchainScalar::I128 => "i128",
        OnchainScalar::Bytes32 => "[u8; 32]",
    }
}

fn rust_bytes(bytes: [u8; 32]) -> String {
    let joined = bytes
        .iter()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

fn valid_seed(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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

fn to_upper_snake(value: &str) -> String {
    to_lower_snake(value).to_ascii_uppercase()
}

fn base58_encode(input: [u8; 32]) -> String {
    const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let zeros = input.iter().take_while(|byte| **byte == 0).count();
    let mut digits = vec![0_u8];
    for byte in input {
        let mut carry = u32::from(byte);
        for digit in &mut digits {
            let value = u32::from(*digit) * 256 + carry;
            *digit = u8::try_from(value % 58).unwrap_or_default();
            carry = value / 58;
        }
        while carry != 0 {
            digits.push(u8::try_from(carry % 58).unwrap_or_default());
            carry /= 58;
        }
    }
    let mut output = String::new();
    for _ in 0..zeros {
        output.push('1');
    }
    for digit in digits.iter().rev() {
        output.push(char::from(ALPHABET[usize::from(*digit)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::onchain::{OnchainCapability, OnchainReason};

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

    fn binding() -> SolanaFungibleBinding {
        match SolanaFungibleBinding::try_new(7, [2_u8; 32], SolanaTokenProgram::Legacy, [4_u8; 32])
        {
            Ok(value) => value,
            Err(error) => panic!("binding rejected: {error}"),
        }
    }

    fn spec() -> SolanaAnchorSpec {
        match SolanaAnchorSpec::try_new(machine(), [9_u8; 32], "machine_state", vec![binding()]) {
            Ok(value) => value,
            Err(error) => panic!("spec rejected: {error}"),
        }
    }

    #[test]
    fn repeated_generation_is_identical() {
        assert_eq!(
            generate_solana_anchor(&spec()),
            generate_solana_anchor(&spec())
        );
    }

    #[test]
    fn generated_core_is_dependency_free_and_inert() {
        let bundle = match generate_solana_anchor(&spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let core_manifest = bundle
            .files()
            .iter()
            .find(|file| file.path().ends_with("core/Cargo.toml"))
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        let project = bundle
            .files()
            .iter()
            .find(|file| file.path().ends_with("core/src/project.rs"))
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        assert!(core_manifest.ends_with("[dependencies]\n"));
        assert!(project.contains("command_admissible"));
        assert!(project.contains("false"));
        assert!(inspect_solana_core_source(project).is_clean());
    }

    #[test]
    fn generated_shell_has_fixed_accounts_and_typed_cpi() {
        let bundle = match generate_solana_anchor(&spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let shell = bundle
            .files()
            .iter()
            .find(|file| file.path().contains("programs/") && file.path().ends_with("src/lib.rs"))
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        let program_manifest = bundle
            .files()
            .iter()
            .find(|file| {
                file.path().starts_with("programs/") && file.path().ends_with("Cargo.toml")
            })
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        assert!(shell.contains("token_interface::transfer_checked"));
        assert!(shell.contains("_token_program.key()"));
        assert!(shell.contains("use solana_sha256_hasher::{hash, hashv};"));
        assert!(!shell.contains("solana_program::hash"));
        assert!(shell.contains("seeds = [STATE_SEED, authority.key().as_ref()]"));
        assert!(shell.contains("expected_state_hash"));
        assert!(program_manifest.contains("solana-sha256-hasher = { version = \"=3.1.0\""));
        assert!(program_manifest.contains("custom-heap = []"));
        assert!(program_manifest.contains("cfg(target_os, values(\"solana\"))"));
        assert!(!shell.contains("UncheckedAccount"));
        assert!(!shell.contains("remaining_accounts"));
        assert!(inspect_solana_shell_source(shell).is_clean());
    }

    #[test]
    fn initialize_authority_matches_execute_lifecycle_type() {
        let bundle = match generate_solana_anchor(&spec()) {
            Ok(value) => value,
            Err(error) => panic!("generation failed: {error}"),
        };
        let shell = bundle
            .files()
            .iter()
            .find(|file| file.path().contains("programs/") && file.path().ends_with("src/lib.rs"))
            .map(GeneratedOnchainFile::content)
            .unwrap_or_default();
        assert!(
            shell.contains("#[account(mut, signer)]\n    pub authority: SystemAccount<'info>,")
        );
        assert!(!shell.contains("#[account(mut)]\n    pub authority: Signer<'info>,"));
    }

    #[test]
    fn scanner_rejects_runtime_access_in_core() {
        let source = "use anchor_lang::prelude::*; fn run() { panic!(\"bad\"); }";
        let report = inspect_solana_core_source(source);
        assert!(!report.is_clean());
        let kinds: BTreeSet<_> = report
            .findings()
            .iter()
            .map(SolanaSafetyFinding::kind)
            .collect();
        assert!(kinds.contains(&SolanaCoreSafetyFindingKind::AnchorDependency));
        assert!(kinds.contains(&SolanaCoreSafetyFindingKind::PanicPath));
    }

    #[test]
    fn binding_set_must_be_exact() {
        assert_eq!(
            SolanaAnchorSpec::try_new(machine(), [9_u8; 32], "machine_state", Vec::new()),
            Err(OnchainModelError::InvalidBinding)
        );
    }

    #[test]
    fn token_program_ids_are_closed() {
        assert_eq!(
            SolanaTokenProgram::try_from_program_id(SolanaTokenProgram::Legacy.program_id()),
            Ok(SolanaTokenProgram::Legacy)
        );
        let token_2022 = [
            6, 221, 246, 225, 238, 117, 143, 222, 24, 66, 93, 188, 228, 108, 205, 218, 182, 26,
            252, 77, 131, 185, 13, 39, 254, 189, 249, 40, 216, 161, 139, 252,
        ];
        assert_eq!(
            SolanaTokenProgram::try_from_program_id(token_2022),
            Err(OnchainModelError::InvalidBinding)
        );
        assert_eq!(
            SolanaTokenProgram::try_from_program_id([3_u8; 32]),
            Err(OnchainModelError::InvalidBinding)
        );
        assert_eq!(
            base58_encode(SolanaTokenProgram::Legacy.program_id()),
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
        );
    }

    #[test]
    fn base58_preserves_leading_zeroes() {
        let mut input = [0_u8; 32];
        input[31] = 1;
        assert!(base58_encode(input).starts_with('1'));
    }
}
