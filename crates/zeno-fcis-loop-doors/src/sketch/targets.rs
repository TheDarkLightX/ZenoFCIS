//! Explicit-target proposals. There is no original program in this interface.

use super::*;
use crate::expr::{Allow, Vocabulary, compare_side, depth, parse, print_symbolic};

/// Owned, validated inputs for one invocation over explicit examples.
///
/// Fields are private and only shared borrows enter search. Columns and targets
/// cannot be changed during an invocation; no search scratch is retained here.
#[derive(Clone, Debug)]
pub struct TargetProblem {
    vocab: Vocabulary,
    columns: Vec<Vec<i128>>,
    targets: Vec<Vec<i128>>,
    sketch: Expr,
    settings: Settings,
}

impl TargetProblem {
    /// Validate distinct rows and one target per row, without enumerating the
    /// Cartesian input domain. Hole numbers follow the parser's reading order.
    pub fn new(
        vocab: Vocabulary,
        rows: Vec<Vec<i128>>,
        targets: Vec<i128>,
        sketch: Expr,
        settings: Settings,
    ) -> Result<Self> {
        if vocab.outputs().len() != 1 {
            return Err(Refusal::new("target-output-count"));
        }
        if rows.is_empty() || rows.len() != targets.len() {
            return Err(Refusal::new("target-row-count"));
        }
        if rows.len() as u128 > u128::from(crate::subject::DEFAULT_MAX_TUPLES) {
            return Err(Refusal::new("target-row-budget"));
        }
        if vocab.inputs().iter().chain(vocab.outputs()).any(|d| {
            let (lo, hi) = d.bounds();
            lo > hi
        }) {
            return Err(Refusal::new("target-domain"));
        }
        let mut distinct = BTreeSet::new();
        for (row, target) in rows.iter().zip(&targets) {
            if row.len() != vocab.inputs().len() {
                return Err(Refusal::new("target-row-width"));
            }
            if !distinct.insert(row) {
                return Err(Refusal::new("target-duplicate-row"));
            }
            if !row.iter().zip(vocab.inputs()).all(|(v, d)| d.contains(*v))
                || !vocab.outputs()[0].contains(*target)
            {
                return Err(Refusal::new("target-value-range"));
            }
        }
        // AST callers receive the same bounds as text callers, before recursive
        // printing/type validation. Parsing also checks choice arity and nesting.
        if depth(&sketch) > crate::expr::MAX_DEPTH {
            return Err(Refusal::new("expression-too-deep"));
        }
        if size(&sketch) > crate::expr::MAX_NODES {
            return Err(Refusal::new("expression-too-large"));
        }
        let sketch = parse(
            &print_symbolic(&sketch),
            0,
            Allow {
                holes: true,
                params: false,
                keep: false,
            },
        )?;
        vocab.check_names(&sketch)?;
        let sketch = vocab.resolve_variants(&sketch)?;
        validate_kind(&sketch, vocab.output_kinds()[0], &vocab)?;
        let columns = (0..vocab.inputs().len())
            .map(|j| rows.iter().map(|row| row[j]).collect())
            .collect();
        Ok(Self {
            vocab,
            columns,
            targets: vec![targets],
            sketch,
            settings,
        })
    }
}

/// Check every alternative, including alternatives never selected by search.
/// Literal kinds come from context; Bool and Int columns remain distinct.
fn validate_kind(node: &Expr, want: Kind, vocab: &Vocabulary) -> Result<()> {
    match node {
        Expr::Int(v) if want == Kind::Int || *v == 0 || *v == 1 => Ok(()),
        Expr::Var(name) if vocab.kind(name)? == want => Ok(()),
        Expr::Hole(_, Hole::Choice(alternatives)) => {
            for alternative in alternatives {
                validate_kind(alternative, want, vocab)?;
            }
            Ok(())
        }
        Expr::Hole(_, Hole::Const(Some((lo, hi))))
            if want == Kind::Int || (*lo >= 0 && *hi <= 1) =>
        {
            Ok(())
        }
        Expr::Hole(_, Hole::Const(None) | Hole::Expr) => {
            Err(Refusal::new("target-finite-holes-only"))
        }
        Expr::Keep | Expr::Param(_) => Err(Refusal::new("target-original-or-parameter")),
        Expr::Not(a) if want == Kind::Bool => validate_kind(a, Kind::Bool, vocab),
        Expr::If(c, a, b) => {
            validate_kind(c, Kind::Bool, vocab)?;
            validate_kind(a, want, vocab)?;
            validate_kind(b, want, vocab)
        }
        Expr::Bin(op, a, b) => {
            let side = if matches!(op, Op::Add | Op::Sub) && want == Kind::Int {
                Kind::Int
            } else if op.is_boolean() && want == Kind::Bool {
                Kind::Bool
            } else if op.is_compare() && want == Kind::Bool {
                let side = compare_side(a, b, vocab)?;
                if !matches!(op, Op::Eq | Op::Ne) && side != Kind::Int {
                    return Err(Refusal::new("target-kind:order"));
                }
                side
            } else {
                return Err(Refusal::new("target-kind:operator"));
            };
            validate_kind(a, side, vocab)?;
            validate_kind(b, side, vocab)
        }
        _ => Err(Refusal::new("target-kind")),
    }
}

/// Untrusted example-search result. No program, receipt or refinement claim is
/// produced. All expressions/fills are owned; no alias into search scratch escapes.
#[derive(Clone, Debug)]
pub struct TargetResult {
    /// Completed means agreement with these examples in the library evaluator.
    pub status: Status,
    /// First completion in the frozen search order, without cost ranking.
    pub expression: Option<Expr>,
    /// Fills keyed by reading-order local hole number.
    pub holes: BTreeMap<usize, Fill>,
    /// Search detail, when search returned normally (including a bounded stop).
    pub search: Option<OutputSearch>,
    /// Terminal assignments or rejected subtrees charged before stopping.
    pub evaluated: usize,
    /// Reserved evaluation/derivation work, including work on refused proposals.
    pub work: u64,
    /// Stop or refusal reason, absent for a completed proposal.
    pub reason: Option<String>,
}

/// Propose one expression against explicit examples using the existing solver.
///
/// Every invocation creates fresh solver/effect-class scratch. The original-based
/// API keeps its completion ranking and lowering behavior. This API has neither
/// an original program nor an expression synthesizer and never lowers anything.
#[must_use]
pub fn propose_targets(problem: &TargetProblem) -> TargetResult {
    let mut solver = Solver {
        subject: TargetView {
            vocab: &problem.vocab,
            columns: &problem.columns,
            targets: &problem.targets,
            size: problem.targets[0].len(),
            original: None,
        },
        settings: problem.settings,
        synthesizer: None,
        evaluated: 0,
        work: 0,
    };
    let mut result = TargetResult {
        status: Status::Inconclusive,
        expression: None,
        holes: BTreeMap::new(),
        search: None,
        evaluated: 0,
        work: 0,
        reason: Some(String::from("sketch-budget")),
    };
    if problem.settings.max_assignments == 0 || problem.settings.max_work == 0 {
        return result;
    }
    let mut kinds = BTreeMap::new();
    let outcome = hole_kinds(
        &problem.sketch,
        problem.vocab.output_kinds()[0],
        &problem.vocab,
        &mut kinds,
    )
    .and_then(|()| solver.solve_output(0, &problem.sketch, &kinds, &constants(&problem.sketch)));
    result.evaluated = solver.evaluated;
    result.work = solver.work;
    match outcome {
        Ok((found, info)) => {
            result.status = info.verdict.clone().unwrap_or(Status::NotFound);
            result.search = Some(info);
            if let Some(completion) = found.into_iter().next() {
                result.status = Status::Completed;
                result.expression = Some(completion.expression);
                result.holes = completion.assignment;
                result.reason = None;
            } else {
                result.reason = Some(String::from("no completion on explicit examples"));
            }
        }
        Err(refusal) => {
            result.status = if refusal.0 == "sketch-work-budget" {
                Status::Inconclusive
            } else {
                Status::Refused
            };
            result.reason = Some(refusal.0);
        }
    }
    result
}
