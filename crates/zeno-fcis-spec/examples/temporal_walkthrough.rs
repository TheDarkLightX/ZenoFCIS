//! Shows finite-trace results and the separate unbounded proof path.
use zeno_fcis_spec::{
    ClaimMode, EvalLimits, Identifier, PredicateProvider, RelExpr, TemporalEvaluation,
    TemporalFormula, TraceStep, evaluate_temporal,
};

struct NoPredicates;

impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

fn step() -> TraceStep {
    TraceStep::try_new(Vec::new()).unwrap_or_else(|| unreachable!())
}

fn show(label: &str, result: TemporalEvaluation) {
    match result {
        TemporalEvaluation::Satisfied => println!("{label}: satisfied"),
        TemporalEvaluation::Counterexample { step } => {
            println!("{label}: counterexample at step {step}")
        }
        TemporalEvaluation::Indeterminate(reason) => {
            println!("{label}: blocked ({reason:?})")
        }
        TemporalEvaluation::ProofObligation => println!("{label}: proof obligation"),
    }
}

fn main() {
    let one_step = vec![step()];
    let two_steps = vec![step(), step()];
    let three_steps = vec![step(), step(), step()];
    let limits = EvalLimits::default();
    let predicates = NoPredicates;

    let always_true = TemporalFormula::Always(Box::new(TemporalFormula::Atom(RelExpr::Bool(true))));
    let eventually_false =
        TemporalFormula::Eventually(Box::new(TemporalFormula::Atom(RelExpr::Bool(false))));
    let next_true = TemporalFormula::Next(Box::new(TemporalFormula::Atom(RelExpr::Bool(true))));

    show(
        "finite always true, 3 events",
        evaluate_temporal(
            &always_true,
            ClaimMode::Finite { horizon: 3 },
            &three_steps,
            &predicates,
            limits,
        ),
    );
    show(
        "finite eventually false, 3 events",
        evaluate_temporal(
            &eventually_false,
            ClaimMode::Finite { horizon: 3 },
            &three_steps,
            &predicates,
            limits,
        ),
    );
    show(
        "finite next true, 1 event",
        evaluate_temporal(
            &next_true,
            ClaimMode::Finite { horizon: 1 },
            &one_step,
            &predicates,
            limits,
        ),
    );
    show(
        "finite next true, 2 events",
        evaluate_temporal(
            &next_true,
            ClaimMode::Finite { horizon: 2 },
            &two_steps,
            &predicates,
            limits,
        ),
    );
    show(
        "unbounded always true",
        evaluate_temporal(
            &always_true,
            ClaimMode::UnboundedProof,
            &three_steps,
            &predicates,
            limits,
        ),
    );
}
