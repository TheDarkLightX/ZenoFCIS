//! Executable abstract payment-model admission; this is not production code.
//! Its result is proved equal to the abstract two-handler specialization.
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Owned immutable admission result, including first-veto evidence.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, PartialEq, Eq)]
pub struct Admission {
    /// Earlier states are restored exactly when any layer vetoes.
    pub reserved: Vec<u64>,
    /// Canonical payment-product verdicts, in handler order.
    pub verdicts: Vec<u8>,
}

#[cfg(verus_keep_ghost)]
verus! {
pub open spec fn calculated(keys: Seq<u64>, capacity: usize, key: u64, send: bool,
    allowed: bool, enabled: bool) -> (Seq<u64>, Seq<u8>) {
    if !enabled { (keys, seq![]) }
    else if !send { (keys, seq![0u8, 0u8]) }
    else if keys.contains(key) { (keys, seq![24u8]) }
    else if keys.len() >= capacity { (keys, seq![27u8]) }
    else if !allowed { (keys, seq![0u8, 25u8]) }
    else { (keys.push(key), seq![0u8, 0u8]) }
}
}

/// Pure model admission, with the same order as the payment product.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures (result.reserved@, result.verdicts@)
        == calculated(keys@, capacity, key, send, allowed, enabled),
))]
#[allow(clippy::ptr_arg)] // The pinned verifier specifies Vec::clone, not slice::to_vec.
pub fn prepare(
    keys: &Vec<u64>,
    capacity: usize,
    key: u64,
    send: bool,
    allowed: bool,
    enabled: bool,
    retry: bool,
) -> Admission {
    let _ = retry;
    if !enabled {
        return Admission {
            reserved: keys.clone(),
            verdicts: Vec::new(),
        };
    }
    if !send {
        return Admission {
            reserved: keys.clone(),
            verdicts: vec![0, 0],
        };
    }
    let mut at = 0usize;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant enabled, send, at <= keys.len(),
            forall|j: int| 0 <= j < at ==> keys@[j] != key,
        decreases keys.len() - at,
    ))]
    while at < keys.len() {
        if keys[at] == key {
            #[cfg(verus_keep_ghost)]
            proof! { assert(keys@.contains(key)); }
            return Admission {
                reserved: keys.clone(),
                verdicts: vec![24],
            };
        }
        at += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! { assert(!keys@.contains(key)); }
    if keys.len() >= capacity {
        return Admission {
            reserved: keys.clone(),
            verdicts: vec![27],
        };
    }
    let mut proposed = keys.clone();
    proposed.push(key);
    if !allowed {
        return Admission {
            reserved: keys.clone(),
            verdicts: vec![0, 25],
        };
    }
    Admission {
        reserved: proposed,
        verdicts: vec![0, 0],
    }
}

/// The complete composed call supplies conservative evidence, never its last leaf.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result.0@ == (if refused || (suspended && !may_invoke) { previous@ }
                         else { proposed@ }),
        result.1 == (if refused { 0u8 } else { copies }),
))]
#[allow(clippy::ptr_arg)] // Preserve the same qualified owned-copy primitive as prepare.
pub fn finish(
    previous: &Vec<u64>,
    proposed: Vec<u64>,
    refused: bool,
    suspended: bool,
    may_invoke: bool,
    copies: u8,
) -> (Vec<u64>, u8) {
    if refused {
        return (previous.clone(), 0);
    }
    let final_state = if suspended && !may_invoke {
        previous.clone()
    } else {
        proposed
    };
    (final_state, copies)
}

#[cfg(verus_keep_ghost)]
verus! {
use crate::model::*;

pub open spec fn model_key(k: u64) -> Key { Key { store: 8, delivery: k as int } }

pub open spec fn model_keys(keys: Seq<u64>) -> Seq<Key> {
    Seq::new(keys.len(), |i: int| model_key(keys[i]))
}

pub open spec fn payment_stack(keys: Seq<u64>, capacity: usize, allowed: bool,
    enabled: bool) -> Seq<Monitor> {
    if !enabled { seq![] }
    else { seq![
        Monitor { policy: Policy::AtMostOne { capacity: capacity as nat }, reserved: model_keys(keys) },
        Monitor { policy: Policy::Allowlist { recipients: if allowed {
            Set::empty().insert(seq![7u8])
        } else { Set::empty() } }, reserved: seq![] },
    ] }
}

pub open spec fn payment_request(k: u64, send: bool, retry: bool) -> Request {
    Request { send, key: model_key(k), payload: seq![], destination: seq![7u8], retry }
}

pub open spec fn code(v: Verdict) -> u8 {
    match v {
        Verdict::Allow => 0, Verdict::Duplicate => 24,
        Verdict::Capacity => 27, Verdict::Recipient => 25,
    }
}

pub proof fn key_projection(keys: Seq<u64>, k: u64)
    ensures model_keys(keys).contains(model_key(k)) == keys.contains(k),
        model_keys(keys.push(k)) == model_keys(keys).push(model_key(k)),
{
    if keys.contains(k) {
        let at = choose|at: int| 0 <= at < keys.len() && keys[at] == k;
        assert(model_keys(keys)[at] == model_key(k));
    }
    if model_keys(keys).contains(model_key(k)) {
        let at = choose|at: int| 0 <= at < model_keys(keys).len()
            && model_keys(keys)[at] == model_key(k);
        assert(keys[at] == k);
    }
    assert(model_keys(keys.push(k)) =~= model_keys(keys).push(model_key(k)));
}

pub proof fn two_handler_evidence(first: Monitor, second: Monitor, q: Request)
    ensures evidence(seq![first, second], q) ==
        (if decide(first, q).verdict is Allow {
            seq![decide(first, q), decide(second, q)]
        } else { seq![decide(first, q)] }),
{
    reveal_with_fuel(scan, 3);
    let one = seq![decide(first, q)];
    assert(scan(seq![first, second], q, 1) =~= one);
    if decide(first, q).verdict is Allow {
        assert forall|i: int| 0 <= i < one.len()
            implies (#[trigger] one[i]).verdict is Allow by { assert(i == 0); }
    } else {
        assert(!(one[0].verdict is Allow));
        assert(!all_allow(one));
    }
    assert(evidence(seq![first, second], q) =~=
        (if decide(first, q).verdict is Allow {
            seq![decide(first, q), decide(second, q)]
        } else { seq![decide(first, q)] }));
}

/// Native model output is an interpretation of the same abstract admission.
/// Full 256-bit production-key decoding is intentionally outside this fixture.
pub proof fn payment_admission_correspondence(keys: Seq<u64>, capacity: usize, k: u64,
    send: bool, allowed: bool, enabled: bool, retry: bool)
    ensures ({
        let ms = payment_stack(keys, capacity, allowed, enabled);
        let q = payment_request(k, send, retry);
        let result = calculated(keys, capacity, k, send, allowed, enabled);
        evidence(ms, q).len() == result.1.len()
        && (forall|i: int| 0 <= i < result.1.len() ==>
            code(evidence(ms, q)[i].verdict) == #[trigger] result.1[i])
        && (enabled ==> prepared(ms, q)[0].reserved == model_keys(result.0)
            && prepared(ms, q)[1].reserved == seq![])
        && (!enabled ==> prepared(ms, q) == seq![])
    }),
{
    key_projection(keys, k);
    let ms = payment_stack(keys, capacity, allowed, enabled);
    let q = payment_request(k, send, retry);
    let result = calculated(keys, capacity, k, send, allowed, enabled);
    if enabled {
        assert(ms =~= seq![ms[0], ms[1]]);
        two_handler_evidence(ms[0], ms[1], q);
        if !send {
            assert(evidence(ms, q).len() == 2);
        } else if keys.contains(k) {
            assert(decide(ms[0], q).verdict is Duplicate);
            assert(evidence(ms, q).len() == 1);
        } else if keys.len() >= capacity {
            assert(decide(ms[0], q).verdict is Capacity);
            assert(evidence(ms, q).len() == 1);
        } else {
            assert(decide(ms[0], q).verdict is Allow);
            assert(evidence(ms, q).len() == 2);
        }
    } else {
        reveal_with_fuel(scan, 1);
    }
    assert forall|i: int| 0 <= i < result.1.len()
        implies code(evidence(ms, q)[i].verdict) == #[trigger] result.1[i] by {
        if i == 0 { } else { assert(i == 1); }
    }
    assert(evidence(ms, q).len() == result.1.len());
    if enabled {
        assert(prepared(ms, q)[0].reserved == model_keys(result.0));
        assert(prepared(ms, q)[1].reserved == seq![]);
    } else {
        assert(prepared(ms, q) =~= Seq::<Monitor>::empty());
    }
}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloned<T: Clone>(value: &T) -> T {
        value.clone()
    }

    #[test]
    fn retry_consults_every_policy() {
        let got = prepare(&vec![1], 2, 1, true, true, true, true);
        assert_eq!(got.verdicts, vec![24]);
        assert_eq!(got.reserved, vec![1]);
    }

    #[test]
    fn veto_restores_earlier_reservations() {
        let got = prepare(&vec![1], 2, 2, true, false, true, false);
        assert_eq!(got.verdicts, vec![0, 25]);
        assert_eq!(got.reserved, vec![1]);
    }

    #[test]
    fn forwarded_suspend_retains_reservation() {
        assert_eq!(finish(&vec![], vec![1], false, true, true, 1), (vec![1], 1));
    }

    #[test]
    fn one_terminal_copy_stays_one_copy() {
        assert_eq!(
            finish(&vec![], vec![1], false, false, true, 1),
            (vec![1], 1)
        );
    }

    #[test]
    fn standard_library_contract_samples() {
        assert_eq!(cloned(&usize::MAX), usize::MAX);
        assert_eq!(cloned(&u8::MAX), u8::MAX);
        assert_eq!(cloned(&u64::MAX), u64::MAX);
        assert!(cloned(&true));
        assert_eq!(cloned(&[1u8, 2]), [1u8, 2]);
        assert_eq!(*cloned(&Box::new([1u8, 2])), [1u8, 2]);
        let original = vec![3u64, 5];
        assert_eq!(cloned(&&original), &original);
        assert_eq!(original.as_slice().to_vec(), original);
        let mut copied = original.clone();
        assert_eq!(copied.len(), 2);
        assert_eq!(copied[0], 3);
        copied.push(7);
        assert_eq!(original, vec![3, 5]);
        assert_eq!(copied, vec![3, 5, 7]);
        assert_eq!(
            prepare(&vec![], 0, 1, true, true, true, false).verdicts,
            vec![27]
        );
        assert_eq!(
            prepare(&vec![1], 0, 1, true, true, true, false).verdicts,
            vec![24]
        );
        assert_eq!(
            prepare(&vec![], 1, 1, true, true, true, false).reserved,
            vec![1]
        );
        assert_eq!(
            prepare(&vec![], 1, 1, false, false, true, false).verdicts,
            vec![0, 0]
        );
        assert!(
            prepare(&vec![], 1, 1, true, false, false, false)
                .verdicts
                .is_empty()
        );
        assert_eq!(
            finish(&vec![3], vec![3, 5], false, true, false, 0),
            (vec![3], 0)
        );
        assert_eq!(
            finish(&vec![3], vec![3, 5], true, false, true, 1),
            (vec![3], 0)
        );
    }
}
