//! A reference solver for tests: it searches each query's declared constants
//! exhaustively through the script evaluator and answers as CVC5, with proof
//! steps, and Z3 would.

use super::verdict::{Answer, Answers, Query};

/// The answer the exhaustive search gives; `unknown` above `limit`
/// assignments.
pub(crate) fn search(query: &Query<'_>, limit: u128) -> Answer {
    match query.script.search(limit) {
        Some(Some(model)) => Answer::Sat(model),
        Some(None) => Answer::Unsat { proof_output: true },
        None => Answer::Unknown,
    }
}

/// Both solvers answer as the exhaustive search does.
pub(crate) fn reference(query: &Query<'_>) -> Answers {
    let answer = search(query, 1 << 22);
    Answers {
        cvc5: answer.clone(),
        z3: answer,
        millis: [None, None],
    }
}
