//! Alternatives of an admitted contract. The caller supplies `Contract::reasons`,
//! the compiler's validated reason classes, rather than all declared IDs.
use super::decision_class::Class;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn selected(reasons: Seq<(u32, Class)>, class: Class) -> Seq<u32>
        decreases reasons.len(),
    {
        if reasons.len() == 0 { Seq::empty() } else {
            let prefix = selected(reasons.drop_last(), class);
            if reasons.last().1 == class { prefix.push(reasons.last().0) } else { prefix }
        }
    }

}

/// Shared class predicate for admissible reason alternatives.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub fn same_class(left: Class, right: Class) -> bool {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(|yes: bool| yes == (left == right)));
    matches!(
        (left, right),
        (Class::Accept, Class::Accept)
            | (Class::Reject, Class::Reject)
            | (Class::CommittedFailure, Class::CommittedFailure)
    )
}

/// Precisely the reasons admitted with `class`, preserving declaration order.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub fn reasons_for(reasons: &[(u32, Class)], class: Class) -> Vec<u32> {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(|out: Vec<u32>| out@ == selected(reasons@, class)));
    let mut out = Vec::new();
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant
        i <= reasons.len(), out@ == selected(reasons@.take(i as int), class)
     , decreases reasons.len() - i,))]
    while i < reasons.len() {
        if same_class(reasons[i].1, class) {
            out.push(reasons[i].0);
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(reasons@.take(i as int + 1) == reasons@.take(i as int).push(reasons@[i as int]));
            assert(reasons@.take(i as int + 1).drop_last() =~= reasons@.take(i as int));
            assert(reasons@.take(i as int + 1).last() == reasons@[i as int]);

        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(reasons@.take(i as int) =~= reasons@); }
    out
}

/// A proposed class, reason and optional successor-state index.
pub type Alternative = (Class, Option<u32>, Option<usize>);

#[cfg(verus_keep_ghost)]
verus! {
    pub open spec fn one(class: Class, reason: Option<u32>, states: nat) -> Seq<Alternative> {
        if class == Class::Reject { seq![(class, reason, None::<usize>)] }
        else { Seq::new(states, |i: int| (class, reason, Some(i as usize))) }
    }
    pub open spec fn for_reasons(ids: Seq<u32>, class: Class, states: nat) -> Seq<Alternative>
        decreases ids.len(),
    {
        if ids.len() == 0 { Seq::empty() }
        else { for_reasons(ids.drop_last(), class, states) + one(class, Some(ids.last()), states) }
    }
    pub open spec fn universe(reasons: Seq<(u32, Class)>, states: nat) -> Seq<Alternative> {
        for_reasons(selected(reasons, Class::Reject), Class::Reject, states)
            + one(Class::Accept, None, states)
            + for_reasons(selected(reasons, Class::CommittedFailure), Class::CommittedFailure, states)
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn append_one(out: &mut Vec<Alternative>, class: Class, reason: Option<u32>, states: usize) {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(final(out)@ == old(out)@ + one(class, reason, states as nat)));
    #[cfg(verus_keep_ghost)]
    let before = verus_exec_expr!(Ghost(out@));
    if same_class(class, Class::Reject) {
        out.push((class, reason, None));
        #[cfg(verus_keep_ghost)]
        proof! { assert(out@ =~= before@ + one(class, reason, states as nat)); }
    } else {
        let mut i = 0;
        #[cfg_attr(verus_keep_ghost, verus_spec(invariant
            i <= states, class != Class::Reject,
            out@ == before@ + one(class, reason, i as nat),
            decreases states - i,
        ))]
        while i < states {
            out.push((class, reason, Some(i)));
            #[cfg(verus_keep_ghost)]
            proof! {
                assert(one(class, reason, (i + 1) as nat) =~=
                    one(class, reason, i as nat).push((class, reason, Some(i))));
                assert(out@ =~= before@ + one(class, reason, (i + 1) as nat));
            }
            i += 1;
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
fn append_reasons(out: &mut Vec<Alternative>, ids: &[u32], class: Class, states: usize) {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(final(out)@ == old(out)@ + for_reasons(ids@, class, states as nat)));
    #[cfg(verus_keep_ghost)]
    let before = verus_exec_expr!(Ghost(out@));
    let mut i = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(invariant
        i <= ids.len(), out@ == before@ + for_reasons(ids@.take(i as int), class, states as nat),
        decreases ids.len() - i,
    ))]
    while i < ids.len() {
        append_one(out, class, Some(ids[i]), states);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(ids@.take(i as int + 1).drop_last() =~= ids@.take(i as int));
            assert(ids@.take(i as int + 1).last() == ids@[i as int]);
            assert(out@ =~= before@ + for_reasons(ids@.take(i as int + 1), class, states as nat));
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(ids@.take(i as int) =~= ids@); }
}

/// All admitted class/reason/successor combinations, in deterministic order.
/// Uses the very same class list that the compiler admits for the policy.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub fn alternatives(reasons: &[(u32, Class)], states: usize) -> Vec<Alternative> {
    #[cfg(verus_keep_ghost)]
    #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
    verus_proof_expr!(ensures(|out: Vec<Alternative>| out@ == universe(reasons@, states as nat)));
    let rejects = reasons_for(reasons, Class::Reject);
    let failures = reasons_for(reasons, Class::CommittedFailure);
    let mut out = Vec::new();
    append_reasons(&mut out, &rejects, Class::Reject, states);
    append_one(&mut out, Class::Accept, None, states);
    append_reasons(&mut out, &failures, Class::CommittedFailure, states);
    #[cfg(verus_keep_ghost)]
    proof! { assert(out@ =~= universe(reasons@, states as nat)); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alternatives_never_cross_reason_classes() {
        let reasons = [(200, Class::Reject), (202, Class::CommittedFailure)];
        let got = alternatives(&reasons, 2);
        assert_eq!(
            got,
            vec![
                (Class::Reject, Some(200), None),
                (Class::Accept, None, Some(0)),
                (Class::Accept, None, Some(1)),
                (Class::CommittedFailure, Some(202), Some(0)),
                (Class::CommittedFailure, Some(202), Some(1)),
            ]
        );
        assert_eq!(alternatives(&reasons, 0).len(), 1);
    }
}
