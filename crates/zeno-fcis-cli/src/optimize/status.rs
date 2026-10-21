//! Total classification of a completed optimizer invocation.
//! `checked_improvement` comes from a retained equivalence receipt and strict
//! componentwise cost comparison in `Search::status_and_exit`. This function
//! classifies that evidence; it does not itself establish equivalence.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Status {
    Improved,
    UnsupportedOriginal,
    Inconclusive,
    NoCheckedImprovement,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) fn classify(
    checked_improvement: bool,
    untypable: bool,
    phases: usize,
    stopped: bool,
) -> Status {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(|s: Status| (s == Status::Improved)
        == checked_improvement
        && (s == Status::UnsupportedOriginal)
            == (!checked_improvement && untypable && phases == 0)
        && (s == Status::NoCheckedImprovement)
            == (!checked_improvement && !untypable && phases > 0 && !stopped)
        && (s == Status::Inconclusive)
            == (!checked_improvement
                && !(untypable && phases == 0)
                && (untypable || phases == 0 || stopped))));
    if checked_improvement {
        Status::Improved
    } else if untypable && phases == 0 {
        Status::UnsupportedOriginal
    } else if untypable || phases == 0 || stopped {
        Status::Inconclusive
    } else {
        Status::NoCheckedImprovement
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Status {
    pub(super) fn name(self) -> &'static str {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|s: &str| s@ == match self {
            Status::Improved => "improved"@,
            Status::UnsupportedOriginal => "unsupported-original"@,
            Status::Inconclusive => "inconclusive"@,
            Status::NoCheckedImprovement => "no-checked-improvement"@,
        }));
        match self {
            Self::Improved => "improved",
            Self::UnsupportedOriginal => "unsupported-original",
            Self::Inconclusive => "inconclusive",
            Self::NoCheckedImprovement => "no-checked-improvement",
        }
    }

    pub(super) fn exit_code(self) -> u8 {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(
            |code: u8| code == if self == Status::Improved { 0u8 } else { 2u8 }
        ));
        match self {
            Self::Improved => 0,
            _ => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Status, classify};

    #[test]
    fn a_search_that_never_ran_cannot_report_no_improvement() {
        assert_eq!(classify(false, true, 0, true), Status::UnsupportedOriginal);
        assert_eq!(classify(false, false, 0, false), Status::Inconclusive);
        assert_eq!(
            classify(false, false, 1, false),
            Status::NoCheckedImprovement
        );
        assert_eq!(classify(false, false, 1, true), Status::Inconclusive);
        // Independently checked supplied improvements survive a stopped search.
        assert_eq!(classify(true, true, 0, true), Status::Improved);
    }
}
