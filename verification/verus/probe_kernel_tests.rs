//! Independent transitive-closure and trace checks of the production probe core.
use super::probe_kernel::{self, Answer};
use std::vec;
use std::vec::Vec;

fn known(answer: Option<Answer>) -> (Option<usize>, Vec<usize>, usize) {
    match answer {
        Some(Answer::Known {
            target,
            trace,
            reachable_count,
        }) => (target, trace, reachable_count),
        _ => panic!("expected a complete probe answer"),
    }
}

#[test]
fn noncommitting_target_is_unreachable() {
    let (target, trace, count) = known(probe_kernel::decide_reachable(
        1,
        0,
        &[1, 1],
        &[false, false],
        &[false, true],
        2,
    ));
    assert_eq!(target, None);
    assert_eq!(trace, vec![]);
    assert_eq!(count, 1);
}

#[test]
fn last_frontier_target_is_reachable() {
    let (target, trace, count) = known(probe_kernel::decide_reachable(
        1,
        0,
        &[1, 2, 2],
        &[true, true, false],
        &[false, false, true],
        3,
    ));
    assert_eq!(target, Some(2));
    assert_eq!(trace, vec![0, 0]);
    assert_eq!(count, 3);
}

#[test]
fn least_state_choice_precedes_shortest_matching_state() {
    let (target, trace, count) = known(probe_kernel::decide_reachable(
        2,
        0,
        &[2, 3, 1, 1, 1, 2, 3, 3],
        &[true, true, false, false, true, false, false, false],
        &[false, true, false, true],
        8,
    ));
    assert_eq!(target, Some(1));
    assert_eq!(trace, vec![0, 0]);
    assert_eq!(count, 4);
}

#[test]
fn bounds_and_invalid_tables_never_become_no_answers() {
    for query in [[true], [false]] {
        assert!(matches!(
            probe_kernel::decide_reachable(1, 0, &[0], &[false], &query, 0),
            Some(Answer::Unknown)
        ));
        let (target, trace, count) = known(probe_kernel::decide_reachable(
            1,
            0,
            &[0],
            &[false],
            &query,
            1,
        ));
        assert_eq!(target, query[0].then_some(0));
        assert!(trace.is_empty());
        assert_eq!(count, 1);
    }
    assert!(probe_kernel::within_limit(Some(0), 0));
    assert!(probe_kernel::within_limit(
        Some(u128::from(u64::MAX)),
        u64::MAX
    ));
    assert!(!probe_kernel::within_limit(Some(u128::MAX), u64::MAX));
    assert!(!probe_kernel::within_limit(None, u64::MAX));
    assert!(probe_kernel::decide_reachable(0, 0, &[], &[], &[true], 0).is_none());
    assert!(probe_kernel::decide_reachable(1, 1, &[0], &[false], &[true], 0).is_none());
    assert!(probe_kernel::decide_reachable(1, 0, &[1], &[false], &[true], 0).is_none());
    assert!(probe_kernel::decide_reachable(1, 0, &[0], &[], &[true], 0).is_none());
}

#[test]
fn returned_traces_are_exclusively_owned() {
    let after = [1, 1];
    let commits = [true, false];
    let query = [false, true];
    let (_, mut first, _) = known(probe_kernel::decide_reachable(
        1, 0, &after, &commits, &query, 2,
    ));
    let (_, second, _) = known(probe_kernel::decide_reachable(
        1, 0, &after, &commits, &query, 2,
    ));
    first.push(123);
    assert_eq!(second, vec![0]);
    assert_eq!(after, [1, 1]);
    assert_eq!(commits, [true, false]);
    assert_eq!(query, [false, true]);
}

#[test]
fn answers_match_independent_closure_on_small_tables() {
    let mut cases = 0;
    for n in 1usize..=3 {
        for k in 1usize..=2 {
            let edges = n * k;
            for number in 0..n.pow(edges as u32) {
                let mut digits = number;
                let after: Vec<usize> = (0..edges)
                    .map(|_| {
                        let t = digits % n;
                        digits /= n;
                        t
                    })
                    .collect();
                for bits in 0usize..1 << edges {
                    let commits: Vec<bool> = (0..edges).map(|j| bits & (1 << j) != 0).collect();
                    let mut distance = vec![vec![n + 1; n]; n];
                    for s in 0..n {
                        distance[s][s] = 0;
                        for i in 0..k {
                            if commits[s * k + i] {
                                let t = after[s * k + i];
                                distance[s][t] = distance[s][t].min(1);
                            }
                        }
                    }
                    for mid in 0..n {
                        for s in 0..n {
                            for t in 0..n {
                                distance[s][t] =
                                    distance[s][t].min(distance[s][mid] + distance[mid][t]);
                            }
                        }
                    }
                    for g in 0..n {
                        for selected in 0usize..1 << n {
                            let query: Vec<bool> =
                                (0..n).map(|s| selected & (1 << s) != 0).collect();
                            let (target, trace, count) = known(probe_kernel::decide_reachable(
                                k,
                                g,
                                &after,
                                &commits,
                                &query,
                                edges as u64,
                            ));
                            let expected = (0..n).find(|s| query[*s] && distance[g][*s] <= n);
                            assert_eq!(target, expected);
                            assert_eq!(count, (0..n).filter(|s| distance[g][*s] <= n).count());
                            let mut state = g;
                            for input in &trace {
                                assert!(*input < k);
                                if commits[state * k + input] {
                                    state = after[state * k + input];
                                }
                            }
                            if let Some(t) = target {
                                assert_eq!(state, t);
                                assert!(query[t]);
                                assert_eq!(trace.len(), distance[g][t]);
                            } else {
                                assert!(trace.is_empty());
                            }
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    std::eprintln!("probe cases: {cases}");
}
