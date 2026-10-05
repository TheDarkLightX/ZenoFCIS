// Private original shallow Lean reference. Included only in the unit test crate.
#[derive(Clone, Copy)]
enum ExportKind { Smt, Lean }

fn preflight_export(
    claim: &ClaimDecl,
    kind: ExportKind,
    limits: ExportLimits,
) -> Result<(), ExportError> {
    let (root, multiplier, temporal_width) = match (kind, claim.mode(), claim.formula()) {
        (ExportKind::Smt, ClaimMode::Relational, ClaimFormula::Relational(value)) => {
            (ExportNode::Rel(value), 1, 1)
        }
        (ExportKind::Smt, ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value))
            if horizon > 0 =>
        {
            if horizon > limits.max_horizon() {
                return Err(ExportError::ResourceLimit);
            }
            let width = u64::from(horizon);
            (ExportNode::Temporal(value), width, width)
        }
        (ExportKind::Smt, ClaimMode::UnboundedProof | ClaimMode::Inductive, _) => {
            return Err(ExportError::UnsupportedMode);
        }
        (ExportKind::Lean, ClaimMode::UnboundedProof, ClaimFormula::Temporal(value)) => {
            (ExportNode::Temporal(value), 1, 1)
        }
        (ExportKind::Lean, _, _) => return Err(ExportError::UnsupportedMode),
        _ => return Err(ExportError::InvalidFormula),
    };

    let mut stack = Vec::new();
    stack.push((root, 1usize, multiplier));
    let mut nodes = 0usize;
    let mut operations = 0u64;
    while let Some((node, depth, render_multiplier)) = stack.pop() {
        nodes = nodes.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        if nodes > limits.max_formula_nodes() || depth > limits.max_formula_depth() {
            return Err(ExportError::ResourceLimit);
        }
        operations = operations
            .checked_add(render_multiplier)
            .ok_or(ExportError::ResourceLimit)?;
        if operations > limits.max_operations() {
            return Err(ExportError::ResourceLimit);
        }
        let next_depth = depth.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        match node {
            ExportNode::Rel(value) => match value {
                RelExpr::Bool(_) => {}
                RelExpr::Not(value) => push_export_node(
                    &mut stack,
                    ExportNode::Rel(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                RelExpr::And(left, right)
                | RelExpr::Or(left, right)
                | RelExpr::Implies(left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                RelExpr::Compare(_, left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                RelExpr::Predicate { arguments, .. } => {
                    for argument in arguments {
                        push_export_node(
                            &mut stack,
                            ExportNode::Value(argument),
                            next_depth,
                            render_multiplier,
                            limits,
                        )?;
                    }
                }
                RelExpr::ForAll {
                    start, end, body, ..
                }
                | RelExpr::Exists {
                    start, end, body, ..
                } => {
                    let expanded = range_multiplier(*start, *end, render_multiplier, limits)?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Rel(body),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
            ExportNode::Value(value) => match value {
                ValueExpr::Int(_) | ValueExpr::Var(_) | ValueExpr::Projection(_) => {}
                ValueExpr::Add(left, right)
                | ValueExpr::Sub(left, right)
                | ValueExpr::Mul(left, right)
                | ValueExpr::Div(_, left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                ValueExpr::Sum {
                    start, end, body, ..
                } => {
                    let expanded = range_multiplier(*start, *end, render_multiplier, limits)?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Value(body),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
            ExportNode::Temporal(value) => match value {
                TemporalFormula::Atom(value) => push_export_node(
                    &mut stack,
                    ExportNode::Rel(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                TemporalFormula::Not(value) | TemporalFormula::Next(value) => push_export_node(
                    &mut stack,
                    ExportNode::Temporal(value),
                    next_depth,
                    render_multiplier,
                    limits,
                )?,
                TemporalFormula::Always(value) | TemporalFormula::Eventually(value) => {
                    let expanded = render_multiplier
                        .checked_mul(temporal_width)
                        .ok_or(ExportError::ResourceLimit)?;
                    if expanded > limits.max_operations() {
                        return Err(ExportError::ResourceLimit);
                    }
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(value),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
                TemporalFormula::And(left, right) | TemporalFormula::Or(left, right) => {
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(left),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(right),
                        next_depth,
                        render_multiplier,
                        limits,
                    )?;
                }
                TemporalFormula::Until(left, right) => {
                    let expanded = render_multiplier
                        .checked_mul(temporal_width)
                        .ok_or(ExportError::ResourceLimit)?;
                    if expanded > limits.max_operations() {
                        return Err(ExportError::ResourceLimit);
                    }
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(left),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                    push_export_node(
                        &mut stack,
                        ExportNode::Temporal(right),
                        next_depth,
                        expanded,
                        limits,
                    )?;
                }
            },
        }
    }
    Ok(())
}

/// Exports an unbounded temporal obligation to Lean source.
pub(crate) fn export_lean(claim: &ClaimDecl) -> Result<ExportedObligation, ExportError> {
    export_lean_with_limits(claim, ExportLimits::default())
}

/// Exports one Lean obligation within an explicit deterministic resource envelope.
pub(crate) fn export_lean_with_limits(
    claim: &ClaimDecl,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !claim.backends().contains(&BackendId::Lean) {
        return Err(ExportError::BackendNotSelected);
    }
    if !matches!(claim.mode(), ClaimMode::UnboundedProof) {
        return Err(ExportError::UnsupportedMode);
    }
    preflight_export(claim, ExportKind::Lean, limits)?;
    let empty_environment = BTreeMap::new();
    let mut budget = LeanRenderBudget::new(limits);
    let formula = match claim.formula() {
        ClaimFormula::Temporal(value) => {
            render_temporal_lean(value, "0", &empty_environment, &mut budget)?
        }
        ClaimFormula::Relational(_) => return Err(ExportError::InvalidFormula),
    };
    let proposition = lean_and(vec![formula.defined, formula.term]);
    let claim_id = claim.id().get();
    let source = format!(
        "-- zeno-fcis/lean-obligation/1\n-- claim-id {claim_id}\nnamespace ZenoFCIS\n\n\
def i128Min : Int := {}\n\
def i128Max : Int := {}\n\
def inI128 (value : Int) : Prop := i128Min <= value ∧ value <= i128Max\n\
def I128 : Type := {{ value : Int // inI128 value }}\n\
def floorDiv (left right : Int) : Int :=\n  if right > 0 then left / right else (-left) / (-right)\n\
def ceilDiv (left right : Int) : Int :=\n  -(floorDiv (-left) right)\n\n\
variable (observe : String → Nat → I128)\n\
variable (predicate : String → List Int → Prop)\n\n\
def claim_{claim_id}\n\
    (observe : String → Nat → I128)\n\
    (predicate : String → List Int → Prop) : Prop :=\n  {proposition}\n\n\
theorem claim_{claim_id}_checked : claim_{claim_id} observe predicate := by\n\
\x20\x20simp [claim_{claim_id}, floorDiv, ceilDiv, inI128, i128Min, i128Max]\n\n\
#print axioms claim_{claim_id}_checked\n\n\
end ZenoFCIS\n",
        lean_int(i128::MIN),
        lean_int(i128::MAX),
    );
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    exported(ToolBackend::Lean, claim, source.into_bytes())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LeanValue {
    term: String,
    defined: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LeanBool {
    term: String,
    defined: String,
}

struct LeanRenderBudget {
    operations: u64,
    bytes: usize,
    next_time_binding: u64,
    limits: ExportLimits,
}
impl LeanRenderBudget {
    const fn new(limits: ExportLimits) -> Self {
        Self {
            operations: 0,
            bytes: 0,
            next_time_binding: 0,
            limits,
        }
    }

    // A child temporal formula must never shadow the time expression passed
    // by its parent. One supply also separates the two binders of `until`.
    fn time_binding(&mut self) -> Result<String, ExportError> {
        let binding = self.next_time_binding;
        self.next_time_binding = binding.checked_add(1).ok_or(ExportError::ResourceLimit)?;
        Ok(format!("time_{binding}"))
    }

    fn operation(&mut self) -> Result<(), ExportError> {
        self.operations = self
            .operations
            .checked_add(1)
            .ok_or(ExportError::ResourceLimit)?;
        if self.operations > self.limits.max_operations() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn bytes(&mut self, bytes: usize) -> Result<(), ExportError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or(ExportError::ResourceLimit)?;
        if self.bytes > self.limits.max_source_bytes() {
            return Err(ExportError::ResourceLimit);
        }
        Ok(())
    }

    fn value(&mut self, value: LeanValue) -> Result<LeanValue, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }

    fn boolean(&mut self, value: LeanBool) -> Result<LeanBool, ExportError> {
        self.bytes(
            value
                .term
                .len()
                .checked_add(value.defined.len())
                .ok_or(ExportError::ResourceLimit)?,
        )?;
        Ok(value)
    }
}

fn lean_int(value: i128) -> String {
    if value < 0 {
        format!("(-{} : Int)", value.unsigned_abs())
    } else {
        format!("({value} : Int)")
    }
}

fn lean_and(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "True".to_owned(),
        [term] => term.clone(),
        _ => format!("({})", terms.join(" ∧ ")),
    }
}

fn lean_or(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "False".to_owned(),
        [term] => term.clone(),
        _ => format!("({})", terms.join(" ∨ ")),
    }
}

fn lean_not(term: &str) -> String {
    format!("¬ ({term})")
}

fn lean_path(path: &ProjectionPath, step: &str) -> String {
    let root = match path.root() {
        ProjectionRoot::Pre => "pre",
        ProjectionRoot::Post => "post",
        ProjectionRoot::Command => "command",
        ProjectionRoot::Context => "context",
        ProjectionRoot::Effects => "effects",
        ProjectionRoot::Outbox => "outbox",
        ProjectionRoot::Events => "events",
    };
    let suffix = path
        .segments()
        .iter()
        .map(|id| id.get().to_string())
        .collect::<Vec<_>>()
        .join("_");
    format!("(observe \"{root}_{suffix}\" {step}).val")
}

fn lean_checked_binary(
    operator: &str,
    left: LeanValue,
    right: LeanValue,
    budget: &mut LeanRenderBudget,
) -> Result<LeanValue, ExportError> {
    budget.operation()?;
    let term = format!("({} {operator} {})", left.term, right.term);
    let defined = lean_and(vec![left.defined, right.defined, format!("inI128 {term}")]);
    budget.value(LeanValue { term, defined })
}

fn render_value_lean(
    value: &ValueExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanValue, ExportError> {
    budget.operation()?;
    let rendered = match value {
        ValueExpr::Int(value) => LeanValue {
            term: lean_int(*value),
            defined: "True".to_owned(),
        },
        ValueExpr::Var(name) => {
            let Some(value) = environment.get(name.as_str()) else {
                return Err(ExportError::InvalidFormula);
            };
            LeanValue {
                term: lean_int(*value),
                defined: "True".to_owned(),
            }
        }
        ValueExpr::Projection(path) => LeanValue {
            term: lean_path(path, step),
            defined: "True".to_owned(),
        },
        ValueExpr::Add(left, right) | ValueExpr::Sub(left, right) | ValueExpr::Mul(left, right) => {
            let operator = match value {
                ValueExpr::Add(_, _) => "+",
                ValueExpr::Sub(_, _) => "-",
                ValueExpr::Mul(_, _) => "*",
                _ => unreachable!(),
            };
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            lean_checked_binary(operator, left, right, budget)?
        }
        ValueExpr::Div(mode, left, right) => {
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            let term = match mode {
                zeno_fcis_spec::DivisionMode::Exact | zeno_fcis_spec::DivisionMode::Floor => {
                    format!("floorDiv {} {}", left.term, right.term)
                }
                zeno_fcis_spec::DivisionMode::Ceil => {
                    format!("ceilDiv {} {}", left.term, right.term)
                }
            };
            let mut conditions = vec![left.defined, right.defined, format!("{} ≠ 0", right.term)];
            if matches!(mode, zeno_fcis_spec::DivisionMode::Exact) {
                let positive_divisor = format!(
                    "(if {} < 0 then -{} else {})",
                    right.term, right.term, right.term
                );
                conditions.push(format!("{} % {positive_divisor} = 0", left.term));
            }
            conditions.push(format!("inI128 ({term})"));
            LeanValue {
                term: format!("({term})"),
                defined: lean_and(conditions),
            }
        }
        ValueExpr::Sum {
            variable,
            start,
            end,
            body,
        } => {
            if end < start || end.saturating_sub(*start) > 4096 {
                return Err(ExportError::InvalidFormula);
            }
            let mut total = LeanValue {
                term: lean_int(0),
                defined: "True".to_owned(),
            };
            for current in *start..*end {
                let mut nested = environment.clone();
                nested.insert(variable.as_str().into(), current);
                let next = render_value_lean(body, step, &nested, budget)?;
                total = lean_checked_binary("+", total, next, budget)?;
            }
            total
        }
    };
    budget.value(rendered)
}

fn strict_bool_lean(
    operator: &str,
    left: LeanBool,
    right: LeanBool,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    budget.boolean(LeanBool {
        term: format!("({} {operator} {})", left.term, right.term),
        defined: lean_and(vec![left.defined, right.defined]),
    })
}

fn render_rel_lean(
    value: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        RelExpr::Bool(value) => LeanBool {
            term: if *value { "True" } else { "False" }.to_owned(),
            defined: "True".to_owned(),
        },
        RelExpr::Not(value) => {
            let value = render_rel_lean(value, step, environment, budget)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) | RelExpr::Or(left, right) | RelExpr::Implies(left, right) => {
            let operator = match value {
                RelExpr::And(_, _) => "∧",
                RelExpr::Or(_, _) => "∨",
                RelExpr::Implies(_, _) => "→",
                _ => unreachable!(),
            };
            let left = render_rel_lean(left, step, environment, budget)?;
            let right = render_rel_lean(right, step, environment, budget)?;
            strict_bool_lean(operator, left, right, budget)?
        }
        RelExpr::Compare(operation, left, right) => {
            let left = render_value_lean(left, step, environment, budget)?;
            let right = render_value_lean(right, step, environment, budget)?;
            LeanBool {
                term: format!(
                    "({} {} {})",
                    left.term,
                    match operation {
                        CompareOp::Eq => "=",
                        CompareOp::NotEq => "≠",
                        CompareOp::Less => "<",
                        CompareOp::LessEq => "<=",
                        CompareOp::Greater => ">",
                        CompareOp::GreaterEq => ">=",
                    },
                    right.term
                ),
                defined: lean_and(vec![left.defined, right.defined]),
            }
        }
        RelExpr::Predicate { name, arguments } => {
            let arguments = arguments
                .iter()
                .map(|value| render_value_lean(value, step, environment, budget))
                .collect::<Result<Vec<_>, _>>()?;
            let defined = lean_and(
                arguments
                    .iter()
                    .map(|argument| argument.defined.clone())
                    .collect(),
            );
            let rendered = arguments
                .iter()
                .map(|argument| argument.term.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            LeanBool {
                term: format!("predicate \"{}\" [{rendered}]", name.as_str()),
                defined,
            }
        }
        RelExpr::ForAll {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool_lean(
            true,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
        RelExpr::Exists {
            variable,
            start,
            end,
            body,
        } => render_bounded_bool_lean(
            false,
            variable,
            *start..*end,
            body,
            step,
            environment,
            budget,
        )?,
    };
    budget.boolean(rendered)
}

fn fold_all_lean(
    values: Vec<LeanBool>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    let mut result = budget.boolean(LeanBool {
        term: "True".to_owned(),
        defined: "True".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![lean_not(&result.term), value.defined]),
        ]);
        let term = lean_and(vec![result.term, value.term]);
        result = budget.boolean(LeanBool { term, defined })?;
    }
    Ok(result)
}

fn fold_exists_lean(
    values: Vec<LeanBool>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    let mut result = budget.boolean(LeanBool {
        term: "False".to_owned(),
        defined: "True".to_owned(),
    })?;
    for value in values {
        budget.operation()?;
        let defined = lean_and(vec![
            result.defined,
            lean_or(vec![result.term.clone(), value.defined]),
        ]);
        let term = lean_or(vec![result.term, value.term]);
        result = budget.boolean(LeanBool { term, defined })?;
    }
    Ok(result)
}

fn render_bounded_bool_lean(
    all: bool,
    variable: &Identifier,
    range: core::ops::Range<i128>,
    body: &RelExpr,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    if range.end < range.start || range.end.saturating_sub(range.start) > 4096 {
        return Err(ExportError::InvalidFormula);
    }
    let mut values = Vec::new();
    for current in range {
        let mut nested = environment.clone();
        nested.insert(variable.as_str().into(), current);
        values.push(render_rel_lean(body, step, &nested, budget)?);
    }
    if all {
        fold_all_lean(values, budget)
    } else {
        fold_exists_lean(values, budget)
    }
}

fn render_temporal_lean(
    value: &TemporalFormula,
    step: &str,
    environment: &BTreeMap<String, i128>,
    budget: &mut LeanRenderBudget,
) -> Result<LeanBool, ExportError> {
    budget.operation()?;
    let rendered = match value {
        TemporalFormula::Atom(value) => render_rel_lean(value, step, environment, budget)?,
        TemporalFormula::Not(value) => {
            let value = render_temporal_lean(value, step, environment, budget)?;
            LeanBool {
                term: lean_not(&value.term),
                defined: value.defined,
            }
        }
        TemporalFormula::And(left, right) | TemporalFormula::Or(left, right) => {
            let operator = if matches!(value, TemporalFormula::And(_, _)) {
                "∧"
            } else {
                "∨"
            };
            let left = render_temporal_lean(left, step, environment, budget)?;
            let right = render_temporal_lean(right, step, environment, budget)?;
            strict_bool_lean(operator, left, right, budget)?
        }
        TemporalFormula::Next(value) => {
            render_temporal_lean(value, &format!("({step} + 1)"), environment, budget)?
        }
        TemporalFormula::Always(value) => {
            let time = budget.time_binding()?;
            let value = render_temporal_lean(value, &time, environment, budget)?;
            LeanBool {
                term: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.term),
                defined: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Eventually(value) => {
            let time = budget.time_binding()?;
            let value = render_temporal_lean(value, &time, environment, budget)?;
            LeanBool {
                term: format!("∃ {time} : Nat, {time} >= {step} ∧ ({})", value.term),
                defined: format!("∀ {time} : Nat, {time} >= {step} → ({})", value.defined),
            }
        }
        TemporalFormula::Until(left, right) => {
            let witness = budget.time_binding()?;
            let prefix = budget.time_binding()?;
            let left_at_prefix = render_temporal_lean(left, &prefix, environment, budget)?;
            let right_at_witness = render_temporal_lean(right, &witness, environment, budget)?;
            LeanBool {
                term: format!(
                    "∃ {witness} : Nat, {witness} >= {step} ∧ ({}) ∧ \
                     ∀ {prefix} : Nat, {step} <= {prefix} → {prefix} < {witness} → ({})",
                    right_at_witness.term, left_at_prefix.term
                ),
                defined: format!(
                    "(∀ {witness} : Nat, {witness} >= {step} → ({})) ∧ \
                     (∀ {prefix} : Nat, {prefix} >= {step} → ({}))",
                    right_at_witness.defined, left_at_prefix.defined
                ),
            }
        }
    };
    budget.boolean(rendered)
}


fn original_export_smt_with_limits(
    claim: &ClaimDecl,
    backend: ToolBackend,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !matches!(backend, ToolBackend::Cvc5 | ToolBackend::Z3)
        || !claim.backends().contains(&backend.spec_backend())
    {
        return Err(ExportError::BackendNotSelected);
    }
    preflight_export(claim, ExportKind::Smt, limits)?;
    let empty_environment = BTreeMap::new();
    let mut budget = SmtRenderBudget::new(limits);
    let (horizon, finite, formula) = match (claim.mode(), claim.formula()) {
        (ClaimMode::Relational, ClaimFormula::Relational(value)) => (
            1,
            false,
            render_rel_smt(value, 0, &empty_environment, &mut budget)?,
        ),
        (ClaimMode::Finite { horizon }, ClaimFormula::Temporal(value)) if horizon > 0 => {
            let mut formulas = Vec::new();
            for length in 1..=horizon {
                formulas.push(render_temporal_smt(
                    value,
                    0,
                    length,
                    &empty_environment,
                    &mut budget,
                )?);
            }
            (horizon, true, select_trace_length(formulas, &mut budget)?)
        }
        (ClaimMode::UnboundedProof | ClaimMode::Inductive, _) => {
            return Err(ExportError::UnsupportedMode);
        }
        _ => return Err(ExportError::InvalidFormula),
    };
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    collect_claim(claim, &mut paths, &mut predicates);
    let mut source = format!(
        "; zeno-fcis/smt-obligation/1\n; claim-id {}\n(set-logic ALL)\n(set-option :produce-models true)\n",
        claim.id().get()
    );
    if backend == ToolBackend::Cvc5 {
        source.push_str("(set-option :produce-proofs true)\n");
    }
    if finite {
        source.push_str("(declare-const zeno_trace_len Int)\n");
        source.push_str(&format!(
            "(assert (and (<= 1 zeno_trace_len) (<= zeno_trace_len {horizon})))\n"
        ));
    }
    for step in 0..horizon {
        for path in &paths {
            let name = smt_path(path, step);
            source.push_str(&format!("(declare-const {name} Int)\n"));
            source.push_str(&format!("(assert {})\n", smt_i128_range(&name)));
        }
    }
    for (name, arity) in predicates {
        source.push_str(&format!(
            "(declare-fun pred_{} ({}) Bool)\n",
            smt_identifier(name.as_str()),
            vec!["Int"; arity].join(" ")
        ));
    }
    source.push_str(&format!(
        "(assert (not {}))
(check-sat)
",
        smt_and(vec![formula.defined, formula.term])
    ));
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    exported(backend, claim, source.into_bytes())
}

fn original_export_inductive_smt_with_limits(
    claim: &ClaimDecl,
    spec: &ProjectSpec,
    backend: ToolBackend,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !matches!(backend, ToolBackend::Cvc5 | ToolBackend::Z3)
        || !claim.backends().contains(&backend.spec_backend())
    {
        return Err(ExportError::BackendNotSelected);
    }
    let (ClaimMode::Inductive, ClaimFormula::Relational(invariant)) =
        (claim.mode(), claim.formula())
    else {
        return Err(ExportError::UnsupportedMode);
    };
    let mut hypotheses = step_hypotheses(claim, spec.laws())?;
    let after = invariant_at(invariant, ProjectionRoot::Post).ok_or(ExportError::InvalidFormula)?;
    let grouped: Vec<(&LawDecl, Option<i128>)> = hypotheses
        .every_commit
        .iter()
        .map(|law| (law, None))
        .chain(
            hypotheses
                .accepts
                .iter()
                .map(|law| (law, Some(ACCEPT_KIND))),
        )
        .chain(
            hypotheses
                .committed_failures
                .iter()
                .map(|law| (law, Some(COMMITTED_FAILURE_KIND))),
        )
        .collect();
    let parts: Vec<&RelExpr> = grouped
        .iter()
        .map(|(law, _)| law.formula())
        .chain([invariant, &after])
        .collect();
    for part in &parts {
        let single = ClaimDecl::new(
            claim.id(),
            claim.name().clone(),
            claim.backends().to_vec(),
            ClaimMode::Relational,
            ClaimFormula::Relational((*part).clone()),
        );
        preflight_export(&single, ExportKind::Smt, limits)?;
    }
    let empty_environment = BTreeMap::new();
    let mut budget = SmtRenderBudget::new(limits);
    let mut rendered = Vec::with_capacity(parts.len());
    for part in &parts {
        rendered.push(render_rel_smt(part, 0, &empty_environment, &mut budget)?);
    }
    let mut paths = BTreeSet::new();
    let mut predicates = BTreeMap::new();
    for part in &parts {
        collect_rel(part, &mut paths, &mut predicates);
    }
    let ids = |group: &[StableId]| {
        group
            .iter()
            .map(|law| law.get().to_string())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let assumptions = claim.assumptions();
    let mut source = format!(
        "; zeno-fcis/smt-inductive-obligation/1\n; claim-id {}\n; every commit: {}\n; accepts: {}\n; committed failures: {}\n(set-logic ALL)\n(set-option :produce-models true)\n",
        claim.id().get(),
        ids(assumptions.every_commit()),
        ids(assumptions.accepts()),
        ids(assumptions.committed_failures()),
    );
    if backend == ToolBackend::Cvc5 {
        source.push_str("(set-option :produce-proofs true)\n");
    }
    for path in &paths {
        let name = smt_path(path, 0);
        source.push_str(&format!("(declare-const {name} Int)\n"));
        source.push_str(&format!("(assert {})\n", smt_i128_range(&name)));
        if let Some(domain) = declared_domain(spec, path) {
            let member = match &domain {
                DeclaredDomain::Values(values) => smt_or(
                    values
                        .iter()
                        .map(|value| format!("(= {name} {})", smt_int(*value)))
                        .collect(),
                ),
                DeclaredDomain::Range(range) => format!(
                    "(and (<= {} {name}) (<= {name} {}))",
                    smt_int(range.min()),
                    smt_int(range.max())
                ),
            };
            source.push_str(&format!("; declared domain\n(assert {member})\n"));
            hypotheses.domains.insert(path.clone(), domain);
        }
    }
    for (name, arity) in predicates {
        source.push_str(&format!(
            "(declare-fun pred_{} ({}) Bool)\n",
            smt_identifier(name.as_str()),
            vec!["Int"; arity].join(" ")
        ));
    }
    if hypotheses.splits_decisions() {
        source.push_str(&format!(
            "; decision kind: {ACCEPT_KIND} accept, {COMMITTED_FAILURE_KIND} committed failure\n(declare-const {DECISION_KIND} Int)\n(assert (or (= {DECISION_KIND} {ACCEPT_KIND}) (= {DECISION_KIND} {COMMITTED_FAILURE_KIND})))\n"
        ));
    }
    let Some((after, before_and_laws)) = rendered.split_last() else {
        return Err(ExportError::InvalidFormula);
    };
    for (index, part) in before_and_laws.iter().enumerate() {
        let holds = smt_and(vec![part.defined.clone(), part.term.clone()]);
        let (label, assertion) = match grouped.get(index) {
            Some((law, None)) => (format!("law {}", law.id().get()), holds),
            Some((law, Some(kind))) => (
                format!(
                    "law {} on {}",
                    law.id().get(),
                    if *kind == ACCEPT_KIND {
                        "accepts"
                    } else {
                        "committed failures"
                    }
                ),
                format!("(=> (= {DECISION_KIND} {kind}) {holds})"),
            ),
            None => ("invariant before the step".to_owned(), holds),
        };
        source.push_str(&format!("; {label}\n(assert {assertion})\n"));
    }
    source.push_str(&format!(
        "; invariant after the step\n(assert (not {}))\n(check-sat)\n",
        smt_and(vec![after.defined.clone(), after.term.clone()])
    ));
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    let mut obligation = exported(backend, claim, source.into_bytes())?;
    obligation.hypotheses = hypotheses;
    Ok(obligation)
}

impl UndefinedReason {
    const fn from_original_indeterminate(reason: IndeterminateReason) -> Option<Self> {
        match reason {
            IndeterminateReason::Overflow => Some(Self::Overflow),
            IndeterminateReason::DivisionByZero => Some(Self::DivisionByZero),
            IndeterminateReason::NonExactDivision => Some(Self::NonExactDivision),
            _ => None,
        }
    }
}
