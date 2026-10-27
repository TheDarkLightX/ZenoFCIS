//! Actual scalar node encoder, production terms and existing runtime owners.
//! Node construction is one dependency of full runtime/query correspondence.
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]

extern crate alloc;
extern crate self as zeno_fcis_core;
extern crate self as zeno_fcis_synthesis;

#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/mod.rs"]
mod value;

#[path = "scalar_owned_ir.rs"]
mod owned_ir;

pub mod finite {
    pub(crate) use crate::value;
    pub use crate::owned_ir::Program;
    pub use crate::evaluation::{Domain, Op};
    pub use crate::{canonical_v2, evaluation, execution_v2};
}

#[path = "../../crates/zeno-fcis-cli/src/symbolic/smt/term.rs"]
mod term;
#[path = "../../crates/zeno-fcis-cli/src/symbolic/smt/script.rs"]
mod script;
pub mod symbolic {
    pub mod smt {
        pub(crate) use crate::term::{Sort, Term};
        pub(crate) use crate::term::names::indexed_name;
        pub(crate) use crate::script::{Script, Values};
    }
    pub(crate) use crate::unsupported::Unsupported;
    pub mod program {
        pub(crate) use crate::encoding::{Program, Encoded, encode, input_values};
        #[cfg(verus_keep_ghost)]
        pub(crate) use crate::bridge;
    }
}
#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/node.rs"]
mod node;

#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/nodes.rs"]
mod nodes;

#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/roots.rs"]
mod roots;

#[cfg(verus_keep_ghost)]
#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/bridge_spec.rs"]
pub(crate) mod bridge;
#[path = "../../crates/zeno-fcis-cli/src/symbolic/unsupported.rs"]
mod unsupported;
#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/encoding.rs"]
mod encoding;
#[path = "../../crates/zeno-fcis-cli/src/symbolic/equivalence/queries.rs"]
mod queries;

#[path = "../../crates/zeno-fcis-cli/src/symbolic/program/library.rs"]
mod library;
