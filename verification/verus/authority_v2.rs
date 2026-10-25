//! Actual source canonical authority closure; registered modules are included whole.
#![no_std]
#![forbid(unsafe_code)]
#![allow(dead_code)]
#![cfg_attr(verus_keep_ghost, feature(proc_macro_hygiene))]
extern crate alloc;
extern crate self as zeno_fcis_core;
#[path = "../../crates/zeno-fcis-core/src/resource.rs"]
pub mod resource;
pub use resource::Resource;
#[cfg(test)]
extern crate std;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/canonical_v2/mod.rs"]
pub mod canonical_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/evaluation/mod.rs"]
pub mod evaluation;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/execution_v2/mod.rs"]
pub mod execution_v2;
#[path = "../../crates/zeno-fcis-synthesis/src/finite/value/mod.rs"]
mod value;

// The exact same external public-API tests also compile under Cargo.
#[cfg(test)]
extern crate self as zeno_fcis_synthesis;
pub mod finite {
    pub(crate) use crate::value;
    pub use crate::evaluation::admission::AdmissionFailure as V2NumericAdmissionFailure;
    pub use crate::evaluation::typed::NumericOp as V2NumericOp;
    pub use crate::evaluation::{Domain, Op};
    pub use crate::execution_v2::numeric::{
        Failure as V2NumericFailure, MAX_NUMERIC_INPUTS, MAX_NUMERIC_NODES, MAX_NUMERIC_OUTPUTS,
        NumericProgram as V2NumericProgram, execute as execute_numeric_v2,
    };
    #[cfg(test)]
    pub use crate::execution_v2::order_fixture_transition as v2_order_fixture_transition;
    pub use crate::execution_v2::{
        InputField as V2InputField, InputLeaf as V2InputLeaf, Limits as V2Limits,
        MeterFailure as V2MeterFailure, Resource as V2Resource, ScalarProgram as V2ScalarProgram,
        authority as v2_authority, catalog as v2_catalog, composition as v2_composition,
        laws as v2_laws, zero_limits as v2_zero_limits,
    };
    pub use crate::value::{
        Atom as V2NumericAtom, Domain as V2NumericDomain, IntegerWidth as V2IntegerWidth,
    };
    pub use crate::{canonical_v2, evaluation, execution_v2};
}
#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_authority.rs"]
mod public_authority;

#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_numeric_execution.rs"]
mod public_numeric;

#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_numeric_capacity.rs"]
mod public_numeric_capacity;

#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_numeric_wide_arithmetic.rs"]
mod public_numeric_wide_arithmetic;

#[cfg(test)]
#[path = "../../crates/zeno-fcis-synthesis/tests/v2_original_compound.rs"]
mod public_original_compound;
