use super::*;
use alloc::vec;
use zeno_fcis_synthesis::finite::Op;

fn program(high: i64) -> Program {
    Program::try_new(
        vec![Domain::Int { min: 0, max: high }],
        vec![Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        }],
        vec![Op::Input(0), Op::Add(0, 0), Op::Sub(1, 0)],
        vec![2],
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}

#[test]
fn concrete_discovers_and_proposes_a_smaller_whole_program() {
    let original = program(3);
    let bank = discover(&original, &[], Budget::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(bank.rules.len(), 1);
    let candidate = replace(&original, &bank.rules[0]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!((original.nodes().len(), candidate.nodes().len()), (3, 1));
    for x in 0..=3 {
        assert_eq!(original.evaluate(&[x]), candidate.evaluate(&[x]));
    }
    assert_eq!(original.nodes().len(), 3);
}

#[test]
fn concrete_corpus_order_duplicates_and_returned_values_do_not_alias() {
    let original = program(3);
    let terms = (0..3)
        .map(|n| node_term(&original, n).unwrap_or_else(|e| panic!("{e:?}")))
        .collect::<Vec<_>>();
    let a = discover(&original, &terms, Budget::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let mut reversed = terms.clone();
    reversed.reverse();
    reversed.extend(terms);
    let b = discover(&original, &reversed, Budget::default()).unwrap_or_else(|e| panic!("{e:?}"));
    // Enumeration work includes supplied duplicates; semantic output does not.
    assert_eq!(a.terms, b.terms);
    assert_eq!(a.classes, b.classes);
    assert_eq!(a.rules, b.rules);
    let mut copied = a.clone();
    copied.rules.clear();
    assert_eq!(a.rules.len(), 1);
}

#[test]
fn concrete_excludes_trapping_cones_and_keeps_whole_program_check_obligation() {
    let original = program(0);
    let bank = discover(&original, &[], Budget::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let target = Program::try_new(
        original.inputs().to_vec(),
        original.outputs().to_vec(),
        vec![
            Op::Input(0),
            Op::Add(0, 0),
            Op::Sub(1, 0),
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(3, 4),
        ],
        vec![2],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let mined = discover(&target, &[], Budget::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!mined.rejected.is_empty());
    let candidate = replace(&target, &bank.rules[0]).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(target.evaluate(&[0]).is_err());
    assert!(candidate.evaluate(&[0]).is_ok());
    // This proposal must be refused by the caller's existing exact checker.
}

#[test]
fn concrete_transfer_only_changes_shape_and_does_not_claim_equivalence() {
    let term = node_term(&program(0), 0).unwrap_or_else(|e| panic!("{e:?}"));
    let rebound =
        rebind(&term, &[Domain::Int { min: 0, max: 5 }]).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        rebound.evaluate(&[5]).unwrap_or_else(|e| panic!("{e:?}")),
        vec![5]
    );
    assert!(rebind(&term, &[Domain::Bool]).is_err());
    assert_eq!(term.inputs(), &[Domain::Int { min: 0, max: 0 }]);
}

#[test]
fn concrete_budget_stops_produce_no_partial_bank() {
    let original = program(3);
    for budget in [
        Budget {
            max_tuples: 1,
            ..Budget::default()
        },
        Budget {
            max_terms: 1,
            ..Budget::default()
        },
        Budget {
            max_cells: 1,
            ..Budget::default()
        },
        Budget {
            max_work: 1,
            ..Budget::default()
        },
    ] {
        assert!(discover(&original, &[], budget).is_err());
    }
}

#[test]
fn concrete_stopped_discovery_keeps_charged_work() {
    let original = program(3);
    let stopped = discover(
        &original,
        &[],
        Budget {
            max_work: 1,
            ..Budget::default()
        },
    )
    .err()
    .unwrap_or_else(|| panic!("expected budget stop"));
    assert_eq!(stopped.work_used, original.nodes().len() as u64);
    assert!(matches!(stopped.reason, Stop::WorkBudget { limit: 1 }));
}
