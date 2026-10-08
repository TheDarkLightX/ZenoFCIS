//! What one query establishes from the two solvers' answers.
//!
//! CVC5's `unsat` with proof steps is a proposal: its proof text is not
//! checked, so the result is Attested. Z3's `unsat` corroborates it and never
//! stands alone. A `sat` answer counts only when its model replays through
//! the library's own evaluators, which makes the refutation Checked whatever
//! either solver said. Anything else establishes nothing: a model that does
//! not replay, two solvers that disagree without a replayed model, `unknown`,
//! a timeout, a crash, or an `unsat` without proof steps.

use std::collections::BTreeMap;

use serde_json::{Value, json};

#[cfg(test)]
use super::smt::Script;

/// The solvers a query runs on, in the order they are asked.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Solver {
    Cvc5,
    Z3,
}

impl Solver {
    pub(crate) const BOTH: [Self; 2] = [Self::Cvc5, Self::Z3];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Cvc5 => "cvc5",
            Self::Z3 => "z3",
        }
    }
}

/// One solver's answer to one query, as the shell observed it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Answer {
    /// `unsat`; `proof_output` when CVC5 printed proof steps.
    Unsat {
        proof_output: bool,
    },
    /// `sat`, with the model's integer values.
    Sat(BTreeMap<String, i128>),
    Unknown,
    /// No usable answer: a process failure, a timeout, or other output.
    Failed(String),
    /// The tools manifest configures no such solver.
    NotConfigured,
}

impl Answer {
    fn json(&self, millis: Option<u64>) -> Value {
        let mut value = match self {
            Self::Unsat { proof_output } => {
                json!({"answer": "unsat", "proof_output": proof_output})
            }
            Self::Sat(_) => json!({"answer": "sat"}),
            Self::Unknown => json!({"answer": "unknown"}),
            Self::Failed(reason) => json!({"answer": "no-answer", "reason": reason}),
            Self::NotConfigured => json!({"answer": "not-configured"}),
        };
        if let Some(millis) = millis {
            value["ms"] = json!(millis);
        }
        value
    }
}

/// Both solvers' answers to one query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Answers {
    pub(crate) cvc5: Answer,
    pub(crate) z3: Answer,
    /// Wall-clock milliseconds of each solver's runs, CVC5 first, when it ran.
    pub(crate) millis: [Option<u64>; 2],
}

impl Answers {
    pub(crate) fn answer(&self, solver: Solver) -> &Answer {
        match solver {
            Solver::Cvc5 => &self.cvc5,
            Solver::Z3 => &self.z3,
        }
    }

    /// The answers as the reports print them; model values belong to the
    /// replayed witness, not here.
    pub(crate) fn json(&self) -> Value {
        json!({
            "cvc5": self.cvc5.json(self.millis[0]),
            "z3": self.z3.json(self.millis[1]),
        })
    }
}

/// One query as the shell runs it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Query<'a> {
    /// A stable name, unique within one check, usable as a file name.
    pub(crate) label: &'a str,
    /// The query, which the tests' reference solver searches exhaustively.
    #[cfg(test)]
    pub(crate) script: &'a Script,
    /// Its SMT-LIB text, which the solvers read.
    pub(crate) source: &'a str,
}

/// Runs one query on every configured solver.
pub(crate) type Solve<'s> = dyn FnMut(&Query<'_>) -> Answers + 's;

/// What one query established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Judgment<W> {
    /// CVC5 answered `unsat` with proof steps, which are not checked
    /// (Attested); `corroborated` when Z3 also answered `unsat`.
    Holds { corroborated: bool },
    /// A model replayed through the library's evaluators (Checked).
    /// `disagreement` when the other solver answered `unsat`.
    Refuted {
        witness: W,
        solver: Solver,
        disagreement: bool,
    },
    /// Nothing was established, for the reason given.
    Inconclusive(String),
}

impl<W> Judgment<W> {
    /// The stable status name.
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Self::Holds { .. } => "holds",
            Self::Refuted { .. } => "refuted",
            Self::Inconclusive(_) => "inconclusive",
        }
    }

    /// The evidence level of the status, as ADR 0003 names it.
    pub(crate) fn evidence(&self) -> &'static str {
        match self {
            Self::Holds { .. } => "attested",
            Self::Refuted { .. } => "checked",
            Self::Inconclusive(_) => "none",
        }
    }
}

/// Judges one query's answers: a model that replays refutes, CVC5's
/// `unsat` with proof steps holds, and anything else is inconclusive.
/// `replay` returns the witness the library's evaluators confirmed, or why
/// they did not.
pub(crate) fn judge<W>(
    answers: &Answers,
    replay: impl Fn(&BTreeMap<String, i128>) -> Result<W, String>,
) -> Judgment<W> {
    let mut failures = Vec::new();
    for solver in Solver::BOTH {
        if let Answer::Sat(model) = answers.answer(solver) {
            match replay(model) {
                Ok(witness) => {
                    let other = match solver {
                        Solver::Cvc5 => &answers.z3,
                        Solver::Z3 => &answers.cvc5,
                    };
                    return Judgment::Refuted {
                        witness,
                        solver,
                        disagreement: matches!(other, Answer::Unsat { .. }),
                    };
                }
                Err(reason) => failures.push(format!(
                    "the {} model did not replay: {reason}",
                    solver.name()
                )),
            }
        }
    }
    if !failures.is_empty() {
        let disagree = Solver::BOTH
            .iter()
            .any(|solver| matches!(answers.answer(*solver), Answer::Unsat { .. }));
        let prefix = if disagree {
            "the solvers disagree: "
        } else {
            ""
        };
        return Judgment::Inconclusive(format!("{prefix}{}", failures.join("; ")));
    }
    match (&answers.cvc5, &answers.z3) {
        (Answer::Unsat { proof_output: true }, z3) => Judgment::Holds {
            corroborated: matches!(z3, Answer::Unsat { .. }),
        },
        (
            Answer::Unsat {
                proof_output: false,
            },
            _,
        ) => Judgment::Inconclusive("CVC5 answered unsat without proof steps".to_owned()),
        (Answer::Unknown, _) => Judgment::Inconclusive("CVC5 answered unknown".to_owned()),
        (Answer::Failed(reason), _) => {
            Judgment::Inconclusive(format!("CVC5 gave no answer: {reason}"))
        }
        (Answer::NotConfigured, _) => {
            Judgment::Inconclusive("CVC5 is not configured; Z3 alone never holds".to_owned())
        }
        (Answer::Sat(_), _) => Judgment::Inconclusive("CVC5's model did not replay".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(cvc5: Answer, z3: Answer) -> Answers {
        Answers {
            cvc5,
            z3,
            millis: [Some(1), Some(1)],
        }
    }

    fn model(value: i128) -> Answer {
        Answer::Sat(BTreeMap::from([("x0".to_owned(), value)]))
    }

    /// Replays a model whose `x0` is negative.
    fn replay(model: &BTreeMap<String, i128>) -> Result<i128, String> {
        match model.get("x0") {
            Some(value) if *value < 0 => Ok(*value),
            Some(value) => Err(format!("x0 = {value} is not a counterexample")),
            None => Err("no x0".to_owned()),
        }
    }

    #[test]
    fn unsat_holds_only_from_cvc5_with_proof_steps() {
        let unsat = |proof_output| Answer::Unsat { proof_output };
        assert_eq!(
            judge(&answers(unsat(true), unsat(false)), replay),
            Judgment::Holds { corroborated: true }
        );
        assert_eq!(
            judge(&answers(unsat(true), Answer::NotConfigured), replay),
            Judgment::Holds {
                corroborated: false
            }
        );
        assert_eq!(
            judge(&answers(unsat(true), Answer::Unknown), replay),
            Judgment::Holds {
                corroborated: false
            }
        );
        for (cvc5, z3) in [
            (unsat(false), unsat(false)),
            (Answer::Unknown, unsat(false)),
            (Answer::Failed("timeout".to_owned()), unsat(false)),
            (Answer::NotConfigured, unsat(false)),
        ] {
            assert!(
                matches!(
                    judge(&answers(cvc5.clone(), z3.clone()), replay),
                    Judgment::Inconclusive(_)
                ),
                "{cvc5:?} {z3:?}"
            );
        }
    }

    #[test]
    fn a_replayed_model_refutes_and_an_unreplayed_one_is_inconclusive() {
        assert_eq!(
            judge(&answers(model(-3), model(-4)), replay),
            Judgment::Refuted {
                witness: -3,
                solver: Solver::Cvc5,
                disagreement: false
            }
        );
        // A replayed model is a counterexample whatever the other solver
        // said; the disagreement is reported with it.
        assert_eq!(
            judge(
                &answers(Answer::Unsat { proof_output: true }, model(-4)),
                replay
            ),
            Judgment::Refuted {
                witness: -4,
                solver: Solver::Z3,
                disagreement: true
            }
        );
        // A planted disagreement: Z3's model does not replay, so CVC5's
        // `unsat` is not trusted either.
        let Judgment::Inconclusive(reason) = judge(
            &answers(Answer::Unsat { proof_output: true }, model(4)),
            replay,
        ) else {
            panic!("a disagreement without a replayed model must be inconclusive")
        };
        assert!(
            reason.starts_with("the solvers disagree: the z3 model did not replay"),
            "{reason}"
        );
        assert!(matches!(
            judge(&answers(model(4), Answer::Unknown), replay),
            Judgment::Inconclusive(_)
        ));
    }
}
