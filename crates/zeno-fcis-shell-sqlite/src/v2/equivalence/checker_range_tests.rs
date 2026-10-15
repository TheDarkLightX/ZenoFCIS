use super::*;
use alloc::vec;
use zeno_fcis_synthesis::finite::Op;

fn ok<T>(result: Result<T, RangeError>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("unexpected range refusal: {error:?}"),
    }
}

fn program<'a>(
    inputs: &'a [Domain],
    outputs: &'a [Domain],
    nodes: &'a [Op],
    roots: &'a [u16],
) -> V2ScalarProgram<'a> {
    V2ScalarProgram {
        inputs,
        outputs,
        nodes,
        roots,
    }
}

fn serial(
    a: &V2ScalarProgram<'_>,
    b: &V2ScalarProgram<'_>,
    cap: u64,
    limit: u64,
) -> Result<(u64, Usage), RangeError> {
    compare_with_usage(a, b, cap, limit)
        .map(|done| (done.tuples(), done.usage()))
        .map_err(RangeError::Comparison)
}

#[test]
fn seek_matches_odometer_and_signed_endpoints() {
    let domains = [Domain::Bool, Domain::Int { min: -2, max: 1 }, Domain::Bool];
    let mut expected = first_tuple(&domains);
    for ordinal in 0..16 {
        assert_eq!(tuple_at(&domains, ordinal), Some(expected.clone()));
        assert_eq!(advance(&domains, &mut expected), ordinal < 15);
    }
    assert_eq!(tuple_at(&domains, 16), None);
    assert_eq!(tuple_at(&[], 0), Some(vec![]));
    assert_eq!(tuple_at(&[], 1), None);
    let full = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    assert_eq!(tuple_at(&[full], 0), Some(vec![i64::MIN]));
    assert_eq!(tuple_at(&[full], u64::MAX), Some(vec![i64::MAX]));
    assert_eq!(
        tuple_at(&[full, full], u64::MAX),
        Some(vec![i64::MIN, i64::MAX])
    );
    assert_eq!(
        tuple_at(&[full, full, Domain::Int { min: 1, max: 0 }], 0),
        None
    );
    assert_eq!(
        tuple_at(
            &[Domain::Int {
                min: i64::MAX,
                max: i64::MAX
            }],
            0
        ),
        Some(vec![i64::MAX])
    );
}

#[test]
fn all_split_points_preserve_usage_traps_and_boundary_precedence() {
    let inputs = [Domain::Bool, Domain::Int { min: -1, max: 2 }];
    let outputs = [Domain::Int { min: -1, max: 2 }];
    let plain = [Op::Input(1)];
    let extra = [Op::Input(1), Op::Bool(true)];
    let changed = [
        Op::Input(0),
        Op::Input(1),
        Op::Int(2),
        Op::Eq(1, 2),
        Op::And(0, 3),
        Op::Int(0),
        Op::Select(4, 5, 1),
    ];
    let trap_a = [Op::Int(i64::MAX), Op::Add(0, 0)];
    let trap_b = [Op::Bool(false), Op::Int(i64::MAX), Op::Add(1, 1)];
    let a = program(&inputs, &outputs, &plain, &[0]);
    let equivalent = program(&inputs, &outputs, &extra, &[0]);
    let changed = program(&inputs, &outputs, &changed, &[6]);
    let trapped_a = program(&inputs, &outputs, &trap_a, &[0]);
    let trapped_b = program(&inputs, &outputs, &trap_b, &[0]);
    for (left, right) in [
        (&a, &a),
        (&a, &equivalent),
        (&a, &changed),
        (&trapped_a, &trapped_b),
    ] {
        for limit in [0, 1, 2, 3, 7, FULL_BUDGET] {
            for split in 1..=8 {
                assert_eq!(
                    compare_two_ranges(left, right, 8, limit, split, u64::MAX),
                    serial(left, right, 8, limit)
                );
            }
        }
    }
    assert!(matches!(
        compare_two_ranges(&a, &changed, 8, 0, 7, u64::MAX),
        Err(RangeError::Comparison(Failure::Counterexample {
            ordinal: 7,
            ..
        }))
    ));
}

#[test]
fn least_witness_wins_but_speculative_work_is_retained() {
    let inputs = [Domain::Int { min: 0, max: 5 }];
    let outputs = inputs;
    let a = program(&inputs, &outputs, &[Op::Input(0)], &[0]);
    let b = program(&inputs, &outputs, &[Op::Int(0)], &[0]);
    // Deliberately evaluate the later interval first.
    let later = ok(scan_range(&a, &b, 6, 0, 3, 6, 12));
    let earlier = ok(scan_range(&a, &b, 6, 0, 0, 3, 12));
    assert_eq!(later.progress(), (3, 6, 3, 1, 12));
    assert_eq!(earlier.progress(), (0, 3, 1, 2, 12));
    let joined = ok(merge_adjacent(earlier, later));
    assert_eq!(joined.progress(), (0, 6, 1, 3, 24));
    assert_eq!(joined.measured_steps(), [3, 3]);
    assert_eq!(finish_range(&a, &b, 6, 0, joined), serial(&a, &b, 6, 0));
    // A witness with all smaller ordinals checked needs no unvisited suffix.
    let prefix = ok(scan_range(&a, &b, 6, 0, 0, 2, 8));
    assert_eq!(finish_range(&a, &b, 6, 0, prefix), serial(&a, &b, 6, 0));
}

#[test]
fn gaps_overlap_reverse_duplicate_and_missing_prefix_refuse() {
    let inputs = [Domain::Int { min: 0, max: 5 }];
    let p = program(&inputs, &[], &[], &[]);
    for (left, right) in [
        ((0, 2), (3, 6)),
        ((0, 3), (2, 6)),
        ((3, 6), (0, 3)),
        ((0, 3), (0, 3)),
    ] {
        let a = ok(scan_range(&p, &p, 6, 0, left.0, left.1, 12));
        let b = ok(scan_range(&p, &p, 6, 0, right.0, right.1, 12));
        assert!(matches!(merge_adjacent(a, b), Err(RangeError::NonAdjacent)));
    }
    for (lo, hi) in [(0, 3), (1, 6)] {
        let partial = ok(scan_range(&p, &p, 6, 0, lo, hi, 12));
        assert_eq!(
            finish_range(&p, &p, 6, 0, partial),
            Err(RangeError::IncompleteCoverage)
        );
    }
}

#[test]
fn exact_subject_roots_cap_and_limit_bind_reports() {
    let inputs = [Domain::Bool];
    let outputs = [Domain::Bool];
    let nodes = [Op::Bool(false), Op::Bool(true)];
    let a = program(&inputs, &outputs, &nodes, &[0]);
    let different_root = program(&inputs, &outputs, &nodes, &[1]);
    for (right_program, cap, limit) in [(&different_root, 2, 2), (&a, 3, 2), (&a, 2, 3)] {
        let left = ok(scan_range(&a, &a, 2, 2, 0, 1, 6));
        let right = ok(scan_range(
            right_program,
            right_program,
            cap,
            limit,
            1,
            2,
            6,
        ));
        assert!(matches!(
            merge_adjacent(left, right),
            Err(RangeError::SubjectMismatch)
        ));
        let complete = ok(scan_range(&a, &a, 2, 2, 0, 2, 12));
        assert_eq!(
            finish_range(right_program, right_program, cap, limit, complete),
            Err(RangeError::SubjectMismatch)
        );
    }
    // Structurally equal programs in distinct owned arrays remain compatible.
    let copied_nodes = nodes.clone();
    let copied = program(&inputs, &outputs, &copied_nodes, &[0]);
    let left = ok(scan_range(&a, &a, 2, 2, 0, 1, 6));
    let right = ok(scan_range(&copied, &copied, 2, 2, 1, 2, 6));
    assert_eq!(
        finish_range(&a, &a, 2, 2, ok(merge_adjacent(left, right))),
        serial(&a, &a, 2, 2)
    );
}

#[test]
fn associative_ordered_merge_preserves_full_result_and_work() {
    let inputs = [Domain::Int { min: 0, max: 5 }];
    let p = program(&inputs, &[], &[Op::Input(0)], &[]);
    let make = |lo, hi| ok(scan_range(&p, &p, 6, 1, lo, hi, 24));
    let left_grouped = ok(merge_adjacent(
        ok(merge_adjacent(make(0, 1), make(1, 4))),
        make(4, 6),
    ));
    let right_grouped = ok(merge_adjacent(
        make(0, 1),
        ok(merge_adjacent(make(1, 4), make(4, 6))),
    ));
    assert_eq!(left_grouped.progress(), (0, 6, 6, 6, 24));
    assert_eq!(left_grouped.progress(), right_grouped.progress());
    assert_eq!(
        finish_range(&p, &p, 6, 1, left_grouped),
        finish_range(&p, &p, 6, 1, right_grouped)
    );
}

#[test]
fn work_preflight_and_admission_order_are_exact() {
    let p = program(&[], &[], &[], &[]);
    assert_eq!(compare_two_ranges(&p, &p, 1, 0, 1, 2), serial(&p, &p, 1, 0));
    assert_eq!(
        compare_two_ranges(&p, &p, 1, 0, 1, 1),
        Err(RangeError::WorkExhausted)
    );
    assert!(matches!(
        scan_range(&p, &p, 1, 0, 0, 1, 1),
        Err(RangeError::WorkExhausted)
    ));
    assert!(matches!(
        scan_range(&p, &p, 0, 0, 0, 0, 0),
        Err(RangeError::Comparison(Failure::DomainTooLarge { .. }))
    ));
    assert!(matches!(
        scan_range(&p, &p, 1, 0, 0, 0, 0),
        Err(RangeError::InvalidRange)
    ));
    let huge = [Domain::Int {
        min: 0,
        max: i64::MAX,
    }];
    let p = program(&huge, &[], &[], &[]);
    assert!(matches!(
        scan_range(&p, &p, 1 << 63, 0, 0, 1 << 63, u64::MAX),
        Err(RangeError::ArithmeticOverflow)
    ));
    let left = ok(scan_range(&p, &p, 1 << 63, 0, 0, 1, 2));
    // Large range refuses before evaluating; this is not an enormous test loop.
    assert_eq!(left.progress(), (0, 1, 1, 1, 2));
}
