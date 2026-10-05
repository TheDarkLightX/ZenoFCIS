//! Exact original shape helpers over the normal nominal authoring AST.
pub(crate) use crate::spec_authoring::*;
use alloc::vec::Vec;

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

pub(crate) fn formula_shape_rel(value: &RelExpr) -> (usize, usize) {
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

pub(crate) fn formula_shape_temporal(value: &TemporalFormula) -> (usize, usize) {
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
