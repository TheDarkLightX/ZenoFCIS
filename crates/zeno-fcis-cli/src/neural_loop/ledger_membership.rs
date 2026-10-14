//! Exact linear membership filter for u8 attempt IDs.
//! No bounds on history length, order or duplicate multiplicity are assumed.
//! Actual-body annotations are an unverified candidate until the pinned gate
//! and independent inventory review pass. Services are not allocator or time bounds.

extern crate alloc;
use self::alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
/// Mathematical ordered filter: retain each prefix element, in order, exactly
/// when its ID occurs nowhere in settled. This is not executable table code.
pub(super) open spec fn retained(reserved: Seq<u8>, settled: Seq<u8>, n: int) -> Seq<u8>
    recommends 0 <= n <= reserved.len(),
    decreases n,
{
    if n <= 0 { Seq::empty() }
    else {
        let previous = retained(reserved, settled, n - 1);
        if settled.contains(reserved[n - 1]) { previous }
        else { previous.push(reserved[n - 1]) }
    }
}

spec fn seen(settled: Seq<u8>, n: int, id: u8) -> bool {
    exists|j: int| 0 <= j < n && settled[j] == id
}

proof fn seen_step(settled: Seq<u8>, n: int, id: u8)
    requires 0 <= n < settled.len(),
    ensures seen(settled, n + 1, id) == (seen(settled, n, id) || settled[n] == id),
{
    if seen(settled, n + 1, id) {
        let j = choose|j: int| 0 <= j < n + 1 && settled[j] == id;
        if j < n { assert(seen(settled, n, id)); }
        else { assert(j == n); }
    }
    if seen(settled, n, id) {
        let j = choose|j: int| 0 <= j < n && settled[j] == id;
        assert(0 <= j < n + 1 && settled[j] == id);
        assert(seen(settled, n + 1, id));
    }
    if settled[n] == id { assert(seen(settled, n + 1, id)); }
}

/// Named source-service total, not a count of compiled instructions. Per
/// settled iteration: guard3 + slice/table accesses2 + casts1 + index checks2
/// + increment/assignment2. Per reserved iteration: guard3 + read/bind2 +
/// slice/table checks2 + cast1 + table read1 + negation/branch2 + increment2.
/// Fixed271 and each push are charged at their actual body sites below.
pub(super) open spec fn source_services(s: int, r: int, u: int) -> int {
    271 + 10 * s + 13 * r + u
}

pub(super) proof fn service_bound(s: int, r: int, u: int)
    requires 0 <= s, 0 <= u <= r,
    ensures source_services(s, r, u) + r <= 271 + 10 * s + 15 * r,
{
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == retained(reserved@, settled@, reserved@.len() as int),
        result@.len() <= reserved@.len(),
))]
pub(super) fn unresolved_attempts(reserved: &[u8], settled: &[u8]) -> Vec<u8> {
    // Function entry1, array storage request1, Boolean stores256, binding1.
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost mut services: int = 259; }
    let mut present = [false; 256];
    let mut index = 0;
    #[cfg(verus_keep_ghost)]
    proof! { services = services + 1; }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= settled.len(),
            forall|id: u8| present@[id as int] == seen(settled@, index as int, id),
            services == 260 + 10 * (index as int),
        decreases settled.len() - index,
    ))]
    while index < settled.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost before = present@; let ghost old_index = index as int; }
        // True loop guard: len/comparison/branch3; read1/cast1/checks2/store1.
        #[cfg(verus_keep_ghost)]
        proof! { services = services + 8; }
        present[settled[index] as usize] = true;
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|id: u8|
                present@[id as int] == seen(settled@, old_index + 1, id) by {
                seen_step(settled@, old_index, id);
                if id == settled@[old_index] {
                    assert(present@[id as int]);
                } else {
                    assert(id as int != settled@[old_index] as int);
                    assert(present@[id as int] == before[id as int]);
                }
            }
        }
        index += 1;
        #[cfg(verus_keep_ghost)]
        proof! { services = services + 2; }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        services = services + 3; // False loop guard.
        assert forall|id: u8|
            present@[id as int] == settled@.contains(id) by {
            assert(seen(settled@, settled@.len() as int, id)
                == settled@.contains(id));
        }
    }
    // Length read1, allocation request1, binding1. Requested payload is R;
    // actual allocator capacity/latency/abort behavior are outside this model.
    let mut unresolved = Vec::with_capacity(reserved.len());
    #[cfg(verus_keep_ghost)]
    proof_decl! { let ghost requested_bytes = reserved@.len() as int; }
    #[cfg(verus_keep_ghost)]
    proof! { services = services + 3; }
    let mut index = 0;
    #[cfg(verus_keep_ghost)]
    proof! { services = services + 1; }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= reserved.len(),
            forall|id: u8| present@[id as int] == settled@.contains(id),
            unresolved@ == retained(reserved@, settled@, index as int),
            unresolved.len() <= index,
            requested_bytes == reserved@.len(),
            services == 267 + 10 * settled@.len() + 13 * (index as int) + unresolved@.len(),
        decreases reserved.len() - index,
    ))]
    while index < reserved.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! { let ghost old_index = index as int; let ghost before = unresolved@; }
        // True guard3, slice read/check/binding3.
        #[cfg(verus_keep_ghost)]
        proof! { services = services + 6; }
        let attempt = reserved[index];
        // Cast1, table check/read2, negation/branch2, on either branch.
        #[cfg(verus_keep_ghost)]
        proof! { services = services + 5; }
        if !present[attempt as usize] {
            unresolved.push(attempt);
            #[cfg(verus_keep_ghost)]
            proof! { services = services + 1; }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal_with_fuel(retained, 1);
            assert(unresolved@ == retained(reserved@, settled@, old_index + 1));
        }
        index += 1;
        #[cfg(verus_keep_ghost)]
        proof! { services = services + 2; }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        services = services + 4; // False guard3 and return/move1.
        assert(services == source_services(settled@.len() as int,
            reserved@.len() as int, unresolved@.len() as int));
        service_bound(settled@.len() as int, reserved@.len() as int,
            unresolved@.len() as int);
        assert(services + requested_bytes
            <= 271 + 10 * settled@.len() + 15 * reserved@.len());
    }
    unresolved
}
