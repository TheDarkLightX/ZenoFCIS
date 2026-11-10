// Experiment-authored Rust udiv/urem lowering probe; Rust 1.97.1 -> wasm32.
// Same resulting Wasm is passed unchanged to both pinned Wasmtime compilers.
// An ordinary Rust source expression does not guarantee any particular CLIF.
#[no_mangle]
#[inline(never)]
pub extern "C" fn divrem_pair(x: u64, y: u64) -> u64 {
    let q = x / y;
    let r = x % y;
    q ^ r.rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn divrem_offset(x: u64, y: u64) -> u64 {
    let q = x / y;
    let r = x % y;
    q.wrapping_add(3) ^ r.rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn divrem_reverse(x: u64, y: u64) -> u64 {
    let r = x % y;
    let q = x / y;
    r.rotate_left(17) ^ q
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn divrem_swapped(x: u64, y: u64) -> u64 {
    (x / y) ^ (y % x).rotate_left(17)
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn div_only(x: u64, y: u64) -> u64 {
    x / y
}
#[no_mangle]
#[inline(never)]
pub extern "C" fn signed_divrem(x: i64, y: i64) -> i64 {
    let q = x / y;
    let r = x % y;
    q ^ r.rotate_left(17)
}
