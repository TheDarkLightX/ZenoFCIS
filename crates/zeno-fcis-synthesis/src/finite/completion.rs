//! Finite exit-path search, followed by independent decreasing-rank checking.
//!
//! This model covers every declared state tuple under a fixed environment.
//! It establishes available finite paths, not scheduler fairness, economic
//! eligibility, external delivery, or production publication authority.

use super::{
    Domain, Error as EvaluationError, MAX_INPUTS, MAX_STEPS, Program, Space, finite_tuple, ir,
};
use crate::{SynthesisError, hash_bytes, hash_canonical};
use alloc::{collections::VecDeque, vec, vec::Vec};
use core::fmt;
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_value::Value;

const MAX_POLICY_BYTES: u64 = 16 * 1024 * 1024;
const PLAN_PROFILE: &str = "zeno-fcis/completion-plan/1";

/// Complete-operation limits checked before graph construction or verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompletionLimits {
    /// Maximum state/command pairs; the hard ceiling is `MAX_INPUTS`.
    pub max_transitions: u64,
    /// Conservative interpreter and graph-work units, at most `MAX_STEPS`.
    pub max_steps: u64,
    /// Maximum complete canonical finite state tuple size.
    pub max_state_bytes: u64,
    /// Maximum complete canonical finite command tuple size.
    pub max_command_bytes: u64,
    /// Maximum canonical completion-plan bytes, reserved before search.
    pub max_policy_bytes: u64,
}
impl Default for CompletionLimits {
    fn default() -> Self {
        Self {
            max_transitions: MAX_INPUTS,
            max_steps: MAX_STEPS,
            max_state_bytes: 4_096,
            max_command_bytes: 4_096,
            max_policy_bytes: 4 * 1024 * 1024,
        }
    }
}

/// Rejected input or evidence; none of these results establishes completion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionError {
    /// Invalid finite model shape or configured limit.
    Invalid(&'static str),
    /// The complete operation does not fit its structural or work envelope.
    Resource(EvaluationError),
    /// A declared program traps, including an unselected command.
    Evaluation {
        /// Exact input tuple: state, followed by command for the step program.
        input: Vec<i64>,
        /// Original interpreter error.
        source: EvaluationError,
    },
    /// A rejected command attempts to change the modeled state.
    RejectedStateChange {
        /// Exact state/command input.
        input: Vec<i64>,
    },
    /// First lexicographic state with no accepted path to a terminal state.
    NoExit {
        /// Exact finite state tuple.
        state: Vec<i64>,
    },
    /// A proposed plan is not evidence for the independently supplied problem.
    InvalidPlan {
        /// Stable diagnostic.
        reason: &'static str,
        /// Affected state, or absent for a whole-plan binding/shape failure.
        state: Option<Vec<i64>>,
    },
    /// Canonical encoding or identity construction failed.
    Encoding(SynthesisError),
}
impl fmt::Display for CompletionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
#[cfg(feature = "std")]
impl std::error::Error for CompletionError {}
impl From<SynthesisError> for CompletionError {
    fn from(error: SynthesisError) -> Self {
        Self::Encoding(error)
    }
}

/// Owned finite model and independently supplied completion/resource contract.
#[derive(Clone, Debug)]
pub struct CompletionProblem {
    step: Program,
    terminal: Program,
    states: usize,
    commands: usize,
    problem_hash: Hash32,
}
impl CompletionProblem {
    /// Requires `(state, command) -> (accepted: Bool, successor state)` and
    /// `state -> terminal: Bool`. All admitted tuples are checked, not sampled.
    pub fn try_new(
        step: Program,
        terminal: Program,
        limits: CompletionLimits,
    ) -> Result<Self, CompletionError> {
        let Some(state_domains) = step.outputs().get(1..) else {
            return Err(CompletionError::Invalid("step-shape"));
        };
        if step.outputs().first() != Some(&Domain::Bool)
            || state_domains.is_empty()
            || step.inputs().get(..state_domains.len()) != Some(state_domains)
            || terminal.inputs() != state_domains
            || terminal.outputs() != [Domain::Bool]
        {
            return Err(CompletionError::Invalid("step-shape"));
        }
        if limits.max_transitions > MAX_INPUTS
            || limits.max_steps > MAX_STEPS
            || limits.max_policy_bytes > MAX_POLICY_BYTES
        {
            return Err(CompletionError::Invalid("limits"));
        }
        let states = Space::try_new(state_domains, MAX_INPUTS)
            .map_err(CompletionError::Resource)?
            .cardinality();
        let command_domains = &step.inputs()[state_domains.len()..];
        let commands = Space::try_new(command_domains, MAX_INPUTS)
            .map_err(CompletionError::Resource)?
            .cardinality();
        let transitions = states
            .checked_mul(commands)
            .ok_or(CompletionError::Invalid("cardinality"))?;
        require("transitions", transitions, limits.max_transitions)?;
        let step_nodes =
            u64::try_from(step.nodes().len()).map_err(|_| CompletionError::Invalid("nodes"))?;
        let terminal_nodes =
            u64::try_from(terminal.nodes().len()).map_err(|_| CompletionError::Invalid("nodes"))?;
        let fields =
            u64::try_from(state_domains.len()).map_err(|_| CompletionError::Invalid("fields"))?;
        // Count graph/index work too; this is a logical bound, not elapsed time.
        let work = states * (terminal_nodes + 2) + transitions * (step_nodes + fields + 4);
        require("steps", work, limits.max_steps)?;
        let state_size = domain_tuple_size(state_domains)?;
        let command_size = domain_tuple_size(command_domains)?;
        require("state-bytes", state_size, limits.max_state_bytes)?;
        require("command-bytes", command_size, limits.max_command_bytes)?;
        // Exact canonical overheads are derived with the existing encoder.
        // Scalar positions have fixed-width I128/U128 encoding in this profile.
        let row_header = value_size(&ir::tuple(vec![Value::U128(0)]))?;
        let plan_header = value_size(&plan_value(Hash32::ZERO, &[]))?;
        require(
            "policy-bytes",
            plan_header + states * (row_header + command_size),
            limits.max_policy_bytes,
        )?;
        let source = hash_bytes(
            "zeno-fcis/completion-checker-source",
            include_bytes!("completion.rs"),
        )?;
        let evaluator = hash_bytes("zeno-fcis/completion-ir-source", include_bytes!("ir.rs"))?;
        let enumeration = hash_bytes(
            "zeno-fcis/completion-space-source",
            include_bytes!("mod.rs"),
        )?;
        let problem_hash = hash_canonical(
            "zeno-fcis/completion-problem",
            &ir::tuple(vec![
                step.value(),
                terminal.value(),
                bytes_value(source),
                bytes_value(evaluator),
                bytes_value(enumeration),
                ir::tuple(
                    [
                        limits.max_transitions,
                        limits.max_steps,
                        limits.max_state_bytes,
                        limits.max_command_bytes,
                        limits.max_policy_bytes,
                    ]
                    .into_iter()
                    .map(|v| Value::U128(v.into()))
                    .collect(),
                ),
            ]),
        )?;
        Ok(Self {
            step,
            terminal,
            states: usize::try_from(states).map_err(|_| CompletionError::Invalid("states"))?,
            commands: usize::try_from(commands)
                .map_err(|_| CompletionError::Invalid("commands"))?,
            problem_hash,
        })
    }
    /// Complete model, contract, budget, and checker-source identity.
    #[must_use]
    pub const fn problem_hash(&self) -> Hash32 {
        self.problem_hash
    }
    /// Number of states whose exit paths must be verified.
    #[must_use]
    pub const fn state_count(&self) -> usize {
        self.states
    }
    /// Number of commands considered at every state.
    #[must_use]
    pub const fn command_count(&self) -> usize {
        self.commands
    }
    fn state_domains(&self) -> &[Domain] {
        &self.step.outputs()[1..]
    }
    fn command_domains(&self) -> &[Domain] {
        &self.step.inputs()[self.state_domains().len()..]
    }
    fn states(&self) -> Result<Space, CompletionError> {
        Space::try_new(self.state_domains(), MAX_INPUTS).map_err(CompletionError::Resource)
    }
    fn commands(&self) -> Result<Space, CompletionError> {
        Space::try_new(self.command_domains(), MAX_INPUTS).map_err(CompletionError::Resource)
    }
    fn terminal(&self, state: &[i64]) -> Result<bool, CompletionError> {
        evaluate(&self.terminal, state).map(|output| output == [1])
    }
}

/// Untrusted proposed policy row in lexicographic state order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionStep {
    /// Zero at terminal states; strictly decreases along the selected commands.
    pub remaining: u32,
    /// Empty at terminal states; otherwise an exact admitted command tuple.
    pub command: Vec<i64>,
}
/// Transport data only. Use `verify_completion` against your own expected problem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompletionPlan {
    /// Expected complete problem identity, checked independently of this field.
    pub problem_hash: Hash32,
    /// Exactly one row per finite state, in lexicographic domain order.
    pub steps: Vec<CompletionStep>,
}

/// Checked finite exit policy. It grants no application or publication authority.
#[derive(Clone, Debug)]
pub struct VerifiedCompletion {
    plan: CompletionPlan,
    domains: Vec<Domain>,
}
impl VerifiedCompletion {
    /// Exact independently checked plan and problem binding.
    #[must_use]
    pub const fn plan(&self) -> &CompletionPlan {
        &self.plan
    }
    /// Canonical versioned plan bytes, bounded by the problem's admission.
    ///
    /// Decoding bytes never grants this type; reconstructed plan rows must pass
    /// `verify_completion` against the consumer's independently chosen problem.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CompletionError> {
        plan_value(self.plan.problem_hash, &self.plan.steps)
            .canonical_bytes()
            .map_err(SynthesisError::Encode)
            .map_err(CompletionError::Encoding)
    }
    /// Gets the checked next command, or `None` at a terminal state.
    pub fn next_command(&self, state: &[i64]) -> Result<Option<&[i64]>, CompletionError> {
        let index = tuple_index(&self.domains, state)?;
        let row = &self.plan.steps[index];
        Ok((row.remaining != 0).then_some(row.command.as_slice()))
    }
}

/// Finds shortest exit paths using reverse BFS; search output remains untrusted.
pub fn find_completion(problem: &CompletionProblem) -> Result<CompletionPlan, CompletionError> {
    let states: Vec<_> = problem.states()?.collect();
    let commands: Vec<_> = problem.commands()?.collect();
    let mut incoming = vec![Vec::<(usize, usize)>::new(); problem.states];
    let mut distances = vec![None::<u32>; problem.states];
    let mut choices = vec![None::<usize>; problem.states];
    let mut queue = VecDeque::new();
    for (index, state) in states.iter().enumerate() {
        if problem.terminal(state)? {
            distances[index] = Some(0);
            queue.push_back(index);
        }
        for (command_index, command) in commands.iter().enumerate() {
            if let Some(target) = successor(problem, state, command)? {
                incoming[target].push((index, command_index));
            }
        }
    }
    while let Some(target) = queue.pop_front() {
        let distance = distances[target]
            .and_then(|value| value.checked_add(1))
            .ok_or(CompletionError::Invalid("search-distance"))?;
        for &(source, command) in &incoming[target] {
            match distances[source] {
                None => {
                    distances[source] = Some(distance);
                    choices[source] = Some(command);
                    queue.push_back(source);
                }
                Some(previous)
                    if previous == distance && choices[source].is_none_or(|old| command < old) =>
                {
                    choices[source] = Some(command);
                }
                _ => {}
            }
        }
    }
    let mut steps = Vec::with_capacity(problem.states);
    for (index, distance) in distances.into_iter().enumerate() {
        let remaining = distance.ok_or_else(|| CompletionError::NoExit {
            state: states[index].clone(),
        })?;
        let command = choices[index]
            .map(|choice| commands[choice].clone())
            .unwrap_or_default();
        steps.push(CompletionStep { remaining, command });
    }
    Ok(CompletionPlan {
        problem_hash: problem.problem_hash,
        steps,
    })
}

/// Independently checks all rows, without invoking search or trusting its graph.
///
/// All declared commands are evaluated to reject hidden traps. Only the selected
/// accepted successor and decreasing rank are used as positive exit evidence.
pub fn verify_completion(
    problem: &CompletionProblem,
    plan: &CompletionPlan,
) -> Result<VerifiedCompletion, CompletionError> {
    if plan.problem_hash != problem.problem_hash {
        return Err(invalid_plan("problem-binding", None));
    }
    if plan.steps.len() != problem.states {
        return Err(invalid_plan("row-count", None));
    }
    for (index, state) in problem.states()?.enumerate() {
        let row = &plan.steps[index];
        let terminal = problem.terminal(&state)?;
        if terminal {
            if row.remaining != 0 || !row.command.is_empty() {
                return Err(invalid_plan("terminal-row", Some(state)));
            }
        } else if row.remaining == 0
            || !usize::try_from(row.remaining).is_ok_and(|rank| rank <= problem.states)
            || !ir::admitted(problem.command_domains(), &row.command)
        {
            return Err(invalid_plan("progress-row", Some(state)));
        }
        let mut selected_target = None;
        for command in problem.commands()? {
            let target = successor(problem, &state, &command)?;
            if !terminal && command == row.command {
                selected_target = target;
            }
        }
        if !terminal {
            let target = selected_target
                .ok_or_else(|| invalid_plan("unaccepted-command", Some(state.clone())))?;
            if plan.steps[target].remaining >= row.remaining {
                return Err(invalid_plan("rank-not-decreasing", Some(state)));
            }
        }
    }
    Ok(VerifiedCompletion {
        plan: plan.clone(),
        domains: problem.state_domains().to_vec(),
    })
}

fn successor(
    problem: &CompletionProblem,
    state: &[i64],
    command: &[i64],
) -> Result<Option<usize>, CompletionError> {
    let mut input = state.to_vec();
    input.extend_from_slice(command);
    let output = evaluate(&problem.step, &input)?;
    if output[0] == 0 {
        if &output[1..] != state {
            return Err(CompletionError::RejectedStateChange { input });
        }
        return Ok(None);
    }
    tuple_index(problem.state_domains(), &output[1..]).map(Some)
}

fn evaluate(program: &Program, input: &[i64]) -> Result<Vec<i64>, CompletionError> {
    program
        .evaluate(input)
        .map_err(|source| CompletionError::Evaluation {
            input: input.to_vec(),
            source,
        })
}

fn tuple_index(domains: &[Domain], values: &[i64]) -> Result<usize, CompletionError> {
    if !ir::admitted(domains, values) {
        return Err(CompletionError::Invalid("state-domain"));
    }
    let mut index = 0usize;
    for (domain, value) in domains.iter().zip(values) {
        let (min, max) = domain.bounds();
        let width = usize::try_from(i128::from(max) - i128::from(min) + 1)
            .map_err(|_| CompletionError::Invalid("state-index"))?;
        let offset = usize::try_from(i128::from(*value) - i128::from(min))
            .map_err(|_| CompletionError::Invalid("state-index"))?;
        index = index
            .checked_mul(width)
            .and_then(|v| v.checked_add(offset))
            .ok_or(CompletionError::Invalid("state-index"))?;
    }
    Ok(index)
}

fn require(resource: &'static str, required: u64, declared: u64) -> Result<(), CompletionError> {
    if required > declared {
        return Err(CompletionError::Resource(EvaluationError::Budget {
            resource,
            required,
            declared,
        }));
    }
    Ok(())
}
fn domain_tuple_size(domains: &[Domain]) -> Result<u64, CompletionError> {
    value_size(&finite_tuple(
        &domains
            .iter()
            .map(|domain| domain.bounds().0)
            .collect::<Vec<_>>(),
    ))
}
fn value_size(value: &Value) -> Result<u64, CompletionError> {
    let bytes = value.canonical_bytes().map_err(SynthesisError::Encode)?;
    u64::try_from(bytes.len()).map_err(|_| CompletionError::Invalid("encoding-size"))
}
fn bytes_value(hash: Hash32) -> Value {
    Value::Bytes(hash.as_bytes().to_vec().into_boxed_slice())
}
fn plan_value(problem_hash: Hash32, steps: &[CompletionStep]) -> Value {
    ir::tuple(vec![
        Value::Text(PLAN_PROFILE.into()),
        bytes_value(problem_hash),
        ir::tuple(
            steps
                .iter()
                .map(|step| {
                    ir::tuple(vec![
                        Value::U128(step.remaining.into()),
                        finite_tuple(&step.command),
                    ])
                })
                .collect(),
        ),
    ])
}
fn invalid_plan(reason: &'static str, state: Option<Vec<i64>>) -> CompletionError {
    CompletionError::InvalidPlan { reason, state }
}
