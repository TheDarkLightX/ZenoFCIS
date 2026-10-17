use super::*;
use crate::transform::{self, Limits};
use crate::transform_plan::{RunLimits, plan};
use std::time::Duration;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

fn fixture(nodes: Vec<Op>, root: u16) -> Vec<u8> {
    Program::try_new(
        vec![Domain::Int { min: 0, max: 7 }],
        vec![Domain::Int { min: 0, max: 7 }],
        nodes,
        vec![root],
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
    .value()
    .unwrap_or_else(|error| panic!("{error:?}"))
    .canonical_bytes()
    .unwrap_or_else(|error| panic!("{error:?}"))
}
fn limits() -> Limits {
    Limits {
        steps: 256,
        input_tuples: 1000,
    }
}
fn generous(workers: u64, chunk_tuples: u64) -> RunLimits {
    RunLimits {
        workers,
        chunk_tuples,
        ..RunLimits::offline()
    }
}

#[test]
fn worker_counts_chunks_limits_and_receipt_bytes_match_serial() {
    let a = fixture(vec![Op::Input(0)], 0);
    let candidates = [
        a.clone(),
        fixture(vec![Op::Input(0), Op::Int(0), Op::Add(0, 1)], 2),
        fixture(vec![Op::Int(0)], 0),
        fixture(vec![Op::Input(0), Op::Int(i128::MAX), Op::Add(0, 1)], 0),
    ];
    for b in candidates {
        for workers in [1, 2, 4] {
            for chunk in [1, 3, 8] {
                for steps in [0, 1, 256] {
                    let semantic = Limits { steps, ..limits() };
                    let serial = transform::check(&a, &b, semantic);
                    let parallel =
                        check(&a, &b, semantic, generous(workers, chunk), Instant::now());
                    assert!(parallel.progress.reserved > 0);
                    assert!(parallel.progress.accounting_complete);
                    match (serial, parallel.result) {
                        (Ok(s), Ok(p)) => {
                            assert_eq!(s, p);
                            assert_eq!(s.receipt(), p.receipt());
                        }
                        (Err(s), Err(Failure::Semantic(p))) => assert_eq!(s, p),
                        pair => panic!("serial/parallel mismatch: {pair:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn delayed_first_middle_and_last_slots_preserve_least_witness_and_all_work() {
    let a = fixture(vec![Op::Input(0)], 0);
    let b = fixture(vec![Op::Int(0)], 0);
    let prepared = transform::prepare(&a, &b, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let p = plan(prepared.dimensions(), generous(4, 2)).unwrap_or_else(|error| panic!("{error:?}"));
    let (left, right) = prepared.subjects();
    for delayed in 0..4 {
        let mut delays = [0; 4];
        delays[delayed] = 10;
        let done = run_ordered(
            &left,
            &right,
            1000,
            256,
            &p,
            Instant::now() + Duration::from_secs(2),
            Faults {
                delays: &delays,
                ..Faults::default()
            },
        );
        assert!(done.progress.accounting_complete);
        assert_eq!(done.progress.completed_chunks, 4);
        assert_eq!(done.progress.evaluated, 5); // first range 0,1; other ranges each first mismatch
        assert_eq!(done.progress.measured_steps, [5, 5]);
        assert_eq!(done.progress.reserved, p.reserved());
        match prepared.finish(done.result.unwrap_or_else(|error| panic!("{error:?}"))) {
            Err(transform::RangeFinishError::Semantic(error)) => {
                assert_eq!(Err(error), transform::check(&a, &b, limits()))
            }
            other => panic!("expected least witness: {other:?}"),
        }
    }
}

#[test]
fn exhaustion_before_dispatch_and_deadline_after_dispatch_retain_reservation() {
    let bytes = fixture(vec![Op::Input(0)], 0);
    let prepared =
        transform::prepare(&bytes, &bytes, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let p = plan(prepared.dimensions(), generous(2, 4)).unwrap_or_else(|error| panic!("{error:?}"));
    let short = check(
        &bytes,
        &bytes,
        limits(),
        RunLimits {
            work_units: p.reserved() - 1,
            ..generous(2, 4)
        },
        Instant::now(),
    );
    assert!(matches!(short.result, Err(Failure::Stopped(Stop::Work))));
    assert_eq!(short.progress.evaluated, 0);
    assert_eq!(short.progress.reserved, p.setup());
    let (a, b) = prepared.subjects();
    let expired = run(&a, &b, 1000, 256, &p, Instant::now());
    assert!(matches!(expired.result, Err(Stop::Deadline)));
    assert_eq!(expired.progress.evaluated, 0);
    assert_eq!(expired.progress.reserved, p.reserved());
    let active = run_ordered(
        &a,
        &b,
        1000,
        256,
        &p,
        Instant::now() + Duration::from_millis(2),
        Faults {
            delays: &[10, 10],
            ..Faults::default()
        },
    );
    assert!(matches!(active.result, Err(Stop::Deadline)));
    assert_eq!(active.progress.evaluated, 8);
    assert!(active.progress.accounting_complete);
    assert_eq!(active.progress.reserved, p.reserved());
}

#[test]
fn foreign_plan_and_admission_refusal_cannot_make_receipt() {
    let bytes = fixture(vec![Op::Input(0)], 0);
    let prepared =
        transform::prepare(&bytes, &bytes, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let mut shape = prepared.dimensions();
    shape.tuples = 7;
    let p = plan(shape, generous(2, 4)).unwrap_or_else(|error| panic!("{error:?}"));
    let (a, b) = prepared.subjects();
    let done = run(
        &a,
        &b,
        1000,
        256,
        &p,
        Instant::now() + Duration::from_secs(1),
    );
    assert!(matches!(done.result, Err(Stop::Dimensions)));
    assert_eq!(done.progress.evaluated, 0);
    let rejected = check(b"bad", &bytes, limits(), generous(2, 4), Instant::now());
    assert!(matches!(
        rejected.result,
        Err(Failure::Semantic(transform::Rejection::Refused(_)))
    ));
    assert!(rejected.progress.reserved > 0);
    assert_eq!(rejected.progress.evaluated, 0);
}

#[test]
fn least_witness_in_the_first_window_stops_dispatch_after_that_window() {
    // Eight one-tuple ranges on two workers; the candidate differs at tuple 0.
    let a = fixture(vec![Op::Input(0)], 0);
    let b = fixture(vec![Op::Int(7)], 0);
    let prepared = transform::prepare(&a, &b, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let p = plan(prepared.dimensions(), generous(2, 1)).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(p.chunks(), 8);
    let (left, right) = prepared.subjects();
    let done = run(
        &left,
        &right,
        1000,
        256,
        &p,
        Instant::now() + Duration::from_secs(2),
    );
    assert!(done.progress.accounting_complete);
    assert_eq!(done.progress.planned_chunks, 8);
    assert_eq!(done.progress.active_workers, 2);
    assert_eq!(done.progress.completed_chunks, 2);
    match prepared.finish(done.result.unwrap_or_else(|error| panic!("{error:?}"))) {
        Err(transform::RangeFinishError::Semantic(error)) => {
            assert_eq!(Err(error), transform::check(&a, &b, limits()))
        }
        other => panic!("expected least witness: {other:?}"),
    }
}

#[test]
fn a_failed_worker_is_inconclusive_with_incomplete_accounting() {
    let a = fixture(vec![Op::Input(0)], 0);
    let prepared = transform::prepare(&a, &a, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let p = plan(prepared.dimensions(), generous(4, 2)).unwrap_or_else(|error| panic!("{error:?}"));
    let (left, right) = prepared.subjects();
    for failing in 0..4 {
        let done = run_ordered(
            &left,
            &right,
            1000,
            256,
            &p,
            Instant::now() + Duration::from_secs(2),
            Faults {
                failing_slot: Some(failing),
                ..Faults::default()
            },
        );
        assert!(matches!(done.result, Err(Stop::Worker)), "slot {failing}");
        assert!(!done.progress.accounting_complete);
        assert_eq!(done.progress.completed_chunks, 3);
        assert_eq!(done.progress.reserved, p.reserved());
    }
}

#[test]
fn a_deadline_that_passes_while_joining_workers_is_inconclusive() {
    let a = fixture(vec![Op::Input(0)], 0);
    let prepared = transform::prepare(&a, &a, limits()).unwrap_or_else(|error| panic!("{error:?}"));
    let p = plan(prepared.dimensions(), generous(2, 4)).unwrap_or_else(|error| panic!("{error:?}"));
    let (left, right) = prepared.subjects();
    let done = run_ordered(
        &left,
        &right,
        1000,
        256,
        &p,
        Instant::now() + Duration::from_millis(200),
        Faults {
            exit_delay_ms: 400,
            ..Faults::default()
        },
    );
    // Every range completed and was merged before the deadline; only the
    // join overran it, which must still make the run inconclusive.
    assert!(matches!(done.result, Err(Stop::Deadline)));
    assert!(done.progress.accounting_complete);
    assert_eq!(done.progress.completed_chunks, 2);
    assert_eq!(done.progress.evaluated, 8);
}

#[test]
fn measured_steps_must_fit_the_admission_reservation_without_overflow() {
    assert!(measured_within_reservation([40, 60], 100));
    assert!(!measured_within_reservation([40, 61], 100));
    assert!(!measured_within_reservation([u128::MAX, 1], u64::MAX));
    assert!(!measured_within_reservation(
        [u128::from(u64::MAX), 1],
        u64::MAX
    ));
}
