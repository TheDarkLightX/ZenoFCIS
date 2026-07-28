#![forbid(unsafe_code)]

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use solana_sha256_hasher::{hash, hashv};
use treasury_machine_core::{Command, Context as CoreContext, Core, Decision, DecisionKind, EffectPlan, EventPlan, ProjectCore, State, MACHINE_HASH, MACHINE_VERSION};

declare_id!("cGfHiC6Kgg3FpFZvgwGcswsCRtp4aBP2fzuXRQPizuN");

const STATE_SEED: &[u8] = b"treasury_state";
const MAX_EVENT_SLOTS: u8 = 1;
const MAX_EFFECT_SLOTS: u8 = 1;

#[program]
pub mod treasury_machine {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, args: InitializeArgs) -> Result<()> {
        let initial_state = args.into_core();
        require!(ProjectCore::invariant(&initial_state), FcisError::InvariantViolation);
        let state = &mut ctx.accounts.state;
        state.version = MACHINE_VERSION;
        state.bump = ctx.bumps.state;
        state.authority = ctx.accounts.authority.key();
        state.sequence = 0;
        state.apply_core(initial_state);
        state.state_hash = hash_state(&initial_state);
        emit!(Initialized { state: state.key(), state_hash: state.state_hash, authority: state.authority });
        Ok(())
    }

    pub fn execute(ctx: Context<Execute>, args: ExecuteArgs) -> Result<()> {
        let before_state = ctx.accounts.state.to_core();
        let actual_state_hash = hash_state(&before_state);
        require!(actual_state_hash == ctx.accounts.state.state_hash, FcisError::StateRootCorrupted);
        require!(actual_state_hash == args.expected_state_hash && ctx.accounts.state.sequence == args.expected_sequence, FcisError::StaleState);
        let command = args.command();
        let clock = Clock::get()?;
        let context = CoreContext { actor: ctx.accounts.actor.key().to_bytes(), chain_domain: hash_chain_domain(&ctx.accounts.state.key()), sequence: ctx.accounts.state.sequence, slot: clock.slot, unix_timestamp: clock.unix_timestamp };
        require!(ProjectCore::command_admissible(&command, &context), FcisError::CommandNotAdmissible);
        let decision = ProjectCore::decide(&before_state, &command, &context);
        if decision.kind == DecisionKind::Reject { return Err(rejection_error(decision.reason_code)); }
        require!(decision.reason_code == 0, FcisError::InvalidDecision);
        require!(ProjectCore::invariant(&decision.next_state), FcisError::InvariantViolation);
        validate_plans(&decision, &before_state, &command, &context)?;
        let post_state_hash = hash_state(&decision.next_state);
        let command_hash = hash_command(&command);
        let context_hash = hash_context(&context);
        let event_plan_hash = hash_event_plan(&decision);
        let effect_plan_hash = hash_effect_plan(&decision);
        let candidate_hash = hashv_owned(&[&MACHINE_HASH, &actual_state_hash, &post_state_hash, &command_hash, &context_hash, &event_plan_hash, &effect_plan_hash]);
        {
            let state = &mut ctx.accounts.state;
            state.apply_core(decision.next_state);
            state.state_hash = post_state_hash;
            state.sequence = state.sequence.checked_add(1).ok_or(FcisError::SequenceOverflow)?;
        }
        apply_effects(&ctx, &decision)?;
        for index in 0..usize::from(decision.event_count) { let planned = decision.events[index]; emit!(DomainEvent { code: planned.code, field_count: planned.field_count, data: planned.data, payload_hash: hash_event(&planned) }); }
        emit!(TransitionCommitted { state: ctx.accounts.state.key(), sequence: ctx.accounts.state.sequence, pre_state_hash: actual_state_hash, post_state_hash, candidate_hash, command_hash, context_hash });
        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct InitializeArgs {
    pub owner: [u8; 32],
    pub balance: u128,
}

impl InitializeArgs { fn into_core(self) -> State { State {
    owner: self.owner,
    balance: self.balance,
} } }

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct ExecuteArgs {
    pub expected_state_hash: [u8; 32],
    pub expected_sequence: u64,
    pub recipient: [u8; 32],
    pub amount: u128,
}

impl ExecuteArgs { fn command(&self) -> Command { Command {
    recipient: self.recipient,
    amount: self.amount,
} } }

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = MachineState::SPACE, seeds = [STATE_SEED, authority.key().as_ref()], bump)]
    pub state: Account<'info, MachineState>,
    #[account(mut, signer)]
    pub authority: SystemAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Execute<'info> {
    #[account(mut, seeds = [STATE_SEED, authority.key().as_ref()], bump = state.bump, has_one = authority)]
    pub state: Account<'info, MachineState>,
    #[account(address = state.authority)]
    pub authority: SystemAccount<'info>,
    pub actor: Signer<'info>,
    #[account(address = Pubkey::new_from_array([2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2]))]
    pub payout_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = Pubkey::new_from_array([4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4]), token::mint = payout_mint, token::authority = state, token::token_program = payout_token_program)]
    pub payout_vault: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, token::mint = payout_mint, token::token_program = payout_token_program)]
    pub payout_destination: InterfaceAccount<'info, TokenAccount>,
    #[account(address = Pubkey::new_from_array([6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172, 28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169]))]
    pub payout_token_program: Interface<'info, TokenInterface>,
}

#[account]
pub struct MachineState {
    pub version: u16,
    pub bump: u8,
    pub authority: Pubkey,
    pub sequence: u64,
    pub state_hash: [u8; 32],
    pub owner: [u8; 32],
    pub balance: u128,
}

impl MachineState {
    pub const SPACE: usize = 131;
    fn to_core(&self) -> State { State {
        owner: self.owner,
        balance: self.balance,
    } }
    fn apply_core(&mut self, value: State) {
        self.owner = value.owner;
        self.balance = value.balance;
    }
}

#[event]
pub struct Initialized { pub state: Pubkey, pub state_hash: [u8; 32], pub authority: Pubkey }
#[event]
pub struct DomainEvent { pub code: u16, pub field_count: u8, pub data: [[u8; 32]; 8], pub payload_hash: [u8; 32] }
#[event]
pub struct TransitionCommitted { pub state: Pubkey, pub sequence: u64, pub pre_state_hash: [u8; 32], pub post_state_hash: [u8; 32], pub candidate_hash: [u8; 32], pub command_hash: [u8; 32], pub context_hash: [u8; 32] }

#[error_code]
pub enum FcisError {
    #[msg("State invariant rejected")] InvariantViolation,
    #[msg("Command admission rejected")] CommandNotAdmissible,
    #[msg("State root differs from committed root")] StateRootCorrupted,
    #[msg("Expected state root or sequence is stale")] StaleState,
    #[msg("Decision encoding is inconsistent")] InvalidDecision,
    #[msg("Plan exceeds its closed authority")] InvalidPlan,
    #[msg("Unknown or malformed capability")] InvalidCapability,
    #[msg("Destination token owner does not match the planned recipient")] InvalidRecipient,
    #[msg("State sequence overflow")] SequenceOverflow,
    #[msg("Transition rejected: Unauthorized")] RejectUnauthorized,
    #[msg("Transition rejected: InsufficientBalance")] RejectInsufficientBalance,
}

fn validate_plans(decision: &Decision, before_state: &State, command: &Command, context: &CoreContext) -> Result<()> {
    require!(decision.event_count <= MAX_EVENT_SLOTS && decision.effect_count <= MAX_EFFECT_SLOTS, FcisError::InvalidPlan);
    let mut prior = [0_u8; 32];
    for index in 0..usize::from(decision.event_count) { let planned = decision.events[index]; require!(event_field_count(planned.code)? == planned.field_count, FcisError::InvalidPlan); let digest = hash_event(&planned); if index != 0 { require!(digest >= prior, FcisError::InvalidPlan); } prior = digest; }
    for index in usize::from(decision.event_count)..decision.events.len() { require!(decision.events[index] == EventPlan::default(), FcisError::InvalidPlan); }
    prior = [0_u8; 32];
    for index in 0..usize::from(decision.effect_count) { let planned = decision.effects[index]; validate_effect(&planned, before_state, command, context)?; let digest = hash_effect(&planned); if index != 0 { require!(digest >= prior, FcisError::InvalidPlan); } prior = digest; let mut uses = 0_u8; for inner in 0..=index { if decision.effects[inner].capability == planned.capability { uses = uses.checked_add(1).ok_or(FcisError::InvalidPlan)?; } } require!(uses <= capability_max_uses(planned.capability)?, FcisError::InvalidPlan); }
    for index in usize::from(decision.effect_count)..decision.effects.len() { require!(decision.effects[index] == EffectPlan::default(), FcisError::InvalidPlan); }
    Ok(())
}

fn validate_effect(planned: &EffectPlan, before_state: &State, command: &Command, context: &CoreContext) -> Result<()> { require!(planned.capability != 0 && planned.amount != 0, FcisError::InvalidCapability); require!(planned.asset_id == capability_asset(planned.capability)? && planned.recipient == expected_recipient(planned.capability, before_state, command, context) && planned.amount <= capability_max_amount(planned.capability)?, FcisError::InvalidCapability); Ok(()) }

fn apply_effects(ctx: &Context<Execute>, decision: &Decision) -> Result<()> {
    for index in 0..usize::from(decision.effect_count) { let planned = decision.effects[index]; match planned.capability {
        7 => {
            require!(ctx.accounts.payout_destination.owner.to_bytes() == planned.recipient, FcisError::InvalidRecipient);
            let amount = u64::try_from(planned.amount).map_err(|_| error!(FcisError::InvalidPlan))?;
            let authority_key = ctx.accounts.authority.key();
            let bump = [ctx.accounts.state.bump];
            let signer_seeds: &[&[u8]] = &[STATE_SEED, authority_key.as_ref(), &bump];
            let signer = &[signer_seeds];
            let cpi_accounts = TransferChecked { from: ctx.accounts.payout_vault.to_account_info(), mint: ctx.accounts.payout_mint.to_account_info(), to: ctx.accounts.payout_destination.to_account_info(), authority: ctx.accounts.state.to_account_info() };
            token_interface::transfer_checked(CpiContext::new_with_signer(ctx.accounts.payout_token_program.key(), cpi_accounts, signer), amount, ctx.accounts.payout_mint.decimals)?;
        }
        _ => return err!(FcisError::InvalidCapability),
    } }
    Ok(())
}

fn event_field_count(code: u16) -> Result<u8> { match code {
    1 => Ok(2),
    _ => err!(FcisError::InvalidPlan),
} }

fn capability_asset(code: u16) -> Result<[u8; 32]> { match code {
    7 => Ok([8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8]),
    _ => err!(FcisError::InvalidCapability),
} }
fn capability_max_amount(code: u16) -> Result<u128> { match code {
    7 => Ok(1000000),
    _ => err!(FcisError::InvalidCapability),
} }
fn capability_max_uses(code: u16) -> Result<u8> { match code {
    7 => Ok(1),
    _ => err!(FcisError::InvalidCapability),
} }
fn expected_recipient(code: u16, before_state: &State, command: &Command, context: &CoreContext) -> [u8; 32] { let _ = (before_state, command, context); match code {
    7 => command.recipient,
    _ => [0_u8; 32],
} }

fn rejection_error(code: u16) -> anchor_lang::error::Error { match code {
    1 => FcisError::RejectUnauthorized.into(),
    2 => FcisError::RejectInsufficientBalance.into(),
    _ => FcisError::InvalidDecision.into(),
} }

fn hash_state(value: &State) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b"zeno-fcis/solana/state/v1\0"); bytes.extend_from_slice(&MACHINE_HASH);
    bytes.extend_from_slice(&1u16.to_be_bytes()); bytes.extend_from_slice(&value.owner);
    bytes.extend_from_slice(&2u16.to_be_bytes()); bytes.extend_from_slice(&value.balance.to_be_bytes());
    hash(&bytes).to_bytes() }
fn hash_command(value: &Command) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b"zeno-fcis/solana/command/v1\0"); bytes.extend_from_slice(&MACHINE_HASH);
    bytes.extend_from_slice(&1u16.to_be_bytes()); bytes.extend_from_slice(&value.recipient);
    bytes.extend_from_slice(&2u16.to_be_bytes()); bytes.extend_from_slice(&value.amount.to_be_bytes());
    hash(&bytes).to_bytes() }
fn hash_context(value: &CoreContext) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b"zeno-fcis/solana/context/v1\0"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.actor); bytes.extend_from_slice(&value.chain_domain); bytes.extend_from_slice(&value.sequence.to_be_bytes()); bytes.extend_from_slice(&value.slot.to_be_bytes()); bytes.extend_from_slice(&value.unix_timestamp.to_be_bytes()); hash(&bytes).to_bytes() }
fn hash_chain_domain(state: &Pubkey) -> [u8; 32] { hashv_owned(&[b"zeno-fcis/solana/domain/v1\0", &crate::ID.to_bytes(), &state.to_bytes(), &MACHINE_HASH]) }
fn hash_event(value: &EventPlan) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b"zeno-fcis/solana/event/v1\0"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.code.to_be_bytes()); bytes.push(value.field_count); for item in value.data { bytes.extend_from_slice(&item); } hash(&bytes).to_bytes() }
fn hash_effect(value: &EffectPlan) -> [u8; 32] { let mut bytes = Vec::new(); bytes.extend_from_slice(b"zeno-fcis/solana/effect/v1\0"); bytes.extend_from_slice(&MACHINE_HASH); bytes.extend_from_slice(&value.capability.to_be_bytes()); bytes.extend_from_slice(&value.asset_id); bytes.extend_from_slice(&value.recipient); bytes.extend_from_slice(&value.amount.to_be_bytes()); hash(&bytes).to_bytes() }
fn hash_event_plan(value: &Decision) -> [u8; 32] { let mut result = hashv_owned(&[b"zeno-fcis/solana/event-plan/v1\0", &MACHINE_HASH, &[value.event_count]]); for index in 0..usize::from(value.event_count) { result = hashv_owned(&[&result, &hash_event(&value.events[index])]); } result }
fn hash_effect_plan(value: &Decision) -> [u8; 32] { let mut result = hashv_owned(&[b"zeno-fcis/solana/effect-plan/v1\0", &MACHINE_HASH, &[value.effect_count]]); for index in 0..usize::from(value.effect_count) { result = hashv_owned(&[&result, &hash_effect(&value.effects[index])]); } result }
fn hashv_owned(values: &[&[u8]]) -> [u8; 32] { hashv(values).to_bytes() }

