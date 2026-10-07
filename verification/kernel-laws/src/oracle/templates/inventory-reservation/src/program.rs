//! The complete finite decision is interpreted by the library from the
//! canonical `program.zcve` and the closed plan in `profile.rs`.
//!
//! The generated Rust transition remains an evidence artifact only. The
//! production authority cannot call it or substitute hand-written staging.

use crate::oracle::authority::finite_decision::FiniteDecisionProgram;
use zeno_fcis_crypto::RustCryptoSha256;

/// Library-owned stock decision evaluator, nominally bound into authority.
pub type StockProgram = FiniteDecisionProgram<RustCryptoSha256>;
