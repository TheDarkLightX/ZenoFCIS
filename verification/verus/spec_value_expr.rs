#![no_std]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
extern crate zeno_fcis_spec as production;
// These are the production crate's actual leaf types. The proof introduces no
// functions or semantic properties for them: constructor contracts preserve the
// opaque operands exactly. The linked native crate is built and digest-bound by
// the proof gate from the same final source revision.
mod ast {
    pub use production::{Identifier, ProjectionPath, RelExpr};
}
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
verus! {
    #[verifier::external_type_specification]
    #[verifier::external_body]
    pub struct ExIdentifier(production::Identifier);
    #[verifier::external_type_specification]
    #[verifier::external_body]
    pub struct ExProjectionPath(production::ProjectionPath);
    #[verifier::external_type_specification]
    #[verifier::external_body]
    pub struct ExRelExpr(production::RelExpr);
}
#[path = "../../crates/zeno-fcis-spec/src/lexical_core.rs"]
mod lexical_core;
#[path = "../../crates/zeno-fcis-spec/src/value_expr.rs"]
mod value_expr;

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[path = "spec_value_expr_tests.rs"]
mod tests;
