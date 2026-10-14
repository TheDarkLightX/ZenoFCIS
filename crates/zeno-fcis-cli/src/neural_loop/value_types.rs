//! Owned values shared by the actual ledger and host-side serialization.

extern crate alloc;
use self::alloc::string::String;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Objective V1: the lexicographic pair (stored instructions, canonical
/// artifact bytes). `Ord` on this struct is that order because `nodes` comes
/// first.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Cost {
    /// Every stored instruction counted once, unused nodes included.
    pub(crate) nodes: u64,
    /// The actual canonical artifact length.
    pub(crate) bytes: u64,
}

/// The identity of the checker whose receipts this request binds, F3's
/// transform checker, as its receipts record it: the checker's semantics
/// version and the library's evaluator identity.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct CheckerIdentity {
    pub(crate) semantics: String,
    pub(crate) evaluator_identity: String,
}

impl Clone for CheckerIdentity {
    #[cfg_attr(verus_keep_ghost, verus_spec(result => ensures result == *self,))]
    fn clone(&self) -> Self {
        Self {
            semantics: self.semantics.clone(),
            evaluator_identity: self.evaluator_identity.clone(),
        }
    }
}
