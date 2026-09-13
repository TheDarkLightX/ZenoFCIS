//! Verify finite exits, then prepare an ordered operation in bounded chunks.
//!
//! Run with `--features synthesis`. The printed result is computation evidence;
//! application laws, authorization, and atomic publication remain separate.

#[cfg(feature = "synthesis")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use zeno_fcis::prelude::*;
    use zeno_fcis::synthesis::finite::{Domain as FiniteDomain, Op, Program};

    let state = FiniteDomain::Int { min: 0, max: 3 };
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
    let proposed = find_completion(&problem)?;
    // The consumer supplies `problem`; the proposed plan cannot choose it.
    let checked = verify_completion(&problem, &proposed)?;
    if checked.next_command(&[3])? != Some([1].as_slice()) {
        return Err("unexpected completion policy".into());
    }

    let accumulator = FiniteDomain::Int {
        min: -100,
        max: 100,
    };
    let item = FiniteDomain::Int { min: -3, max: 3 };
    let program = Program::try_new(
        vec![accumulator, item],
        vec![accumulator],
        vec![Op::Input(0), Op::Input(1), Op::Add(0, 1)],
        vec![2],
    )?;
    // Visible example bindings. A real application supplies the exact trusted
    // root/version and complete admitted invocation/context/profile commitment.
    let current = PreparationContext {
        state_root: Hash32::new([1; 32]),
        state_version: 0,
        invocation_hash: Hash32::new([2; 32]),
    };
    let limits = BudgetLimits::zero()
        .with_limit(Resource::Read, 6)
        .with_limit(Resource::Write, 3)
        .with_limit(Resource::Candidate, 3)
        .with_limit(Resource::Byte, 1_000);
    let mut prepared = PreparedFold::start(
        program,
        vec![0],
        vec![vec![1], vec![2], vec![3]],
        current,
        PreparationLimits {
            max_chunk_items: 2,
            ..PreparationLimits::default()
        },
        limits,
    )?;
    prepared.advance(0, 2)?;
    if prepared.finish(current) != Err(PreparationError::Incomplete) {
        return Err("partial preparation exposed a result".into());
    }
    prepared.advance(2, 1)?;
    let output = prepared.finish(current)?;
    if output != [6] {
        return Err("prepared result differs from the whole operation".into());
    }
    println!(
        "{{\"status\":\"passed\",\"states_checked\":{},\"maximum_exit_steps\":3,\"items_prepared\":{},\"result\":{:?},\"publication_authority\":false}}",
        problem.state_count(),
        prepared.processed_items(),
        output
    );
    Ok(())
}

#[cfg(not(feature = "synthesis"))]
fn main() {}
