//! Runs the self-contained Mini Determinator semantic reference.

use zeno_fcis_spec::{
    MiniBudget, MiniDecision, MiniDeterminator, MiniState, WorkerInstruction, WorkerProgram,
    WorkspaceCell,
};

fn worker(id: u32, output: u32, operation: WorkerInstruction) -> WorkerProgram {
    WorkerProgram::new(
        id,
        vec![1],
        vec![output],
        vec![
            WorkerInstruction::Get(1),
            operation,
            WorkerInstruction::Put(output),
            WorkerInstruction::Return,
        ],
    )
}

fn main() {
    let pre = MiniState::try_new(vec![WorkspaceCell::new(1, 10)])
        .unwrap_or_else(|| unreachable!("example state has unique slots"));
    let programs = vec![
        worker(2, 3, WorkerInstruction::Multiply(2)),
        worker(1, 2, WorkerInstruction::Add(5)),
    ];
    let first = MiniDeterminator::execute_programs(&pre, &programs, &[2, 1], MiniBudget::default());
    let second =
        MiniDeterminator::execute_programs(&pre, &programs, &[1, 2], MiniBudget::default());
    assert_eq!(
        first, second,
        "completion order changed the semantic result"
    );

    match first.decision() {
        MiniDecision::Accepted { state, returns } => {
            println!("accepted");
            for cell in state.cells() {
                println!("slot {} = {}", cell.slot(), cell.value());
            }
            for (worker, value) in returns {
                println!("worker {worker} returned {value}");
            }
        }
        MiniDecision::Rejected(conflict) => {
            println!(
                "rejected: slot {} conflicts between workers {} and {}",
                conflict.slot(),
                conflict.first_worker(),
                conflict.second_worker()
            );
        }
        MiniDecision::Blocked(reason) => {
            println!("blocked: {reason:?}");
        }
    }
}
