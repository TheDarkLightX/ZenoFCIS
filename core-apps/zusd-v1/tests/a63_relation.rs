//! Finite A63 law-form compatibility; no application admission or receipt claim.
#[path = "support/reference.rs"]
mod complete;
use zenofcis_zusd_v1::{Failure, common};

#[test]
fn a63_conjoined_reason_form_preserves_the_complete_reference_relation() {
    let mut domain = vec![Ok(0u8), Ok(1), Ok(2)];
    domain.extend(common::REASONS.iter().map(|id| Err(Failure::Reject(*id))));
    domain.extend([
        Err(Failure::Reject(999)),
        Err(Failure::Arithmetic),
        Err(Failure::Domain),
        Err(Failure::Canonical),
        Err(Failure::Capacity),
        Err(Failure::Conservation),
        Err(Failure::Law(1)),
        Err(Failure::UnboundProfile),
        Err(Failure::Budget {
            lane: 0,
            limit: 0,
            attempted: 1,
        }),
    ]);
    for reference in &domain {
        for candidate in &domain {
            let previous = match candidate {
                Err(Failure::Reject(id)) => {
                    common::REASONS.contains(id)
                        && reference.as_ref().err() == Some(&Failure::Reject(*id))
                }
                Ok(value) => reference.as_ref().ok() == Some(value),
                Err(_) => false,
            };
            assert_eq!(complete::lawful(candidate, reference), previous);
        }
        if reference.is_ok()
            || matches!(reference, Err(Failure::Reject(id)) if common::REASONS.contains(id))
        {
            complete::row(*reference, *reference);
        }
    }
    println!(
        "A63 form equivalence: {} candidate/reference pairs; all 29 scoped conditions conjoin",
        domain.len() * domain.len()
    );
}
