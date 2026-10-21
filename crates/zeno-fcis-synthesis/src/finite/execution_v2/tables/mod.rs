//! Closed, owned tables/1 execution. Finite qualification is separate from scalar proofs.
//! Binding and serialization are preparation; only Authority recomputes decisions.
#![allow(missing_docs)]
mod authority;
mod collection;
mod expression;
mod order_laws;
mod ordered;
#[cfg(test)]
mod ordered_tests;
mod policy;
pub mod redemption;
mod redemption_laws;
mod schema;
#[cfg(test)]
mod tests;

pub use super::composition::Raw;
pub use super::{Limits, Resource, Usage, zero_limits};
use alloc::{boxed::Box, vec::Vec};
pub use authority::{
    Authority, Decision, Evaluation, OrderedQuery, Publication, PublicationOutcome,
};
pub use expression::{LawReport, check_laws};
pub use zeno_fcis_codec::CollectionLimits;
pub use zeno_fcis_value::{Field, MapEntry, Value, ValueRef};

pub const PROFILE: &str = "tables/1";
pub const POLICY_VERSION: i128 = 3;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Failure {
    Descriptor,
    Policy,
    Schema,
    Wire,
    Kind,
    Arithmetic,
    Undefined,
    Law(u32),
    Replay,
    Budget(super::MeterFailure),
}
impl From<super::MeterFailure> for Failure {
    fn from(value: super::MeterFailure) -> Self {
        Self::Budget(value)
    }
}

#[derive(Clone, Debug)]
pub struct Type {
    pub id: u32,
    pub name: Vec<u8>,
    pub shape: Shape,
}
#[derive(Clone, Debug)]
pub struct FieldType {
    pub id: u16,
    pub name: Vec<u8>,
    pub type_id: u32,
}
#[derive(Clone, Debug)]
pub enum Shape {
    Unit,
    Bool,
    U128 {
        min: u128,
        max: u128,
    },
    I128 {
        min: i128,
        max: i128,
    },
    Bytes {
        min: u32,
        max: u32,
    },
    Tuple(Vec<u32>),
    Record(Vec<FieldType>),
    Vector {
        element: u32,
        max: u32,
        bytes: u64,
    },
    Maybe(u32),
    Table {
        key: u32,
        value: u32,
        set: bool,
        limits: CollectionLimits,
    },
}

/// Closed expressions. Match alone makes payload extraction conditional.
#[derive(Clone, Debug)]
pub enum Expr {
    Literal(u32, Value),
    Input(u8, Vec<u16>),
    Payload,
    EntryKey,
    EntryValue,
    Tuple(u32, Vec<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Empty(u32),
    Get(u32, Box<Expr>, Box<Expr>),
    Contains(u32, Box<Expr>, Box<Expr>),
    Len(u32, Box<Expr>),
    IsEmpty(u32, Box<Expr>),
    Insert(u32, Box<Expr>, Box<Expr>, Box<Expr>),
    Update(u32, Box<Expr>, Box<Expr>, Box<Expr>),
    Remove(u32, Box<Expr>, Box<Expr>),
    MatchLookup(Box<Expr>, Box<Expr>, Box<Expr>),
    MatchEdit(Box<Expr>, Box<Expr>, Box<Expr>),
    ErrorReason,
    Reason,
    Project(Box<Expr>, u16),
    MatchBool(Box<Expr>, Box<Expr>, Box<Expr>),
    All(Box<Expr>, Box<Expr>),
    Any(Box<Expr>, Box<Expr>),
    Count(Box<Expr>, Box<Expr>),
    SumI128(Box<Expr>, Box<Expr>),
    /// Law-local U256 sum; cannot become a persisted Value or general arithmetic operand.
    SumU128(Box<Expr>, Box<Expr>),
    /// Closed ordered operator: operation, comparator, view type, auxiliary type, arguments.
    Ordered(u8, u8, u32, u32, Vec<Expr>),
}
#[derive(Clone, Debug)]
pub enum Plan {
    Reject(u32),
    RejectEdit,
    Accept(Vec<(u16, Expr)>),
    MatchLookup(Expr, Box<Plan>, Box<Plan>),
    MatchEdit(Expr, Box<Plan>, Box<Plan>),
}
#[derive(Clone, Debug)]
pub struct Case {
    pub when: Expr,
    pub plan: Plan,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    State,
    Accept,
    Reject,
    Any,
    Genesis,
}
#[derive(Clone, Debug)]
pub enum Law {
    Predicate {
        id: u32,
        scope: Scope,
        expression: Expr,
    },
    Frame {
        id: u32,
        field: u16,
        keys: Vec<Expr>,
    },
}
impl Law {
    pub fn id(&self) -> u32 {
        match self {
            Self::Predicate { id, .. } | Self::Frame { id, .. } => *id,
        }
    }
}
/// Mandatory binding of an ordered view to original state fields.
#[derive(Clone, Debug)]
pub struct OrderSpec {
    pub mode: u8,
    pub map: u16,
    pub sequence: Option<u16>,
}
#[derive(Clone, Debug)]
pub struct Descriptor {
    pub order: Option<OrderSpec>,
    pub profile: Vec<u8>,
    pub types: Vec<Type>,
    pub roots: [u32; 3],
    pub frame_bytes: [u64; 3],
    pub genesis: Value,
    pub reasons: Vec<u32>,
    pub cases: Vec<Case>,
    pub laws: Vec<Law>,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditError {
    MissingKey,
    ItemTooLarge,
    Full,
    SnapshotTooLarge,
    Empty,
    SequenceTooLarge,
}
impl EditError {
    pub fn reason(self) -> u32 {
        match self {
            Self::MissingKey => 303,
            Self::ItemTooLarge => 306,
            Self::Full => 307,
            Self::SnapshotTooLarge => 308,
            Self::Empty => 314,
            Self::SequenceTooLarge => 315,
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) enum Data {
    Value(Value),
    WideSum([u128; 2]),
    Lookup(Option<Value>),
    Edit(Result<Value, EditError>),
}
impl Data {
    fn value(self) -> Result<Value, Failure> {
        match self {
            Self::Value(v) => Ok(v),
            _ => Err(Failure::Kind),
        }
    }
    fn boolean(self) -> Result<bool, Failure> {
        match self.value()?.view() {
            ValueRef::Bool(v) => Ok(v),
            _ => Err(Failure::Kind),
        }
    }
    fn integer(self) -> Result<i128, Failure> {
        match self.value()?.view() {
            ValueRef::I128(v) => Ok(v),
            _ => Err(Failure::Kind),
        }
    }
}
