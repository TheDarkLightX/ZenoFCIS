//! Self-contained shared-nothing deterministic coordination model.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

/// One scalar cell in the coordinator's canonical workspace.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceCell {
    slot: u32,
    value: i128,
}
impl WorkspaceCell {
    /// Creates a cell.
    #[must_use]
    pub const fn new(slot: u32, value: i128) -> Self {
        Self { slot, value }
    }
    /// Returns the slot identifier.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }
    /// Returns the value.
    #[must_use]
    pub const fn value(self) -> i128 {
        self.value
    }
}

/// Canonically sorted global workspace state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiniState {
    cells: Box<[WorkspaceCell]>,
}
impl MiniState {
    /// Creates a state and rejects duplicate slots.
    pub fn try_new(mut cells: Vec<WorkspaceCell>) -> Option<Self> {
        cells.sort_unstable();
        if cells.windows(2).any(|pair| pair[0].slot == pair[1].slot) {
            None
        } else {
            Some(Self {
                cells: cells.into_boxed_slice(),
            })
        }
    }
    /// Returns canonical cells.
    #[must_use]
    pub const fn cells(&self) -> &[WorkspaceCell] {
        &self.cells
    }
    /// Reads one cell.
    #[must_use]
    pub fn get(&self, slot: u32) -> Option<i128> {
        self.cells
            .binary_search_by_key(&slot, |cell| cell.slot)
            .ok()
            .map(|index| self.cells[index].value)
    }
}

/// Private worker result plus its explicit complete write footprint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateWork {
    worker: u32,
    write_footprint: Box<[u32]>,
    writes: Box<[WorkspaceCell]>,
    returned: i128,
}
impl PrivateWork {
    /// Creates a private result. Validation occurs at canonical join.
    #[must_use]
    pub fn new(
        worker: u32,
        write_footprint: Vec<u32>,
        writes: Vec<WorkspaceCell>,
        returned: i128,
    ) -> Self {
        Self {
            worker,
            write_footprint: write_footprint.into_boxed_slice(),
            writes: writes.into_boxed_slice(),
            returned,
        }
    }
    /// Returns the worker ID.
    #[must_use]
    pub const fn worker(&self) -> u32 {
        self.worker
    }
    /// Returns the declared complete write footprint.
    #[must_use]
    pub const fn write_footprint(&self) -> &[u32] {
        &self.write_footprint
    }
    /// Returns proposed private writes.
    #[must_use]
    pub const fn writes(&self) -> &[WorkspaceCell] {
        &self.writes
    }
    /// Returns the get/put-style synchronization result.
    #[must_use]
    pub const fn returned(&self) -> i128 {
        self.returned
    }
}

/// One bounded instruction executed inside an isolated worker workspace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerInstruction {
    /// Replace the worker accumulator with a literal.
    Constant(i128),
    /// Get a slot from the private overlay or immutable spawn snapshot.
    Get(u32),
    /// Add a literal to the accumulator with checked arithmetic.
    Add(i128),
    /// Subtract a literal from the accumulator with checked arithmetic.
    Subtract(i128),
    /// Multiply the accumulator by a literal with checked arithmetic.
    Multiply(i128),
    /// Put the accumulator into a private workspace slot.
    Put(u32),
    /// Return the accumulator to the coordinator and terminate the worker.
    Return,
}

/// A finite worker program with explicit complete read and write footprints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerProgram {
    worker: u32,
    read_footprint: Box<[u32]>,
    write_footprint: Box<[u32]>,
    instructions: Box<[WorkerInstruction]>,
}
impl WorkerProgram {
    /// Creates a worker program. Footprint completeness is checked on execution.
    #[must_use]
    pub fn new(
        worker: u32,
        read_footprint: Vec<u32>,
        write_footprint: Vec<u32>,
        instructions: Vec<WorkerInstruction>,
    ) -> Self {
        Self {
            worker,
            read_footprint: read_footprint.into_boxed_slice(),
            write_footprint: write_footprint.into_boxed_slice(),
            instructions: instructions.into_boxed_slice(),
        }
    }
    /// Returns the stable worker identifier.
    #[must_use]
    pub const fn worker(&self) -> u32 {
        self.worker
    }
    /// Returns the declared conservative read footprint.
    #[must_use]
    pub const fn read_footprint(&self) -> &[u32] {
        &self.read_footprint
    }
    /// Returns the declared complete write footprint.
    #[must_use]
    pub const fn write_footprint(&self) -> &[u32] {
        &self.write_footprint
    }
    /// Returns the finite instruction sequence.
    #[must_use]
    pub const fn instructions(&self) -> &[WorkerInstruction] {
        &self.instructions
    }
}

/// Canonical replay record for one isolated worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerTrace {
    worker: u32,
    reads: Box<[WorkspaceCell]>,
    writes: Box<[WorkspaceCell]>,
    returned: i128,
}
impl WorkerTrace {
    /// Returns the worker identifier.
    #[must_use]
    pub const fn worker(&self) -> u32 {
        self.worker
    }
    /// Returns reads in program order.
    #[must_use]
    pub const fn reads(&self) -> &[WorkspaceCell] {
        &self.reads
    }
    /// Returns final private writes in stable slot order.
    #[must_use]
    pub const fn writes(&self) -> &[WorkspaceCell] {
        &self.writes
    }
    /// Returns the worker result.
    #[must_use]
    pub const fn returned(&self) -> i128 {
        self.returned
    }
}

/// Coordinator command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MiniCommand {
    /// Spawn private workspaces and join their returned deltas canonically.
    Execute(Vec<PrivateWork>),
}

/// Deterministic resource envelope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MiniBudget {
    max_workers: usize,
    max_writes: usize,
    max_steps_per_worker: usize,
}
impl MiniBudget {
    /// Creates nonzero bounds.
    pub const fn try_new(max_workers: usize, max_writes: usize) -> Option<Self> {
        if max_workers == 0 || max_writes == 0 {
            None
        } else {
            Some(Self {
                max_workers,
                max_writes,
                max_steps_per_worker: 4096,
            })
        }
    }
    /// Creates nonzero worker, write, and per-worker step bounds.
    pub const fn try_with_steps(
        max_workers: usize,
        max_writes: usize,
        max_steps_per_worker: usize,
    ) -> Option<Self> {
        if max_workers == 0 || max_writes == 0 || max_steps_per_worker == 0 {
            None
        } else {
            Some(Self {
                max_workers,
                max_writes,
                max_steps_per_worker,
            })
        }
    }
    /// Returns the worker bound.
    #[must_use]
    pub const fn max_workers(self) -> usize {
        self.max_workers
    }
    /// Returns the total-write bound.
    #[must_use]
    pub const fn max_writes(self) -> usize {
        self.max_writes
    }
    /// Returns the finite instruction bound for each worker.
    #[must_use]
    pub const fn max_steps_per_worker(self) -> usize {
        self.max_steps_per_worker
    }
}
impl Default for MiniBudget {
    fn default() -> Self {
        Self {
            max_workers: 64,
            max_writes: 4096,
            max_steps_per_worker: 4096,
        }
    }
}

/// Stable write/write conflict witness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MergeConflict {
    slot: u32,
    first_worker: u32,
    second_worker: u32,
}
impl MergeConflict {
    /// Returns the conflicting slot.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }
    /// Returns the lower canonical worker ID.
    #[must_use]
    pub const fn first_worker(self) -> u32 {
        self.first_worker
    }
    /// Returns the higher canonical worker ID.
    #[must_use]
    pub const fn second_worker(self) -> u32 {
        self.second_worker
    }
}

/// Why execution remains blocked without an authoritative transition.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiniBlocker {
    DuplicateWorker,
    DuplicatePrivateWrite,
    MissingFootprintEvidence,
    WorkerBudget,
    WriteBudget,
    StepBudget,
    DuplicateReadFootprint,
    InvalidCompletionOrder,
    MissingReadFootprintEvidence,
    MissingCell,
    ArithmeticOverflow,
    MissingReturn,
    InstructionAfterReturn,
}

/// Complete pure Mini Determinator decision.
#[allow(missing_docs)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MiniDecision {
    /// Canonical disjoint merge and worker returns.
    Accepted {
        state: MiniState,
        returns: Box<[(u32, i128)]>,
    },
    /// Stable conflict rejection. The caller retains its exact pre-state.
    Rejected(MergeConflict),
    /// Missing evidence or resource exhaustion. The caller retains its exact pre-state.
    Blocked(MiniBlocker),
}

/// Complete schedule-independent result plus canonical worker replay traces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiniRun {
    decision: MiniDecision,
    traces: Box<[WorkerTrace]>,
}
impl MiniRun {
    /// Returns the pure coordinator decision.
    #[must_use]
    pub const fn decision(&self) -> &MiniDecision {
        &self.decision
    }
    /// Returns worker traces sorted by stable worker identifier.
    #[must_use]
    pub const fn traces(&self) -> &[WorkerTrace] {
        &self.traces
    }
}

/// Stateless pure coordinator and canonical join implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct MiniDeterminator;
impl MiniDeterminator {
    fn blocked(reason: MiniBlocker) -> MiniRun {
        MiniRun {
            decision: MiniDecision::Blocked(reason),
            traces: Box::new([]),
        }
    }

    /// Spawns finite isolated worker programs, then performs one canonical join.
    ///
    /// `completion_order` models arbitrary physical completion order. It must be
    /// an exact permutation of program worker IDs and cannot affect the result.
    #[must_use]
    pub fn execute_programs(
        pre: &MiniState,
        programs: &[WorkerProgram],
        completion_order: &[u32],
        budget: MiniBudget,
    ) -> MiniRun {
        if programs.len() > budget.max_workers {
            return Self::blocked(MiniBlocker::WorkerBudget);
        }
        let mut programs = programs.to_vec();
        programs.sort_by_key(WorkerProgram::worker);
        if programs
            .windows(2)
            .any(|pair| pair[0].worker == pair[1].worker)
        {
            return Self::blocked(MiniBlocker::DuplicateWorker);
        }
        let expected: BTreeSet<u32> = programs.iter().map(WorkerProgram::worker).collect();
        let actual: BTreeSet<u32> = completion_order.iter().copied().collect();
        if completion_order.len() != programs.len()
            || actual.len() != completion_order.len()
            || actual != expected
        {
            return Self::blocked(MiniBlocker::InvalidCompletionOrder);
        }

        let mut private_results = Vec::with_capacity(programs.len());
        let mut traces = Vec::with_capacity(programs.len());
        for program in &programs {
            if program.instructions.len() > budget.max_steps_per_worker {
                return Self::blocked(MiniBlocker::StepBudget);
            }
            let reads: BTreeSet<u32> = program.read_footprint.iter().copied().collect();
            if reads.len() != program.read_footprint.len() {
                return Self::blocked(MiniBlocker::DuplicateReadFootprint);
            }
            let writes: BTreeSet<u32> = program.write_footprint.iter().copied().collect();
            if writes.len() != program.write_footprint.len() {
                return Self::blocked(MiniBlocker::MissingFootprintEvidence);
            }
            let mut accumulator = 0_i128;
            let mut private = BTreeMap::<u32, i128>::new();
            let mut read_trace = Vec::new();
            let mut returned = None;
            for instruction in &program.instructions {
                if returned.is_some() {
                    return Self::blocked(MiniBlocker::InstructionAfterReturn);
                }
                match *instruction {
                    WorkerInstruction::Constant(value) => accumulator = value,
                    WorkerInstruction::Get(slot) => {
                        if !reads.contains(&slot) {
                            return Self::blocked(MiniBlocker::MissingReadFootprintEvidence);
                        }
                        let Some(value) = private.get(&slot).copied().or_else(|| pre.get(slot))
                        else {
                            return Self::blocked(MiniBlocker::MissingCell);
                        };
                        accumulator = value;
                        read_trace.push(WorkspaceCell::new(slot, value));
                    }
                    WorkerInstruction::Add(value) => {
                        let Some(next) = accumulator.checked_add(value) else {
                            return Self::blocked(MiniBlocker::ArithmeticOverflow);
                        };
                        accumulator = next;
                    }
                    WorkerInstruction::Subtract(value) => {
                        let Some(next) = accumulator.checked_sub(value) else {
                            return Self::blocked(MiniBlocker::ArithmeticOverflow);
                        };
                        accumulator = next;
                    }
                    WorkerInstruction::Multiply(value) => {
                        let Some(next) = accumulator.checked_mul(value) else {
                            return Self::blocked(MiniBlocker::ArithmeticOverflow);
                        };
                        accumulator = next;
                    }
                    WorkerInstruction::Put(slot) => {
                        if !writes.contains(&slot) {
                            return Self::blocked(MiniBlocker::MissingFootprintEvidence);
                        }
                        private.insert(slot, accumulator);
                    }
                    WorkerInstruction::Return => returned = Some(accumulator),
                }
            }
            let Some(returned) = returned else {
                return Self::blocked(MiniBlocker::MissingReturn);
            };
            if private.keys().copied().collect::<BTreeSet<_>>() != writes {
                return Self::blocked(MiniBlocker::MissingFootprintEvidence);
            }
            let private_writes: Vec<_> = private
                .into_iter()
                .map(|(slot, value)| WorkspaceCell::new(slot, value))
                .collect();
            private_results.push(PrivateWork::new(
                program.worker,
                program.write_footprint.to_vec(),
                private_writes.clone(),
                returned,
            ));
            traces.push(WorkerTrace {
                worker: program.worker,
                reads: read_trace.into_boxed_slice(),
                writes: private_writes.into_boxed_slice(),
                returned,
            });
        }
        let decision = Self::execute(pre, &MiniCommand::Execute(private_results), budget);
        MiniRun {
            decision,
            traces: traces.into_boxed_slice(),
        }
    }

    /// Executes private workspace results independent of their completion order.
    #[must_use]
    pub fn execute(pre: &MiniState, command: &MiniCommand, budget: MiniBudget) -> MiniDecision {
        let MiniCommand::Execute(input) = command;
        if input.len() > budget.max_workers {
            return MiniDecision::Blocked(MiniBlocker::WorkerBudget);
        }
        let mut work = input.clone();
        work.sort_by_key(PrivateWork::worker);
        if work.windows(2).any(|pair| pair[0].worker == pair[1].worker) {
            return MiniDecision::Blocked(MiniBlocker::DuplicateWorker);
        }
        let total_writes = work.iter().fold(0usize, |total, value| {
            total.saturating_add(value.writes.len())
        });
        if total_writes > budget.max_writes {
            return MiniDecision::Blocked(MiniBlocker::WriteBudget);
        }
        let mut ownership = BTreeMap::new();
        let mut merged = BTreeMap::new();
        let mut returns = Vec::with_capacity(work.len());
        for private in &work {
            let mut footprint: BTreeSet<u32> = private.write_footprint.iter().copied().collect();
            if footprint.len() != private.write_footprint.len() {
                return MiniDecision::Blocked(MiniBlocker::MissingFootprintEvidence);
            }
            let mut writes = private.writes.to_vec();
            writes.sort_unstable();
            if writes.windows(2).any(|pair| pair[0].slot == pair[1].slot) {
                return MiniDecision::Blocked(MiniBlocker::DuplicatePrivateWrite);
            }
            for write in writes {
                if !footprint.remove(&write.slot) {
                    return MiniDecision::Blocked(MiniBlocker::MissingFootprintEvidence);
                }
                if let Some(first) = ownership.insert(write.slot, private.worker) {
                    return MiniDecision::Rejected(MergeConflict {
                        slot: write.slot,
                        first_worker: first,
                        second_worker: private.worker,
                    });
                }
                merged.insert(write.slot, write.value);
            }
            if !footprint.is_empty() {
                return MiniDecision::Blocked(MiniBlocker::MissingFootprintEvidence);
            }
            returns.push((private.worker, private.returned));
        }
        let mut cells: BTreeMap<u32, i128> = pre
            .cells
            .iter()
            .map(|cell| (cell.slot, cell.value))
            .collect();
        for (slot, value) in merged {
            cells.insert(slot, value);
        }
        let next = MiniState {
            cells: cells
                .into_iter()
                .map(|(slot, value)| WorkspaceCell::new(slot, value))
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        };
        MiniDecision::Accepted {
            state: next,
            returns: returns.into_boxed_slice(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> MiniState {
        MiniState::try_new(vec![WorkspaceCell::new(1, 10)]).unwrap_or_else(|| unreachable!())
    }
    fn worker(id: u32, slot: u32, value: i128) -> PrivateWork {
        PrivateWork::new(id, vec![slot], vec![WorkspaceCell::new(slot, value)], value)
    }
    #[test]
    fn permutations_have_equal_complete_results() {
        let pre = state();
        let first = MiniCommand::Execute(vec![worker(2, 3, 30), worker(1, 2, 20)]);
        let second = MiniCommand::Execute(vec![worker(1, 2, 20), worker(2, 3, 30)]);
        assert_eq!(
            MiniDeterminator::execute(&pre, &first, MiniBudget::default()),
            MiniDeterminator::execute(&pre, &second, MiniBudget::default())
        );
    }
    #[test]
    fn conflict_is_stable_and_pre_state_is_unchanged() {
        let pre = state();
        let command = MiniCommand::Execute(vec![worker(9, 2, 20), worker(3, 2, 21)]);
        assert_eq!(
            MiniDeterminator::execute(&pre, &command, MiniBudget::default()),
            MiniDecision::Rejected(MergeConflict {
                slot: 2,
                first_worker: 3,
                second_worker: 9
            })
        );
        assert_eq!(pre, state());
    }

    fn program(worker: u32, output: u32, operation: WorkerInstruction) -> WorkerProgram {
        WorkerProgram::new(
            worker,
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

    #[test]
    fn finite_programs_are_schedule_independent() {
        let pre = state();
        let programs = vec![
            program(2, 3, WorkerInstruction::Multiply(2)),
            program(1, 2, WorkerInstruction::Add(5)),
        ];
        let left =
            MiniDeterminator::execute_programs(&pre, &programs, &[2, 1], MiniBudget::default());
        let right =
            MiniDeterminator::execute_programs(&pre, &programs, &[1, 2], MiniBudget::default());
        assert_eq!(left, right);
        assert_eq!(left.traces()[0].worker(), 1);
        assert_eq!(left.traces()[1].worker(), 2);
        let MiniDecision::Accepted { state, returns } = left.decision() else {
            panic!("expected accepted canonical join");
        };
        assert_eq!(
            state.cells(),
            &[
                WorkspaceCell::new(1, 10),
                WorkspaceCell::new(2, 15),
                WorkspaceCell::new(3, 20),
            ]
        );
        assert_eq!(returns.as_ref(), &[(1, 15), (2, 20)]);
    }

    #[test]
    fn workers_cannot_observe_other_private_workspaces() {
        let pre = state();
        let producer = program(1, 2, WorkerInstruction::Add(5));
        let consumer = WorkerProgram::new(
            2,
            vec![2],
            vec![3],
            vec![
                WorkerInstruction::Get(2),
                WorkerInstruction::Put(3),
                WorkerInstruction::Return,
            ],
        );
        let run = MiniDeterminator::execute_programs(
            &pre,
            &[producer, consumer],
            &[1, 2],
            MiniBudget::default(),
        );
        assert_eq!(
            run.decision(),
            &MiniDecision::Blocked(MiniBlocker::MissingCell)
        );
        assert_eq!(pre, state());
    }

    #[test]
    fn arithmetic_and_step_failures_roll_back() {
        let pre = MiniState::try_new(vec![WorkspaceCell::new(1, i128::MAX)])
            .unwrap_or_else(|| unreachable!());
        let overflow = program(1, 2, WorkerInstruction::Add(1));
        let run =
            MiniDeterminator::execute_programs(&pre, &[overflow], &[1], MiniBudget::default());
        assert_eq!(
            run.decision(),
            &MiniDecision::Blocked(MiniBlocker::ArithmeticOverflow)
        );
        assert_eq!(pre.get(1), Some(i128::MAX));

        let budget = MiniBudget::try_with_steps(1, 1, 3).unwrap_or_else(|| unreachable!());
        let run = MiniDeterminator::execute_programs(
            &state(),
            &[program(1, 2, WorkerInstruction::Add(1))],
            &[1],
            budget,
        );
        assert_eq!(
            run.decision(),
            &MiniDecision::Blocked(MiniBlocker::StepBudget)
        );
    }

    #[test]
    fn program_conflicts_have_one_stable_witness() {
        let programs = vec![
            program(9, 2, WorkerInstruction::Add(1)),
            program(3, 2, WorkerInstruction::Add(2)),
        ];
        let run =
            MiniDeterminator::execute_programs(&state(), &programs, &[9, 3], MiniBudget::default());
        assert_eq!(
            run.decision(),
            &MiniDecision::Rejected(MergeConflict {
                slot: 2,
                first_worker: 3,
                second_worker: 9
            })
        );
    }
}
