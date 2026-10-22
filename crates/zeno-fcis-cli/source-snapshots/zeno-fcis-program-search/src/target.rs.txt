//! Care-masked top-down synthesis of a supplied target, for proposers outside
//! this crate.
//!
//! [`synthesize_target`](crate::target::synthesize_target) runs the crate's
//! own top-down solver (the one the portfolio uses: case splits on input
//! predicates, arithmetic and equality inversion, two-valued selection) on any
//! target vector rather than on a program's outputs. Positions whose `care`
//! flag is false are don't-cares: the plan may take any value there. The
//! result is an untrusted plan over the inputs; nothing here checks a
//! program, and a caller that turns a plan into a program must have it judged
//! like any other proposal.
//!
//! Deterministic, no I/O. Over the work or cell budget the solve stops with a
//! typed [`Stop`] and no plan.

use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::Domain;

use crate::Stop;
use crate::ir::{Kind, Node, domain_kind};
use crate::space::Meter;
use crate::topdown::solve_target;

/// The kind of value a target holds.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TargetKind {
    /// Integers.
    Int,
    /// Booleans, as 0 and 1.
    Bool,
}

/// One plan instruction; operands are indices of earlier plan nodes.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PlanNode {
    /// The input at this position.
    Input(u16),
    /// An integer constant.
    Int(i128),
    /// A Boolean constant.
    Bool(bool),
    /// Checked addition.
    Add(usize, usize),
    /// Checked subtraction.
    Sub(usize, usize),
    /// Checked nonnegative multiplication.
    Mul(usize, usize),
    /// Rounded-down division.
    DivFloor(usize, usize),
    /// Rounded-up division.
    DivCeil(usize, usize),
    /// Full product followed by rounded-down division.
    MulDivFloor(usize, usize, usize),
    /// Full product followed by rounded-up division.
    MulDivCeil(usize, usize, usize),
    /// Exact comparison of two three-factor products.
    ProductsGe([usize; 3], [usize; 3]),
    /// Equality.
    Eq(usize, usize),
    /// Integer less-than.
    Lt(usize, usize),
    /// Boolean and.
    And(usize, usize),
    /// Boolean negation.
    Not(usize),
    /// `Select(c, a, b)`: `a` where `c` holds, else `b`.
    Select(usize, usize, usize),
}

/// A plan as a shared graph: operands come before their users, and `root`
/// is the plan's value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    /// Instructions in operand-first order.
    pub nodes: Vec<PlanNode>,
    /// Index of the result instruction.
    pub root: usize,
}

/// Limits of one solve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetBudget {
    /// Search depth (clamped to [`crate::MAX_DEPTH`]).
    pub depth: u32,
    /// Case-split predicates tried per subproblem.
    pub width: usize,
    /// Subproblems solved before only constants and atoms remain.
    pub solve_limit: u32,
    /// Most `i128` cells of plan values.
    pub max_cells: u64,
    /// Work units, counted as in [`crate::Budget::work_units`].
    pub work_units: u64,
}

impl Default for TargetBudget {
    fn default() -> Self {
        Self {
            depth: 4,
            width: 6,
            solve_limit: 20_000,
            max_cells: 1 << 26,
            work_units: 1 << 32,
        }
    }
}

/// Synthesizes a plan over the inputs whose value equals `target` wherever
/// `care` holds.
///
/// `columns[k]` is input `k`'s value on every considered tuple; `target` and
/// `care` have one entry per tuple. Every input domain must be `Bool` or
/// `Int`. `Ok(None)` means no plan was found within the depth and solve
/// limits; it never means that none exists.
pub fn synthesize_target(
    inputs: &[Domain],
    columns: &[Vec<i128>],
    target: &[i128],
    care: &[bool],
    kind: TargetKind,
    budget: &TargetBudget,
) -> Result<Option<Plan>, Stop> {
    synthesize_target_counted(inputs, columns, target, care, kind, budget).0
}

/// [`synthesize_target`], also returning the work units it used (all of
/// them when the budget stopped it), so a caller can share one budget
/// across several solves.
pub fn synthesize_target_counted(
    inputs: &[Domain],
    columns: &[Vec<i128>],
    target: &[i128],
    care: &[bool],
    kind: TargetKind,
    budget: &TargetBudget,
) -> (Result<Option<Plan>, Stop>, u64) {
    let mut meter = Meter::new(budget.work_units);
    let result = solve_counted(inputs, columns, target, care, kind, budget, &mut meter);
    let used = match result {
        Err(Stop::WorkBudget { .. }) => budget.work_units,
        _ => meter.used().min(budget.work_units),
    };
    (result, used)
}

fn solve_counted(
    inputs: &[Domain],
    columns: &[Vec<i128>],
    target: &[i128],
    care: &[bool],
    kind: TargetKind,
    budget: &TargetBudget,
    meter: &mut Meter,
) -> Result<Option<Plan>, Stop> {
    let size = target.len();
    if columns.len() != inputs.len()
        || care.len() != size
        || columns.iter().any(|column| column.len() != size)
        || u32::try_from(size).is_err()
    {
        return Err(Stop::Inconsistent {
            code: "target-shape",
        });
    }
    if inputs.iter().any(|domain| domain_kind(*domain).is_none()) {
        return Err(Stop::Inconsistent {
            code: "unsupported-domain",
        });
    }
    let mut pos = Vec::new();
    let mut want = Vec::new();
    for (position, (value, cared)) in target.iter().zip(care).enumerate() {
        if *cared {
            pos.push(u32::try_from(position).unwrap_or(u32::MAX));
            want.push(*value);
        }
    }
    let kind = match kind {
        TargetKind::Int => Kind::Int,
        TargetKind::Bool => Kind::Bool,
    };
    let graph = solve_target(
        inputs,
        columns.to_vec(),
        size,
        &pos,
        &want,
        kind,
        budget.depth,
        budget.width,
        budget.solve_limit,
        budget.max_cells,
        meter,
    )?;
    let Some(graph) = graph else {
        return Ok(None);
    };
    let Some(root) = graph.roots.first().copied() else {
        return Err(Stop::Inconsistent { code: "plan-root" });
    };
    let nodes = graph
        .nodes
        .iter()
        .map(|node| match *node {
            Node::Input(k) => PlanNode::Input(k),
            Node::Int(value) => PlanNode::Int(value),
            Node::Bool(value) => PlanNode::Bool(value),
            Node::Add(a, b) => PlanNode::Add(a, b),
            Node::Sub(a, b) => PlanNode::Sub(a, b),
            Node::Mul(a, b) => PlanNode::Mul(a, b),
            Node::DivFloor(a, b) => PlanNode::DivFloor(a, b),
            Node::DivCeil(a, b) => PlanNode::DivCeil(a, b),
            Node::MulDivFloor(a, b, d) => PlanNode::MulDivFloor(a, b, d),
            Node::MulDivCeil(a, b, d) => PlanNode::MulDivCeil(a, b, d),
            Node::ProductsGe(a, b) => PlanNode::ProductsGe(a, b),
            Node::Eq(a, b) => PlanNode::Eq(a, b),
            Node::Lt(a, b) => PlanNode::Lt(a, b),
            Node::And(a, b) => PlanNode::And(a, b),
            Node::Not(a) => PlanNode::Not(a),
            Node::Select(c, a, b) => PlanNode::Select(c, a, b),
        })
        .collect();
    Ok(Some(Plan { nodes, root }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn value(plan: &Plan, row: &[i128]) -> Option<i128> {
        use zeno_fcis_synthesis::finite::Op;
        let arithmetic = |op: Op, args: &[i128]| {
            let columns: Vec<Vec<i128>> = args.iter().map(|v| vec![*v]).collect();
            let columns: Vec<&[i128]> = columns.iter().map(Vec::as_slice).collect();
            crate::arithmetic::evaluate(op, &columns)?.first().copied()
        };
        let mut values: Vec<i128> = Vec::new();
        for node in &plan.nodes {
            let at = |index: usize| values.get(index).copied();
            let next = match *node {
                PlanNode::Input(k) => row.get(usize::from(k)).copied()?,
                PlanNode::Int(value) => value,
                PlanNode::Bool(value) => i128::from(value),
                PlanNode::Add(a, b) => at(a)?.checked_add(at(b)?)?,
                PlanNode::Sub(a, b) => at(a)?.checked_sub(at(b)?)?,
                PlanNode::Mul(a, b) => arithmetic(Op::Mul(0, 1), &[at(a)?, at(b)?])?,
                PlanNode::DivFloor(a, b) => arithmetic(Op::DivFloor(0, 1), &[at(a)?, at(b)?])?,
                PlanNode::DivCeil(a, b) => arithmetic(Op::DivCeil(0, 1), &[at(a)?, at(b)?])?,
                PlanNode::MulDivFloor(a, b, d) => {
                    arithmetic(Op::MulDivFloor(0, 1, 2), &[at(a)?, at(b)?, at(d)?])?
                }
                PlanNode::MulDivCeil(a, b, d) => {
                    arithmetic(Op::MulDivCeil(0, 1, 2), &[at(a)?, at(b)?, at(d)?])?
                }
                PlanNode::ProductsGe(a, b) => arithmetic(
                    Op::ProductsGe([0, 1, 2], [3, 4, 5]),
                    &[
                        at(a[0])?,
                        at(a[1])?,
                        at(a[2])?,
                        at(b[0])?,
                        at(b[1])?,
                        at(b[2])?,
                    ],
                )?,
                PlanNode::Eq(a, b) => i128::from(at(a)? == at(b)?),
                PlanNode::Lt(a, b) => i128::from(at(a)? < at(b)?),
                PlanNode::And(a, b) => i128::from(at(a)? == 1 && at(b)? == 1),
                PlanNode::Not(a) => i128::from(at(a)? == 0),
                PlanNode::Select(c, a, b) => {
                    if at(c)? == 1 {
                        at(a)?
                    } else {
                        at(b)?
                    }
                }
            };
            values.push(next);
        }
        values.get(plan.root).copied()
    }

    /// x in 0..=5, flag Bool: every tuple in order, last input fastest.
    fn space() -> (Vec<Domain>, Vec<Vec<i128>>) {
        let inputs = vec![Domain::Int { min: 0, max: 5 }, Domain::Bool];
        let mut x = Vec::new();
        let mut flag = Vec::new();
        for a in 0..=5 {
            for b in 0..=1 {
                x.push(a);
                flag.push(b);
            }
        }
        (inputs, vec![x, flag])
    }

    #[test]
    fn arithmetic_plan_oracle_keeps_wide_full_products_and_product_comparisons() {
        let wide = 1i128 << 100;
        let mut plan = Plan {
            nodes: vec![
                PlanNode::Input(0),
                PlanNode::Int(3),
                PlanNode::MulDivFloor(0, 1, 1),
            ],
            root: 2,
        };
        assert_eq!(value(&plan, &[wide]), Some(wide));
        plan.nodes.push(PlanNode::ProductsGe([0, 1, 1], [0, 1, 1]));
        plan.root = 3;
        assert_eq!(value(&plan, &[wide]), Some(1));
        plan.nodes.push(PlanNode::Mul(0, 0));
        plan.root = 4;
        assert_eq!(value(&plan, &[wide]), None);
    }

    #[test]
    fn a_care_masked_target_is_met_where_it_is_cared_for() {
        let (inputs, columns) = space();
        // Where the flag holds the target is x + 2; elsewhere it is noise and
        // does not matter.
        let target: Vec<i128> = columns[0]
            .iter()
            .zip(&columns[1])
            .map(|(x, flag)| if *flag == 1 { x + 2 } else { 1000 - 7 * x })
            .collect();
        let care: Vec<bool> = columns[1].iter().map(|flag| *flag == 1).collect();
        let plan = synthesize_target(
            &inputs,
            &columns,
            &target,
            &care,
            TargetKind::Int,
            &TargetBudget::default(),
        );
        let plan = plan.ok().flatten();
        assert!(plan.is_some(), "a plan for x + 2 on the flagged tuples");
        let plan = plan.unwrap_or(Plan {
            nodes: Vec::new(),
            root: 0,
        });
        for row in 0..target.len() {
            if care[row] {
                assert_eq!(
                    value(&plan, &[columns[0][row], columns[1][row]]),
                    Some(target[row])
                );
            }
        }
        // Without the mask the noisy target needs a different, larger plan
        // or none at all: the mask is what made x + 2 enough.
        let all = vec![true; target.len()];
        let full = synthesize_target(
            &inputs,
            &columns,
            &target,
            &all,
            TargetKind::Int,
            &TargetBudget::default(),
        )
        .ok()
        .flatten();
        assert_ne!(full.as_ref(), Some(&plan));
    }

    #[test]
    fn a_boolean_region_is_described_exactly() {
        let (inputs, columns) = space();
        let target: Vec<i128> = columns[0]
            .iter()
            .zip(&columns[1])
            .map(|(x, flag)| i128::from(*x < 3 && *flag == 1))
            .collect();
        let all = vec![true; target.len()];
        let plan = synthesize_target(
            &inputs,
            &columns,
            &target,
            &all,
            TargetKind::Bool,
            &TargetBudget::default(),
        )
        .ok()
        .flatten();
        let Some(plan) = plan else {
            panic!("a plan for x < 3 && flag");
        };
        for row in 0..target.len() {
            assert_eq!(
                value(&plan, &[columns[0][row], columns[1][row]]),
                Some(target[row])
            );
        }
    }

    #[test]
    fn solves_are_deterministic() {
        let (inputs, columns) = space();
        let target: Vec<i128> = columns[0]
            .iter()
            .map(|x| if *x == 4 { 9 } else { x - 1 })
            .collect();
        let care = vec![true; target.len()];
        let run = || {
            synthesize_target(
                &inputs,
                &columns,
                &target,
                &care,
                TargetKind::Int,
                &TargetBudget::default(),
            )
        };
        let first = run();
        assert!(matches!(first, Ok(Some(_))));
        assert_eq!(first, run());
    }

    #[test]
    fn an_exhausted_work_budget_stops_with_no_plan() {
        let (inputs, columns) = space();
        let target: Vec<i128> = columns[0].iter().map(|x| x * 3 % 5).collect();
        let care = vec![true; target.len()];
        let budget = TargetBudget {
            work_units: 10,
            ..TargetBudget::default()
        };
        let stopped =
            synthesize_target(&inputs, &columns, &target, &care, TargetKind::Int, &budget);
        assert_eq!(stopped, Err(Stop::WorkBudget { limit: 10 }));
    }

    #[test]
    fn an_exhausted_cell_budget_stops_with_no_plan() {
        let (inputs, columns) = space();
        let target: Vec<i128> = columns[0].iter().map(|x| x * 3 % 5).collect();
        let care = vec![true; target.len()];
        // Each value vector holds 12 cells, so a second one is over budget.
        let budget = TargetBudget {
            max_cells: 20,
            ..TargetBudget::default()
        };
        let stopped =
            synthesize_target(&inputs, &columns, &target, &care, TargetKind::Int, &budget);
        assert!(
            matches!(stopped, Err(Stop::CellsOverBudget { cells, limit: 20 }) if cells > 20),
            "{stopped:?}"
        );
    }

    #[test]
    fn a_misshapen_target_is_refused() {
        let (inputs, columns) = space();
        let stopped = synthesize_target(
            &inputs,
            &columns,
            &[1, 2],
            &[true, true],
            TargetKind::Int,
            &TargetBudget::default(),
        );
        assert_eq!(
            stopped,
            Err(Stop::Inconsistent {
                code: "target-shape"
            })
        );
    }
}
