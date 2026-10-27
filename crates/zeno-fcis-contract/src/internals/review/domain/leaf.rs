//! Shared leaf domains, least values, and program positions.

use crate::internals::declarations::Source;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// An admitted leaf domain, as the schema declares it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub enum LeafDomain {
    Bool,
    Int { min: i128, max: i128 },
    Sum { type_id: u32, variants: Vec<u16> },
}

/// One program input position: a root field, or a whole scalar root.
#[derive(Clone, Debug)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub struct Position {
    /// The rules name: `pre.100.110`, `command.101`, `context.102.130`.
    pub name: String,
    pub source: Source,
    pub field: Option<u16>,
    pub type_id: u32,
    pub domain: LeafDomain,
}

impl LeafDomain {
    #[cfg_attr(verus_keep_ghost,verus_spec(result => ensures result == least_spec(*self),))]
    pub fn least(&self) -> Option<i128> {
        match self {
            Self::Bool => Some(0),
            Self::Int { min, max } => {
                if min <= max {
                    Some(*min)
                } else {
                    None
                }
            }
            Self::Sum { variants, .. } => {
                if variants.is_empty() {
                    None
                } else {
                    Some(variants[0] as i128)
                }
            }
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn least_spec(domain: LeafDomain) -> Option<i128> {
    match domain {
        LeafDomain::Bool => Some(0),
        LeafDomain::Int { min, max } => if min <= max { Some(min) } else { None },
        LeafDomain::Sum { variants, .. } => if variants@.len() > 0 { Some(variants@[0] as i128) } else { None },
    }
}
pub open spec fn least_or_zero(domain: LeafDomain) -> i128 {
    match least_spec(domain) { Some(value) => value, None => 0 }
}
}
