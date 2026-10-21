//! Restricted, law-relative integer induction checked by the Lean kernel.
//!
//! Export constructs inert source. It grants no runtime authority. The initial
//! state and the binding from runtime observations to these fields remain explicit
//! application premises. Arithmetic is strict, including underneath implication.

use super::*;

const IDENTITY: &str = "zeno-fcis/lean-inductive/1";
const THEOREM: &str = "history_holds";

/// Exports a conditional finite-history theorem for a linear inductive claim.
pub fn export_inductive_lean(
    claim: &ClaimDecl,
    spec: &ProjectSpec,
) -> Result<ExportedObligation, ExportError> {
    export_inductive_lean_with_limits(claim, spec, ExportLimits::default())
}

/// Exports within the same deterministic resource envelope as SMT induction.
pub fn export_inductive_lean_with_limits(
    claim: &ClaimDecl,
    spec: &ProjectSpec,
    limits: ExportLimits,
) -> Result<ExportedObligation, ExportError> {
    if !claim.backends().contains(&BackendId::Lean) {
        return Err(ExportError::BackendNotSelected);
    }
    let (ClaimMode::Inductive, ClaimFormula::Relational(invariant)) =
        (claim.mode(), claim.formula())
    else {
        return Err(ExportError::UnsupportedMode);
    };
    let mut hypotheses = step_hypotheses(claim, spec.laws())?;
    let after = invariant_at(invariant, ProjectionRoot::Post).ok_or(ExportError::InvalidFormula)?;
    let resolved = hypotheses.clone();
    let grouped = grouped_laws(&resolved);
    let parts: Vec<&RelExpr> = grouped
        .iter()
        .map(|(law, _)| law.formula())
        .chain([invariant, &after])
        .collect();
    let mut paths = BTreeSet::new();
    for part in &parts {
        let single = ClaimDecl::new(
            claim.id(),
            claim.name().clone(),
            claim.backends().to_vec(),
            ClaimMode::Relational,
            ClaimFormula::Relational((*part).clone()),
        );
        preflight_smt(&single, limits)?;
        collect_rel(part, &mut paths, &mut BTreeMap::new());
    }
    let mut budget = SmtRenderBudget::new(limits);
    let rendered: Vec<SmtBool> = parts
        .iter()
        .map(|part| render_relation(part, &mut budget))
        .collect::<Result<_, _>>()?;
    let mut assumptions = Vec::new();
    for path in &paths {
        let term = projection(path)?;
        assumptions.push(format!("inI128 {term}"));
        hypotheses.paths.insert(path.clone());
        if let Some(domain) = declared_domain(spec, path) {
            assumptions.push(domain_formula(&term, &domain));
            hypotheses.domains.insert(path.clone(), domain);
        }
    }
    assumptions.push("(kind = 1 ∨ kind = 2)".to_owned());
    let mut manifest = b"zeno-fcis/lean-inductive-laws/1\0".to_vec();
    for (index, (law, kind)) in grouped.iter().enumerate() {
        let holds = satisfied(&rendered[index]);
        manifest.extend_from_slice(&law.id().get().to_be_bytes());
        manifest.push(kind.unwrap_or(0));
        manifest.extend_from_slice(&(holds.len() as u64).to_be_bytes());
        manifest.extend_from_slice(holds.as_bytes());
        assumptions.push(match kind {
            Some(kind) => format!("(kind = {kind} → {holds})"),
            None => holds,
        });
    }
    let project_sha = hash_hex(RustCryptoSha256::hash(&spec.canonical_bytes()?));
    let law_sha = hash_hex(RustCryptoSha256::hash(&manifest));
    let mut source = format!(
        "-- {IDENTITY}\n-- claim-id {}\n-- project-sha256 {project_sha}\n-- law-manifest-sha256 {law_sha}\n-- theorem {THEOREM}\n\
         -- Conditional base case: the application's exact initial state satisfies StateInv.\n\
         -- Runtime premises: all assumed laws are enforced for their stated decision kinds;\n\
         -- observed values lie in their declared domains; rejects/refusals commit nothing\n\
         -- and preserve state; the runtime law checker reads these same projection paths.\n\
         -- Pilot base-case and observation checks: tests/induction.rs (durable-counter),\n\
         -- tests/claims.rs (account-lockout); see docs/INDUCTIVE_CLAIMS.md.\n\
         import Init\n\n\
         def inI128 (x : Int) : Prop := {} <= x ∧ x <= {}\n\n",
        claim.id().get(),
        integer(i128::MIN),
        integer(i128::MAX)
    );
    source.push_str("structure State where\n");
    let mut state_fields = BTreeSet::new();
    let mut input_fields = BTreeSet::new();
    for path in &paths {
        match path.root() {
            ProjectionRoot::Pre | ProjectionRoot::Post => {
                state_fields.insert(field_name(path));
            }
            ProjectionRoot::Command | ProjectionRoot::Context => {
                input_fields.insert(field_name(path));
            }
            ProjectionRoot::Effects | ProjectionRoot::Outbox | ProjectionRoot::Events => {
                return Err(ExportError::UnsupportedMode);
            }
        }
    }
    // Unit supplies a constructor even for a constant invariant with no paths.
    source.push_str("  unit : Unit := ()\n");
    for field in state_fields {
        source.push_str(&format!("  {field} : Int\n"));
    }
    source.push_str("\nstructure Inputs where\n  unit : Unit := ()\n");
    for field in input_fields {
        source.push_str(&format!("  {field} : Int\n"));
    }
    let before = &rendered[grouped.len()];
    let inv = satisfied(before).replace("pre.", "s.");
    source.push_str(&format!("\ndef StateInv (s : State) : Prop := {inv}\n\ndef Admitted (pre post : State) (inputs : Inputs) (kind : Nat) : Prop :=\n  {}\n\n", conjunction(assumptions)));
    source.push_str("theorem step_preserves (pre post : State) (inputs : Inputs) (kind : Nat) :\n  StateInv pre → Admitted pre post inputs kind → StateInv post := by\n  intro hinv hadmitted\n  simp only [StateInv, Admitted, inI128, true_and, and_true] at *\n  repeat' first | apply And.intro | intro\n  all_goals\n    try simp only [Decidable.imp_iff_not_or] at *\n    omega\n\n");
    source.push_str(HISTORY);
    if source.len() > limits.max_source_bytes() {
        return Err(ExportError::ResourceLimit);
    }
    let mut result = exported(ToolBackend::Lean, claim, source.into_bytes())?;
    result.hypotheses = hypotheses;
    Ok(result)
}

fn grouped_laws(hypotheses: &StepHypotheses) -> Vec<(&LawDecl, Option<u8>)> {
    hypotheses
        .every_commit
        .iter()
        .map(|law| (law, None))
        .chain(hypotheses.accepts.iter().map(|law| (law, Some(1))))
        .chain(
            hypotheses
                .committed_failures
                .iter()
                .map(|law| (law, Some(2))),
        )
        .collect()
}

const HISTORY: &str = r#"def Step (s s' : State) : Prop :=
  (∃ inputs kind, Admitted s s' inputs kind) ∨ s' = s

inductive Chain (step : State → State → Prop) : State → List State → Prop where
  | nil (g : State) : Chain step g []
  | cons {g s : State} {tail : List State} : step g s → Chain step s tail → Chain step g (s :: tail)

theorem history_holds (g : State) (hg : StateInv g) :
  ∀ (h : List State), Chain Step g h → ∀ s ∈ h, StateInv s := by
  intro h
  induction h generalizing g with
  | nil => intro _ target member; cases member
  | cons next tail ih =>
    intro chain target member
    cases chain with
    | cons transition rest =>
      have hs : StateInv next := by
        rcases transition with ⟨inputs, kind, admitted⟩ | same
        · exact step_preserves g next inputs kind hg admitted
        · simpa only [same] using hg
      rcases List.mem_cons.mp member with rfl | member
      · exact hs
      · exact ih next hs rest target member

#print axioms history_holds
"#;

fn integer(value: i128) -> String {
    if value < 0 {
        format!("(-{} : Int)", value.unsigned_abs())
    } else {
        format!("({value} : Int)")
    }
}

fn conjunction(terms: Vec<String>) -> String {
    match terms.as_slice() {
        [] => "True".to_owned(),
        [term] => term.clone(),
        _ => format!("({})", terms.join(" ∧ ")),
    }
}

fn field_name(path: &ProjectionPath) -> String {
    let prefix = match path.root() {
        ProjectionRoot::Pre | ProjectionRoot::Post => "p",
        ProjectionRoot::Command => "command",
        ProjectionRoot::Context => "context",
        ProjectionRoot::Effects => "effects",
        ProjectionRoot::Outbox => "outbox",
        ProjectionRoot::Events => "events",
    };
    format!(
        "{prefix}_{}",
        path.segments()
            .iter()
            .map(|id| id.get().to_string())
            .collect::<Vec<_>>()
            .join("_")
    )
}

fn projection(path: &ProjectionPath) -> Result<String, ExportError> {
    let owner = match path.root() {
        ProjectionRoot::Pre => "pre",
        ProjectionRoot::Post => "post",
        ProjectionRoot::Command | ProjectionRoot::Context => "inputs",
        ProjectionRoot::Effects | ProjectionRoot::Outbox | ProjectionRoot::Events => {
            return Err(ExportError::UnsupportedMode);
        }
    };
    Ok(format!("({owner}.{})", field_name(path)))
}

fn domain_formula(term: &str, domain: &DeclaredDomain) -> String {
    match domain {
        DeclaredDomain::Values(values) => {
            let members = values
                .iter()
                .map(|value| format!("{term} = {}", integer(*value)))
                .collect::<Vec<_>>();
            if members.is_empty() {
                "False".to_owned()
            } else {
                format!("({})", members.join(" ∨ "))
            }
        }
        DeclaredDomain::Range(range) => format!(
            "({} <= {term} ∧ {term} <= {})",
            integer(range.min()),
            integer(range.max())
        ),
    }
}

fn satisfied(value: &SmtBool) -> String {
    conjunction(vec![value.defined.clone(), value.term.clone()])
}

fn checked_binary(
    operator: &str,
    left: SmtValue,
    right: SmtValue,
    budget: &mut SmtRenderBudget,
) -> Result<SmtValue, ExportError> {
    budget.operation()?;
    let term = format!("({} {operator} {})", left.term, right.term);
    let defined = conjunction(vec![left.defined, right.defined, format!("inI128 {term}")]);
    budget.value(SmtValue { term, defined })
}

fn render_value(value: &ValueExpr, budget: &mut SmtRenderBudget) -> Result<SmtValue, ExportError> {
    budget.operation()?;
    let result = match value {
        ValueExpr::Int(value) => SmtValue {
            term: integer(*value),
            defined: "True".to_owned(),
        },
        ValueExpr::Projection(path) => SmtValue {
            term: projection(path)?,
            defined: "True".to_owned(),
        },
        ValueExpr::Add(left, right) => checked_binary(
            "+",
            render_value(left, budget)?,
            render_value(right, budget)?,
            budget,
        )?,
        ValueExpr::Sub(left, right) => checked_binary(
            "-",
            render_value(left, budget)?,
            render_value(right, budget)?,
            budget,
        )?,
        ValueExpr::Mul(left, right) => {
            if !is_constant(left) && !is_constant(right) {
                return Err(ExportError::UnsupportedMode);
            }
            checked_binary(
                "*",
                render_value(left, budget)?,
                render_value(right, budget)?,
                budget,
            )?
        }
        ValueExpr::Var(_) | ValueExpr::Div(_, _, _) | ValueExpr::Sum { .. } => {
            return Err(ExportError::UnsupportedMode);
        }
    };
    budget.value(result)
}

fn is_constant(value: &ValueExpr) -> bool {
    match value {
        ValueExpr::Int(_) => true,
        ValueExpr::Add(left, right) | ValueExpr::Sub(left, right) | ValueExpr::Mul(left, right) => {
            is_constant(left) && is_constant(right)
        }
        ValueExpr::Projection(_)
        | ValueExpr::Var(_)
        | ValueExpr::Div(_, _, _)
        | ValueExpr::Sum { .. } => false,
    }
}

fn strict_binary(
    operator: &str,
    left: &RelExpr,
    right: &RelExpr,
    budget: &mut SmtRenderBudget,
) -> Result<SmtBool, ExportError> {
    let left = render_relation(left, budget)?;
    let right = render_relation(right, budget)?;
    budget.boolean(SmtBool {
        term: format!("({} {operator} {})", left.term, right.term),
        defined: conjunction(vec![left.defined, right.defined]),
    })
}

fn render_relation(expr: &RelExpr, budget: &mut SmtRenderBudget) -> Result<SmtBool, ExportError> {
    budget.operation()?;
    let result = match expr {
        RelExpr::Bool(value) => SmtBool {
            term: if *value { "True" } else { "False" }.to_owned(),
            defined: "True".to_owned(),
        },
        RelExpr::Not(inner) => {
            let value = render_relation(inner, budget)?;
            SmtBool {
                term: format!("¬ ({})", value.term),
                defined: value.defined,
            }
        }
        RelExpr::And(left, right) => strict_binary("∧", left, right, budget)?,
        RelExpr::Or(left, right) => strict_binary("∨", left, right, budget)?,
        RelExpr::Implies(left, right) => strict_binary("→", left, right, budget)?,
        RelExpr::Compare(op, left, right) => {
            let left = render_value(left, budget)?;
            let right = render_value(right, budget)?;
            let op = match op {
                CompareOp::Eq => "=",
                CompareOp::NotEq => "≠",
                CompareOp::Less => "<",
                CompareOp::LessEq => "<=",
                CompareOp::Greater => ">",
                CompareOp::GreaterEq => ">=",
            };
            SmtBool {
                term: format!("({} {op} {})", left.term, right.term),
                defined: conjunction(vec![left.defined, right.defined]),
            }
        }
        RelExpr::Predicate { .. } | RelExpr::ForAll { .. } | RelExpr::Exists { .. } => {
            return Err(ExportError::UnsupportedMode);
        }
    };
    budget.boolean(result)
}

#[cfg(test)]
#[path = "lean_inductive_tests.rs"]
mod tests;

pub(super) fn source_is_bound(obligation: &ExportedObligation) -> bool {
    commitment::<RustCryptoSha256>(zeno_fcis_codec::domains::FORMAL_SOURCE, obligation.source())
        .is_ok_and(|hash| hash == obligation.source_hash)
        && std::str::from_utf8(obligation.source()).is_ok_and(|source| {
            source.starts_with(&format!(
                "-- {IDENTITY}\n-- claim-id {}\n",
                obligation.claim_id.get()
            )) && source
                .lines()
                .filter(|line| *line == "-- theorem history_holds")
                .count()
                == 1
                && source
                    .lines()
                    .filter(|line| *line == "#print axioms history_holds")
                    .count()
                    == 1
        })
}

pub(super) fn theorem_report_is_bound(text: &str) -> bool {
    let mut names = text.lines().filter_map(|line| {
        line.split_once("depends on axioms:")
            .map(|(name, _)| name.trim().trim_matches('\''))
    });
    names.next() == Some(THEOREM) && names.next().is_none()
}
