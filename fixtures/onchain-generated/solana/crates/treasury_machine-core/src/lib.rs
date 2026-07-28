#![no_std]
#![forbid(unsafe_code)]

mod project;
pub use project::ProjectCore;

pub const MACHINE_HASH: [u8; 32] = [189, 92, 97, 42, 214, 72, 79, 160, 37, 232, 242, 182, 222, 233, 168, 104, 169, 165, 75, 215, 190, 184, 194, 144, 71, 96, 1, 27, 133, 64, 57, 222];
pub const MACHINE_VERSION: u16 = 1;
pub const REASON_UNAUTHORIZED: u16 = 1;
pub const REASON_INSUFFICIENT_BALANCE: u16 = 2;
pub const EVENT_PAID: u16 = 1;
pub const CAPABILITY_PAYOUT: u16 = 7;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DecisionKind {
    #[default]
    Accept,
    Reject,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct State {
    pub owner: [u8; 32],
    pub balance: u128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Command {
    pub recipient: [u8; 32],
    pub amount: u128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Context {
    pub actor: [u8; 32],
    pub chain_domain: [u8; 32],
    pub sequence: u64,
    pub slot: u64,
    pub unix_timestamp: i64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventPlan {
    pub code: u16,
    pub field_count: u8,
    pub data: [[u8; 32]; 8],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EffectPlan {
    pub capability: u16,
    pub asset_id: [u8; 32],
    pub recipient: [u8; 32],
    pub amount: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decision {
    pub kind: DecisionKind,
    pub reason_code: u16,
    pub next_state: State,
    pub event_count: u8,
    pub events: [EventPlan; 1],
    pub effect_count: u8,
    pub effects: [EffectPlan; 1],
}

impl Default for Decision {
    fn default() -> Self {
        Self { kind: DecisionKind::Accept, reason_code: 0, next_state: State::default(), event_count: 0, events: [EventPlan::default(); 1], effect_count: 0, effects: [EffectPlan::default(); 1] }
    }
}

impl Decision {
    pub fn rejected(reason_code: u16) -> Self { Self { kind: DecisionKind::Reject, reason_code, ..Self::default() } }
    pub fn accepted(next_state: State) -> Self { Self { next_state, ..Self::default() } }
}

pub trait Core {
    fn command_admissible(command: &Command, context: &Context) -> bool;
    fn invariant(state: &State) -> bool;
    fn decide(state: &State, command: &Command, context: &Context) -> Decision;
}

pub fn event_paid(recipient: [u8; 32], amount: u128) -> EventPlan {
    let mut planned = EventPlan { code: 1, field_count: 2, ..EventPlan::default() };
    planned.data[0] = recipient;
    planned.data[1] = { let mut output = [0_u8; 32]; let bytes = amount.to_be_bytes(); output[16..].copy_from_slice(&bytes); output };
    planned
}

pub fn effect_payout(amount: u128, before_state: &State, command: &Command, context: &Context) -> EffectPlan {
    let _ = (before_state, command, context);
    EffectPlan { capability: 7, asset_id: [8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8, 8], recipient: command.recipient, amount }
}

