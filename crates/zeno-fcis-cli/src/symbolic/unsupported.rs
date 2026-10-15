#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, std_specs::convert::IntoSpec};

/// A construct the encoding does not translate. A check that meets one is
/// inconclusive.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[cfg_attr(not(verus_keep_ghost), derive(Clone, Debug, Eq, PartialEq))]
pub(crate) struct Unsupported {
    reason: String,
}

#[cfg(verus_keep_ghost)]
verus! {
    impl Unsupported {
        pub(crate) closed spec fn reason_view(self) -> Seq<char> {
            self.reason@
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Unsupported {
    pub(crate) fn new<R: Into<String>>(reason: R) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Self|
            R::obeys_into_spec() ==> result.reason_view() == reason.into_spec()@));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Unsupported::reason_view); }
        Self {
            reason: reason.into(),
        }
    }

    pub(crate) fn reason(&self) -> &str {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: &str| result@ == self.reason_view()));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(Unsupported::reason_view); }
        &self.reason
    }
}
