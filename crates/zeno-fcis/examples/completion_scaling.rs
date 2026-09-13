//! Reproducible finite-chain sizes; elapsed times are local measurements only.

#[cfg(feature = "synthesis")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;
    use zeno_fcis::prelude::*;
    use zeno_fcis::synthesis::finite::{Domain as FiniteDomain, Op, Program};

    for states in [8_i64, 64, 512, 4096] {
        let state = FiniteDomain::Int {
            min: 0,
            max: states - 1,
        };
        let step = Program::try_new(
            vec![state, FiniteDomain::Bool],
            vec![FiniteDomain::Bool, state],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Int(0),
                Op::Int(1),
                Op::Lt(2, 0),
                Op::And(1, 4),
                Op::Sub(0, 3),
                Op::Select(5, 6, 0),
            ],
            vec![5, 7],
        )?;
        let terminal = Program::try_new(
            vec![state],
            vec![FiniteDomain::Bool],
            vec![Op::Input(0), Op::Int(0), Op::Eq(0, 1)],
            vec![2],
        )?;
        let problem = CompletionProblem::try_new(step, terminal, CompletionLimits::default())?;
        let start = Instant::now();
        let proposed = find_completion(&problem)?;
        let search_micros = start.elapsed().as_micros();
        let start = Instant::now();
        let verified = verify_completion(&problem, &proposed)?;
        let bytes = verified.canonical_bytes()?;
        let imported = verify_completion_bytes(&problem, &bytes)?;
        let verify_and_import_micros = start.elapsed().as_micros();
        if imported.plan() != verified.plan() || imported.next_command(&[states - 1])? != Some(&[1])
        {
            return Err("finite chain result changed".into());
        }
        println!(
            "{{\"states\":{states},\"state_command_pairs\":{},\"plan_bytes\":{},\"search_micros\":{search_micros},\"verify_and_import_micros\":{verify_and_import_micros},\"authority\":\"none\"}}",
            states * 2,
            bytes.len()
        );
    }
    Ok(())
}

#[cfg(not(feature = "synthesis"))]
fn main() {}
