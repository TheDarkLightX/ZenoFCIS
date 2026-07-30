//! Total bounded relational and finite-trace temporal evaluation.

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::ast::*;
use crate::{MAX_FORMULA_DEPTH, MAX_FORMULA_NODES};

/// Contract for one host-owned pure data predicate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedPredicate {
    name: Identifier,
    argument_count: u16,
}
impl NamedPredicate {
    /// Creates a predicate signature.
    #[must_use]
    pub const fn new(name: Identifier, argument_count: u16) -> Self {
        Self {
            name,
            argument_count,
        }
    }
    /// Returns the stable predicate name.
    #[must_use]
    pub const fn name(&self) -> &Identifier {
        &self.name
    }
    /// Returns the exact scalar argument count.
    #[must_use]
    pub const fn argument_count(&self) -> u16 {
        self.argument_count
    }
}

/// Pure host interface for complex typed data predicates.
pub trait PredicateProvider {
    /// Returns `None` when the exact predicate is missing or cannot decide.
    fn evaluate(&self, name: &Identifier, arguments: &[i128]) -> Option<bool>;
}

/// One observed scalar value bound to a typed projection path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    path: ProjectionPath,
    value: i128,
}
impl Observation {
    /// Creates an observation.
    #[must_use]
    pub const fn new(path: ProjectionPath, value: i128) -> Self {
        Self { path, value }
    }
    /// Returns the typed projection path.
    #[must_use]
    pub const fn path(&self) -> &ProjectionPath {
        &self.path
    }
    /// Returns the scalar value.
    #[must_use]
    pub const fn value(&self) -> i128 {
        self.value
    }
}

/// One logical trace step. Steps are events, not wall-clock durations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceStep {
    observations: Box<[Observation]>,
}
impl TraceStep {
    /// Sorts observations and rejects duplicate projection paths.
    pub fn try_new(mut observations: Vec<Observation>) -> Option<Self> {
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
    pub const fn observations(&self) -> &[Observation] {
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
pub struct EvalLimits {
    max_operations: u64,
    max_quantifier_iterations: u64,
    max_predicate_calls: u64,
}
impl EvalLimits {
    /// Creates nonzero evaluator bounds.
    pub const fn try_new(
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
    pub const fn max_operations(self) -> u64 {
        self.max_operations
    }
    /// Returns the quantifier-iteration bound.
    #[must_use]
    pub const fn max_quantifier_iterations(self) -> u64 {
        self.max_quantifier_iterations
    }
    /// Returns the predicate-call bound.
    #[must_use]
    pub const fn max_predicate_calls(self) -> u64 {
        self.max_predicate_calls
    }
}
impl Default for EvalLimits {
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
pub enum IndeterminateReason {
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
pub enum EvalOutcome {
    True,
    False,
    Indeterminate(IndeterminateReason),
}
impl EvalOutcome {
    /// Returns a Boolean only for determinate outcomes.
    #[must_use]
    pub const fn determinate(self) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::False => Some(false),
            Self::Indeterminate(_) => None,
        }
    }
}

/// Context captured once for relational evaluation.
pub struct EvaluationContext<'a, P: PredicateProvider + ?Sized> {
    step: &'a TraceStep,
    predicates: &'a P,
    limits: EvalLimits,
}
impl<'a, P: PredicateProvider + ?Sized> EvaluationContext<'a, P> {
    /// Binds one immutable trace step, predicate provider, and limit set.
    #[must_use]
    pub const fn new(step: &'a TraceStep, predicates: &'a P, limits: EvalLimits) -> Self {
        Self {
            step,
            predicates,
            limits,
        }
    }
}

/// Finite temporal evaluation or unbounded proof obligation.
#[allow(missing_docs)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemporalEvaluation {
    Satisfied,
    Counterexample { step: usize },
    Indeterminate(IndeterminateReason),
    ProofObligation,
}

struct Fuel {
    operations: u64,
    quantifiers: u64,
    predicates: u64,
    limits: EvalLimits,
}
impl Fuel {
    fn new(limits: EvalLimits) -> Self {
        Self {
            operations: 0,
            quantifiers: 0,
            predicates: 0,
            limits,
        }
    }
    fn operation(&mut self) -> Result<(), IndeterminateReason> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(IndeterminateReason::OperationLimit)?;
        if self.operations > self.limits.max_operations {
            Err(IndeterminateReason::OperationLimit)
        } else {
            Ok(())
        }
    }
    fn quantifier(&mut self) -> Result<(), IndeterminateReason> {
        self.quantifiers = self
            .quantifiers
            .checked_add(1)
            .ok_or(IndeterminateReason::QuantifierLimit)?;
        if self.quantifiers > self.limits.max_quantifier_iterations {
            Err(IndeterminateReason::QuantifierLimit)
        } else {
            Ok(())
        }
    }
    fn predicate(&mut self) -> Result<(), IndeterminateReason> {
        self.predicates = self
            .predicates
            .checked_add(1)
            .ok_or(IndeterminateReason::PredicateLimit)?;
        if self.predicates > self.limits.max_predicate_calls {
            Err(IndeterminateReason::PredicateLimit)
        } else {
            Ok(())
        }
    }
}

/// Evaluates one relational formula over an immutable context.
#[must_use]
pub fn evaluate_relational<P: PredicateProvider + ?Sized>(
    formula: &RelExpr,
    context: EvaluationContext<'_, P>,
) -> EvalOutcome {
    let shape = formula_shape_rel(formula);
    if formula_exceeds_hard_limits(shape) || shape.0 > operation_bound(context.limits) {
        return EvalOutcome::Indeterminate(IndeterminateReason::ResourceLimit);
    }
    let mut fuel = Fuel::new(context.limits);
    let mut variables = Vec::new();
    match eval_rel(
        formula,
        context.step,
        context.predicates,
        &mut variables,
        &mut fuel,
    ) {
        Ok(true) => EvalOutcome::True,
        Ok(false) => EvalOutcome::False,
        Err(reason) => EvalOutcome::Indeterminate(reason),
    }
}

/// Evaluates a finite claim or returns an unbounded proof obligation.
#[must_use]
pub fn evaluate_temporal<P: PredicateProvider + ?Sized>(
    formula: &TemporalFormula,
    mode: ClaimMode,
    trace: &[TraceStep],
    predicates: &P,
    limits: EvalLimits,
) -> TemporalEvaluation {
    let shape = formula_shape_temporal(formula);
    if formula_exceeds_hard_limits(shape) {
        return TemporalEvaluation::Indeterminate(IndeterminateReason::ResourceLimit);
    }
    if matches!(mode, ClaimMode::UnboundedProof) {
        return TemporalEvaluation::ProofObligation;
    }
    if shape.0 > operation_bound(limits) {
        return TemporalEvaluation::Indeterminate(IndeterminateReason::ResourceLimit);
    }
    let horizon = match mode {
        ClaimMode::Finite { horizon } => horizon,
        ClaimMode::Relational => 1,
        ClaimMode::UnboundedProof => 0,
    };
    if trace.is_empty() {
        return TemporalEvaluation::Indeterminate(IndeterminateReason::EmptyTrace);
    }
    if trace.len() > usize::try_from(horizon).unwrap_or(usize::MAX) {
        return TemporalEvaluation::Indeterminate(IndeterminateReason::HorizonExceeded);
    }
    let mut fuel = Fuel::new(limits);
    match eval_temporal_at(formula, 0, trace, predicates, &mut fuel) {
        Ok(true) => TemporalEvaluation::Satisfied,
        Ok(false) => TemporalEvaluation::Counterexample {
            step: first_failure(formula, trace, predicates, limits),
        },
        Err(reason) => TemporalEvaluation::Indeterminate(reason),
    }
}

fn formula_exceeds_hard_limits((nodes, depth): (usize, usize)) -> bool {
    depth > MAX_FORMULA_DEPTH || nodes > MAX_FORMULA_NODES
}

fn operation_bound(limits: EvalLimits) -> usize {
    usize::try_from(limits.max_operations()).unwrap_or(usize::MAX)
}

fn eval_rel<P: PredicateProvider + ?Sized>(
    formula: &RelExpr,
    step: &TraceStep,
    predicates: &P,
    variables: &mut Vec<(Identifier, i128)>,
    fuel: &mut Fuel,
) -> Result<bool, IndeterminateReason> {
    fuel.operation()?;
    match formula {
        RelExpr::Bool(value) => Ok(*value),
        RelExpr::Not(value) => Ok(!eval_rel(value, step, predicates, variables, fuel)?),
        RelExpr::And(left, right) => {
            let left = eval_rel(left, step, predicates, variables, fuel)?;
            let right = eval_rel(right, step, predicates, variables, fuel)?;
            Ok(left && right)
        }
        RelExpr::Or(left, right) => {
            let left = eval_rel(left, step, predicates, variables, fuel)?;
            let right = eval_rel(right, step, predicates, variables, fuel)?;
            Ok(left || right)
        }
        RelExpr::Implies(left, right) => {
            let left = eval_rel(left, step, predicates, variables, fuel)?;
            let right = eval_rel(right, step, predicates, variables, fuel)?;
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
        RelExpr::Predicate { name, arguments } => {
            fuel.predicate()?;
            let mut values = Vec::with_capacity(arguments.len());
            for argument in arguments.iter() {
                values.push(eval_value(argument, step, variables, fuel)?);
            }
            predicates
                .evaluate(name, &values)
                .ok_or(IndeterminateReason::MissingPredicate)
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
                variables.push((variable.clone(), value));
                let result = eval_rel(body, step, predicates, variables, fuel);
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
                variables.push((variable.clone(), value));
                let result = eval_rel(body, step, predicates, variables, fuel);
                variables.pop();
                if result? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn eval_value(
    value: &ValueExpr,
    step: &TraceStep,
    variables: &[(Identifier, i128)],
    fuel: &mut Fuel,
) -> Result<i128, IndeterminateReason> {
    fuel.operation()?;
    match value {
        ValueExpr::Int(value) => Ok(*value),
        ValueExpr::Var(name) => variables
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == name)
            .map(|(_, value)| *value)
            .ok_or(IndeterminateReason::MissingProjection),
        ValueExpr::Projection(path) => step.get(path).ok_or(IndeterminateReason::MissingProjection),
        ValueExpr::Add(left, right) => eval_value(left, step, variables, fuel)?
            .checked_add(eval_value(right, step, variables, fuel)?)
            .ok_or(IndeterminateReason::Overflow),
        ValueExpr::Sub(left, right) => eval_value(left, step, variables, fuel)?
            .checked_sub(eval_value(right, step, variables, fuel)?)
            .ok_or(IndeterminateReason::Overflow),
        ValueExpr::Mul(left, right) => eval_value(left, step, variables, fuel)?
            .checked_mul(eval_value(right, step, variables, fuel)?)
            .ok_or(IndeterminateReason::Overflow),
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
            let mut owned = variables.to_vec();
            for current in *start..*end {
                fuel.quantifier()?;
                owned.push((variable.clone(), current));
                let addend = eval_value(body, step, &owned, fuel)?;
                owned.pop();
                total = total
                    .checked_add(addend)
                    .ok_or(IndeterminateReason::Overflow)?;
            }
            Ok(total)
        }
    }
}

fn divide(mode: DivisionMode, left: i128, right: i128) -> Result<i128, IndeterminateReason> {
    if right == 0 {
        return Err(IndeterminateReason::DivisionByZero);
    }
    let quotient = left
        .checked_div(right)
        .ok_or(IndeterminateReason::Overflow)?;
    let remainder = left
        .checked_rem(right)
        .ok_or(IndeterminateReason::Overflow)?;
    match mode {
        DivisionMode::Exact if remainder != 0 => Err(IndeterminateReason::NonExactDivision),
        DivisionMode::Exact => Ok(quotient),
        DivisionMode::Floor if remainder != 0 && (left < 0) != (right < 0) => {
            quotient.checked_sub(1).ok_or(IndeterminateReason::Overflow)
        }
        DivisionMode::Floor => Ok(quotient),
        DivisionMode::Ceil if remainder != 0 && (left < 0) == (right < 0) => {
            quotient.checked_add(1).ok_or(IndeterminateReason::Overflow)
        }
        DivisionMode::Ceil => Ok(quotient),
    }
}
fn validate_range(start: i128, end: i128) -> Result<(), IndeterminateReason> {
    if end < start {
        Err(IndeterminateReason::InvalidRange)
    } else {
        Ok(())
    }
}

fn eval_temporal_at<P: PredicateProvider + ?Sized>(
    formula: &TemporalFormula,
    index: usize,
    trace: &[TraceStep],
    predicates: &P,
    fuel: &mut Fuel,
) -> Result<bool, IndeterminateReason> {
    fuel.operation()?;
    match formula {
        TemporalFormula::Atom(value) => {
            eval_rel(value, &trace[index], predicates, &mut Vec::new(), fuel)
        }
        TemporalFormula::Not(value) => {
            Ok(!eval_temporal_at(value, index, trace, predicates, fuel)?)
        }
        TemporalFormula::And(left, right) => {
            let left = eval_temporal_at(left, index, trace, predicates, fuel)?;
            let right = eval_temporal_at(right, index, trace, predicates, fuel)?;
            Ok(left && right)
        }
        TemporalFormula::Or(left, right) => {
            let left = eval_temporal_at(left, index, trace, predicates, fuel)?;
            let right = eval_temporal_at(right, index, trace, predicates, fuel)?;
            Ok(left || right)
        }
        TemporalFormula::Next(value) => {
            if index + 1 < trace.len() {
                eval_temporal_at(value, index + 1, trace, predicates, fuel)
            } else {
                Ok(false)
            }
        }
        TemporalFormula::Always(value) => {
            for position in index..trace.len() {
                if !eval_temporal_at(value, position, trace, predicates, fuel)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        TemporalFormula::Eventually(value) => {
            for position in index..trace.len() {
                if eval_temporal_at(value, position, trace, predicates, fuel)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        TemporalFormula::Until(left, right) => {
            for position in index..trace.len() {
                if eval_temporal_at(right, position, trace, predicates, fuel)? {
                    return Ok(true);
                }
                if !eval_temporal_at(left, position, trace, predicates, fuel)? {
                    return Ok(false);
                }
            }
            Ok(false)
        }
    }
}

fn first_failure<P: PredicateProvider + ?Sized>(
    formula: &TemporalFormula,
    trace: &[TraceStep],
    predicates: &P,
    limits: EvalLimits,
) -> usize {
    for position in 0..trace.len() {
        let mut fuel = Fuel::new(limits);
        if matches!(
            eval_temporal_at(formula, position, trace, predicates, &mut fuel),
            Ok(false)
        ) {
            return position;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoPredicates;
    impl PredicateProvider for NoPredicates {
        fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
            None
        }
    }
    fn path() -> ProjectionPath {
        ProjectionPath::try_new(
            ProjectionRoot::Pre,
            vec![StableId::new(1).unwrap_or_else(|| unreachable!())],
        )
        .unwrap_or_else(|| unreachable!())
    }
    #[test]
    fn strong_next_is_false_at_final_step() {
        let trace =
            TraceStep::try_new(vec![Observation::new(path(), 1)]).unwrap_or_else(|| unreachable!());
        let formula = TemporalFormula::Next(Box::new(TemporalFormula::Atom(RelExpr::Bool(true))));
        assert_eq!(
            evaluate_temporal(
                &formula,
                ClaimMode::Finite { horizon: 1 },
                &[trace],
                &NoPredicates,
                EvalLimits::default()
            ),
            TemporalEvaluation::Counterexample { step: 0 }
        );
    }
    #[test]
    fn division_errors_are_indeterminate() {
        let trace = TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!());
        let formula = RelExpr::Compare(
            CompareOp::Eq,
            ValueExpr::Div(
                DivisionMode::Exact,
                Box::new(ValueExpr::Int(1)),
                Box::new(ValueExpr::Int(0)),
            ),
            ValueExpr::Int(0),
        );
        assert_eq!(
            evaluate_relational(
                &formula,
                EvaluationContext::new(&trace, &NoPredicates, EvalLimits::default())
            ),
            EvalOutcome::Indeterminate(IndeterminateReason::DivisionByZero)
        );
    }

    fn division_equals(mode: DivisionMode, left: i128, right: i128, expected: i128) -> EvalOutcome {
        let trace = TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!());
        let formula = RelExpr::Compare(
            CompareOp::Eq,
            ValueExpr::Div(
                mode,
                Box::new(ValueExpr::Int(left)),
                Box::new(ValueExpr::Int(right)),
            ),
            ValueExpr::Int(expected),
        );
        evaluate_relational(
            &formula,
            EvaluationContext::new(&trace, &NoPredicates, EvalLimits::default()),
        )
    }

    #[test]
    fn floor_and_ceil_follow_mathematical_sign_rules() {
        for (left, right, floor, ceil) in [
            (5, 2, 2, 3),
            (-5, 2, -3, -2),
            (5, -2, -3, -2),
            (-5, -2, 2, 3),
            (6, -2, -3, -3),
        ] {
            assert_eq!(
                division_equals(DivisionMode::Floor, left, right, floor),
                EvalOutcome::True
            );
            assert_eq!(
                division_equals(DivisionMode::Ceil, left, right, ceil),
                EvalOutcome::True
            );
        }
    }

    #[test]
    fn exact_division_and_signed_overflow_fail_closed() {
        assert_eq!(
            division_equals(DivisionMode::Exact, 6, -2, -3),
            EvalOutcome::True
        );

        let trace = TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!());
        for (left, right, reason) in [
            (5, -2, IndeterminateReason::NonExactDivision),
            (i128::MIN, -1, IndeterminateReason::Overflow),
        ] {
            let formula = RelExpr::Compare(
                CompareOp::Eq,
                ValueExpr::Div(
                    DivisionMode::Exact,
                    Box::new(ValueExpr::Int(left)),
                    Box::new(ValueExpr::Int(right)),
                ),
                ValueExpr::Int(0),
            );
            assert_eq!(
                evaluate_relational(
                    &formula,
                    EvaluationContext::new(&trace, &NoPredicates, EvalLimits::default())
                ),
                EvalOutcome::Indeterminate(reason)
            );
        }
    }

    #[test]
    fn direct_formula_node_count_is_checked_against_operation_budget() {
        let trace = TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!());
        let relational = RelExpr::And(Box::new(RelExpr::Bool(true)), Box::new(RelExpr::Bool(true)));
        let two_operations = EvalLimits::try_new(2, 1, 1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_relational(
                &relational,
                EvaluationContext::new(&trace, &NoPredicates, two_operations),
            ),
            EvalOutcome::Indeterminate(IndeterminateReason::ResourceLimit)
        );
        let three_operations = EvalLimits::try_new(3, 1, 1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_relational(
                &relational,
                EvaluationContext::new(&trace, &NoPredicates, three_operations),
            ),
            EvalOutcome::True
        );

        let temporal = TemporalFormula::And(
            Box::new(TemporalFormula::Atom(RelExpr::Bool(true))),
            Box::new(TemporalFormula::Atom(RelExpr::Bool(true))),
        );
        let four_operations = EvalLimits::try_new(4, 1, 1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_temporal(
                &temporal,
                ClaimMode::Finite { horizon: 1 },
                core::slice::from_ref(&trace),
                &NoPredicates,
                four_operations,
            ),
            TemporalEvaluation::Indeterminate(IndeterminateReason::ResourceLimit)
        );
        let five_operations = EvalLimits::try_new(5, 1, 1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_temporal(
                &temporal,
                ClaimMode::Finite { horizon: 1 },
                core::slice::from_ref(&trace),
                &NoPredicates,
                five_operations,
            ),
            TemporalEvaluation::Satisfied
        );
    }

    #[test]
    fn unbounded_proof_obligations_ignore_runtime_operation_budget() {
        let formula = TemporalFormula::Atom(RelExpr::Bool(true));
        let one_operation = EvalLimits::try_new(1, 1, 1).unwrap_or_else(|| unreachable!());
        assert_eq!(
            evaluate_temporal(
                &formula,
                ClaimMode::UnboundedProof,
                &[],
                &NoPredicates,
                one_operation,
            ),
            TemporalEvaluation::ProofObligation
        );
    }
}
