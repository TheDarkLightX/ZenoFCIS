//! Small deterministic search with a separately replayed, complete output table.
use super::{Result, checked};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::program_bytes;
use zeno_fcis_program_search::{self as search, Settings};
use zeno_fcis_synthesis::{
    finite::{Program, V2Resource, v2_zero_limits},
    finite_runtime::import_program,
};
const ORIGINAL: &[u8] = include_bytes!(
    "../../../crates/zeno-fcis-cli/tests/fixtures/i128-programs/boolean-kernel-original.zcve"
);
const SEED: &[u8] = include_bytes!(
    "../../../crates/zeno-fcis-cli/tests/fixtures/i128-programs/boolean-kernel-candidate.zcve"
);
fn output(program: &Program, input: &[i128]) -> Result<Vec<i128>> {
    checked(
        program
            .execute_v2(input, v2_zero_limits().with_limit(V2Resource::Step, 64))
            .into_parts()
            .0,
    )
}
pub(super) fn run() -> Result<Value> {
    let original = checked(import_program(ORIGINAL))?;
    let mut roots = original.roots().to_vec();
    roots.swap(0, 1);
    let swapped = checked(Program::try_new(
        original.inputs().to_vec(),
        original.outputs().to_vec(),
        original.nodes().to_vec(),
        roots,
    ))?;
    let swapped_bytes = checked(program_bytes(&swapped))?;
    let mut settings = Settings::default();
    settings.budget.max_tuples = 16;
    settings.budget.max_cells = 4096;
    settings.budget.work_units = 2_000_000;
    settings.budget.rounds = 3;
    let found = search::search(ORIGINAL, &[&swapped_bytes, SEED], &settings);
    let search::Outcome::Improved(candidate) = found else {
        return Err(format!("search stopped: {found:?}"));
    };
    let selected = checked(import_program(&candidate.bytes))?;
    let mut first_difference = None;
    let mut table = Vec::new();
    for ordinal in 0..16 {
        let input = (0..4)
            .rev()
            .map(|bit| i128::from((ordinal >> bit) & 1))
            .collect::<Vec<_>>();
        let before = output(&original, &input)?;
        let wrong = output(&swapped, &input)?;
        if first_difference.is_none() && before != wrong {
            first_difference =
                Some(json!({"ordinal":ordinal,"input":input,"original":before,"swapped":wrong}));
        }
        let after = output(&selected, &input)?;
        if before != after {
            return Err("independent full-domain replay failed".into());
        }
        table.push(json!({"input":input,"output":after}));
    }
    if !candidate
        .provenance
        .refused
        .iter()
        .any(|r| r.proposer == "seed" && r.refusal == search::Refusal::Differs { ordinal: 1 })
    {
        return Err("search did not refuse the swapped seed at ordinal 1".into());
    }
    Ok(
        json!({"act":"search","domain":16,"passed":table.len(),"before_nodes":original.nodes().len(),"after_nodes":selected.nodes().len(),
        "first_difference":first_difference,"winner":candidate.provenance.winner,"work":candidate.provenance.work_used,
        "table":table,"nodes":selected.nodes().iter().enumerate().map(|(i,op)|format!("{i}: {op:?}")).collect::<Vec<_>>(),"roots":selected.roots()}),
    )
}
