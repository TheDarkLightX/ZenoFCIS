//! Closed price/quorum products frozen before execution; no symbolic signature
//! flags. These math inputs are explicit admission premises, not native reports.
use super::account_law_tests::{clock, id, limits, params};
use super::*;
use alloc::{vec, vec::Vec};
fn setup() -> (FeedRules, Vec<Source>) {
    let p = super::lifecycle_tests::profile();
    (
        FeedRules {
            feed: [1; 32],
            profile: [2; 32],
            query: [3; 32],
            clock: p
                .parameters
                .clock
                .unwrap_or_else(|| panic!("test fixture value is absent")),
            diversity: Diversity {
                minimum: [3; 5],
                max_same_class: [1; 5],
            },
            max_deviation_bps: 10000,
            bounds: p.bounds,
            frame_bytes: p.frame_bytes,
        },
        (1..=4)
            .map(|i| Source {
                id: id(i),
                operator: id(i),
                venue: id(i),
                family: id(i),
                transport: id(i),
                jurisdiction: id(i),
            })
            .collect(),
    )
}
fn observations(n: usize, round: u64) -> Vec<Observation> {
    (0..n)
        .map(|i| Observation {
            id: [i as u8 + 1; 32],
            admission: [i as u8 + 10; 32],
            reporter: id(i as u8 + 1),
            source: id(i as u8 + 1),
            feed: [1; 32],
            profile: [2; 32],
            query: [3; 32],
            round,
            value: PriceE8::new(2).unwrap_or_else(|error| panic!("test fixture result: {error:?}")),
            epoch: 0,
            seconds: 0,
        })
        .collect()
}
fn permutations(xs: &[Observation]) -> Vec<Vec<Observation>> {
    if xs.is_empty() {
        return vec![vec![]];
    }
    let mut out = Vec::new();
    for i in 0..xs.len() {
        let mut rest = xs.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head.clone());
            out.push(tail);
        }
    }
    out
}
fn compare(r: &FeedRules, sources: &[Source], os: &[Observation], now: u64) {
    // Both actual entry points invoke their independently written complete law.
    assert!(
        aggregate(r, 0, 0, now, os, sources, limits())
            .result()
            .is_ok()
    );
    assert!(
        time_quorum(r, 0, 0, now, os, sources, limits())
            .result()
            .is_ok()
    );
}
#[test]
fn oracle_finite_quorum_permutations_source_patterns_and_all_epoch_time_reductions() {
    let (r, sources) = setup();
    let mut cases = 0;
    for n in 0..=4 {
        for pattern in 0..4usize.pow(n as u32) {
            let mut code = pattern;
            let mut os = observations(n, 0);
            for o in &mut os {
                o.source = id((code % 4 + 1) as u8);
                code /= 4;
            }
            for os in permutations(&os) {
                compare(&r, &sources, &os, 0);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 6565);
    for a in 1..=5 {
        for b in 1..=5 {
            for c in 1..=5 {
                let mut os = observations(3, 0);
                for (o, p) in os.iter_mut().zip([a, b, c]) {
                    o.value = PriceE8::new(p)
                        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
                }
                for os in permutations(&os) {
                    compare(&r, &sources, &os, 0);
                    cases += 1;
                }
            }
        }
    }
    for epochs in 0..125 {
        for times in 0..125 {
            let mut e = epochs;
            let mut t = times;
            let mut os = observations(3, 0);
            for o in &mut os {
                o.epoch = e % 5;
                o.seconds = t % 5;
                e /= 5;
                t /= 5;
            }
            for os in permutations(&os) {
                compare(&r, &sources, &os, 4);
                cases += 1;
            }
        }
    }
    for dimension in 0..5 {
        for pattern in 0..256 {
            for minimum in 1..=3 {
                for maximum in 1..=3 {
                    let mut sources = sources.clone();
                    let mut code = pattern;
                    for s in &mut sources {
                        let class = id((code % 4 + 1) as u8);
                        code /= 4;
                        match dimension {
                            0 => s.operator = class,
                            1 => s.venue = class,
                            2 => s.family = class,
                            3 => s.transport = class,
                            _ => s.jurisdiction = class,
                        }
                    }
                    let mut r = r.clone();
                    r.diversity.minimum[dimension] = minimum;
                    r.diversity.max_same_class[dimension] = maximum;
                    compare(&r, &sources, &observations(4, 0), 0);
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 112585);
    let mut alias_cases = 0;
    for n in 0..=4 {
        for pattern in 0..4usize.pow(n as u32) {
            let mut os = observations(n, 0);
            let mut code = pattern;
            for o in &mut os {
                o.reporter = id((code % 4 + 1) as u8);
                code /= 4;
            }
            let duplicate = os
                .iter()
                .enumerate()
                .any(|(i, o)| os[..i].iter().any(|p| p.reporter == o.reporter));
            let actual = aggregate(&r, 0, 0, 0, &os, &sources, limits())
                .into_parts()
                .0;
            if duplicate {
                assert_eq!(actual, Err(Failure::InvalidState));
            } else {
                assert!(actual.is_ok());
            }
            alias_cases += 1;
        }
    }
    assert_eq!(alias_cases, 341);
    // Time remains defined when the corresponding price deviation overflows.
    let mut os = observations(3, 0);
    os[2].value =
        PriceE8::new(u128::MAX).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    assert_eq!(
        aggregate(&r, 0, 0, 0, &os, &sources, limits())
            .into_parts()
            .0,
        Err(Failure::Arithmetic)
    );
    assert!(matches!(
        time_quorum(&r, 0, 0, 0, &os, &sources, limits()).result(),
        Ok(Ok(_))
    ));
    std::println!(
        "oracle finite quorum: determined_cases={cases}, encoding_alias_controls={alias_cases}, total_entrypoint_checks={}",
        cases * 2 + alias_cases + 2
    );
}
#[test]
fn oracle_finite_two_round_price_slots_have_complete_outcomes_including_unavailable_security() {
    let (r, sources) = setup();
    let a0 = aggregate(&r, 0, 0, 0, &observations(4, 0), &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let mut os = observations(4, 1);
    for o in &mut os {
        o.value = PriceE8::new(1).unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
        o.epoch = 1;
        o.seconds = 1;
    }
    let a1 = aggregate(&r, 1, 1, 1, &os, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let q0 = time_quorum(&r, 0, 0, 0, &observations(4, 0), &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let q1 = time_quorum(&r, 1, 1, 1, &os, &sources, limits())
        .into_parts()
        .0
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"))
        .unwrap_or_else(|error| panic!("test fixture result: {error:?}"));
    let commands = [
        PriceCommand::Stage((a0.clone()).into()),
        PriceCommand::Stage((a1.clone()).into()),
        PriceCommand::Commit,
        PriceCommand::SyncTime(q0),
        PriceCommand::SyncTime(q1),
        PriceCommand::Quarantine(a0.id()),
        PriceCommand::Quarantine([99; 32]),
        PriceCommand::AdvanceEpoch,
    ];
    let mut count = 0;
    let mut accepts = 0u8;
    let mut worst = [0; 3];
    for committed in [false, true] {
        for pending in 0..3 {
            for quarantine in 0..4 {
                for epoch in 0..=4 {
                    for watermark in 0..=4 {
                        for nonce in 0..=4 {
                            if committed && pending == 1 || pending == 2 && epoch == 0 {
                                continue;
                            }
                            let a = if pending == 1 { &a0 } else { &a1 };
                            let s = PriceState {
                                committed: committed.then(|| a0.clone()),
                                pending: (pending > 0).then(|| Pending {
                                    aggregate: a.clone(),
                                    published: a.numbers().oldest_epoch,
                                    dispute_until: a.numbers().oldest_epoch + 2,
                                    earliest: a.numbers().oldest_epoch + 2,
                                }),
                                history: if committed { vec![a0.clone()] } else { vec![] },
                                quarantines: match quarantine {
                                    0 => vec![],
                                    1 => vec![[1; 32]],
                                    2 => vec![a0.id()],
                                    _ => vec![a1.id()],
                                },
                                watermark,
                                epoch,
                                nonce,
                            };
                            let original = s.clone();
                            for security in 0..3 {
                                let mut p = params();
                                if security == 2 {
                                    p.economy
                                        .as_mut()
                                        .unwrap_or_else(|| panic!("test fixture value is absent"))
                                        .debt_ceiling = UsdExposureE18(1);
                                }
                                let sec = SecurityInputs {
                                    selected_bonds: [ZdexE18(0); 3],
                                    stress: if security == 0 {
                                        None
                                    } else {
                                        Some(StressE8(100_000_000))
                                    },
                                    stress_source: [7; 32],
                                    valuation_root: [8; 32],
                                };
                                for now in 0..=4 {
                                    for sent in 0..=4 {
                                        for c in &commands {
                                            let (out, used) = price_relation(
                                                &r,
                                                PriceContext {
                                                    parameters: &p,
                                                    nonce: sent,
                                                    clock: clock(now),
                                                    security: &sec,
                                                },
                                                &s,
                                                c,
                                                limits(),
                                            )
                                            .into_parts();
                                            for (i, res) in
                                                [Resource::Step, Resource::Read, Resource::Byte]
                                                    .into_iter()
                                                    .enumerate()
                                            {
                                                worst[i] = worst[i].max(used.used(res));
                                            }
                                            let out = out.unwrap_or_else(|e| {
                                                panic!("undefined price slot c={c:?}: {e:?}")
                                            });
                                            if out.is_ok() {
                                                accepts |= match c {
                                                    PriceCommand::Stage(_) => 1,
                                                    PriceCommand::Commit => 2,
                                                    PriceCommand::SyncTime(_) => 4,
                                                    PriceCommand::Quarantine(_) => 8,
                                                    PriceCommand::AdvanceEpoch => 16,
                                                };
                                            }
                                            count += 1;
                                        }
                                    }
                                }
                            }
                            assert_eq!(s, original);
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 1_380_000);
    assert_eq!(accepts, 31);
    std::println!(
        "oracle finite prices: cases={count}, accepts_by_kind={accepts}, max_step_read_byte={worst:?}"
    );
}
