//! Test-only reading of closed Boolean laws after native table expansion.

use std::collections::BTreeMap;
use zeno_fcis_spec::{
    CompareOp, ProjectLimits, RelExpr, SourceLimits, ValueExpr, elaborate_project, parse_project,
};

pub(crate) fn laws(source: &str) -> BTreeMap<u32, bool> {
    let parsed = parse_project(source, SourceLimits::default())
        .unwrap_or_else(|error| panic!("table fixture: {error:?}"));
    let project = elaborate_project(parsed, ProjectLimits::default())
        .unwrap_or_else(|error| panic!("table fixture: {error:?}"));
    project
        .laws()
        .iter()
        .map(|law| (law.id().get(), closed(law.formula())))
        .collect()
}

fn closed(formula: &RelExpr) -> bool {
    match formula {
        RelExpr::Bool(value) => *value,
        RelExpr::Not(body) => !closed(body),
        RelExpr::And(a, b) => closed(a) & closed(b),
        RelExpr::Or(a, b) => closed(a) | closed(b),
        RelExpr::Compare(CompareOp::Eq, ValueExpr::Int(a), ValueExpr::Int(b)) => a == b,
        other => panic!("expected only a closed Boolean law: {other:?}"),
    }
}
