//! Native checks of production typed operations, with no replacement value types.
#![allow(dead_code)]
extern crate alloc;
extern crate self as zeno_fcis_core;
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
pub(crate) mod value;
pub mod finite {
    pub(crate) use crate::value;
    pub use crate::{canonical_v2, evaluation, execution_v2};
}

use evaluation::typed::{Failure, Instruction, Values, node};
use value::{Atom, IntegerWidth};

fn main() {
    let input = Values::Typed(&[]);
    let fail = Err(Failure::Arithmetic);
    assert_eq!(
        node(
            Instruction::Cast(true, 0),
            input,
            Values::Typed(&[Atom::U128(u128::MAX)])
        ),
        fail,
        "checked_cast_refuses_truncation"
    );
    assert_eq!(
        node(
            Instruction::Cast(false, 0),
            input,
            Values::Typed(&[Atom::I128(-1)])
        ),
        fail
    );
    assert_eq!(
        node(
            Instruction::Cast(true, 0),
            input,
            Values::Typed(&[Atom::U128(7)])
        ),
        Ok(Atom::I128(7))
    );
    assert_eq!(
        node(
            Instruction::Cast(false, 0),
            input,
            Values::Typed(&[Atom::I128(7)])
        ),
        Ok(Atom::U128(7))
    );
    assert_eq!(
        node(
            Instruction::ScalarDiv(IntegerWidth::Unsigned128, true, 0, 1),
            input,
            Values::Typed(&[Atom::U128(5), Atom::U128(2)])
        ),
        Ok(Atom::U128(3))
    );
    assert_eq!(
        node(
            Instruction::ScalarMul(IntegerWidth::Unsigned128, 0, 1),
            input,
            Values::Typed(&[Atom::U128(u128::MAX), Atom::U128(2)])
        ),
        fail
    );
    assert_eq!(
        node(
            Instruction::ScalarMul(IntegerWidth::Signed64, 0, 1),
            input,
            Values::Typed(&[Atom::I128(i64::MAX as i128), Atom::I128(2)])
        ),
        fail
    );
    println!("typed_native_witnesses passed");
}
