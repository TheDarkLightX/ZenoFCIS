//! Native oracle arithmetic, accounting and price regressions.
use zeno_fcis_synthesis::finite::v2_oracle::*;

fn limits() -> Limits {
    zero_limits().with_limit(Resource::Step, 1_000_000)
}
fn u(s: &str) -> u128 {
    s.parse()
        .unwrap_or_else(|_| panic!("invalid independent integer: {s}"))
}
fn p(s: &str) -> PriceE8 {
    PriceE8::new(u(s)).unwrap_or_else(|_| panic!("positive fixture"))
}

#[test]
fn oracle_full_width_arithmetic_matches_independent_integer_vectors() {
    let mut counts = [0; 3];
    for row in include_str!("test-data/oracle/arithmetic.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let xs: Vec<_> = row.split('\t').collect();
        match xs[0] {
            "M" => {
                counts[0] += 1;
                let result = median3(
                    [p(xs[1]), p(xs[2]), p(xs[3])],
                    [7, 3, 5],
                    [8, 2, 6],
                    limits(),
                );
                if xs[4] == "overflow" {
                    assert_eq!(result.result(), Err(&Failure::Arithmetic));
                } else {
                    let numbers = result.result().unwrap_or_else(|e| panic!("{row}: {e:?}"));
                    assert_eq!(
                        (
                            numbers.median.raw(),
                            numbers.confidence,
                            numbers.deviation_bps
                        ),
                        (u(xs[4]), u(xs[5]), u(xs[6])),
                        "{row}"
                    );
                    assert_eq!((numbers.oldest_epoch, numbers.median_seconds), (3, 6));
                }
            }
            "C" => {
                counts[1] += 1;
                let result = security_capacity(
                    ZdexE18(u(xs[1])),
                    StressE8(u(xs[2])),
                    u(xs[3]),
                    u(xs[4]),
                    limits(),
                );
                if xs[5] == "overflow" {
                    assert_eq!(result.result(), Err(&Failure::Arithmetic), "{row}");
                } else {
                    assert_eq!(result.result(), Ok(&UsdExposureE18(u(xs[5]))), "{row}");
                }
            }
            "B" => {
                counts[2] += 1;
                let mut bonds = [u(xs[1]), u(xs[2]), u(xs[3])];
                bonds.sort();
                let s = bonds[0] + bonds[1];
                let out = security_capacity(ZdexE18(s), StressE8(100_000_000), 1, 2, limits());
                assert_eq!(out.result(), Ok(&UsdExposureE18(u(xs[5]))));
                assert_eq!(u(xs[4]) <= u(xs[5]), xs[6] == "1");
            }
            _ => panic!("unexpected vector"),
        }
    }
    assert_eq!(counts, [129, 324, 625]);
}

fn capacity_parameters(a: u128, b: u128, debt_ceiling: u128) -> Parameters {
    Parameters {
        clock: Some(ClockParameters {
            backend: [5; 32],
            heartbeat: 1,
            confirmation: 1,
            dispute_open: 2,
            inclusion: 0,
            max_age: 4,
            max_epoch_seconds: 1,
            max_time_lag: 2,
        }),
        disputes: Some(DisputeParameters {
            adjudicator: [6; 32],
            minimum_bond: ZdexE18(1),
            challenge_bond: ZdexE18(1),
            slash: ZdexE18(1),
            exit_delay: 1,
            finality_delay: 0,
            appeal_delay: 0,
            false_price_slashable: true,
            treasury: Id::new(&[8]).unwrap_or_else(|_| panic!("ID")),
            slash_recipient: EscrowRecipient::Challenger,
            upheld_bond_recipient: EscrowRecipient::Challenger,
            rejected_bond_recipient: EscrowRecipient::ReporterOwner,
        }),
        economy: Some(EconomicParameters {
            reward_unit: RewardUnit {
                asset: Id::new(&[9]).unwrap_or_else(|_| panic!("ID")),
                scale: 18,
            },
            reward_per_report: 1,
            alpha_numerator: a,
            alpha_denominator: b,
            stress_source: [7; 32],
            stress_haircut_numerator: 1,
            stress_haircut_denominator: 1,
            debt_ceiling: UsdExposureE18(debt_ceiling),
            liquidation_cap: UsdExposureE18(0),
            redemption_cap: UsdExposureE18(0),
            other_cap: UsdExposureE18(0),
            window: 4,
        }),
    }
}

#[test]
fn oracle_full_width_coalition_capacity_matches_independent_integer_vectors() {
    let mut count = 0;
    for row in include_str!("test-data/oracle/capacity-pairs.tsv")
        .lines()
        .filter(|s| !s.starts_with('#'))
    {
        let xs: Vec<_> = row.split('\t').collect();
        assert_eq!((xs[0], xs.len()), ("P", 8), "{row}");
        let inputs = SecurityInputs {
            selected_bonds: [u(xs[1]), u(xs[2]), u(xs[3])].map(ZdexE18),
            stress: Some(StressE8(u(xs[4]))),
            stress_source: [7; 32],
            valuation_root: [8; 32],
        };
        let status = |ceiling| {
            capacity_status(
                &capacity_parameters(u(xs[5]), u(xs[6]), ceiling),
                &inputs,
                limits(),
            )
            .into_parts()
            .0
            .unwrap_or_else(|e| panic!("{row}: {e:?}"))
        };
        if xs[7] == "overflow" {
            assert_eq!(status(0), CapacityStatus::Unavailable, "{row}");
        } else {
            let capacity = UsdExposureE18(u(xs[7]));
            assert_eq!(
                status(capacity.0),
                CapacityStatus::Available(capacity),
                "{row}"
            );
            if let Some(over) = capacity.0.checked_add(1) {
                assert_eq!(status(over), CapacityStatus::OverCap(capacity), "{row}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 750);
}

#[test]
fn oracle_arithmetic_refusal_and_retry_preserve_measured_prefix() {
    let full = security_capacity(ZdexE18(u128::MAX), StressE8(1), 1, 2, limits());
    assert!(full.result().is_ok());
    assert_eq!(full.usage().used(Resource::Step), 130);
    for bound in [0, 1, 2, 50, 129] {
        let run = || {
            security_capacity(
                ZdexE18(u128::MAX),
                StressE8(1),
                1,
                2,
                zero_limits().with_limit(Resource::Step, bound),
            )
        };
        let a = run();
        let b = run();
        assert!(matches!(a.result(), Err(Failure::Budget(_))));
        assert_eq!(a.result(), b.result());
        assert_eq!(a.usage(), b.usage());
        assert_eq!(a.usage().used(Resource::Step), bound);
    }
    let exact = security_capacity(
        ZdexE18(u128::MAX),
        StressE8(1),
        1,
        2,
        zero_limits().with_limit(Resource::Step, 130),
    );
    assert_eq!(full.result(), exact.result());
    let invalid = security_capacity(ZdexE18(1), StressE8(1), 1, 1, limits());
    assert_eq!(invalid.result(), Err(&Failure::InvalidProfile));
    assert_eq!(invalid.usage().used(Resource::Step), 1);
}
