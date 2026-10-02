//! Independent native oracle: wider arithmetic, no production evaluator calls.
use super::*;
use alloc::vec;

const RESOURCES: [Resource; 8] = [
    Resource::Read,
    Resource::Write,
    Resource::Candidate,
    Resource::Effect,
    Resource::Byte,
    Resource::WitnessByte,
    Resource::Depth,
    Resource::Step,
];

fn contains(domain: Domain, value: i64) -> bool {
    match domain {
        Domain::Bool => value == 0 || value == 1,
        Domain::Int { min, max } => min <= value && value <= max,
    }
}

pub(super) fn domains_match(domains: &[Domain], values: &[i64]) -> bool {
    domains.len() == values.len() && domains.iter().zip(values).all(|(d, v)| contains(*d, *v))
}

pub(super) fn reference_node(op: &Op, input: &[i64], values: &[i64]) -> Result<i64, Failure> {
    let read = |id: u16| {
        values
            .get(usize::from(id))
            .copied()
            .ok_or(Failure::Reference)
    };
    match *op {
        Op::Input(id) => input
            .get(usize::from(id))
            .copied()
            .ok_or(Failure::Reference),
        Op::Int(value) => Ok(value),
        Op::Bool(value) => Ok(if value { 1 } else { 0 }),
        Op::Not(id) => Ok(if read(id)? == 0 { 1 } else { 0 }),
        Op::Select(c, a, b) => {
            let condition = read(c)?;
            let left = read(a)?;
            let right = read(b)?;
            Ok(if condition == 1 { left } else { right })
        }
        Op::Add(a, b) | Op::Sub(a, b) => {
            let left = i128::from(read(a)?);
            let right = i128::from(read(b)?);
            let value = if matches!(op, Op::Add(_, _)) {
                left + right
            } else {
                left - right
            };
            i64::try_from(value).map_err(|_| Failure::Arithmetic)
        }
        Op::Eq(a, b) => Ok(if read(a)? == read(b)? { 1 } else { 0 }),
        Op::Lt(a, b) => Ok(if read(a)? < read(b)? { 1 } else { 0 }),
        Op::And(a, b) => {
            let left = read(a)?;
            let right = read(b)?;
            Ok(if left == 1 && right == 1 { 1 } else { 0 })
        }
    }
}

fn reference(
    inputs: &[Domain],
    outputs: &[Domain],
    nodes: &[Op],
    roots: &[u16],
    input: &[i64],
    quota: u64,
) -> (Result<Vec<i64>, Failure>, u64) {
    if !domains_match(inputs, input) {
        return (Err(Failure::InputDomain), 0);
    }
    let mut steps = 0u64;
    let mut values = Vec::new();
    for op in nodes {
        let proposed = u128::from(steps) + 1;
        if proposed > u128::from(quota) {
            return (
                Err(Failure::Budget(MeterFailure {
                    resource: Resource::Step,
                    limit: quota,
                    attempted: proposed as u64,
                    overflow: false,
                })),
                steps,
            );
        }
        steps = proposed as u64;
        match reference_node(op, input, &values) {
            Ok(value) => values.push(value),
            Err(error) => return (Err(error), steps),
        }
    }
    let mut output = Vec::new();
    for id in roots {
        match values.get(usize::from(*id)) {
            Some(value) => output.push(*value),
            None => return (Err(Failure::Reference), steps),
        }
    }
    if !domains_match(outputs, &output) {
        return (Err(Failure::OutputDomain), steps);
    }
    (Ok(output), steps)
}

fn compare(inputs: &[Domain], outputs: &[Domain], nodes: &[Op], roots: &[u16], input: &[i64]) {
    for quota in (0..=nodes.len() as u64 + 1).chain([u64::MAX]) {
        let (expected, steps) = reference(inputs, outputs, nodes, roots, input, quota);
        let (actual, usage) = execute(
            inputs,
            outputs,
            nodes,
            roots,
            input,
            zero_limits().with_limit(Resource::Step, quota),
        )
        .into_parts();
        assert_eq!(
            actual, expected,
            "quota={quota}; nodes={nodes:?}; input={input:?}"
        );
        for resource in RESOURCES {
            assert_eq!(
                usage.used(resource),
                if resource == Resource::Step { steps } else { 0 }
            );
        }
    }
}

#[test]
fn quotas_traps_unused_nodes_and_refusals_match_wider_oracle() {
    let full = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let nodes = [Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1), Op::Int(7)];
    compare(&[], &[full], &nodes, &[3], &[]);
    for (quota, error, steps) in [
        (
            2,
            Failure::Budget(MeterFailure {
                resource: Resource::Step,
                limit: 2,
                attempted: 3,
                overflow: false,
            }),
            2,
        ),
        (3, Failure::Arithmetic, 3),
        (u64::MAX, Failure::Arithmetic, 3),
    ] {
        let (result, usage) = execute(
            &[],
            &[full],
            &nodes,
            &[3],
            &[],
            zero_limits().with_limit(Resource::Step, quota),
        )
        .into_parts();
        assert_eq!(result, Err(error));
        assert_eq!(usage.used(Resource::Step), steps);
    }
    let unselected = [
        Op::Bool(false),
        Op::Int(i64::MAX),
        Op::Int(1),
        Op::Add(1, 2),
        Op::Int(7),
        Op::Select(0, 3, 4),
    ];
    compare(&[], &[full], &unselected, &[5], &[]);
    let maximum = vec![Op::Int(7); 256];
    compare(&[], &[full], &maximum, &[0], &[]);
    compare(&[Domain::Bool], &[full], &[Op::Input(0)], &[0], &[2]);
    compare(&[full], &[full], &[Op::Input(0)], &[0], &[]);
    compare(&[], &[Domain::Bool], &[Op::Int(2)], &[0], &[]);
    compare(&[], &[full, full], &[Op::Int(7)], &[0, 1], &[]);
}

#[test]
fn all_instructions_boundaries_and_output_abi_match_wider_oracle() {
    let full = Domain::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    for left in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
        for right in [i64::MIN, -1, 0, 1, i64::MAX] {
            for op in [
                Op::Add(0, 1),
                Op::Sub(0, 1),
                Op::Eq(0, 1),
                Op::Lt(0, 1),
                Op::And(0, 1),
            ] {
                compare(
                    &[full, full],
                    &[full],
                    &[Op::Input(0), Op::Input(1), op],
                    &[2],
                    &[left, right],
                );
            }
        }
        compare(&[full], &[full], &[Op::Input(0), Op::Not(0)], &[1], &[left]);
    }
    for flag in [false, true] {
        compare(
            &[],
            &[full, full, Domain::Bool],
            &[
                Op::Bool(flag),
                Op::Int(19),
                Op::Int(-23),
                Op::Select(0, 1, 2),
            ],
            &[3, 1, 0],
            &[],
        );
    }
    compare(
        &[],
        &[full, full, full],
        &[Op::Int(11), Op::Int(-7)],
        &[1, 0, 1],
        &[],
    );
    for bad in [
        Op::Input(u16::MAX),
        Op::Not(0),
        Op::Add(0, 0),
        Op::Sub(0, 0),
        Op::Eq(0, 0),
        Op::Lt(0, 0),
        Op::And(0, 0),
        Op::Select(0, 0, 0),
    ] {
        compare(&[], &[full], &[bad], &[0], &[]);
    }
}

#[test]
fn charges_match_u128_oracle_for_every_resource_and_preserve_all_other_counters() {
    for (index, resource) in RESOURCES.into_iter().enumerate() {
        for current in [0u64, 1, 7, u64::MAX - 1, u64::MAX] {
            for amount in [0u64, 1, 7, u64::MAX] {
                for limit in [0u64, 1, 7, u64::MAX - 1, u64::MAX] {
                    let mut limits = zero_limits();
                    for (other, class) in RESOURCES.into_iter().enumerate() {
                        limits = limits.with_limit(
                            class,
                            if other == index {
                                limit
                            } else {
                                other as u64 + 31
                            },
                        );
                    }
                    let mut before = [0; 8];
                    for (other, value) in before.iter_mut().enumerate() {
                        *value = other as u64 + 3;
                    }
                    before[index] = current;
                    let mut meter = Meter {
                        limits,
                        used: Usage { counters: before },
                    };
                    let total = u128::from(current) + u128::from(amount);
                    let expected = if total > u128::from(u64::MAX) {
                        Err(MeterFailure {
                            resource,
                            limit,
                            attempted: u64::MAX,
                            overflow: true,
                        })
                    } else if total > u128::from(limit) {
                        Err(MeterFailure {
                            resource,
                            limit,
                            attempted: total as u64,
                            overflow: false,
                        })
                    } else {
                        Ok(())
                    };
                    assert_eq!(meter.charge(resource, amount), expected);
                    assert_eq!(meter.limits, limits);
                    let mut after = before;
                    if expected.is_ok() {
                        after[index] = total as u64;
                    }
                    assert_eq!(meter.used.counters, after);
                }
            }
        }
    }
}

#[test]
fn buffer_refusal_clears_both_buffers_without_rolling_back_consumption() {
    let mut meter = meter::new(zero_limits().with_limit(Resource::Step, 9));
    let mut values = vec![99];
    let mut output = vec![99];
    assert_eq!(
        evaluate_into(
            &[],
            &[Domain::Bool],
            &[Op::Int(2)],
            &[0],
            &[],
            &mut meter,
            &mut values,
            &mut output
        ),
        Err(Failure::OutputDomain)
    );
    assert!(values.is_empty() && output.is_empty());
    assert_eq!(meter.used.used(Resource::Step), 1);
    assert_eq!(
        evaluate_into(
            &[],
            &[Domain::Bool],
            &[Op::Bool(true)],
            &[0],
            &[],
            &mut meter,
            &mut values,
            &mut output
        ),
        Ok(())
    );
    assert_eq!(output, [1]);
    assert_eq!(meter.used.used(Resource::Step), 2);
}
