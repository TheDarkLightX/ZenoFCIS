// Source-authored Rust -> optimized wasm32 probe for the pinned Cranelift
// unsigned i64 quotient/product/remainder reconstruction pilot.
// In each mode, the *same* resulting .wasm is given to both compiler binaries.
// All operations are explicit wrapping u64; division by zero traps/panics.
// These are deliberately constructed contexts, not corpus-prevalence evidence.
#[no_mangle]
#[inline(never)]
pub extern "C" fn live_q_m_r(x: u64, y: u64) -> u64 {
    let q = x / y;
    let m = q.wrapping_mul(y);
    let r = x.wrapping_sub(m);
    q ^ m.rotate_left(7) ^ r.rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn live_reversed_mul(x: u64, y: u64) -> u64 {
    let q = x / y;
    let m = y.wrapping_mul(q);
    let r = x.wrapping_sub(m);
    q ^ m.rotate_left(7) ^ r.rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn live_original_rem(x: u64, y: u64) -> u64 {
    let q = x / y;
    let m = q.wrapping_mul(y);
    let r = x % y;
    q ^ m.rotate_left(7) ^ r.rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn live_two_chains(x: u64, y: u64) -> u64 {
    let q = x / y;
    let m = q.wrapping_mul(y);
    let r = x.wrapping_sub(m);
    let x2 = x.wrapping_add(17);
    let q2 = x2 / y;
    let m2 = q2.wrapping_mul(y);
    let r2 = x2.wrapping_sub(m2);
    (q ^ m.rotate_left(7) ^ r.rotate_left(17))
        ^ q2.rotate_left(19) ^ m2.rotate_left(23) ^ r2.rotate_left(29)
}
// Negative control: imul(q, x), not imul(q, y). This is not a remainder.
#[no_mangle]
#[inline(never)]
pub extern "C" fn wrong_multiplier(x: u64, y: u64) -> u64 {
    let q = x / y;
    let m = q.wrapping_mul(x);
    let r = x.wrapping_sub(m);
    q ^ m.rotate_left(7) ^ r.rotate_left(17)
}
