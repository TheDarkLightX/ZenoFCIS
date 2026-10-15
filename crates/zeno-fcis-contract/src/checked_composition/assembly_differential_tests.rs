//! Differential evidence for the validator's trusted implementation: its
//! route-specialized terms agree with the library evaluator's execution of
//! the aggregate.
//!
//! On every compared input, with the target set to route `k`, each output
//! the evaluator computes for the aggregate is the value of the validator's
//! specialized term; each output the evaluator computes for leaf `k`, on the
//! aggregate input read at the leaf's keys, is the value of its renamed
//! term; and the aggregate's selected case, paired by the validator, decides
//! as the leaf's selected case: the same class and reason, the leaf's value
//! in each of its slots and every other slot unchanged. For two leaves every
//! aggregate input is compared. For 4 to 32 leaves, every input of each
//! route's leaf is compared, with the slots the leaf does not read drawn
//! from a fixed seeded sample.

use std::collections::{BTreeMap, BTreeSet};

use super::aggregate::{Layout, state_field};
use super::assembly::{self, CompiledAggregate, aggregate_key, leaf_key};
use super::hierarchy_tests::{chain, group, grouped};
use super::leaf;
use super::manifest::Manifest;
use super::structure;
use super::term::Key;
use super::tests::{Ring, fail};
use super::{CompositionSources, aggregate, check};
use crate::internals::declarations::Source;
use crate::internals::graph::Atom;
use crate::internals::model::{Contract, Expr, ScalarDomain};

/// Fixed assignments of the slots a route's leaf does not read.
const SAMPLES: usize = 8;

/// SplitMix64 from a fixed seed, so the sample is the same on every run.
struct Sample(u64);

impl Sample {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn within(&mut self, (min, max): (i64, i64)) -> i64 {
        let width = u64::try_from(i128::from(max) - i128::from(min) + 1).unwrap_or(u64::MAX);
        let offset = i64::try_from(self.next() % width).unwrap_or_else(fail);
        min + offset
    }
}

/// A successor value in the evaluator's outputs and inputs.
fn value(expr: &Expr, outputs: &[i64], input: &dyn Fn(Source, u16) -> i64) -> i64 {
    match expr {
        Expr::Constant(Atom::Bool(value)) => i64::from(*value),
        Expr::Constant(Atom::I128(value)) => i64::try_from(*value).unwrap_or_else(fail),
        Expr::Constant(Atom::Sum { variant, .. }) => i64::from(*variant),
        Expr::Input(source, field) => input(*source, *field),
        Expr::Output(index) => outputs[*index],
        other => fail(format!("unexpected successor value {other:?}")),
    }
}

/// Every tuple of values within `bounds`, in order.
fn tuples(bounds: &[(i64, i64)]) -> Vec<Vec<i64>> {
    let mut out = vec![Vec::new()];
    for (min, max) in bounds {
        out = out
            .into_iter()
            .flat_map(|prefix| {
                (*min..=*max).map(move |value| {
                    let mut tuple = prefix.clone();
                    tuple.push(value);
                    tuple
                })
            })
            .collect();
    }
    out
}

/// Compares a composition of `n` registers; returns the inputs compared.
fn compare(ring: &Ring, n: usize) -> u64 {
    let manifest_text = ring.manifest_text();
    let sources = CompositionSources {
        manifest: &manifest_text,
        leaves: &ring.leaves,
        aggregate: None,
    };
    let files = aggregate(sources).unwrap_or_else(fail);
    check(CompositionSources {
        aggregate: Some(&files),
        ..sources
    })
    .unwrap_or_else(fail);
    let manifest = Manifest::read(&manifest_text).unwrap_or_else(fail);
    let compiled: Vec<leaf::Compiled> = manifest
        .leaves
        .iter()
        .zip(&ring.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<_, _>>()
        .unwrap_or_else(fail);
    let judged = structure::judge(&manifest, &compiled).unwrap_or_else(fail);
    let layout = Layout::of(&manifest, &compiled, &judged.edges).unwrap_or_else(fail);
    let built = CompiledAggregate::read(&files).unwrap_or_else(fail);
    let contract = built.contract().unwrap_or_else(fail);
    let program = contract.program().unwrap_or_else(fail);
    let (arena, routes) = assembly::specialization(&manifest, &compiled, &judged.edges, &contract)
        .unwrap_or_else(fail);
    let keys: Vec<Key> = contract
        .bindings
        .iter()
        .map(|(source, field)| {
            aggregate_key(*source, field.unwrap_or_else(|| fail("a root input")))
        })
        .collect();
    let position: BTreeMap<Key, usize> = keys
        .iter()
        .enumerate()
        .map(|(at, key)| (*key, at))
        .collect();
    let bounds: Vec<(i64, i64)> = contract
        .input_domains
        .iter()
        .map(|domain| match domain {
            ScalarDomain::Bool => (0, 1),
            ScalarDomain::Int { min, max } => (*min, *max),
        })
        .collect();
    let target = position[&Key::Command(layout.target.0)];
    let mut sample = Sample(0x5EED_0A55_E3B1_0000 ^ u64::try_from(n).unwrap_or_else(fail));
    let mut compared = 0u64;
    for (route, terms) in routes.iter().enumerate() {
        let leaf = &compiled[route];
        let leaf_contract =
            Contract::build(&leaf.declarations, &leaf.rules, leaf.commitment).unwrap_or_else(fail);
        let leaf_program = leaf_contract.program().unwrap_or_else(fail);
        let leaf_at = |source: Source, field: u16| -> usize {
            position[&leaf_key(&layout, route, source, field)
                .unwrap_or_else(|| fail("an unmapped leaf input"))]
        };
        let leaf_positions: Vec<usize> = leaf_contract
            .bindings
            .iter()
            .map(|(source, field)| leaf_at(*source, field.unwrap_or_else(|| fail("a root input"))))
            .collect();
        let local: Vec<usize> = leaf_positions
            .iter()
            .copied()
            .collect::<BTreeSet<usize>>()
            .into_iter()
            .filter(|at| *at != target)
            .collect();
        // The inputs the leaf does not read; one with a single value takes it.
        let free: Vec<usize> = (0..keys.len())
            .filter(|at| *at != target && !local.contains(at))
            .collect();
        let draws: Vec<Vec<i64>> = if free.iter().all(|at| bounds[*at].0 == bounds[*at].1) {
            vec![free.iter().map(|at| bounds[*at].0).collect()]
        } else {
            (0..SAMPLES)
                .map(|_| free.iter().map(|at| sample.within(bounds[*at])).collect())
                .collect()
        };
        let local_bounds: Vec<(i64, i64)> = local.iter().map(|at| bounds[*at]).collect();
        for tuple in tuples(&local_bounds) {
            for draw in &draws {
                let mut x = vec![0i64; keys.len()];
                x[target] = i64::try_from(route).unwrap_or_else(fail);
                for (at, v) in local.iter().zip(&tuple).chain(free.iter().zip(draw)) {
                    x[*at] = *v;
                }
                let at = |key: Key| position.get(&key).map(|at| x[*at]);
                let actual = program.evaluate(&x).unwrap_or_else(fail);
                for (output, id) in terms.aggregate_outputs.iter().enumerate() {
                    assert_eq!(
                        arena.evaluate(*id, &at),
                        Some(actual[output]),
                        "n={n} route {route} aggregate output {output} at {x:?}"
                    );
                }
                let y: Vec<i64> = leaf_positions.iter().map(|at| x[*at]).collect();
                let leaf_actual = leaf_program.evaluate(&y).unwrap_or_else(fail);
                for (output, id) in terms.leaf_outputs.iter().enumerate() {
                    assert_eq!(
                        arena.evaluate(*id, &at),
                        Some(leaf_actual[output]),
                        "n={n} route {route} leaf output {output} at {x:?}"
                    );
                }
                // The aggregate's selected case decides as the leaf's.
                let (case, leaf_case) = (actual[0], leaf_actual[0]);
                assert_eq!(
                    terms.cases.get(&case),
                    Some(&leaf_case),
                    "n={n} route {route} at {x:?}"
                );
                let index = |case: i64| usize::try_from(case).unwrap_or_else(fail);
                let (mine, theirs) = (
                    &contract.branches[index(case)],
                    &leaf_contract.branches[index(leaf_case)],
                );
                assert_eq!((mine.class, mine.reason), (theirs.class, theirs.reason));
                let aggregate_input =
                    |source: Source, field: u16| x[position[&aggregate_key(source, field)]];
                let leaf_input = |source: Source, field: u16| x[leaf_at(source, field)];
                let leaf_values: BTreeMap<u16, i64> = theirs
                    .assignments
                    .iter()
                    .map(|assignment| {
                        (
                            assignment.field,
                            value(&assignment.value, &leaf_actual, &leaf_input),
                        )
                    })
                    .collect();
                let posts: BTreeMap<u16, i64> = mine
                    .assignments
                    .iter()
                    .map(|assignment| {
                        (
                            assignment.field,
                            value(&assignment.value, &actual, &aggregate_input),
                        )
                    })
                    .collect();
                if !mine.assignments.is_empty() {
                    for (slot, (owner, field)) in layout.slots.iter().enumerate() {
                        let post = posts[&state_field(slot)];
                        if *owner == route {
                            assert_eq!(
                                post, leaf_values[field],
                                "n={n} route {route} slot {slot} at {x:?}"
                            );
                        } else {
                            assert_eq!(
                                post,
                                x[position[&Key::State(state_field(slot))]],
                                "n={n} route {route} frames slot {slot}"
                            );
                        }
                    }
                }
                compared += 1;
            }
        }
    }
    compared
}

#[test]
fn route_specialization_agrees_with_the_evaluator() {
    // Two leaves: every aggregate input, 8 * 8 registers, 5 actions, 2 routes.
    assert_eq!(compare(&Ring::new(2), 2), 640);
    for n in [4, 8, 16, 32] {
        let compared = compare(&Ring::new(n), n);
        // Per route: the leaf's register, its peer's and 5 actions, with
        // each of the fixed samples of the other registers.
        let expected = u64::try_from(n * 8 * 8 * 5 * SAMPLES).unwrap_or_else(fail);
        assert_eq!(compared, expected);
        println!("route specialization agrees with the evaluator: n={n}, {compared} inputs");
    }
}

#[test]
fn route_specialization_agrees_on_an_environment_input() {
    // Register 0 reads its peer from the environment: an aggregate context
    // field. Every input of each route's leaf, with the other inputs drawn
    // from the fixed sample.
    let mut root = group(10, "chain", &[1, 2, 3]);
    root["inputs"] = serde_json::json!([{"id": 1, "leaf": 1, "field": 202}]);
    let ring = grouped(&chain(3), serde_json::json!([root]), 10);
    assert_eq!(
        compare(&ring, 3),
        u64::try_from(3 * 8 * 8 * 5 * SAMPLES).unwrap_or_else(fail)
    );
}
