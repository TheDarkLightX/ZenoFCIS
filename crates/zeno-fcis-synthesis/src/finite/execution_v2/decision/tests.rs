use super::super::meter;
use super::super::order_fixture;
use super::*;
use alloc::vec;

fn limits() -> super::super::Limits {
    let mut l = super::super::zero_limits();
    for r in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ] {
        l = l.with_limit(r, u64::MAX);
    }
    l
}
fn empty<'a>() -> Inputs<'a, 'a> {
    Inputs {
        state: &[],
        command: RootView::Record(&[]),
        context: RootView::Record(&[]),
    }
}
fn branch<'a>(
    class: Class,
    reason: Option<u32>,
    assignments: &'a [Assignment<'a>],
    effects: &'a [DeliveryPlan<'a>],
    outbox: &'a [DeliveryPlan<'a>],
) -> Branch<'a> {
    Branch {
        code: 0,
        class,
        reason,
        assignments,
        effects,
        outbox,
    }
}
fn delivery<'a>(ordinal: u32, payload: &'a [PayloadField<'a>]) -> DeliveryPlan<'a> {
    DeliveryPlan {
        ordinal,
        channel: 300,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"carrier")),
        payload,
        idempotency: Expr::Constant(Atom::U128(72)),
    }
}
#[test]
fn empty_classes_and_reason_partition() {
    for class in [Class::Accept, Class::Reject, Class::CommittedFailure] {
        for reason in [None, Some(0), Some(17)] {
            let mut meter = meter::new(limits());
            let mut attempts = vec![Attempt::Write(91, false)];
            let result = construct(
                empty(),
                &[],
                0,
                &[branch(class, reason, &[], &[], &[])],
                &mut meter,
                &mut attempts,
            );
            let valid = matches!(
                (class, reason),
                (Class::Accept, None)
                    | (Class::Reject, Some(17))
                    | (Class::CommittedFailure, Some(17))
            );
            assert_eq!(result.is_ok(), valid);
            if let Ok(c) = result {
                assert_eq!((c.class(), c.reason()), (class, reason));
                assert_eq!(c.pre(), &[]);
                assert_eq!(c.post(), &[]);
                assert_eq!(c.patch(), &[]);
                assert_eq!(c.effects(), &[]);
                assert_eq!(c.outbox(), &[]);
            } else {
                assert!(matches!(result, Err(Failure::Reason)));
            }
            assert_eq!(
                attempts,
                vec![Attempt::Write(91, false), Attempt::Candidate(true)]
            );
            assert_eq!(meter.used.counters, [0, 0, 1, 0, 0, 0, 0, 0]);
        }
    }
}
#[test]
fn independent_inventory_complete_tuple_864_cases() {
    // Independent stock rules; field IDs are spelled directly, no production resolver.
    let mut checked = 0;
    for available in 0..6i128 {
        for reserved in 0..6i128 {
            for action in 0..4 {
                for quantity in 0..3i128 {
                    for authorized in [false, true] {
                        let pre = [
                            Field {
                                id: 110,
                                value: Atom::I128(available),
                            },
                            Field {
                                id: 111,
                                value: Atom::I128(reserved),
                            },
                        ];
                        let cmd = [Field {
                            id: 120,
                            value: Atom::I128(quantity),
                        }];
                        let context = [Field {
                            id: 130,
                            value: Atom::Bool(authorized),
                        }];
                        let sufficient = match action {
                            0 => available >= quantity,
                            1 | 2 => reserved >= quantity,
                            _ => true,
                        };
                        let capacity = match action {
                            0 => reserved + quantity <= 5,
                            1 | 3 => available + quantity <= 5,
                            _ => true,
                        };
                        let accept = authorized && sufficient && capacity;
                        let expected_reason = if !authorized {
                            Some(200)
                        } else if !sufficient {
                            Some(201)
                        } else if !capacity {
                            Some(202)
                        } else {
                            None
                        };
                        let (a, r) = if accept {
                            match action {
                                0 => (available - quantity, reserved + quantity),
                                1 => (available + quantity, reserved - quantity),
                                2 => (available, reserved - quantity),
                                _ => (available + quantity, reserved),
                            }
                        } else {
                            (available, reserved)
                        };
                        let emit = accept && action == 2;
                        let output = [Atom::I128(a), Atom::I128(r), Atom::Bool(emit)];
                        let assignments = [
                            Assignment {
                                field: 110,
                                value: Expr::Output(0),
                                domain: Domain::I128 { min: 0, max: 5 },
                            },
                            Assignment {
                                field: 111,
                                value: Expr::Output(1),
                                domain: Domain::I128 { min: 0, max: 5 },
                            },
                        ];
                        let payload = [PayloadField {
                            field: 140,
                            value: Expr::Input(Source::Command, 120),
                        }];
                        let outbox = [DeliveryPlan {
                            when: Expr::Output(2),
                            ..delivery(0, &payload)
                        }];
                        let branches = [
                            Branch {
                                code: 0,
                                ..branch(Class::Accept, None, &assignments, &[], &outbox)
                            },
                            Branch {
                                code: 1,
                                ..branch(Class::Reject, Some(200), &[], &[], &[])
                            },
                            Branch {
                                code: 2,
                                ..branch(Class::Reject, Some(201), &[], &[], &[])
                            },
                            Branch {
                                code: 3,
                                ..branch(Class::Reject, Some(202), &[], &[], &[])
                            },
                        ];
                        let code = expected_reason.map_or(0, |r| i128::from(r - 199));
                        let mut meter = meter::new(limits());
                        let mut attempts = Vec::new();
                        let c = construct(
                            Inputs {
                                state: &pre,
                                command: RootView::Record(&cmd),
                                context: RootView::Record(&context),
                            },
                            &output,
                            code,
                            &branches,
                            &mut meter,
                            &mut attempts,
                        )
                        .unwrap_or_else(|error| panic!("independent_inventory_complete_tuple_864_cases fixture failed: {error:?}"));
                        assert_eq!(
                            c.class(),
                            if accept { Class::Accept } else { Class::Reject }
                        );
                        assert_eq!(c.reason(), expected_reason);
                        assert_eq!(c.pre(), pre);
                        let expected_post = if accept {
                            vec![
                                Field {
                                    id: 110,
                                    value: Atom::I128(a),
                                },
                                Field {
                                    id: 111,
                                    value: Atom::I128(r),
                                },
                            ]
                        } else {
                            vec![]
                        };
                        assert_eq!(c.post(), expected_post);
                        let mut expected_patch = Vec::new();
                        if accept && a != available {
                            expected_patch.push(Patch {
                                field: 110,
                                before: Atom::I128(available),
                                after: Atom::I128(a),
                            });
                        }
                        if accept && r != reserved {
                            expected_patch.push(Patch {
                                field: 111,
                                before: Atom::I128(reserved),
                                after: Atom::I128(r),
                            });
                        }
                        assert_eq!(c.patch(), expected_patch);
                        assert!(c.effects().is_empty());
                        let expected_outbox = if emit {
                            vec![Delivery {
                                ordinal: 0,
                                channel: 300,
                                destination: Atom::Text(b"carrier"),
                                payload: vec![Field {
                                    id: 140,
                                    value: Atom::I128(quantity),
                                }],
                                idempotency: Atom::U128(72),
                            }]
                        } else {
                            vec![]
                        };
                        assert_eq!(c.outbox(), expected_outbox);
                        let mut expected_attempts = vec![Attempt::Candidate(true)];
                        if accept {
                            expected_attempts
                                .extend([Attempt::Write(110, true), Attempt::Write(111, true)]);
                        }
                        if emit {
                            expected_attempts.push(Attempt::Effect(true, 0, true));
                        }
                        assert_eq!(attempts, expected_attempts);
                        assert_eq!(
                            meter.used.counters,
                            [
                                0,
                                if accept { 2 } else { 0 },
                                1,
                                u64::from(emit),
                                0,
                                0,
                                0,
                                0
                            ]
                        );
                        assert_eq!(
                            pre,
                            [
                                Field {
                                    id: 110,
                                    value: Atom::I128(available)
                                },
                                Field {
                                    id: 111,
                                    value: Atom::I128(reserved)
                                }
                            ]
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 864);
}
#[test]
fn full_width_types_committed_failure_and_passthrough() {
    let raw = [0, 255, 42];
    let variants = [0, u16::MAX];
    let values = [
        Atom::I128(i128::MIN),
        Atom::I128(i128::MAX),
        Atom::U128(u128::MAX),
        Atom::Bool(false),
        Atom::Enum {
            type_id: 0,
            variant: 0,
        },
        Atom::Sum {
            type_id: u32::MAX,
            variant: u16::MAX,
        },
        Atom::Bytes(&raw),
        Atom::Text(b"hello"),
    ];
    let domains = [
        Domain::I128 {
            min: i128::MIN,
            max: i128::MAX,
        },
        Domain::I128 {
            min: i128::MIN,
            max: i128::MAX,
        },
        Domain::U128 {
            min: 0,
            max: u128::MAX,
        },
        Domain::Bool,
        Domain::Enum {
            type_id: 0,
            variants: &variants,
        },
        Domain::Sum {
            type_id: u32::MAX,
            variants: &variants,
        },
        Domain::Bytes,
        Domain::Text,
    ];
    let pre: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, &value)| Field {
            id: i as u16,
            value,
        })
        .collect();
    let assignments: Vec<_> = domains
        .iter()
        .enumerate()
        .map(|(i, &domain)| Assignment {
            field: i as u16,
            value: Expr::Input(Source::State, i as u16),
            domain,
        })
        .collect();
    let payload = [
        PayloadField {
            field: 0,
            value: Expr::Input(Source::State, 2),
        },
        PayloadField {
            field: 65535,
            value: Expr::Input(Source::State, 6),
        },
    ];
    let effects = [delivery(3, &payload), delivery(9, &payload)];
    let outbox = [delivery(2, &payload), delivery(7, &payload)];
    let branches = [branch(
        Class::CommittedFailure,
        Some(204),
        &assignments,
        &effects,
        &outbox,
    )];
    let mut meter = meter::new(limits());
    let mut attempts = Vec::new();
    let c = construct(
        Inputs {
            state: &pre,
            command: RootView::Record(&[]),
            context: RootView::Record(&[]),
        },
        &[],
        0,
        &branches,
        &mut meter,
        &mut attempts,
    )
    .unwrap_or_else(|error| {
        panic!("full_width_types_committed_failure_and_passthrough fixture failed: {error:?}")
    });
    assert_eq!(
        (c.class(), c.reason()),
        (Class::CommittedFailure, Some(204))
    );
    assert_eq!(c.pre(), pre);
    assert_eq!(c.post(), pre);
    assert!(c.patch().is_empty());
    for (observed, ordinals) in [(c.effects(), [3, 9]), (c.outbox(), [2, 7])] {
        assert_eq!(observed.len(), 2);
        for (d, ordinal) in observed.iter().zip(ordinals) {
            assert_eq!(
                d,
                &Delivery {
                    ordinal,
                    channel: 300,
                    destination: Atom::Text(b"carrier"),
                    payload: vec![
                        Field {
                            id: 0,
                            value: Atom::U128(u128::MAX)
                        },
                        Field {
                            id: 65535,
                            value: Atom::Bytes(&raw)
                        }
                    ],
                    idempotency: Atom::U128(72)
                }
            );
        }
    }
    assert_eq!(meter.used.counters, [0, 8, 1, 4, 0, 0, 0, 0]);
}
#[test]
fn refusal_retains_private_counters_prefix_and_discards_partial_candidate() {
    let pre = [
        Field {
            id: 0,
            value: Atom::I128(0),
        },
        Field {
            id: 1,
            value: Atom::I128(0),
        },
    ];
    let assignments = [
        Assignment {
            field: 0,
            value: Expr::Constant(Atom::I128(1)),
            domain: Domain::I128 { min: 0, max: 1 },
        },
        Assignment {
            field: 1,
            value: Expr::Output(8),
            domain: Domain::I128 { min: 0, max: 1 },
        },
    ];
    let branches = [branch(Class::Accept, None, &assignments, &[], &[])];
    for write_limit in 0..=2 {
        let mut meter = meter::new(limits().with_limit(Resource::Write, write_limit));
        meter.used.counters = [3, 0, 7, 11, 13, 17, 19, 23];
        let mut attempts = vec![Attempt::Effect(false, 7, false)];
        let r = construct(
            Inputs {
                state: &pre,
                command: RootView::Record(&[]),
                context: RootView::Record(&[]),
            },
            &[],
            0,
            &branches,
            &mut meter,
            &mut attempts,
        );
        if write_limit < 2 {
            assert!(matches!(
                r,
                Err(Failure::Budget(MeterFailure {
                    resource: Resource::Write,
                    ..
                }))
            ));
        } else {
            assert!(matches!(r, Err(Failure::Reference)));
        }
        assert_eq!(meter.used.counters, [3, write_limit, 8, 11, 13, 17, 19, 23]);
        assert_eq!(attempts[0], Attempt::Effect(false, 7, false));
        assert_eq!(attempts[1], Attempt::Candidate(true));
        assert_eq!(attempts[2], Attempt::Write(0, write_limit > 0));
        if write_limit > 0 {
            assert_eq!(attempts[3], Attempt::Write(1, write_limit > 1));
        }
        assert_eq!(pre[0].value, Atom::I128(0));
    }
    let mut meter = meter::new(limits());
    meter.used.counters = [u64::MAX; 8];
    let mut attempts = Vec::new();
    assert!(matches!(
        construct(empty(), &[], 0, &[], &mut meter, &mut attempts),
        Err(Failure::Budget(MeterFailure {
            resource: Resource::Candidate,
            overflow: true,
            ..
        }))
    ));
    assert_eq!(meter.used.counters, [u64::MAX; 8]);
    assert_eq!(attempts, vec![Attempt::Candidate(false)]);
}
#[test]
fn malformed_plans_order_and_no_op_reject() {
    let pre = [Field {
        id: 0,
        value: Atom::Bool(false),
    }];
    let inputs = Inputs {
        state: &pre,
        command: RootView::Record(&[]),
        context: RootView::Record(&[]),
    };
    let assignment = [Assignment {
        field: 0,
        value: Expr::Constant(Atom::Bool(false)),
        domain: Domain::Bool,
    }];
    let malformed = [
        (
            branch(Class::Accept, None, &[], &[], &[]),
            Failure::Successor,
        ),
        (
            branch(Class::Reject, Some(1), &assignment, &[], &[]),
            Failure::RejectPlan,
        ),
    ];
    for (b, error) in malformed {
        let mut m = meter::new(limits());
        let mut a = Vec::new();
        assert_eq!(
            construct(inputs, &[], 0, &[b], &mut m, &mut a).err(),
            Some(error)
        );
    }
    let mut m = meter::new(limits());
    let mut a = Vec::new();
    let c = construct(
        inputs,
        &[],
        0,
        &[branch(Class::Accept, None, &assignment, &[], &[])],
        &mut m,
        &mut a,
    )
    .unwrap_or_else(|error| {
        panic!("malformed_plans_order_and_no_op_reject fixture failed: {error:?}")
    });
    assert_eq!(c.post(), pre);
    assert!(c.patch().is_empty());
    assert_eq!(m.used.used(Resource::Write), 1);
    let mut m = meter::new(limits());
    let mut a = Vec::new();
    let c = construct(
        inputs,
        &[],
        0,
        &[branch(Class::Reject, Some(1), &[], &[], &[])],
        &mut m,
        &mut a,
    )
    .unwrap_or_else(|error| {
        panic!("malformed_plans_order_and_no_op_reject fixture failed: {error:?}")
    });
    assert!(c.post().is_empty());
    assert_eq!(m.used.used(Resource::Write), 0);
    let bad_inputs = [
        Field {
            id: 1,
            value: Atom::Bool(false),
        },
        Field {
            id: 0,
            value: Atom::Bool(true),
        },
    ];
    assert_eq!(
        construct(
            Inputs {
                state: &bad_inputs,
                ..empty()
            },
            &[],
            0,
            &[],
            &mut m,
            &mut a
        )
        .err(),
        Some(Failure::InputOrder)
    );
    for branches in [
        vec![
            Branch {
                code: 1,
                ..branch(Class::Accept, None, &[], &[], &[])
            },
            Branch {
                code: 0,
                ..branch(Class::Accept, None, &[], &[], &[])
            },
        ],
        vec![
            branch(Class::Accept, None, &[], &[], &[]),
            branch(Class::Accept, None, &[], &[], &[]),
        ],
        vec![],
    ] {
        assert_eq!(
            construct(empty(), &[], 0, &branches, &mut m, &mut a).err(),
            Some(Failure::Branch)
        );
    }
}
#[test]
fn closed_domains_sources_deliveries_and_late_refusal() {
    let pre = [Field {
        id: 0,
        value: Atom::I128(0),
    }];
    for (value, domain, valid) in [
        (Atom::I128(2), Domain::I128 { min: 0, max: 1 }, false),
        (Atom::I128(-1), Domain::I128 { min: 0, max: 1 }, false),
        (Atom::I128(1), Domain::I128 { min: 0, max: 1 }, true),
        (Atom::Bool(true), Domain::I128 { min: 0, max: 1 }, false),
        (Atom::I128(0), Domain::I128 { min: 1, max: 0 }, false),
    ] {
        let assignments = [Assignment {
            field: 0,
            value: Expr::Constant(value),
            domain,
        }];
        let branches = [branch(Class::Accept, None, &assignments, &[], &[])];
        let mut m = meter::new(limits());
        let mut a = Vec::new();
        assert_eq!(
            construct(
                Inputs {
                    state: &pre,
                    ..empty()
                },
                &[],
                0,
                &branches,
                &mut m,
                &mut a
            )
            .is_ok(),
            valid
        );
        assert_eq!(m.used.counters, [0, 1, 1, 0, 0, 0, 0, 0]);
    }
    let command = [Field {
        id: 5,
        value: Atom::I128(10),
    }];
    let context = [Field {
        id: 5,
        value: Atom::I128(11),
    }];
    let input = Inputs {
        state: &pre,
        command: RootView::Record(&command),
        context: RootView::Record(&context),
    };
    assert_eq!(
        resolve(input, &[], Expr::Input(Source::State, 0)),
        Some(Atom::I128(0))
    );
    assert_eq!(
        resolve(input, &[], Expr::Input(Source::Command, 5)),
        Some(Atom::I128(10))
    );
    assert_eq!(
        resolve(input, &[], Expr::Input(Source::Context, 5)),
        Some(Atom::I128(11))
    );
    assert_eq!(resolve(input, &[], Expr::Input(Source::Context, 99)), None);
    let variants = [0, 65535];
    assert!(admitted(
        Domain::Enum {
            type_id: 0,
            variants: &variants
        },
        Atom::Enum {
            type_id: 0,
            variant: 0
        }
    ));
    assert!(!admitted(
        Domain::Enum {
            type_id: 0,
            variants: &variants
        },
        Atom::Sum {
            type_id: 0,
            variant: 0
        }
    ));
    assert!(!admitted(
        Domain::Sum {
            type_id: 1,
            variants: &variants
        },
        Atom::Sum {
            type_id: 1,
            variant: 1
        }
    ));
    assert!(!admitted(Domain::U128 { min: 0, max: 10 }, Atom::U128(11)));
    for right in [
        Atom::I128(0),
        Atom::U128(0),
        Atom::Bool(false),
        Atom::Bytes(b"x"),
        Atom::Text(b"x"),
    ] {
        for left in [
            Atom::I128(0),
            Atom::U128(0),
            Atom::Bool(false),
            Atom::Bytes(b"x"),
            Atom::Text(b"x"),
        ] {
            assert_eq!(equal(left, right), left == right);
        }
    }
    let malformed_payload = [
        PayloadField {
            field: 1,
            value: Expr::Constant(Atom::Bool(true)),
        },
        PayloadField {
            field: 0,
            value: Expr::Constant(Atom::Bool(false)),
        },
    ];
    let valid = delivery(0, &[]);
    let invalid = delivery(1, &malformed_payload);
    let deliveries = [valid, invalid];
    let mut m = meter::new(limits());
    m.used.counters = [1, 2, 3, 4, 5, 6, 7, 8];
    let mut a = vec![Attempt::Candidate(false)];
    let r = construct(
        empty(),
        &[],
        0,
        &[branch(
            Class::CommittedFailure,
            Some(204),
            &[],
            &deliveries,
            &[],
        )],
        &mut m,
        &mut a,
    );
    assert_eq!(r.err(), Some(Failure::Delivery));
    assert_eq!(m.used.counters, [1, 2, 4, 6, 5, 6, 7, 8]);
    assert_eq!(
        a,
        vec![
            Attempt::Candidate(false),
            Attempt::Candidate(true),
            Attempt::Effect(false, 0, true),
            Attempt::Effect(false, 1, true)
        ]
    );
    let mut m = meter::new(limits().with_limit(Resource::Effect, 1));
    let mut a = Vec::new();
    let single_delivery = [valid];
    let r = construct(
        empty(),
        &[],
        0,
        &[branch(
            Class::Accept,
            None,
            &[],
            &single_delivery,
            &single_delivery,
        )],
        &mut m,
        &mut a,
    );
    assert!(matches!(
        r,
        Err(Failure::Budget(MeterFailure {
            resource: Resource::Effect,
            attempted: 2,
            ..
        }))
    ));
    assert_eq!(m.used.counters, [0, 0, 1, 1, 0, 0, 0, 0]);
    assert_eq!(
        a,
        vec![
            Attempt::Candidate(true),
            Attempt::Effect(false, 0, true),
            Attempt::Effect(true, 0, false)
        ]
    );
    for plan in [
        vec![valid, valid],
        vec![DeliveryPlan {
            channel: 0,
            ..valid
        }],
        vec![DeliveryPlan {
            when: Expr::Constant(Atom::I128(1)),
            ..valid
        }],
    ] {
        let mut m = meter::new(limits());
        let mut a = Vec::new();
        assert!(
            construct(
                empty(),
                &[],
                0,
                &[branch(Class::Accept, None, &[], &plan, &[])],
                &mut m,
                &mut a
            )
            .is_err()
        );
    }
}

// This emitted program is only the test subject producing fixture outputs.
// Expected complete decisions below are independently restated from domain rules.

#[test]
fn independent_order_full_schema_product_1728_typed_decisions() {
    let status_variants = [160, 161, 162, 163, 164, 165];
    let mut checked = 0;
    let mut unlawful_pre = 0;
    for status in 160u16..=165 {
        for attempts in 0i128..=3 {
            for action in 150u16..=155 {
                for callback in 0i128..=3 {
                    for caller in 170u16..=172 {
                        // No law500 precondition: all 24 schema-admitted pre-states are examined.
                        if (161..=164).contains(&status) && attempts == 0 {
                            unlawful_pre += 1;
                        }
                        let required = match action {
                            150 | 155 => 170,
                            151 | 152 => 171,
                            _ => 172,
                        };
                        let (class, reason, next_status, next_attempts, request) = if caller
                            != required
                        {
                            (Class::Reject, Some(200), status, attempts, None)
                        } else {
                            match (action, status) {
                                (151 | 152, 161) if callback != attempts => {
                                    (Class::Reject, Some(202), status, attempts, None)
                                }
                                (150, 160) if attempts == 3 => {
                                    (Class::Reject, Some(203), status, attempts, None)
                                }
                                (150, 160) => (
                                    Class::Accept,
                                    None,
                                    161,
                                    attempts + 1,
                                    Some((300, attempts + 1, 175)),
                                ),
                                (151, 161) => {
                                    (Class::Accept, None, 162, attempts, Some((301, attempts, 0)))
                                }
                                (152, 161) => {
                                    (Class::CommittedFailure, Some(204), 160, attempts, None)
                                }
                                (153, 162) => (Class::Accept, None, 163, attempts, None),
                                (154, 163) => (Class::Accept, None, 164, attempts, None),
                                (155, 160) => (Class::Accept, None, 165, attempts, None),
                                (155, 161) => (
                                    Class::Accept,
                                    None,
                                    165,
                                    attempts,
                                    Some((300, attempts, 176)),
                                ),
                                _ => (Class::Reject, Some(201), status, attempts, None),
                            }
                        };
                        let fixture = order_fixture::transition(&[
                            i64::from(action - 150),
                            i64::from(status - 160),
                            attempts as i64,
                            callback as i64,
                            i64::from(caller - 170),
                        ])
                        .unwrap_or_else(|| panic!("missing fixture value in independent_order_full_schema_product_1728_typed_decisions"));
                        let pre = [
                            Field {
                                id: 120,
                                value: Atom::Enum {
                                    type_id: 105,
                                    variant: status,
                                },
                            },
                            Field {
                                id: 121,
                                value: Atom::I128(attempts),
                            },
                        ];
                        let command = [
                            Field {
                                id: 125,
                                value: Atom::Sum {
                                    type_id: 111,
                                    variant: action,
                                },
                            },
                            Field {
                                id: 126,
                                value: Atom::I128(callback),
                            },
                        ];
                        let context = [Field {
                            id: 130,
                            value: Atom::Enum {
                                type_id: 107,
                                variant: caller,
                            },
                        }];
                        let output = [
                            Atom::Enum {
                                type_id: 105,
                                variant: u16::try_from(fixture[1]).unwrap_or_else(|error| panic!("independent_order_full_schema_product_1728_typed_decisions fixture failed: {error:?}")) + 160,
                            },
                            Atom::I128(i128::from(fixture[2])),
                        ];
                        let assignments = [
                            Assignment {
                                field: 120,
                                value: Expr::Output(0),
                                domain: Domain::Enum {
                                    type_id: 105,
                                    variants: &status_variants,
                                },
                            },
                            Assignment {
                                field: 121,
                                value: Expr::Output(1),
                                domain: Domain::I128 { min: 0, max: 3 },
                            },
                        ];
                        let capture_payload = [
                            PayloadField {
                                field: 140,
                                value: Expr::Output(1),
                            },
                            PayloadField {
                                field: 141,
                                value: Expr::Constant(Atom::Enum {
                                    type_id: 108,
                                    variant: 175,
                                }),
                            },
                        ];
                        let void_payload = [
                            PayloadField {
                                field: 140,
                                value: Expr::Output(1),
                            },
                            PayloadField {
                                field: 141,
                                value: Expr::Constant(Atom::Enum {
                                    type_id: 108,
                                    variant: 176,
                                }),
                            },
                        ];
                        let ship_payload = [PayloadField {
                            field: 145,
                            value: Expr::Output(1),
                        }];
                        let payment = DeliveryPlan {
                            ordinal: 0,
                            channel: 300,
                            when: Expr::Constant(Atom::Bool(true)),
                            destination: Expr::Constant(Atom::Text(b"payment-provider")),
                            payload: &capture_payload,
                            idempotency: Expr::Output(1),
                        };
                        let capture = [payment];
                        let void = [DeliveryPlan {
                            payload: &void_payload,
                            ..payment
                        }];
                        let shipping = [DeliveryPlan {
                            channel: 301,
                            destination: Expr::Constant(Atom::Text(b"carrier")),
                            payload: &ship_payload,
                            ..payment
                        }];
                        let branches = [
                            Branch {
                                code: 0,
                                ..branch(Class::Reject, Some(200), &[], &[], &[])
                            },
                            Branch {
                                code: 1,
                                ..branch(Class::Reject, Some(201), &[], &[], &[])
                            },
                            Branch {
                                code: 2,
                                ..branch(Class::Reject, Some(202), &[], &[], &[])
                            },
                            Branch {
                                code: 3,
                                ..branch(Class::Reject, Some(203), &[], &[], &[])
                            },
                            Branch {
                                code: 4,
                                ..branch(Class::Accept, None, &assignments, &[], &capture)
                            },
                            Branch {
                                code: 5,
                                ..branch(Class::Accept, None, &assignments, &[], &shipping)
                            },
                            Branch {
                                code: 6,
                                ..branch(Class::CommittedFailure, Some(204), &assignments, &[], &[])
                            },
                            Branch {
                                code: 7,
                                ..branch(Class::Accept, None, &assignments, &[], &[])
                            },
                            Branch {
                                code: 8,
                                ..branch(Class::Accept, None, &assignments, &[], &[])
                            },
                            Branch {
                                code: 9,
                                ..branch(Class::Accept, None, &assignments, &[], &[])
                            },
                            Branch {
                                code: 10,
                                ..branch(Class::Accept, None, &assignments, &[], &void)
                            },
                        ];
                        let mut meter = meter::new(limits());
                        let mut observations = Vec::new();
                        let c = construct(
                            Inputs {
                                state: &pre,
                                command: RootView::Record(&command),
                                context: RootView::Record(&context),
                            },
                            &output,
                            i128::from(fixture[0]),
                            &branches,
                            &mut meter,
                            &mut observations,
                        )
                        .unwrap_or_else(|error| panic!("independent_order_full_schema_product_1728_typed_decisions fixture failed: {error:?}"));
                        assert_eq!((c.class(), c.reason()), (class, reason));
                        assert_eq!(c.pre(), pre);
                        assert!(c.effects().is_empty());
                        let commits = class != Class::Reject;
                        let expected_post = if commits {
                            vec![
                                Field {
                                    id: 120,
                                    value: Atom::Enum {
                                        type_id: 105,
                                        variant: next_status,
                                    },
                                },
                                Field {
                                    id: 121,
                                    value: Atom::I128(next_attempts),
                                },
                            ]
                        } else {
                            vec![]
                        };
                        assert_eq!(c.post(), expected_post);
                        let mut expected_patch = Vec::new();
                        if commits && next_status != status {
                            expected_patch.push(Patch {
                                field: 120,
                                before: Atom::Enum {
                                    type_id: 105,
                                    variant: status,
                                },
                                after: Atom::Enum {
                                    type_id: 105,
                                    variant: next_status,
                                },
                            });
                        }
                        if commits && next_attempts != attempts {
                            expected_patch.push(Patch {
                                field: 121,
                                before: Atom::I128(attempts),
                                after: Atom::I128(next_attempts),
                            });
                        }
                        assert_eq!(c.patch(), expected_patch);
                        let expected_outbox = match request {
                            None => vec![],
                            Some((channel, number, operation)) => vec![Delivery {
                                ordinal: 0,
                                channel,
                                destination: Atom::Text(if channel == 300 {
                                    b"payment-provider"
                                } else {
                                    b"carrier"
                                }),
                                payload: if channel == 300 {
                                    vec![
                                        Field {
                                            id: 140,
                                            value: Atom::I128(number),
                                        },
                                        Field {
                                            id: 141,
                                            value: Atom::Enum {
                                                type_id: 108,
                                                variant: operation,
                                            },
                                        },
                                    ]
                                } else {
                                    vec![Field {
                                        id: 145,
                                        value: Atom::I128(number),
                                    }]
                                },
                                idempotency: Atom::I128(number),
                            }],
                        };
                        assert_eq!(c.outbox(), expected_outbox);
                        let mut expected_observations = vec![Attempt::Candidate(true)];
                        if commits {
                            expected_observations
                                .extend([Attempt::Write(120, true), Attempt::Write(121, true)]);
                        }
                        if request.is_some() {
                            expected_observations.push(Attempt::Effect(true, 0, true));
                        }
                        assert_eq!(observations, expected_observations);
                        assert_eq!(
                            meter.used.counters,
                            [
                                0,
                                if commits { 2 } else { 0 },
                                1,
                                u64::from(request.is_some()),
                                0,
                                0,
                                0,
                                0
                            ]
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 1728);
    assert_eq!(unlawful_pre, 288);
}
