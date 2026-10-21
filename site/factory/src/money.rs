//! The Spot pilot's exact 12-node swap graph, with explicit bounded inputs.
use super::{Result, checked};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    V2IntegerWidth as Width, V2NumericAtom as Atom, V2NumericDomain as Domain,
    V2NumericFailure as Failure, V2NumericOp as Op, V2NumericProgram as Program, V2Resource,
    execute_numeric_v2, v2_zero_limits,
};
// Identical to verification/arithmetic/graphs.rs::SWAP_NODES. A source-data
// check in factory_check.py guards this transcription independently.
const U: Width = Width::Unsigned128;
const UNSIGNED: Domain<'static> = Domain::U128 {
    min: 0,
    max: u128::MAX,
};
const NODES: [Op; 12] = [
    Op::Input(0),
    Op::Input(1),
    Op::Input(2),
    Op::Input(3),
    Op::U128(10_000),
    Op::U128(1),
    Op::MulDivCeil(0, 1, 4),
    Op::Sub(U, 0, 6),
    Op::Add(U, 2, 7),
    Op::MulDivFloor(3, 7, 8),
    Op::Sub(U, 3, 9),
    Op::ProductsGe([8, 10, 5], [2, 3, 5]),
];
const ROOTS: [u16; 6] = [6, 7, 9, 8, 10, 11];
const OUTPUTS: [Domain<'static>; 6] = [
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    Domain::Bool,
];
fn execute(
    input: [u128; 4],
    wide: bool,
) -> Result<(std::result::Result<Vec<Atom<'static>>, Failure>, u64)> {
    let cap = if wide { u128::MAX } else { 3_000_000_000 };
    let domains = [
        Domain::U128 { min: 0, max: cap },
        Domain::U128 {
            min: 0,
            max: 10_000,
        },
        Domain::U128 { min: 1, max: cap },
        Domain::U128 { min: 1, max: cap },
    ];
    let program = Program {
        inputs: &domains,
        outputs: &OUTPUTS,
        nodes: &NODES,
        roots: &ROOTS,
    };
    checked(program.admit())?;
    let (result, usage) = execute_numeric_v2(
        program,
        &input.map(Atom::U128),
        v2_zero_limits().with_limit(V2Resource::Step, 12),
    )
    .into_parts();
    Ok((result, usage.used(V2Resource::Step)))
}
pub(super) fn run(request: &Value) -> Result<Value> {
    let mut input = [1000, 30, 10_000, 20_000];
    for (index, name) in ["amount", "fee_bps", "reserve_in", "reserve_out"]
        .iter()
        .enumerate()
    {
        if let Some(value) = request.get(name) {
            let text = value
                .as_str()
                .ok_or("money inputs must be decimal strings")?;
            if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
                return Err("enter nonnegative whole numbers".into());
            }
            input[index] = checked(text.parse())?;
        }
    }
    let (result, steps) = execute(input, false)?;
    let values = checked(result)?;
    let outputs = values
        .iter()
        .map(|value| match value {
            Atom::U128(v) => json!(v.to_string()),
            Atom::Bool(v) => json!(v),
            other => json!(format!("{other:?}")),
        })
        .collect::<Vec<_>>();
    // The safe pilot bounds cannot overflow this graph. The separate probe
    // declares wider U128 input ranges and keeps the identical 12 nodes.
    let (overflow, refused_steps) = execute([1, 0, u128::MAX, 1], true)?;
    if overflow != Err(Failure::Arithmetic) {
        return Err("overflow probe was not refused".into());
    }
    Ok(
        json!({"act":"money","nodes":NODES.len(),"input":input.map(|v|v.to_string()),"outputs":outputs,"steps":steps,
        "overflow":{"input":["1","0",u128::MAX.to_string(),"1"],"refusal":"Arithmetic","steps":refused_steps,"node":8},
        "scope":"one supplied swap tuple and one wider-domain overflow probe"}),
    )
}
