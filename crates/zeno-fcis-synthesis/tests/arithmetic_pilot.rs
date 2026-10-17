//! Independent finite characterization and the actual numeric evaluator bridge.
// Shared proof subjects retain public entry points for their standalone harness.
#![allow(unreachable_pub, clippy::unwrap_used)]
#[path = "../../../verification/arithmetic/graphs.rs"]
mod graphs;
#[allow(dead_code)]
#[path = "../../../verification/arithmetic/spot.rs"]
mod spot;
#[allow(dead_code)]
#[path = "../../../verification/arithmetic/spot_bounds.rs"]
mod spot_bounds;
#[allow(dead_code)]
#[path = "../src/finite/value/wide.rs"]
mod wide;

use zeno_fcis_synthesis::finite::{
    V2NumericAtom as Atom, V2NumericDomain as Domain, V2NumericFailure as Failure,
    V2NumericOp as Op, V2NumericProgram as Program, V2Resource as Resource, execute_numeric_v2,
    v2_zero_limits,
};

fn run(
    nodes: &[Op],
    inputs: &[Domain<'_>],
    values: &[u128],
    outputs: &[Domain<'_>],
    roots: &[u16],
    budget: u64,
) -> (Result<Vec<Atom<'static>>, Failure>, u64) {
    let p = Program {
        inputs,
        outputs,
        nodes,
        roots,
    };
    assert_eq!(p.admit(), Ok(()));
    let values: Vec<_> = values.iter().copied().map(Atom::U128).collect();
    let (outcome, usage) = execute_numeric_v2(
        p,
        &values,
        v2_zero_limits().with_limit(Resource::Step, budget),
    )
    .into_parts();
    (outcome, usage.used(Resource::Step))
}

fn u128_at(values: &[Atom<'_>], index: usize) -> u128 {
    match values[index] {
        Atom::U128(v) => v,
        _ => panic!("unsigned result expected"),
    }
}

#[test]
fn reduced_complete_fee_domain_and_full_width_edges() {
    let mut rows = 0;
    for gross in 0u128..=63 {
        for rate in 0u128..=10_000 {
            let expected = (gross * rate).div_ceil(10_000);
            assert_eq!(
                spot::fee_split(gross, rate),
                Some((expected, gross - expected))
            );
            rows += 1;
        }
    }
    assert_eq!(rows, 640_064);
    for gross in [
        0,
        1,
        9_999,
        10_000,
        3_000_000_000,
        1u128 << 127,
        u128::MAX - 1,
        u128::MAX,
    ] {
        for rate in [0, 1, 2, 200, 9_999, 10_000] {
            // Independent decomposition keeps every temporary within u128.
            let expected = (gross / 10_000) * rate + ((gross % 10_000) * rate).div_ceil(10_000);
            let actual = spot::fee_split(gross, rate);
            assert_eq!(actual, Some((expected, gross - expected)));
            let (fee, net) = actual.unwrap();
            println!("ARITHMETIC fee {gross} {rate} {fee} {net}");
        }
    }
    assert_eq!(spot::fee_split(u128::MAX, 10_001), None);
    println!("ARITHMETIC_COUNT fee_reduced {rows}");
}

#[test]
fn scaled_swap_bridge_checks_full_output_and_every_eager_node() {
    let bounds = graphs::analyse_graph(
        graphs::Graph::Swap,
        &graphs::SWAP_INPUTS,
        &graphs::SWAP_NODES,
    )
    .unwrap();
    assert_eq!(bounds.len(), graphs::SWAP_NODES.len());
    let roots: Vec<_> = (0..bounds.len() as u16).collect();
    let mut rows = 0;
    for rin in 1u128..=8 {
        for rout in 1u128..=8 {
            for gross in 0u128..=15 {
                for rate in [0u128, 1, 200, 9_999, 10_000] {
                    let fee = (gross * rate).div_ceil(10_000);
                    let net = gross - fee;
                    let den = rin + net;
                    let out = rout * net / den;
                    let (actual, used) = run(
                        &graphs::SWAP_NODES,
                        &graphs::SWAP_INPUTS,
                        &[gross, rate, rin, rout],
                        &bounds,
                        &roots,
                        12,
                    );
                    let actual = actual.unwrap();
                    assert_eq!(used, 12);
                    assert_eq!(u128_at(&actual, 6), fee);
                    assert_eq!(u128_at(&actual, 7), net);
                    assert_eq!(u128_at(&actual, 8), den);
                    assert_eq!(u128_at(&actual, 9), out);
                    assert_eq!(u128_at(&actual, 10), rout - out);
                    assert_eq!(actual[11], Atom::Bool(true));
                    let q = spot::exact_in(gross, rate, rin, rout).unwrap();
                    assert_eq!(
                        (q.fee, q.net, q.output, q.input_reserve, q.output_reserve),
                        (fee, net, out, den, rout - out)
                    );
                    assert!(den * (rout - out) >= rin * rout);
                    rows += 1;
                }
            }
        }
    }
    assert_eq!(rows, 5_120);
    for (gross, rate, rin, rout) in [
        (3_000_000_000, 1, 3_000_000_000, 3_000_000_000),
        (3_000_000_000, 0, 3_000_000_000, 3_000_000_000),
        (3_000_000_000, 10_000, 1, 3_000_000_000),
        (0, 0, 1, 1),
        (1, 1, 1, 1),
    ] {
        let (actual, used) = run(
            &graphs::SWAP_NODES,
            &graphs::SWAP_INPUTS,
            &[gross, rate, rin, rout],
            &graphs::SWAP_OUTPUTS,
            &graphs::SWAP_ROOTS,
            12,
        );
        let actual = actual.unwrap();
        assert_eq!(used, 12);
        println!(
            "ARITHMETIC swap {gross} {rate} {rin} {rout} {} {} {} {} {}",
            u128_at(&actual, 0),
            u128_at(&actual, 1),
            u128_at(&actual, 2),
            u128_at(&actual, 3),
            u128_at(&actual, 4)
        );
        assert_eq!(actual[5], Atom::Bool(true));
    }
    println!("ARITHMETIC_COUNT swap_scaled {rows}");
}

#[test]
fn scaled_burn_and_both_mint_arms_use_the_numeric_driver() {
    let burn_bounds = graphs::analyse_graph(
        graphs::Graph::Burn,
        &graphs::BURN_INPUTS,
        &graphs::BURN_NODES,
    )
    .unwrap();
    let mint_bounds = graphs::analyse_graph(
        graphs::Graph::Mint,
        &graphs::MINT_INPUTS,
        &graphs::MINT_NODES,
    )
    .unwrap();
    let mut burns = 0;
    for lp in 0u128..=15 {
        for reserve in 1u128..=16 {
            for supply in 1u128..=16 {
                let (result, used) = run(
                    &graphs::BURN_NODES,
                    &graphs::BURN_INPUTS,
                    &[lp, reserve, supply],
                    &burn_bounds,
                    &[0, 1, 2, 3],
                    4,
                );
                assert_eq!(used, 4);
                let result = result.unwrap();
                let out = u128_at(&result, 3);
                assert_eq!(out, lp * reserve / supply);
                assert!(out * supply <= lp * reserve && lp * reserve < (out + 1) * supply);
                if lp <= supply {
                    assert_eq!(spot::lp_burn(lp, reserve, supply), Some(out));
                } else {
                    assert_eq!(spot::lp_burn(lp, reserve, supply), None);
                }
                burns += 1;
            }
        }
    }
    let mut mints = 0;
    for a0 in 0u128..=7 {
        for a1 in 0u128..=7 {
            for r0 in 1u128..=4 {
                for r1 in 1u128..=4 {
                    for supply in 1u128..=4 {
                        let roots: Vec<_> = (0..9).collect();
                        let (result, used) = run(
                            &graphs::MINT_NODES,
                            &graphs::MINT_INPUTS,
                            &[a0, a1, r0, r1, supply],
                            &mint_bounds,
                            &roots,
                            9,
                        );
                        assert_eq!(used, 9);
                        let result = result.unwrap();
                        let x = a0 * supply / r0;
                        let y = a1 * supply / r1;
                        assert_eq!(u128_at(&result, 5), x);
                        assert_eq!(u128_at(&result, 6), y);
                        assert_eq!(u128_at(&result, 8), x.min(y));
                        mints += 1;
                    }
                }
            }
        }
    }
    assert_eq!((burns, mints), (4_096, 4_096));
    for (lp, reserve, supply) in [
        (u128::MAX, u128::MAX, u128::MAX),
        (1, u128::MAX, 3),
        (3_000_000_000, 3_000_000_000, 3_000_000_000),
        (0, u128::MAX, 1),
    ] {
        let out = spot::lp_burn(lp, reserve, supply).unwrap();
        println!("ARITHMETIC burn {lp} {reserve} {supply} {out}");
    }
    assert_eq!(spot::lp_burn(1, 1, 0), None);
    println!("ARITHMETIC_COUNT burn_scaled {burns}");
    println!("ARITHMETIC_COUNT mint_scaled {mints}");
}

#[test]
fn profile_refusal_is_distinct_from_eager_arithmetic_and_budget_refusal() {
    let mut mint = graphs::MINT_NODES;
    mint[8] = Op::Select(7, 6, 5);
    assert!(graphs::analyse_graph(graphs::Graph::Mint, &graphs::MINT_INPUTS, &mint).is_none());
    let mut inputs = graphs::SWAP_INPUTS;
    inputs[2] = Domain::U128 {
        min: 0,
        max: graphs::LIMIT,
    };
    assert!(graphs::analyse_graph(graphs::Graph::Swap, &inputs, &graphs::SWAP_NODES).is_none());
    let mut width = graphs::SWAP_NODES;
    width[7] = Op::Sub(zeno_fcis_synthesis::finite::V2IntegerWidth::Signed64, 0, 6);
    assert!(graphs::analyse_graph(graphs::Graph::Swap, &graphs::SWAP_INPUTS, &width).is_none());
    assert!(spot_bounds::analyse(1, 0, 1).is_none());
    assert!(spot_bounds::analyse(graphs::LIMIT + 1, 1, 1).is_none());
    let mut eager = graphs::SWAP_NODES.to_vec();
    eager.extend([Op::U128(0), Op::MulDivFloor(5, 5, 12)]);
    let (result, used) = run(
        &eager,
        &graphs::SWAP_INPUTS,
        &[2, 0, 3, 5],
        &graphs::SWAP_OUTPUTS,
        &graphs::SWAP_ROOTS,
        14,
    );
    assert_eq!(result, Err(Failure::Arithmetic));
    assert_eq!(used, 14);
    for _ in 0..2 {
        let (result, used) = run(
            &eager,
            &graphs::SWAP_INPUTS,
            &[2, 0, 3, 5],
            &graphs::SWAP_OUTPUTS,
            &graphs::SWAP_ROOTS,
            13,
        );
        assert!(matches!(result, Err(Failure::Budget(_))));
        assert_eq!(used, 13);
    }
    let (result, used) = run(
        &graphs::SWAP_NODES,
        &graphs::SWAP_INPUTS,
        &[graphs::LIMIT + 1, 0, 1, 1],
        &graphs::SWAP_OUTPUTS,
        &graphs::SWAP_ROOTS,
        12,
    );
    assert_eq!(result, Err(Failure::InputDomain));
    assert_eq!(used, 0);
}
