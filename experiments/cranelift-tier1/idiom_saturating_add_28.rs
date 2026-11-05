// Pure exported scalar operations to test what an idiomatic Rust compiler
// actually lowers to Wasm. Not an external application, benchmark, or claim
// that LLVM/Wasm preserves a given human-written source pattern.
//
// Compile with pinned Rust 1.97.1 on wasm32-unknown-unknown, opt 0 and 3.
// The same generated .wasm artifact is fed to both Wasmtime compilers.
#[no_mangle]
pub extern "C" fn sat_add(x: i64, y: i64) -> i64 {
    x.saturating_add(y)
}

#[no_mangle]
pub extern "C" fn sat_add_xor(x: i64, y: i64) -> i64 {
    x.saturating_add(y) ^ 1
}

#[no_mangle]
pub extern "C" fn sat_add_mix(x: i64, y: i64) -> i64 {
    x.saturating_add(y).wrapping_add((x as u64).wrapping_shr(7) as i64)
}

#[no_mangle]
pub extern "C" fn sign_clamp(x: i64) -> i64 {
    if x < 0 { i64::MIN } else { i64::MAX }
}

#[no_mangle]
pub extern "C" fn wrap_add(x: i64, y: i64) -> i64 {
    x.wrapping_add(y)
}
