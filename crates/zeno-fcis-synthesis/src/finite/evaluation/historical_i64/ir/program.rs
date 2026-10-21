//! Owned immutable scalar program data shared by its native and checked getters.
use super::{Domain, Op};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Structurally validated, closed expression graph; construction is not a proof.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub struct Program {
    pub(super) inputs: Vec<Domain>,
    pub(super) outputs: Vec<Domain>,
    pub(super) nodes: Vec<Op>,
    pub(super) roots: Vec<u16>,
}

#[cfg(verus_keep_ghost)]
verus! {
impl Program {
    pub closed spec fn view(&self) -> (Seq<Domain>, Seq<Domain>, Seq<Op>, Seq<u16>) {
        (self.inputs@, self.outputs@, self.nodes@, self.roots@)
    }
}
}

impl Program {
    /// Input domains in ABI order.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().0,))]
    pub fn inputs(&self) -> &[Domain] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Program::view); }
        &self.inputs
    }
    /// Output domains in ABI order.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().1,))]
    pub fn outputs(&self) -> &[Domain] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Program::view); }
        &self.outputs
    }
    /// Typed topological instructions.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().2,))]
    pub fn nodes(&self) -> &[Op] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Program::view); }
        &self.nodes
    }
    /// Result nodes in ABI order.
    #[must_use]
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result@ == self.view().3,))]
    pub fn roots(&self) -> &[u16] {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Program::view); }
        &self.roots
    }
}
