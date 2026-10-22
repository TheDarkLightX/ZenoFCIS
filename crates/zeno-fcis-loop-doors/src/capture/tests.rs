use super::*;

fn table() -> ReferenceTable {
    ReferenceTable::new(
        vec![Domain::Int { min: 0, max: 3 }],
        vec![Domain::Bool],
        vec![vec![1], vec![1], vec![0], vec![0]],
    )
    .unwrap_or_else(|e| panic!("fixture: {e:?}"))
}

#[test]
fn table_program_matches_every_row_including_nullary_boolean_and_i128_edges() {
    for t in [
        table(),
        ReferenceTable::new(
            vec![],
            vec![Domain::Int {
                min: i128::MIN,
                max: i128::MAX,
            }],
            vec![vec![i128::MIN]],
        )
        .unwrap_or_else(|e| panic!("nullary: {e:?}")),
        ReferenceTable::new(
            vec![Domain::Bool, Domain::Int { min: -2, max: 1 }],
            vec![
                Domain::Bool,
                Domain::Int {
                    min: i128::MIN,
                    max: i128::MAX,
                },
            ],
            (0..8)
                .map(|i| vec![i % 2, if i < 4 { i128::MIN } else { i128::MAX }])
                .collect(),
        )
        .unwrap_or_else(|e| panic!("mixed: {e:?}")),
    ] {
        let p = t.program().unwrap_or_else(|e| panic!("encodes: {e:?}"));
        for (point, row) in t.points().iter().zip(t.rows()) {
            assert_eq!(p.evaluate(point).ok().as_ref(), Some(row));
        }
    }
}

#[test]
fn fitting_examples_is_only_a_proposal_and_repeated_failure_is_filtered() {
    let t = table();
    let p = Program::try_new(
        t.inputs().to_vec(),
        t.outputs().to_vec(),
        vec![Op::Bool(false)],
        vec![0],
    )
    .unwrap_or_else(|e| panic!("fixture: {e:?}"));
    assert_eq!(
        precheck(&t, &p, &[2]).unwrap_or_else(|e| panic!("abi: {e:?}")),
        None
    );
    assert_eq!(
        differences(&t, &p).unwrap_or_else(|e| panic!("abi: {e:?}")),
        vec![0, 1]
    );
    assert_eq!(
        precheck(&t, &p, &[0, 2]).unwrap_or_else(|e| panic!("abi: {e:?}")),
        Some(0)
    );
    assert_eq!(choose_counterexample(&t, &[1, 2, 3], Some(0)), Some(3));
    assert_eq!(choose_counterexample(&t, &[1, 3], Some(2)), Some(1));
}

#[test]
fn coverage_reports_unreachable_outcomes_and_does_not_invent_trapping_prefixes() {
    let t = table();
    let p = Program::try_new(
        t.inputs().to_vec(),
        t.outputs().to_vec(),
        vec![Op::Bool(true), Op::Bool(false), Op::Select(0, 1, 0)],
        vec![2],
    )
    .unwrap_or_else(|e| panic!("fixture: {e:?}"));
    let rows = coverage(&t, &p, &[0]).unwrap_or_else(|e| panic!("coverage: {e:?}"));
    assert_eq!(rows[0].example, None);
    assert!(rows[1].covered);
    let trap = Program::try_new(
        t.inputs().to_vec(),
        t.outputs().to_vec(),
        vec![
            Op::Int(i128::MAX),
            Op::Int(1),
            Op::Add(0, 1),
            Op::Bool(true),
            Op::Bool(false),
            Op::Select(3, 3, 4),
        ],
        vec![5],
    )
    .unwrap_or_else(|e| panic!("traps eagerly: {e:?}"));
    assert!(
        coverage(&t, &trap, &[])
            .unwrap_or_else(|e| panic!("coverage: {e:?}"))
            .iter()
            .all(|r| r.example.is_none())
    );
}

#[test]
fn incomplete_tables_wrong_abi_and_large_domains_never_construct_a_target() {
    assert!(ReferenceTable::new(vec![Domain::Bool], vec![Domain::Bool], vec![vec![0]]).is_err());
    assert!(ReferenceTable::new(vec![], vec![Domain::Bool], vec![vec![2]]).is_err());
    assert!(points(&[Domain::Int { min: 0, max: 4096 }], MAX_TUPLES).is_err());
    assert_eq!(
        points(&[], MAX_TUPLES).unwrap_or_else(|e| panic!("nullary: {e:?}")),
        vec![Vec::<i128>::new()]
    );
}

#[test]
fn a_table_that_exceeds_the_existing_checker_encoding_is_a_budget_stop() {
    let t = ReferenceTable::new(
        vec![Domain::Int { min: 0, max: 1023 }],
        vec![Domain::Int { min: 0, max: 1023 }],
        (0..1024).map(|v| vec![v]).collect(),
    )
    .unwrap_or_else(|e| panic!("table: {e:?}"));
    assert_eq!(
        t.program()
            .err()
            .unwrap_or_else(|| panic!("unexpected encoding"))
            .0,
        "reference-encoding-budget"
    );
}
