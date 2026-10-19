//! Each proposer on decision programs of declared core-component instances.
//!
//! `tests/data/NAME.original.zcve` was written by `zeno-fcis contract
//! export-program` for the instance `NAME` of `core-components/`, and
//! `NAME.optimized.zcve` by `zeno-fcis optimize --profile checked-i128-v1` on
//! it. The oracle is the library evaluator, through `Program::evaluate`.

use zeno_fcis_program_search::{Outcome, Proposers, Settings, search};
use zeno_fcis_synthesis::finite::{Domain, Error, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

fn fixture(name: &str, kind: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/data/{name}.{kind}.zcve",
        env!("CARGO_MANIFEST_DIR")
    );
    let historical = std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let old = zeno_fcis_synthesis::finite_runtime::import_i64_program(&historical)
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        old.value()
            .unwrap_or_else(|e| panic!("{e:?}"))
            .zcve_bytes()
            .unwrap_or_else(|e| panic!("{e:?}")),
        historical
    );
    let profile = old.profile().as_bytes();
    assert_eq!(
        &historical[..10],
        &[8, 0, 0, 0, 4, 6, 0, 0, 0, profile.len() as u8]
    );
    assert_eq!(&historical[10..10 + profile.len()], profile);
    // A fresh current-route graph for proposer qualification. The retained
    // source fixture stays on the legacy replay route.
    let mut bytes = historical[..10].to_vec();
    bytes[9] += 1;
    bytes.extend_from_slice(old.profile().replace("i64", "i128").as_bytes());
    bytes.extend_from_slice(&historical[10 + profile.len()..]);
    bytes
}

fn import(bytes: &[u8]) -> Program {
    match import_program(bytes) {
        Ok(program) => program,
        Err(error) => panic!("import refused: {error:?}"),
    }
}

fn tuples(inputs: &[Domain]) -> Vec<Vec<i128>> {
    let mut all = vec![Vec::new()];
    for domain in inputs {
        let (min, max) = domain.bounds();
        all = all
            .iter()
            .flat_map(|prefix| {
                (min..=max).map(move |value| {
                    let mut tuple = prefix.clone();
                    tuple.push(value);
                    tuple
                })
            })
            .collect();
    }
    all
}

fn table(program: &Program) -> Vec<Result<Vec<i128>, String>> {
    tuples(program.inputs())
        .iter()
        .map(|tuple| {
            program.evaluate(tuple).map_err(|error| match error {
                Error::Arithmetic => "arithmetic".to_owned(),
                other => format!("{other:?}"),
            })
        })
        .collect()
}

/// The candidate's nodes after checking it against the original.
fn exact_nodes(original: &[u8], outcome: &Outcome) -> usize {
    let Outcome::Improved(candidate) = outcome else {
        panic!("expected an improvement, got {outcome:?}");
    };
    let (before, after) = (import(original), import(&candidate.bytes));
    assert_eq!(before.inputs(), after.inputs());
    assert_eq!(before.outputs(), after.outputs());
    assert_eq!(table(&before), table(&after));
    after.nodes().len()
}

fn only(top_down: bool, resub: bool, fold: bool) -> Settings {
    Settings {
        proposers: Proposers {
            top_down,
            resub,
            fold,
            diagram: false,
            ..Proposers::default()
        },
        ..Settings::default()
    }
}

#[test]
fn resubstitution_beats_optimize_and_stays_exact() {
    for (name, limit) in [("reservation-pool-c2-q1", 33), ("bounded-counter-c8", 21)] {
        let (original, optimized) = (fixture(name, "original"), fixture(name, "optimized"));
        let outcome = search(&original, &[&optimized], &only(false, true, false));
        let nodes = exact_nodes(&original, &outcome);
        assert!(nodes <= limit, "{name}: {nodes}");
        assert!(nodes < import(&optimized).nodes().len(), "{name}: {nodes}");
    }
}

#[test]
fn top_down_synthesis_is_exact() {
    for name in [
        "retry-budget-c8",
        "finite-phase-machine-c1",
        "consumable-budget-c3",
    ] {
        let original = fixture(name, "original");
        let outcome = search(&original, &[], &only(true, false, false));
        let nodes = exact_nodes(&original, &outcome);
        assert!(nodes < import(&original).nodes().len(), "{name}");
    }
}

#[test]
fn counterexample_guided_top_down_is_exact() {
    let original = fixture("idempotency-slot-c8", "original");
    let mut settings = only(true, false, false);
    settings.shape.cegis_tuples = 1024; // force the sampled path
    settings.shape.sample = 512;
    let nodes = exact_nodes(&original, &search(&original, &[], &settings));
    assert!(nodes <= import(&original).nodes().len());
}

#[test]
fn decision_list_folding_shrinks_the_optimized_program() {
    let name = "rate-limiter-n2-w2";
    let (original, optimized) = (fixture(name, "original"), fixture(name, "optimized"));
    let outcome = search(&original, &[&optimized], &only(false, false, true));
    assert!(exact_nodes(&original, &outcome) < import(&optimized).nodes().len());
}

#[test]
fn the_full_portfolio_is_exact_and_deterministic_on_every_fixture() {
    for name in [
        "reservation-pool-c2-q1",
        "bounded-counter-c8",
        "retry-budget-c8",
        "finite-phase-machine-c1",
        "consumable-budget-c3",
        "rate-limiter-n2-w2",
    ] {
        let (original, optimized) = (fixture(name, "original"), fixture(name, "optimized"));
        let first = search(&original, &[&optimized], &Settings::default());
        exact_nodes(&original, &first);
        assert_eq!(
            first,
            search(&original, &[&optimized], &Settings::default()),
            "{name}"
        );
    }
}
