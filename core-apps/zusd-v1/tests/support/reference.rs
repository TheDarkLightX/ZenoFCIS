use zenofcis_zusd_v1::{Failure, common};
/// Finite row relation: all ordered F_i, legitimate acceptance, failure
/// prohibition and exact complete outcomes. Reference rows come solely from
/// the Python integer relation, never from a producer case or law 991.
pub fn lawful<T: PartialEq>(
    candidate: &Result<T, Failure>,
    reference: &Result<T, Failure>,
) -> bool {
    // A63 form: every `on reject:ID` condition is an implication, and ALL
    // declared conditions conjoin. The independent reference row fixes F_ID;
    // the producer's selected case supplies no predicate or eligibility fact.
    let reasons_hold = common::REASONS.iter().all(|id| {
        candidate.as_ref().err() != Some(&Failure::Reject(*id))
            || reference.as_ref().err() == Some(&Failure::Reject(*id))
    });
    let exact_accept = match candidate {
        Ok(value) => reference.as_ref().ok() == Some(value),
        Err(Failure::Reject(id)) => common::REASONS.contains(id),
        Err(_) => false, // E forbids technical failure; unbound rows are outside E.
    };
    reasons_hold && exact_accept
}
pub fn row<T: PartialEq + std::fmt::Debug>(
    candidate: Result<T, Failure>,
    reference: Result<T, Failure>,
) {
    if reference == Err(Failure::UnboundProfile) {
        assert_eq!(candidate, reference);
        assert!(!lawful(&candidate, &reference));
        for id in common::REASONS {
            assert!(!lawful(&Err(Failure::Reject(*id)), &reference));
        }
        return; // outside eligibility: no lawful business decision is counted.
    }
    assert!(
        lawful(&candidate, &reference),
        "candidate={candidate:?}\nreference={reference:?}"
    );
    let mut legal = usize::from(reference.is_ok());
    for id in common::REASONS {
        let alternative = Err(Failure::Reject(*id));
        let allowed = lawful(&alternative, &reference);
        assert_eq!(
            allowed,
            reference.as_ref().err() == Some(&Failure::Reject(*id))
        );
        legal += usize::from(allowed);
    }
    assert_eq!(
        legal, 1,
        "zero/reject-all/multiple lawful complete outcomes"
    );
    if reference.is_ok() {
        assert!(!lawful(&Err(Failure::UnboundProfile), &reference));
    }
}
