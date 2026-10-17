//! Proposer arithmetic evaluated by the existing library, never by a second
//! implementation of multiplication, division or product comparison.

use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

/// Evaluate one arithmetic instruction on integer columns. Its operands index
/// the columns. The result is invocation-owned scratch, with no checking or
/// publication authority. D1 enforces nonnegative full-product operands,
/// zero-denominator and result-overflow refusals.
#[must_use]
pub fn evaluate(op: Op, columns: &[&[i128]]) -> Option<Vec<i128>> {
    if columns.is_empty() || columns.len() > 6 {
        return None;
    }
    let boolean = match op {
        Op::Mul(..)
        | Op::DivFloor(..)
        | Op::DivCeil(..)
        | Op::MulDivFloor(..)
        | Op::MulDivCeil(..) => false,
        Op::ProductsGe(..) => true,
        _ => return None,
    };
    let count = columns[0].len();
    if columns.iter().any(|c| c.len() != count) {
        return None;
    }
    let integer = Domain::Int {
        min: i128::MIN,
        max: i128::MAX,
    };
    let mut nodes = (0..columns.len())
        .map(|k| Some(Op::Input(u16::try_from(k).ok()?)))
        .collect::<Option<Vec<_>>>()?;
    nodes.push(op);
    let program = Program::try_new(
        vec![integer; columns.len()],
        vec![if boolean { Domain::Bool } else { integer }],
        nodes,
        vec![u16::try_from(columns.len()).ok()?],
    )
    .ok()?;
    let mut row = Vec::with_capacity(columns.len());
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        row.clear();
        row.extend(columns.iter().map(|c| c[index]));
        let output = program.evaluate(&row).ok()?;
        result.push(*output.first()?);
    }
    Some(result)
}
