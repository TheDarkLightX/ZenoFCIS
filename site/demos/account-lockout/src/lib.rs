//! The account-lockout example's authority, program, and law checker, run in
//! the browser over the library's in-memory reference shell.
//!
//! Everything the page can ask for is plain Rust in [`demo`], which the host
//! tests exercise without WebAssembly. The pointer handling that the C ABI
//! needs is confined to [`abi`].

pub mod abi;
pub mod demo;
