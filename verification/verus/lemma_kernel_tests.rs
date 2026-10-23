//! Native contracts for allocation, slice reads, push/update and parent traversal.
use super::lemma_kernel;
use std::collections::VecDeque;
use std::vec::Vec;
use std::{eprintln, vec};

fn reference(
    n: usize,
    k: usize,
    g: usize,
    after: &[usize],
    commits: &[bool],
) -> (Vec<bool>, Vec<Option<(usize, usize)>>, Vec<usize>) {
    let mut seen = vec![false; n];
    let mut parents = vec![None; n];
    let mut depth = vec![0; n];
    let mut todo = VecDeque::from([g]);
    seen[g] = true;
    while let Some(s) = todo.pop_front() {
        for i in 0..k {
            let next = if commits[s * k + i] {
                after[s * k + i]
            } else {
                s
            };
            if !seen[next] {
                seen[next] = true;
                parents[next] = Some((s, i));
                depth[next] = depth[s] + 1;
                todo.push_back(next);
            }
        }
    }
    (seen, parents, depth)
}

#[test]
fn reachable_matches_independent_fifo_on_all_small_tables() {
    let mut cases = 0;
    for n in 1usize..=3 {
        for k in 1usize..=2 {
            let edges = n * k;
            for number in 0..n.pow(edges as u32) {
                let mut digits = number;
                let after: Vec<usize> = (0..edges)
                    .map(|_| {
                        let value = digits % n;
                        digits /= n;
                        value
                    })
                    .collect();
                for bits in 0usize..1 << edges {
                    let commits: Vec<bool> = (0..edges).map(|j| bits & (1 << j) != 0).collect();
                    for g in 0..n {
                        let actual = lemma_kernel::reachable(n, k, g, &after, &commits);
                        let expected = reference(n, k, g, &after, &commits);
                        assert_eq!(actual.seen, expected.0);
                        assert_eq!(actual.parent, expected.1);
                        assert_eq!(actual.depth, expected.2);
                        cases += 1;
                    }
                }
            }
        }
    }
    eprintln!("reachability tables: {cases}");
}

#[test]
fn vector_contracts_have_native_witnesses_and_exclusive_results() {
    let after = vec![1, 2, 2];
    let commits = vec![true, true, false];
    let a = lemma_kernel::reachable(3, 1, 0, &after, &commits);
    let mut b = lemma_kernel::reachable(3, 1, 0, &after, &commits);
    b.seen[0] = false;
    b.parent[1] = None;
    b.depth[1] = 999;
    assert!(a.seen[0]);
    assert_eq!(a.parent[1], Some((0, 0)));
    assert_eq!(a.depth, vec![0, 1, 2]);
    assert_eq!(after, vec![1, 2, 2]);
    assert_eq!(commits, vec![true, true, false]);
    let mut values = vec![7usize; 3];
    values[1] = 9;
    values.push(11);
    assert_eq!(&values[..], &[7, 9, 7, 11]);
    let mut inputs = vec![1, 2, 3];
    inputs.reverse();
    assert_eq!(inputs, vec![3, 2, 1]);
}

#[test]
fn verdict_matches_transitive_closure_and_complete_edge_count() {
    let mut cases = 0;
    for n in 1usize..=3 {
        let k = 1;
        for number in 0..n.pow(n as u32) {
            let mut digits = number;
            let after: Vec<usize> = (0..n)
                .map(|_| {
                    let v = digits % n;
                    digits /= n;
                    v
                })
                .collect();
            for bits in 0usize..1 << n {
                let commits: Vec<bool> = (0..n).map(|j| bits & (1 << j) != 0).collect();
                // Independent reachability oracle: transitive closure, without parents.
                let mut relation = vec![vec![false; n]; n];
                for s in 0..n {
                    relation[s][s] = true;
                    if commits[s] {
                        relation[s][after[s]] = true;
                    }
                }
                for mid in 0..n {
                    for from in 0..n {
                        for to in 0..n {
                            relation[from][to] |= relation[from][mid] && relation[mid][to];
                        }
                    }
                }
                for g in 0..n {
                    for admitted in 0usize..1 << n {
                        let holds: Vec<bool> = (0..n).map(|s| admitted & (1 << s) != 0).collect();
                        let decision = lemma_kernel::decide(k, g, &after, &commits, &holds);
                        assert!(decision.is_some());
                        let Some(d) = decision else { continue };
                        let failures: Vec<usize> = (0..n)
                            .filter(|s| holds[*s] && commits[*s] && !holds[after[*s]])
                            .collect();
                        let false_lemma = (0..n).any(|s| relation[g][s] && !holds[s]);
                        if holds[g] && failures.is_empty() {
                            assert!(!false_lemma);
                        }
                        let expected = if false_lemma {
                            lemma_kernel::Verdict::False
                        } else if !failures.is_empty() {
                            lemma_kernel::Verdict::NotInductive
                        } else {
                            lemma_kernel::Verdict::Inductive
                        };
                        assert_eq!(d.verdict, expected);
                        assert_eq!(d.initiation, holds[g]);
                        assert_eq!(d.count, failures.len());
                        assert_eq!(d.search.seen, relation[g]);
                        if let Some(target) = d.target {
                            let mut state = g;
                            for input in &d.trace {
                                assert!(*input < k);
                                if commits[state] {
                                    state = after[state];
                                }
                            }
                            assert_eq!(state, target);
                            assert!(!holds[target]);
                            if holds[g] {
                                let c = d.counterexample;
                                assert!(c.is_some());
                                if let Some((before, input, next)) = c {
                                    assert!(
                                        relation[g][before] && holds[before] && commits[before]
                                    );
                                    assert_eq!(input, 0);
                                    assert_eq!(next, target);
                                    assert_eq!(after[before], target);
                                }
                            } else {
                                assert!(d.trace.is_empty());
                                assert_eq!(target, g);
                            }
                        } else if let Some((before, input, next)) = d.counterexample {
                            assert_eq!(Some(&before), failures.first());
                            assert_eq!(input, 0);
                            assert_eq!(next, after[before]);
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    eprintln!("decision/holds cases: {cases}");
}

#[test]
fn initiation_is_required() {
    let d = lemma_kernel::decide(1, 0, &[0], &[false], &[false]);
    assert!(d.is_some());
    if let Some(d) = d {
        assert_eq!(d.verdict, lemma_kernel::Verdict::False);
        assert!(!d.initiation);
        assert!(d.trace.is_empty());
    }
}
#[test]
fn noncommitting_raw_successor_cannot_change_reachability() {
    let d = lemma_kernel::decide(1, 0, &[1, 1], &[false, false], &[true, false]);
    assert!(d.is_some());
    if let Some(d) = d {
        assert_eq!(d.search.seen, vec![true, false]);
        assert_eq!(d.verdict, lemma_kernel::Verdict::Inductive);
    }
}
#[test]
fn search_must_process_last_frontier_state() {
    let d = lemma_kernel::decide(1, 0, &[1, 2, 2], &[true, true, false], &[true, true, false]);
    assert!(d.is_some());
    if let Some(d) = d {
        assert_eq!(d.search.seen, vec![true, true, true]);
        assert_eq!(d.verdict, lemma_kernel::Verdict::False);
        assert_eq!(d.trace, vec![0, 0]);
    }
}
#[test]
fn counterexample_successor_is_excluded() {
    let holds = [true, false];
    let after = [1, 1];
    let d = lemma_kernel::decide(1, 0, &after, &[true, false], &holds);
    assert!(d.is_some());
    if let Some(d) = d {
        assert_eq!(d.verdict, lemma_kernel::Verdict::False);
        assert!(d.counterexample.is_some());
        if let Some((before, input, next)) = d.counterexample {
            assert!(holds[before]);
            assert_eq!(input, 0);
            assert!(!holds[next]);
            assert_eq!(after[before], next);
        }
    }
}
#[test]
fn malformed_table_refusals_and_checked_product() {
    assert!(lemma_kernel::decide(0, 0, &[], &[], &[true]).is_none());
    assert!(lemma_kernel::decide(1, 0, &[], &[], &[]).is_none());
    assert!(lemma_kernel::decide(1, 1, &[0], &[true], &[true]).is_none());
    assert!(lemma_kernel::decide(1, 0, &[0], &[], &[true]).is_none());
    assert!(lemma_kernel::decide(1, 0, &[1], &[true], &[true]).is_none());
    assert!(!lemma_kernel::valid_tables(usize::MAX, 2, 0, &[], &[]));
    assert_eq!(usize::MAX.checked_mul(2), None);
    assert_eq!(3usize.checked_mul(4), Some(12));
}
