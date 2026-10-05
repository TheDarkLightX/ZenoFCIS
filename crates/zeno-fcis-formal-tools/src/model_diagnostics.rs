//! Private closed model diagnostics. No callbacks, law authority, or publication.

use std::boxed::Box;
use std::vec::Vec;

use crate::spec_authoring::*;

/// One observed scalar value bound to a typed projection path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModelObservation {
    path: ProjectionPath,
    value: i128,
}
impl ModelObservation {
    /// Creates an observation.
    #[must_use]
    pub(super) const fn new(path: ProjectionPath, value: i128) -> Self {
        Self { path, value }
    }
    /// Returns the typed projection path.
    #[must_use]
    #[cfg(test)]
    pub(super) const fn path(&self) -> &ProjectionPath {
        &self.path
    }
    /// Returns the scalar value.
    #[must_use]
    #[cfg(test)]
    pub(super) const fn value(&self) -> i128 {
        self.value
    }
}

/// One logical trace step. Steps are events, not wall-clock durations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModelStep {
    observations: Box<[ModelObservation]>,
}
impl ModelStep {
    /// Sorts observations and rejects duplicate projection paths.
    pub(super) fn try_new(mut observations: Vec<ModelObservation>) -> Option<Self> {
        observations.sort_by(|a, b| a.path.cmp(&b.path));
        if observations
            .windows(2)
            .any(|pair| pair[0].path == pair[1].path)
        {
            None
        } else {
            Some(Self {
                observations: observations.into_boxed_slice(),
            })
        }
    }
    /// Returns observations in canonical path order.
    #[must_use]
    #[cfg(test)]
    pub(super) const fn observations(&self) -> &[ModelObservation] {
        &self.observations
    }
    fn get(&self, path: &ProjectionPath) -> Option<i128> {
        self.observations
            .binary_search_by(|value| value.path.cmp(path))
            .ok()
            .map(|index| self.observations[index].value)
    }
}

/// Explicit deterministic evaluator limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DiagnosticLimits {
    max_operations: u64,
    max_quantifier_iterations: u64,
    max_predicate_calls: u64,
}
impl DiagnosticLimits {
    /// Creates nonzero evaluator bounds.
    #[cfg(test)]
    pub(super) const fn try_new(
        max_operations: u64,
        max_quantifier_iterations: u64,
        max_predicate_calls: u64,
    ) -> Option<Self> {
        if max_operations == 0 || max_quantifier_iterations == 0 || max_predicate_calls == 0 {
            None
        } else {
            Some(Self {
                max_operations,
                max_quantifier_iterations,
                max_predicate_calls,
            })
        }
    }
    /// Returns the operation bound.
    #[must_use]
    pub(super) const fn max_operations(self) -> u64 {
        self.max_operations
    }
    /// Returns the quantifier-iteration bound.
    #[must_use]
    pub(super) const fn max_quantifier_iterations(self) -> u64 {
        self.max_quantifier_iterations
    }
    /// Returns the predicate-call bound.
    #[must_use]
    pub(super) const fn max_predicate_calls(self) -> u64 {
        self.max_predicate_calls
    }
}
impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            max_operations: 1_000_000,
            max_quantifier_iterations: 100_000,
            max_predicate_calls: 100_000,
        }
    }
}

/// Why a built-in evaluation cannot decide.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelReason {
    MissingProjection,
    MissingPredicate,
    Overflow,
    DivisionByZero,
    NonExactDivision,
    InvalidRange,
    OperationLimit,
    QuantifierLimit,
    PredicateLimit,
    EmptyTrace,
    HorizonExceeded,
    ResourceLimit,
}

/// Three-way relational evaluation result.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DiagnosticOutcome {
    True,
    False,
    Indeterminate(ModelReason),
}
impl DiagnosticOutcome {
    /// Returns a Boolean only for determinate outcomes.
    #[must_use]
    #[cfg(test)]
    pub(super) const fn determinate(self) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::False => Some(false),
            Self::Indeterminate(_) => None,
        }
    }
}

/// Context captured once for relational evaluation.
pub(super) struct ModelContext<'a> {
    step: &'a ModelStep,
    limits: DiagnosticLimits,
}
impl<'a> ModelContext<'a> {
    pub(super) const fn new(step: &'a ModelStep, limits: DiagnosticLimits) -> Self {
        Self { step, limits }
    }
}

/// Finite temporal evaluation or unbounded proof obligation.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TemporalDiagnosis {
    Satisfied,
    Counterexample { step: usize },
    Indeterminate(ModelReason),
    ProofObligation,
}

struct Fuel {
    operations: u64,
    quantifiers: u64,
    predicates: u64,
    limits: DiagnosticLimits,
}
impl Fuel {
    fn new(limits: DiagnosticLimits) -> Self {
        Self {
            operations: 0,
            quantifiers: 0,
            predicates: 0,
            limits,
        }
    }
    fn operation(&mut self) -> Result<(), ModelReason> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(ModelReason::OperationLimit)?;
        if self.operations > self.limits.max_operations {
            Err(ModelReason::OperationLimit)
        } else {
            Ok(())
        }
    }
    fn quantifier(&mut self) -> Result<(), ModelReason> {
        self.quantifiers = self
            .quantifiers
            .checked_add(1)
            .ok_or(ModelReason::QuantifierLimit)?;
        if self.quantifiers > self.limits.max_quantifier_iterations() {
            Err(ModelReason::QuantifierLimit)
        } else {
            Ok(())
        }
    }
    fn predicate(&mut self) -> Result<(), ModelReason> {
        self.predicates = self
            .predicates
            .checked_add(1)
            .ok_or(ModelReason::PredicateLimit)?;
        if self.predicates > self.limits.max_predicate_calls() {
            Err(ModelReason::PredicateLimit)
        } else {
            Ok(())
        }
    }
}

/// Evaluates one relational formula over an immutable context.
#[must_use]
pub(super) fn check_relation(formula: &RelExpr, context: ModelContext<'_>) -> DiagnosticOutcome {
    let shape = formula_shape_rel(formula);
    if formula_exceeds_hard_limits(shape) || shape.0 > operation_bound(context.limits) {
        return DiagnosticOutcome::Indeterminate(ModelReason::ResourceLimit);
    }
    let mut fuel = Fuel::new(context.limits);
    let mut variables = Vec::new();
    match eval_rel(formula, context.step, &mut variables, &mut fuel) {
        Ok(true) => DiagnosticOutcome::True,
        Ok(false) => DiagnosticOutcome::False,
        Err(reason) => DiagnosticOutcome::Indeterminate(reason),
    }
}

/// Evaluates a finite claim or returns an unbounded proof obligation.
#[must_use]
pub(super) fn check_trace(
    formula: &TemporalFormula,
    mode: ClaimMode,
    trace: &[ModelStep],
    limits: DiagnosticLimits,
) -> TemporalDiagnosis {
    let shape = formula_shape_temporal(formula);
    if formula_exceeds_hard_limits(shape) {
        return TemporalDiagnosis::Indeterminate(ModelReason::ResourceLimit);
    }
    if matches!(mode, ClaimMode::UnboundedProof) {
        return TemporalDiagnosis::ProofObligation;
    }
    if shape.0 > operation_bound(limits) {
        return TemporalDiagnosis::Indeterminate(ModelReason::ResourceLimit);
    }
    let horizon = match mode {
        ClaimMode::Finite { horizon } => horizon,
        ClaimMode::Relational | ClaimMode::Inductive => 1,
        ClaimMode::UnboundedProof => 0,
    };
    if trace.is_empty() {
        return TemporalDiagnosis::Indeterminate(ModelReason::EmptyTrace);
    }
    if trace.len() > usize::try_from(horizon).unwrap_or(usize::MAX) {
        return TemporalDiagnosis::Indeterminate(ModelReason::HorizonExceeded);
    }
    let mut fuel = Fuel::new(limits);
    match eval_temporal_at(formula, 0, trace, &mut fuel) {
        Ok(true) => TemporalDiagnosis::Satisfied,
        Ok(false) => TemporalDiagnosis::Counterexample {
            step: first_failure(formula, trace, limits),
        },
        Err(reason) => TemporalDiagnosis::Indeterminate(reason),
    }
}

fn formula_exceeds_hard_limits((nodes, depth): (usize, usize)) -> bool {
    depth > MAX_FORMULA_DEPTH || nodes > MAX_FORMULA_NODES
}

fn operation_bound(limits: DiagnosticLimits) -> usize {
    usize::try_from(limits.max_operations()).unwrap_or(usize::MAX)
}

fn eval_rel<'formula>(
    formula: &'formula RelExpr,
    step: &ModelStep,
    variables: &mut Vec<(&'formula Identifier, i128)>,
    fuel: &mut Fuel,
) -> Result<bool, ModelReason> {
    fuel.operation()?;
    match formula {
        RelExpr::Bool(value) => Ok(*value),
        RelExpr::Not(value) => Ok(!eval_rel(value, step, variables, fuel)?),
        RelExpr::And(left, right) => {
            let left = eval_rel(left, step, variables, fuel)?;
            let right = eval_rel(right, step, variables, fuel)?;
            Ok(left && right)
        }
        RelExpr::Or(left, right) => {
            let left = eval_rel(left, step, variables, fuel)?;
            let right = eval_rel(right, step, variables, fuel)?;
            Ok(left || right)
        }
        RelExpr::Implies(left, right) => {
            let left = eval_rel(left, step, variables, fuel)?;
            let right = eval_rel(right, step, variables, fuel)?;
            Ok(!left || right)
        }
        RelExpr::Compare(operation, left, right) => {
            let left = eval_value(left, step, variables, fuel)?;
            let right = eval_value(right, step, variables, fuel)?;
            Ok(match operation {
                CompareOp::Eq => left == right,
                CompareOp::NotEq => left != right,
                CompareOp::Less => left < right,
                CompareOp::LessEq => left <= right,
                CompareOp::Greater => left > right,
                CompareOp::GreaterEq => left >= right,
            })
        }
        RelExpr::Predicate { arguments, .. } => {
            fuel.predicate()?;
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments.iter() {
                values.push(eval_value(argument, step, variables, fuel)?);
            }
            Err(ModelReason::MissingPredicate)
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => {
            validate_range(*start, *end)?;
            for value in *start..*end {
                fuel.quantifier()?;
                variables.push((variable, value));
                let result = eval_rel(body, step, variables, fuel);
                variables.pop();
                if !result? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => {
            validate_range(*start, *end)?;
            for value in *start..*end {
                fuel.quantifier()?;
                variables.push((variable, value));
                let result = eval_rel(body, step, variables, fuel);
                variables.pop();
                if result? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn eval_value<'formula>(
    value: &'formula ValueExpr,
    step: &ModelStep,
    variables: &mut Vec<(&'formula Identifier, i128)>,
    fuel: &mut Fuel,
) -> Result<i128, ModelReason> {
    fuel.operation()?;
    match value {
        ValueExpr::Int(value) => Ok(*value),
        ValueExpr::Var(name) => variables
            .iter()
            .rev()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, value)| *value)
            .ok_or(ModelReason::MissingProjection),
        ValueExpr::Projection(path) => step.get(path).ok_or(ModelReason::MissingProjection),
        ValueExpr::Add(left, right) => eval_value(left, step, variables, fuel)?
            .checked_add(eval_value(right, step, variables, fuel)?)
            .ok_or(ModelReason::Overflow),
        ValueExpr::Sub(left, right) => eval_value(left, step, variables, fuel)?
            .checked_sub(eval_value(right, step, variables, fuel)?)
            .ok_or(ModelReason::Overflow),
        ValueExpr::Mul(left, right) => eval_value(left, step, variables, fuel)?
            .checked_mul(eval_value(right, step, variables, fuel)?)
            .ok_or(ModelReason::Overflow),
        ValueExpr::Div(mode, left, right) => divide(
            *mode,
            eval_value(left, step, variables, fuel)?,
            eval_value(right, step, variables, fuel)?,
        ),
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            validate_range(*start, *end)?;
            let mut total = 0i128;
            for current in *start..*end {
                fuel.quantifier()?;
                variables.push((variable, current));
                let addend = eval_value(body, step, variables, fuel);
                variables.pop();
                total = total.checked_add(addend?).ok_or(ModelReason::Overflow)?;
            }
            Ok(total)
        }
    }
}

fn divide(mode: DivisionMode, left: i128, right: i128) -> Result<i128, ModelReason> {
    if right == 0 {
        return Err(ModelReason::DivisionByZero);
    }
    let quotient = left.checked_div(right).ok_or(ModelReason::Overflow)?;
    let remainder = left.checked_rem(right).ok_or(ModelReason::Overflow)?;
    match mode {
        DivisionMode::Exact if remainder != 0 => Err(ModelReason::NonExactDivision),
        DivisionMode::Exact => Ok(quotient),
        DivisionMode::Floor if remainder != 0 && (left < 0) != (right < 0) => {
            quotient.checked_sub(1).ok_or(ModelReason::Overflow)
        }
        DivisionMode::Floor => Ok(quotient),
        DivisionMode::Ceil if remainder != 0 && (left < 0) == (right < 0) => {
            quotient.checked_add(1).ok_or(ModelReason::Overflow)
        }
        DivisionMode::Ceil => Ok(quotient),
    }
}
fn validate_range(start: i128, end: i128) -> Result<(), ModelReason> {
    if end < start {
        Err(ModelReason::InvalidRange)
    } else {
        Ok(())
    }
}

fn eval_temporal_at(
    formula: &TemporalFormula,
    index: usize,
    trace: &[ModelStep],
    fuel: &mut Fuel,
) -> Result<bool, ModelReason> {
    fuel.operation()?;
    match formula {
        TemporalFormula::Atom(value) => eval_rel(value, &trace[index], &mut Vec::new(), fuel),
        TemporalFormula::Not(value) => Ok(!eval_temporal_at(value, index, trace, fuel)?),
        TemporalFormula::And(left, right) => {
            let left = eval_temporal_at(left, index, trace, fuel)?;
            let right = eval_temporal_at(right, index, trace, fuel)?;
            Ok(left && right)
        }
        TemporalFormula::Or(left, right) => {
            let left = eval_temporal_at(left, index, trace, fuel)?;
            let right = eval_temporal_at(right, index, trace, fuel)?;
            Ok(left || right)
        }
        TemporalFormula::Next(value) => {
            if index + 1 < trace.len() {
                eval_temporal_at(value, index + 1, trace, fuel)
            } else {
                Ok(false)
            }
        }
        TemporalFormula::Always(value) => {
            for position in index..trace.len() {
                if !eval_temporal_at(value, position, trace, fuel)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        TemporalFormula::Eventually(value) => {
            for position in index..trace.len() {
                if eval_temporal_at(value, position, trace, fuel)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        TemporalFormula::Until(left, right) => {
            for position in index..trace.len() {
                if eval_temporal_at(right, position, trace, fuel)? {
                    return Ok(true);
                }
                if !eval_temporal_at(left, position, trace, fuel)? {
                    return Ok(false);
                }
            }
            Ok(false)
        }
    }
}

fn first_failure(
    formula: &TemporalFormula,
    trace: &[ModelStep],
    limits: DiagnosticLimits,
) -> usize {
    for position in 0..trace.len() {
        let mut fuel = Fuel::new(limits);
        if matches!(
            eval_temporal_at(formula, position, trace, &mut fuel),
            Ok(false)
        ) {
            return position;
        }
    }
    0
}

fn formula_shape_value(value: &ValueExpr) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            ValueExpr::Add(left, right)
            | ValueExpr::Sub(left, right)
            | ValueExpr::Mul(left, right)
            | ValueExpr::Div(_, left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
            ValueExpr::Sum { body, .. } => {
                stack.push((body, current_depth.saturating_add(1)));
            }
            ValueExpr::Int(_) | ValueExpr::Var(_) | ValueExpr::Projection(_) => {}
        }
    }
    (nodes, depth)
}

fn formula_shape_rel(value: &RelExpr) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            RelExpr::Not(inner) => {
                stack.push((inner, current_depth.saturating_add(1)));
            }
            RelExpr::And(left, right)
            | RelExpr::Or(left, right)
            | RelExpr::Implies(left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
            RelExpr::Compare(_, left, right) => {
                for value in [left, right] {
                    let (value_nodes, value_depth) = formula_shape_value(value);
                    nodes = nodes.saturating_add(value_nodes);
                    depth = depth.max(current_depth.saturating_add(value_depth));
                }
            }
            RelExpr::Predicate { arguments, .. } => {
                for value in arguments {
                    let (value_nodes, value_depth) = formula_shape_value(value);
                    nodes = nodes.saturating_add(value_nodes);
                    depth = depth.max(current_depth.saturating_add(value_depth));
                }
            }
            RelExpr::ForAll { body, .. } | RelExpr::Exists { body, .. } => {
                stack.push((body, current_depth.saturating_add(1)));
            }
            RelExpr::Bool(_) => {}
        }
    }
    (nodes, depth)
}

fn formula_shape_temporal(value: &TemporalFormula) -> (usize, usize) {
    let mut stack = Vec::new();
    stack.push((value, 1usize));
    let mut nodes = 0usize;
    let mut depth = 0usize;
    while let Some((current, current_depth)) = stack.pop() {
        nodes = nodes.saturating_add(1);
        depth = depth.max(current_depth);
        match current {
            TemporalFormula::Atom(relational) => {
                let (relational_nodes, relational_depth) = formula_shape_rel(relational);
                nodes = nodes.saturating_add(relational_nodes);
                depth = depth.max(current_depth.saturating_add(relational_depth));
            }
            TemporalFormula::Not(inner)
            | TemporalFormula::Next(inner)
            | TemporalFormula::Always(inner)
            | TemporalFormula::Eventually(inner) => {
                stack.push((inner, current_depth.saturating_add(1)));
            }
            TemporalFormula::And(left, right)
            | TemporalFormula::Or(left, right)
            | TemporalFormula::Until(left, right) => {
                stack.push((left, current_depth.saturating_add(1)));
                stack.push((right, current_depth.saturating_add(1)));
            }
        }
    }
    (nodes, depth)
}
