use zenofcis_zusd_v1::{ledger::*, *};
fn limits() -> Limits {
    Limits {
        steps: 1_000_000,
        reads: 1_000,
        bytes: 1_000_000,
    }
}
#[test]
fn exact_wide_oracle() {
    for line in include_str!("test-data/wide.tsv").lines() {
        let x: Vec<_> = line.split('\t').collect();
        let a = x[0].parse().unwrap();
        let b = x[1].parse().unwrap();
        let d = x[2].parse().unwrap();
        let result = mul_div(a, b, d, limits());
        if x[3] == "refuse" {
            assert_eq!(result.result(), Err(Failure::Arithmetic));
        } else {
            assert_eq!(
                result.result(),
                Ok(&x[3].parse::<u128>().unwrap()),
                "{line}"
            );
        }
    }
    assert_eq!(
        divided_product(u128::MAX, u128::MAX, Q, u128::MAX, limits()).result(),
        Ok(&(u128::MAX / Q))
    );
}
#[test]
fn matches_existing_numeric_engine_without_signed_projection() {
    use zeno_fcis_synthesis::finite::{
        V2NumericAtom as A, V2NumericDomain as D, V2NumericOp as O, V2NumericProgram as P,
        V2Resource, execute_numeric_v2, v2_zero_limits,
    };
    for a in [0, 1, 1 << 127, u128::MAX] {
        for b in [1, Q, u128::MAX] {
            for d in [0, 1, Q, u128::MAX] {
                let nodes = [O::U128(a), O::U128(b), O::U128(d), O::MulDivFloor(0, 1, 2)];
                let domains = [D::U128 {
                    min: 0,
                    max: u128::MAX,
                }];
                let roots = [3];
                let p = P {
                    inputs: &[],
                    outputs: &domains,
                    nodes: &nodes,
                    roots: &roots,
                };
                let (native, _) =
                    execute_numeric_v2(p, &[], v2_zero_limits().with_limit(V2Resource::Step, 100))
                        .into_parts();
                let ours = mul_div(a, b, d, limits());
                match native {
                    Ok(v) => assert_eq!(
                        ours.result(),
                        Ok(&match v[0] {
                            A::U128(n) => n,
                            _ => panic!(),
                        })
                    ),
                    Err(_) => assert_eq!(ours.result(), Err(Failure::Arithmetic)),
                }
            }
        }
    }
}
#[test]
fn canonical_wide_conservation_and_owned_results() {
    let pre = Sheet {
        balances: vec![
            Balance {
                account: [0; 32],
                quantity: Quantity::Agrs(u128::MAX),
            },
            Balance {
                account: [1; 32],
                quantity: Quantity::Agrs(u128::MAX),
            },
        ],
        supply: 0,
        active_debt: 0,
        default_debt: 0,
    };
    let frozen = pre.clone();
    let observed = observe_sheet(&pre, 2, limits());
    assert!(observed.result().is_ok());
    assert_eq!(pre, frozen);
    let value = observed.result().unwrap().value();
    let mut bytes = vec![];
    value.encode_zcve_to(&mut bytes).unwrap();
    let decoded = zeno_fcis_codec::decode_value(&bytes, Default::default()).unwrap();
    assert_eq!(&decoded, value);
    bytes.push(0);
    assert!(zeno_fcis_codec::decode_value(&bytes, Default::default()).is_err());
    let post = pre.clone();
    assert!(check_transition(&pre, &post, 2, limits()).result().is_ok());
    let mut wrong = post;
    wrong.balances[1].quantity = Quantity::Zdex(u128::MAX);
    assert_eq!(
        check_transition(&pre, &wrong, 2, limits()).result(),
        Err(Failure::Conservation)
    );
    let mut duplicate = pre.clone();
    duplicate.balances[1].account = [0; 32];
    assert!(matches!(
        observe_sheet(&duplicate, 2, limits()).result(),
        Err(Failure::Canonical)
    ));
    assert!(matches!(
        observe_sheet(&pre, 1, limits()).result(),
        Err(Failure::Capacity)
    ));
}
#[test]
fn exact_refusal_prefix_and_retry() {
    let a = mul_div(u128::MAX, u128::MAX, 1, limits());
    assert_eq!(a.result(), Err(Failure::Arithmetic));
    let b = mul_div(u128::MAX, u128::MAX, 1, limits());
    assert_eq!(a.usage(), b.usage());
    let capped = Limits {
        steps: a.usage().steps() - 1,
        ..limits()
    };
    assert!(matches!(
        mul_div(u128::MAX, u128::MAX, 1, capped).result(),
        Err(Failure::Budget { .. })
    ));
}
#[test]
fn exact_effects_debt_and_atomic_frame() {
    let pre = Sheet {
        balances: vec![],
        supply: 0,
        active_debt: 0,
        default_debt: 0,
    };
    let leg = Movement {
        from: None,
        to: Some([7; 32]),
        quantity: Quantity::Zusd(1 << 127),
    };
    let result = compose(&pre, &[leg], (1 << 127, 0), 2, limits());
    let plan = result.result().unwrap();
    assert_eq!(
        plan.post().balances,
        vec![Balance {
            account: [7; 32],
            quantity: leg.quantity
        }]
    );
    assert_eq!(plan.movements(), &[leg]);
    assert!(pre.balances.is_empty());
    assert!(matches!(
        compose(&pre, &[leg], (0, 0), 2, limits()).result(),
        Err(Failure::Conservation)
    ));
    let bad = Movement {
        from: Some([7; 32]),
        to: Some([8; 32]),
        quantity: Quantity::Zusd(u128::MAX),
    };
    assert!(matches!(
        compose(&pre, &[leg, bad], (1 << 127, 0), 2, limits()).result(),
        Err(Failure::Reject(1033))
    ));
    assert!(pre.balances.is_empty());
    let counterfeit = Movement {
        quantity: Quantity::Agrs(1),
        ..leg
    };
    assert!(matches!(
        compose(&pre, &[counterfeit], (0, 0), 2, limits()).result(),
        Err(Failure::Conservation)
    ));
}
