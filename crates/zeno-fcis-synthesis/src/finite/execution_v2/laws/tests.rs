use super::*;
use alloc::vec;

fn limits() -> Limits {
    super::super::zero_limits()
        .with_limit(Resource::Step, u64::MAX)
        .with_limit(Resource::Read, u64::MAX)
}
fn usage() -> Usage {
    super::super::execute(&[], &[], &[], &[], &[], limits()).usage()
}
fn base<'a>(truth: &'a [Op<'a>]) -> Vec<Law<'a>> {
    vec![
        Law {
            id: 10,
            kind: Kind::StateInvariant,
            scope: Scope::Committing,
            genesis: true,
            program: Program {
                nodes: truth,
                root: 0,
            },
        },
        Law {
            id: 20,
            kind: Kind::RejectNoAuthority,
            scope: Scope::Reject,
            genesis: false,
            program: Program {
                nodes: truth,
                root: 0,
            },
        },
        Law {
            id: 30,
            kind: Kind::CommittedFailureEffects,
            scope: Scope::CommittedFailure,
            genesis: false,
            program: Program {
                nodes: truth,
                root: 0,
            },
        },
        Law {
            id: 40,
            kind: Kind::DecisionConformance,
            scope: Scope::Always,
            genesis: false,
            program: Program {
                nodes: truth,
                root: 0,
            },
        },
        Law {
            id: 50,
            kind: Kind::InitialCondition,
            scope: Scope::Always,
            genesis: true,
            program: Program {
                nodes: truth,
                root: 0,
            },
        },
    ]
}
fn candidate<'a>(class: Class, post: &'a [Field<'a>]) -> Candidate<'a> {
    Candidate {
        class,
        reason: match class {
            Class::Accept => None,
            _ => Some(200),
        },
        post: RootView::Record(post),
        patch: &[],
        effects: &[],
        outbox: &[],
        reads: &[],
        attempts: &[],
        usage: usage(),
    }
}
fn transition<'a>(pre: &'a [Field<'a>], candidate: &'a Candidate<'a>) -> Frame<'a> {
    Frame::Transition {
        pre: RootView::Record(pre),
        command: RootView::Record(&[]),
        context: RootView::Record(&[]),
        candidate,
    }
}

#[test]
fn exact_scope_table_for_every_ordinary_kind_and_all_phases() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let falsehood = [Op::Literal(Atom::Bool(false))];
    let ordinary = [
        Kind::AssetConservation,
        Kind::MintBurnAuthorization,
        Kind::DebitCreditEffectEquality,
        Kind::FeeAndRounding,
        Kind::AuthoritySubjectRecipient,
    ];
    let scopes = [
        Scope::Always,
        Scope::Accept,
        Scope::Reject,
        Scope::CommittedFailure,
        Scope::Committing,
    ];
    // Independent declared truth table: rows are scope, columns A/R/F/genesis.
    let table = [
        [true, true, true],
        [true, false, false],
        [false, true, false],
        [false, false, true],
        [true, false, true],
    ];
    for kind in ordinary {
        for (s, scope) in scopes.iter().enumerate() {
            for genesis in [false, true] {
                let mut laws = base(&truth);
                laws.push(Law {
                    id: 60,
                    kind,
                    scope: *scope,
                    genesis,
                    program: Program {
                        nodes: &falsehood,
                        root: 0,
                    },
                });
                let candidates = [
                    candidate(Class::Accept, &[]),
                    candidate(Class::Reject, &[]),
                    candidate(Class::CommittedFailure, &[]),
                ];
                let frames = [
                    transition(&[], &candidates[0]),
                    transition(&[], &candidates[1]),
                    transition(&[], &candidates[2]),
                    Frame::Genesis {
                        initial: RootView::Record(&[]),
                    },
                ];
                for (phase, frame) in frames.iter().enumerate() {
                    let expected = if phase == 3 { genesis } else { table[s][phase] };
                    let (result, used, diagnostics, _) =
                        evaluate(&laws, &[10, 20, 30, 40, 50, 60], frame, limits()).into_parts();
                    assert_eq!(
                        result,
                        if expected {
                            Err(Failure::Violated)
                        } else {
                            Ok(())
                        }
                    );
                    assert_eq!(
                        diagnostics[5].verdict,
                        if expected {
                            Verdict::Refused(Failure::Violated)
                        } else {
                            Verdict::Skipped
                        }
                    );
                    let baseline = [2, 2, 3, 2][phase];
                    assert_eq!(used.used(Resource::Step), baseline + u64::from(expected));
                }
            }
        }
    }
}
#[test]
fn mandatory_families_ids_and_even_skipped_malformed_programs_refuse() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let frame = Frame::Genesis {
        initial: RootView::Record(&[]),
    };
    for omit in 0..5 {
        let mut laws = base(&truth);
        laws.remove(omit);
        assert_eq!(
            evaluate(&laws, &[], &frame, limits()).into_parts().0,
            Err(Failure::Metadata)
        );
    }
    let mut laws = base(&truth);
    laws[1].id = 10;
    assert_eq!(
        evaluate(&laws, &[], &frame, limits()).into_parts().0,
        Err(Failure::Metadata)
    );
    laws = base(&truth);
    for required in [&[999][..], &[10, 10][..], &[0][..]] {
        assert_eq!(
            evaluate(&laws, required, &frame, limits()).into_parts().0,
            Err(Failure::Metadata)
        );
    }
    for i in 0..5 {
        let mut laws = base(&truth);
        laws[i].genesis = !laws[i].genesis;
        assert_eq!(
            evaluate(&laws, &[], &frame, limits()).into_parts().0,
            Err(Failure::Metadata)
        );
    }
    let malformed = [Op::Not(0)];
    let mut laws = base(&truth);
    laws[1].program = Program {
        nodes: &malformed,
        root: 0,
    };
    let (result, used, diagnostics, reads) = evaluate(&laws, &[], &frame, limits()).into_parts();
    assert_eq!(result, Err(Failure::Metadata));
    assert_eq!(used.used(Resource::Step), 0);
    assert!(diagnostics.is_empty() && reads.is_empty());
}
#[test]
fn actual_initial_state_and_no_genesis_fixture() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let condition = [
        Op::Observe(Observation::Initial(7)),
        Op::Literal(Atom::I128(0)),
        Op::Eq(0, 1),
    ];
    for value in [i128::MIN, -1, 0, 1, i128::MAX] {
        let initial = [Field {
            id: 7,
            value: Atom::I128(value),
        }];
        let frame = Frame::Genesis {
            initial: RootView::Record(&initial),
        };
        let mut laws = base(&truth);
        laws[4].program = Program {
            nodes: &condition,
            root: 2,
        };
        let (result, used, _, reads) = evaluate(&laws, &[], &frame, limits()).into_parts();
        assert_eq!(
            result,
            if value == 0 {
                Ok(())
            } else {
                Err(Failure::Violated)
            }
        );
        assert_eq!(used.used(Resource::Step), 4);
        assert_eq!(used.used(Resource::Read), 1);
        assert_eq!(reads[0].observation, Observation::Initial(7));
    }
    for unavailable in [
        Observation::Pre(7),
        Observation::Command(7),
        Observation::Context(7),
        Observation::Class,
        Observation::Reason,
    ] {
        let program = [Op::Observe(unavailable)];
        let mut laws = base(&truth);
        laws[4].program = Program {
            nodes: &program,
            root: 0,
        };
        assert_eq!(
            evaluate(
                &laws,
                &[],
                &Frame::Genesis {
                    initial: RootView::Record(&[])
                },
                limits()
            )
            .into_parts()
            .0,
            Err(Failure::Undefined)
        );
    }
}
#[test]
fn independent_stock_conservation_corpus_checks_actual_pre_and_post() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let program = [
        Op::Observe(Observation::Pre(1)),
        Op::Observe(Observation::Pre(2)),
        Op::Add(0, 1),
        Op::Observe(Observation::Post(1)),
        Op::Observe(Observation::Post(2)),
        Op::Add(3, 4),
        Op::Eq(2, 5),
    ];
    let mut laws = base(&truth);
    laws.push(Law {
        id: 60,
        kind: Kind::AssetConservation,
        scope: Scope::Committing,
        genesis: false,
        program: Program {
            nodes: &program,
            root: 6,
        },
    });
    for a in 0..=5 {
        for b in 0..=5 {
            for c in 0..=5 {
                for d in 0..=5 {
                    let pre = [
                        Field {
                            id: 1,
                            value: Atom::I128(a),
                        },
                        Field {
                            id: 2,
                            value: Atom::I128(b),
                        },
                    ];
                    let post = [
                        Field {
                            id: 1,
                            value: Atom::I128(c),
                        },
                        Field {
                            id: 2,
                            value: Atom::I128(d),
                        },
                    ];
                    for class in [Class::Accept, Class::CommittedFailure] {
                        let candidate = candidate(class, &post);
                        let frame = transition(&pre, &candidate);
                        let result = evaluate(&laws, &[], &frame, limits()).into_parts().0;
                        assert_eq!(
                            result,
                            if a + b == c + d {
                                Ok(())
                            } else {
                                Err(Failure::Violated)
                            }
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn typed_equality_wide_arithmetic_and_eager_undefined_nodes() {
    assert!(!atoms::equal(Atom::Bool(true), Atom::I128(1)));
    assert!(!atoms::equal(Atom::I128(1), Atom::U128(1)));
    assert!(!atoms::equal(
        Atom::Enum {
            type_id: 1,
            variant: 2
        },
        Atom::Sum {
            type_id: 1,
            variant: 2
        }
    ));
    assert!(!atoms::equal(Atom::Bytes(b"same"), Atom::Text(b"same")));
    assert!(atoms::equal(Atom::Bytes(b"same"), Atom::Bytes(b"same")));
    assert!(!atoms::equal(Atom::Bytes(b"same"), Atom::Bytes(b"samf")));
    let truth = [Op::Literal(Atom::Bool(true))];
    let bad = [
        Op::Literal(Atom::Bool(true)),
        Op::Literal(Atom::I128(i128::MAX)),
        Op::Literal(Atom::I128(1)),
        Op::Add(1, 2),
    ];
    let mut laws = base(&truth);
    laws[4].program = Program {
        nodes: &bad,
        root: 0,
    };
    assert_eq!(
        evaluate(
            &laws,
            &[],
            &Frame::Genesis {
                initial: RootView::Record(&[])
            },
            limits()
        )
        .into_parts()
        .0,
        Err(Failure::Undefined)
    );
    assert!(matches!(
        atoms::binary(0, Atom::I128(i128::MIN), Atom::I128(i128::MAX)),
        Ok(Atom::I128(-1))
    ));
    assert!(matches!(
        atoms::binary(0, Atom::U128(u128::MAX), Atom::U128(1)),
        Err(Failure::Undefined)
    ));
    assert!(matches!(
        atoms::binary(1, Atom::U128(0), Atom::U128(1)),
        Err(Failure::Undefined)
    ));
}
#[test]
fn shared_meter_prefixes_exhaustion_and_overflow_are_preserved() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let program = [
        Op::Observe(Observation::Initial(7)),
        Op::Literal(Atom::I128(0)),
        Op::Eq(0, 1),
    ];
    let mut laws = base(&truth);
    laws[4].program = Program {
        nodes: &program,
        root: 2,
    };
    let initial = [Field {
        id: 7,
        value: Atom::I128(0),
    }];
    let frame = Frame::Genesis {
        initial: RootView::Record(&initial),
    };
    for step in 0..=5 {
        for read in 0..=2 {
            let (result, used, _, reads) = evaluate(
                &laws,
                &[],
                &frame,
                super::super::zero_limits()
                    .with_limit(Resource::Step, step)
                    .with_limit(Resource::Read, read),
            )
            .into_parts();
            assert_eq!(result.is_ok(), step >= 4 && read >= 1);
            assert_eq!(
                used.used(Resource::Step),
                if read == 0 { step.min(2) } else { step.min(4) }
            );
            assert_eq!(reads.len(), usize::from(step >= 2));
            if !reads.is_empty() {
                assert_eq!(reads[0].permitted, read > 0);
            }
        }
    }
    let mut meter = meter::new(limits());
    meter.used.counters = [3, 5, 7, 11, 13, 17, 19, 23];
    let old = Diagnostic {
        id: 999,
        verdict: Verdict::Skipped,
    };
    let mut diagnostics = vec![old];
    let mut reads = Vec::new();
    assert_eq!(
        evaluate_into(&laws, &[], &frame, &mut meter, &mut diagnostics, &mut reads),
        Ok(())
    );
    assert_eq!(meter.used.counters, [4, 5, 7, 11, 13, 17, 19, 27]);
    assert_eq!(diagnostics[0], old);
    meter.used.counters[7] = u64::MAX;
    assert!(
        matches!(evaluate_into(&laws,&[],&frame,&mut meter,&mut diagnostics,&mut reads),Err(Failure::Budget(e)) if e.overflow)
    );
    assert_eq!(meter.used.counters, [4, 5, 7, 11, 13, 17, 19, u64::MAX]);
}
#[test]
fn class_reason_and_order_are_observed() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let check = [
        Op::Observe(Observation::Reason),
        Op::Literal(Atom::I128(200)),
        Op::Eq(0, 1),
    ];
    let mut laws = base(&truth);
    laws[1].program = Program {
        nodes: &check,
        root: 2,
    };
    let mut candidate = candidate(Class::Reject, &[]);
    let frame = transition(&[], &candidate);
    assert_eq!(
        evaluate(&laws, &[], &frame, limits()).into_parts().0,
        Ok(())
    );
    candidate.reason = Some(201);
    let frame = transition(&[], &candidate);
    let (result, _, diagnostics, _) = evaluate(&laws, &[], &frame, limits()).into_parts();
    assert_eq!(result, Err(Failure::Violated));
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[1].id, 20);
    laws.swap(1, 3);
    let (_, _, diagnostics, _) = evaluate(&laws, &[], &frame, limits()).into_parts();
    assert_eq!(diagnostics.len(), 4);
    assert_eq!(diagnostics[3].id, 20);
}
#[test]
fn no_law_count_cap_and_structural_frame_refusals() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let mut laws = base(&truth);
    for id in 100..5200 {
        laws.push(Law {
            id,
            kind: Kind::FeeAndRounding,
            scope: Scope::Always,
            genesis: true,
            program: Program {
                nodes: &truth,
                root: 0,
            },
        });
    }
    let result = evaluate(
        &laws,
        &[],
        &Frame::Genesis {
            initial: RootView::Record(&[]),
        },
        limits(),
    )
    .into_parts();
    assert_eq!(result.0, Ok(()));
    assert_eq!(result.2.len(), 5105);
    let duplicate = [
        Field {
            id: 7,
            value: Atom::I128(0),
        },
        Field {
            id: 7,
            value: Atom::I128(0),
        },
    ];
    assert_eq!(
        evaluate(
            &base(&truth),
            &[],
            &Frame::Genesis {
                initial: RootView::Record(&duplicate)
            },
            limits()
        )
        .into_parts()
        .0,
        Err(Failure::Frame)
    );
    let mut candidate = candidate(Class::Reject, &duplicate);
    let frame = transition(&[], &candidate);
    assert_eq!(
        evaluate(&base(&truth), &[], &frame, limits())
            .into_parts()
            .0,
        Err(Failure::Frame)
    );
    candidate.post = RootView::Record(&[]);
    candidate.reason = None;
    let frame = transition(&[], &candidate);
    assert_eq!(
        evaluate(&base(&truth), &[], &frame, limits())
            .into_parts()
            .0,
        Err(Failure::Frame)
    );
}

#[test]
fn complete_frame_selectors_preserve_source_field_order_type_and_candidate() {
    let pre = [Field {
        id: 7,
        value: Atom::I128(11),
    }];
    let command = [Field {
        id: 7,
        value: Atom::I128(22),
    }];
    let context = [Field {
        id: 7,
        value: Atom::I128(33),
    }];
    let post = [Field {
        id: 7,
        value: Atom::I128(44),
    }];
    let patch = [Patch {
        field: 7,
        before: Atom::I128(11),
        after: Atom::I128(44),
    }];
    let payload = [Field {
        id: 9,
        value: Atom::I128(55),
    }];
    let out_payload = [Field {
        id: 9,
        value: Atom::I128(66),
    }];
    let effects = [Delivery {
        ordinal: 0,
        channel: 300,
        destination: Atom::Text(b"effect-target"),
        payload: payload.to_vec(),
        idempotency: Atom::Bytes(b"effect-id"),
    }];
    let outbox = [Delivery {
        ordinal: 0,
        channel: 301,
        destination: Atom::Text(b"outbox-target"),
        payload: out_payload.to_vec(),
        idempotency: Atom::Bytes(b"outbox-id"),
    }];
    let reads = [TraceRead {
        source: 1,
        selector: Selector::Field(71),
        permitted: true,
    }];
    let attempts = [Attempt::Write(72, false), Attempt::Effect(true, 73, true)];
    let frame = Frame::Transition {
        pre: RootView::Record(&pre),
        command: RootView::Record(&command),
        context: RootView::Record(&context),
        candidate: &Candidate {
            class: Class::CommittedFailure,
            reason: Some(203),
            post: RootView::Record(&post),
            patch: &patch,
            effects: &effects,
            outbox: &outbox,
            reads: &reads,
            attempts: &attempts,
            usage: usage(),
        },
    };
    let integers = [
        (Observation::Pre(7), 11),
        (Observation::Command(7), 22),
        (Observation::Context(7), 33),
        (Observation::Post(7), 44),
        (Observation::Class, 2),
        (Observation::Reason, 203),
        (Observation::PatchField(0), 7),
        (Observation::PatchBefore(0), 11),
        (Observation::PatchAfter(0), 44),
        (Observation::EffectOrdinal(0), 0),
        (Observation::EffectChannel(0), 300),
        (Observation::EffectPayload(0, 9), 55),
        (Observation::OutboxOrdinal(0), 0),
        (Observation::OutboxChannel(0), 301),
        (Observation::OutboxPayload(0, 9), 66),
        (Observation::ReadSource(0), 1),
        (Observation::ReadId(0), 71),
        (Observation::WriteSource(0), 1),
        (Observation::WriteId(0), 72),
        (Observation::EffectAttemptSource(0), 3),
        (Observation::EffectAttemptId(0), 73),
    ];
    for (selector, expected) in integers {
        assert!(matches!(frame::observe(&frame,selector),Some(Atom::I128(v)) if v==expected));
    }
    for selector in [
        Observation::PostLength,
        Observation::PatchLength,
        Observation::EffectLength,
        Observation::OutboxLength,
        Observation::ReadLength,
        Observation::WriteLength,
        Observation::EffectAttemptLength,
    ] {
        assert!(matches!(
            frame::observe(&frame, selector),
            Some(Atom::U128(1))
        ));
    }
    for (selector, expected) in [
        (Observation::HasReason, true),
        (Observation::ReadPermitted(0), true),
        (Observation::WritePermitted(0), false),
        (Observation::EffectAttemptPermitted(0), true),
    ] {
        assert!(matches!(frame::observe(&frame,selector),Some(Atom::Bool(v)) if v==expected));
    }
    assert!(matches!(
        frame::observe(&frame, Observation::EffectDestination(0)),
        Some(Atom::Text(b"effect-target"))
    ));
    assert!(matches!(
        frame::observe(&frame, Observation::OutboxDestination(0)),
        Some(Atom::Text(b"outbox-target"))
    ));
    assert!(matches!(
        frame::observe(&frame, Observation::EffectIdempotency(0)),
        Some(Atom::Bytes(b"effect-id"))
    ));
    assert!(matches!(
        frame::observe(&frame, Observation::OutboxIdempotency(0)),
        Some(Atom::Bytes(b"outbox-id"))
    ));
    for selector in [
        Observation::Initial(7),
        Observation::Post(999),
        Observation::PatchBefore(1),
        Observation::EffectPayload(0, 8),
        Observation::OutboxDestination(1),
        Observation::ReadId(1),
    ] {
        assert!(frame::observe(&frame, selector).is_none());
    }
}
#[test]
fn multiplication_division_and_explicit_numeric_projections() {
    // Native signed division supplies the independent truncating oracle. Small
    // complete sign grid avoids floating point and wide multiplication overflow.
    for a in -35i128..=35 {
        for b in -12i128..=12 {
            for mode in [Division::Exact, Division::Floor, Division::Ceil] {
                let expected = if b == 0 {
                    None
                } else {
                    let q = a / b;
                    let r = a % b;
                    match mode {
                        Division::Exact => {
                            if r == 0 {
                                Some(q)
                            } else {
                                None
                            }
                        }
                        Division::Floor => Some(q - i128::from(r != 0 && (a < 0) != (b < 0))),
                        Division::Ceil => Some(q + i128::from(r != 0 && (a < 0) == (b < 0))),
                    }
                };
                match (atoms::divide(mode, Atom::I128(a), Atom::I128(b)), expected) {
                    (Ok(Atom::I128(x)), Some(y)) => assert_eq!(x, y),
                    (Err(Failure::Undefined), None) => {}
                    _ => panic!("wrong signed division"),
                }
            }
        }
    }
    for (a, b, mode, expected) in [
        (i128::MIN, 1, Division::Exact, Some(i128::MIN)),
        (i128::MIN, -1, Division::Floor, None),
        (i128::MIN, i128::MIN, Division::Exact, Some(1)),
        (i128::MAX, 2, Division::Ceil, Some(i128::MAX / 2 + 1)),
    ] {
        match (atoms::divide(mode, Atom::I128(a), Atom::I128(b)), expected) {
            (Ok(Atom::I128(x)), Some(y)) => assert_eq!(x, y),
            (Err(Failure::Undefined), None) => {}
            _ => panic!("wrong boundary division"),
        }
    }
    assert!(matches!(
        atoms::binary(5, Atom::I128(i128::MIN), Atom::I128(-1)),
        Err(Failure::Undefined)
    ));
    assert!(matches!(
        atoms::binary(5, Atom::U128(u128::MAX), Atom::U128(1)),
        Ok(Atom::U128(u128::MAX))
    ));
    assert!(matches!(
        atoms::to_i128(Atom::Bool(true)),
        Ok(Atom::I128(1))
    ));
    assert!(matches!(
        atoms::to_i128(Atom::Sum {
            type_id: 101,
            variant: 121
        }),
        Ok(Atom::I128(121))
    ));
    assert!(matches!(
        atoms::to_i128(Atom::U128(u128::MAX)),
        Err(Failure::Undefined)
    ));
}

#[test]
fn explicit_delivery_ordinals_preserve_gaps_nonzero_starts_and_maximum() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let laws = base(&truth);
    let make = |ordinal| Delivery {
        ordinal,
        channel: 300,
        destination: Atom::Text(b"target"),
        payload: vec::Vec::new(),
        idempotency: Atom::Bytes(b"id"),
    };
    for ordinals in [vec![u32::MAX], vec![3, 9], vec![0, 7, u32::MAX]] {
        let deliveries: Vec<_> = ordinals.iter().map(|id| make(*id)).collect();
        let mut candidate = candidate(Class::CommittedFailure, &[]);
        candidate.effects = &deliveries;
        candidate.outbox = &deliveries;
        let frame = transition(&[], &candidate);
        assert_eq!(
            evaluate(&laws, &[], &frame, limits()).into_parts().0,
            Ok(())
        );
        for (index, ordinal) in ordinals.iter().enumerate() {
            assert!(
                matches!(frame::observe(&frame,Observation::EffectOrdinal(index)),Some(Atom::I128(v)) if v==*ordinal as i128)
            );
        }
    }
    for ordinals in [[3, 3], [9, 3]] {
        let deliveries = ordinals.map(make);
        let mut candidate = candidate(Class::Accept, &[]);
        candidate.outbox = &deliveries;
        let frame = transition(&[], &candidate);
        assert_eq!(
            evaluate(&laws, &[], &frame, limits()).into_parts().0,
            Err(Failure::Frame)
        );
    }
}

#[test]
fn scalar_roots_are_explicit_and_cannot_alias_record_field_zero() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let command = Atom::Sum {
        type_id: 101,
        variant: 121,
    };
    let candidate = candidate(Class::Accept, &[]);
    let frame = Frame::Transition {
        pre: RootView::Record(&[]),
        command: RootView::Leaf(command),
        context: RootView::Leaf(Atom::Bool(true)),
        candidate: &candidate,
    };
    assert!(matches!(
        frame::observe(&frame, Observation::CommandRoot),
        Some(Atom::Sum {
            type_id: 101,
            variant: 121
        })
    ));
    assert!(frame::observe(&frame, Observation::Command(0)).is_none());
    assert!(matches!(
        frame::observe(&frame, Observation::ContextRoot),
        Some(Atom::Bool(true))
    ));
    let program = [
        Op::Observe(Observation::CommandRoot),
        Op::ToI128(0),
        Op::Literal(Atom::I128(121)),
        Op::Eq(1, 2),
        Op::Observe(Observation::ContextRoot),
        Op::ToI128(4),
        Op::Literal(Atom::I128(1)),
        Op::Eq(5, 6),
        Op::And(3, 7),
    ];
    let mut laws = base(&truth);
    laws[3].program = Program {
        nodes: &program,
        root: 8,
    };
    assert_eq!(
        evaluate(&laws, &[], &frame, limits()).into_parts().0,
        Ok(())
    );
    let initial_program = [
        Op::Observe(Observation::InitialRoot),
        Op::Literal(Atom::I128(0)),
        Op::Eq(0, 1),
    ];
    laws[4].program = Program {
        nodes: &initial_program,
        root: 2,
    };
    for value in [i128::MIN, 0, i128::MAX] {
        let frame = Frame::Genesis {
            initial: RootView::Leaf(Atom::I128(value)),
        };
        assert_eq!(
            evaluate(&laws, &[], &frame, limits()).into_parts().0,
            if value == 0 {
                Ok(())
            } else {
                Err(Failure::Violated)
            }
        );
        assert!(frame::observe(&frame, Observation::Initial(0)).is_none());
    }
    let record = [Field {
        id: 0,
        value: Atom::I128(0),
    }];
    let frame = Frame::Genesis {
        initial: RootView::Record(&record),
    };
    assert!(frame::observe(&frame, Observation::InitialRoot).is_none());
    assert!(matches!(
        frame::observe(&frame, Observation::Initial(0)),
        Some(Atom::I128(0))
    ));
    assert_eq!(
        evaluate(&laws, &[], &frame, limits()).into_parts().0,
        Err(Failure::Undefined)
    );
}

// Independent arithmetic oracle: use a wider mathematical total, never Meter::charge.
fn guarded_charge(used: &mut [u64; 8], cap: [u64; 8], index: usize) -> Result<(), Failure> {
    let next = u128::from(used[index]) + 1;
    if next > u128::from(cap[index]) || next > u128::from(u64::MAX) {
        return Err(Failure::Budget(super::super::MeterFailure {
            resource: if index == 0 {
                Resource::Read
            } else {
                Resource::Step
            },
            limit: cap[index],
            attempted: next.min(u128::from(u64::MAX)) as u64,
            overflow: next > u128::from(u64::MAX),
        }));
    }
    used[index] = next as u64;
    Ok(())
}

#[test]
fn guarded_exact_shared_meter_and_attempt_prefix_oracle() {
    let initial = [Field {
        id: 7,
        value: Atom::Bool(true),
    }];
    let old = ReadAttempt {
        law: u32::MAX,
        node: usize::MAX,
        observation: Observation::OutboxPayload(usize::MAX, u16::MAX),
        permitted: false,
    };
    for guard in [Atom::Bool(false), Atom::Bool(true), Atom::I128(1)] {
        for present in [false, true] {
            let frame = Frame::Genesis {
                initial: RootView::Record(if present { &initial } else { &[] }),
            };
            let nodes = [
                Op::Literal(guard),
                Op::ObserveWhen(0, Observation::Initial(7), Atom::Bool(true)),
                Op::Literal(Atom::Bool(true)),
                Op::Eq(1, 2),
            ];
            let program = Program {
                nodes: &nodes,
                root: 3,
            };
            for start_read in [0, 3, u64::MAX] {
                for start_step in [0, 23, u64::MAX] {
                    for cap_read in [0, 3, 4, u64::MAX] {
                        for cap_step in [0, 1, 2, 3, 4, 24, 25, 26, 27, u64::MAX] {
                            let start = [start_read, 5, 7, 11, 13, 17, 19, start_step];
                            let cap = [cap_read, 0, 0, 0, 0, 0, 0, cap_step];
                            let mut expected_used = start;
                            let mut expected_reads = vec![old];
                            let mut expected = Ok(());
                            for node in 0..4 {
                                expected = guarded_charge(&mut expected_used, cap, 7);
                                if expected.is_err() {
                                    break;
                                }
                                if node != 1 {
                                    continue;
                                }
                                match guard {
                                    Atom::Bool(false) => {}
                                    Atom::Bool(true) => {
                                        expected = guarded_charge(&mut expected_used, cap, 0);
                                        expected_reads.push(ReadAttempt {
                                            law: 41,
                                            node: 1,
                                            observation: Observation::Initial(7),
                                            permitted: expected.is_ok(),
                                        });
                                        if expected.is_ok() && !present {
                                            expected = Err(Failure::Undefined);
                                        }
                                    }
                                    _ => expected = Err(Failure::Undefined),
                                }
                                if expected.is_err() {
                                    break;
                                }
                            }
                            let mut meter = meter::new(Limits { counters: cap });
                            meter.used.counters = start;
                            let mut reads = vec![old];
                            let result =
                                predicate::evaluate(&program, &frame, 41, &mut meter, &mut reads);
                            assert_eq!(result, expected);
                            assert_eq!(meter.used.counters, expected_used);
                            assert_eq!(meter.limits.counters, cap);
                            assert_eq!(reads, expected_reads);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn guarded_full_width_atoms_fallback_types_and_actual_values() {
    let pairs = [
        (Atom::Bool(true), Atom::Bool(false)),
        (Atom::I128(i128::MIN), Atom::I128(i128::MAX)),
        (Atom::U128(u128::MAX), Atom::U128(0)),
        (
            Atom::Enum {
                type_id: u32::MAX,
                variant: u16::MAX,
            },
            Atom::Enum {
                type_id: 0,
                variant: 0,
            },
        ),
        (
            Atom::Sum {
                type_id: u32::MAX,
                variant: u16::MAX,
            },
            Atom::Sum {
                type_id: 0,
                variant: 0,
            },
        ),
        (Atom::Bytes(&[0, 128, 255]), Atom::Bytes(&[])),
        (Atom::Text(b"\0\x7fexact"), Atom::Text(b"default")),
    ];
    for (actual, default) in pairs {
        for active in [false, true] {
            let frame = Frame::Genesis {
                initial: RootView::Leaf(actual),
            };
            let nodes = [
                Op::Literal(Atom::Bool(active)),
                Op::ObserveWhen(0, Observation::InitialRoot, default),
                Op::Literal(if active { actual } else { default }),
                Op::Eq(1, 2),
            ];
            let mut meter = meter::new(limits());
            let mut reads = Vec::new();
            assert_eq!(
                predicate::evaluate(
                    &Program {
                        nodes: &nodes,
                        root: 3
                    },
                    &frame,
                    41,
                    &mut meter,
                    &mut reads
                ),
                Ok(())
            );
            assert_eq!(
                meter.used.counters,
                [u64::from(active), 0, 0, 0, 0, 0, 0, 4]
            );
            assert_eq!(reads.len(), usize::from(active));
        }
    }
    let frame = Frame::Genesis {
        initial: RootView::Record(&[]),
    };
    for (default, expected) in [
        (Atom::I128(1), Err(Failure::Undefined)),
        (Atom::Bool(false), Err(Failure::Violated)),
        (Atom::Bool(true), Ok(())),
    ] {
        let nodes = [
            Op::Literal(Atom::Bool(false)),
            Op::ObserveWhen(0, Observation::OutboxPayload(0, 140), default),
        ];
        let mut meter = meter::new(super::super::zero_limits().with_limit(Resource::Step, 2));
        let mut reads = Vec::new();
        assert_eq!(
            predicate::evaluate(
                &Program {
                    nodes: &nodes,
                    root: 1
                },
                &frame,
                41,
                &mut meter,
                &mut reads
            ),
            expected
        );
        assert_eq!(meter.used.counters, [0, 0, 0, 0, 0, 0, 0, 2]);
        assert!(reads.is_empty());
    }
}

#[test]
fn guarded_metadata_checks_inactive_defaults_and_nonprior_guards() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let frame = Frame::Genesis {
        initial: RootView::Record(&[]),
    };
    for guard in [1, 2, usize::MAX] {
        let nodes = [
            Op::Literal(Atom::Bool(false)),
            Op::ObserveWhen(guard, Observation::PreRoot, Atom::Bool(true)),
        ];
        let mut laws = base(&truth);
        // Reject-only law is inactive at genesis; its graph must still be checked.
        laws[1].program = Program {
            nodes: &nodes,
            root: 1,
        };
        let (result, used, diagnostics, reads) =
            evaluate(&laws, &[], &frame, limits()).into_parts();
        assert_eq!(result, Err(Failure::Metadata));
        assert_eq!(used.counters, [0; 8]);
        assert!(diagnostics.is_empty() && reads.is_empty());
        let mut meter = meter::new(limits());
        let mut attempts = Vec::new();
        assert_eq!(
            predicate::evaluate(&laws[1].program, &frame, 20, &mut meter, &mut attempts),
            Err(Failure::Undefined)
        );
        assert_eq!(meter.used.used(Resource::Step), 2);
        assert!(attempts.is_empty());
    }
    for byte in 0..=255 {
        let bytes = [byte];
        for default in [Atom::Text(&bytes), Atom::Bytes(&bytes)] {
            let nodes = [
                Op::Literal(Atom::Bool(false)),
                Op::ObserveWhen(0, Observation::PreRoot, default),
            ];
            let mut laws = base(&truth);
            laws[1].program = Program {
                nodes: &nodes,
                root: 1,
            };
            let (result, used, _, reads) = evaluate(&laws, &[], &frame, limits()).into_parts();
            let valid = matches!(default, Atom::Bytes(_)) || byte < 128;
            assert_eq!(
                result,
                if valid {
                    Ok(())
                } else {
                    Err(Failure::Metadata)
                }
            );
            assert_eq!(used.used(Resource::Step), if valid { 2 } else { 0 });
            assert!(reads.is_empty());
        }
    }
}

#[test]
fn guarded_conditional_observations_preserve_scope_genesis_and_first_failure() {
    let truth = [Op::Literal(Atom::Bool(true))];
    let guarded = [
        Op::Literal(Atom::Bool(false)),
        Op::ObserveWhen(0, Observation::OutboxPayload(0, 140), Atom::Bool(true)),
    ];
    let ordinary = [
        Scope::Always,
        Scope::Accept,
        Scope::Reject,
        Scope::CommittedFailure,
        Scope::Committing,
    ];
    let table = [
        [true, true, true],
        [true, false, false],
        [false, true, false],
        [false, false, true],
        [true, false, true],
    ];
    for (row, scope) in ordinary.into_iter().enumerate() {
        for genesis in [false, true] {
            let mut laws = base(&truth);
            laws.push(Law {
                id: 60,
                kind: Kind::DebitCreditEffectEquality,
                scope,
                genesis,
                program: Program {
                    nodes: &guarded,
                    root: 1,
                },
            });
            let candidates = [
                candidate(Class::Accept, &[]),
                candidate(Class::Reject, &[]),
                candidate(Class::CommittedFailure, &[]),
            ];
            let frames = [
                transition(&[], &candidates[0]),
                transition(&[], &candidates[1]),
                transition(&[], &candidates[2]),
                frame_genesis(),
            ];
            for (phase, frame) in frames.iter().enumerate() {
                let applicable = if phase == 3 {
                    genesis
                } else {
                    table[row][phase]
                };
                let (result, used, diagnostics, reads) = evaluate(
                    &laws,
                    &[60],
                    frame,
                    super::super::zero_limits().with_limit(Resource::Step, 20),
                )
                .into_parts();
                assert_eq!(result, Ok(()));
                assert_eq!(
                    diagnostics[5].verdict,
                    if applicable {
                        Verdict::Satisfied
                    } else {
                        Verdict::Skipped
                    }
                );
                assert_eq!(
                    used.used(Resource::Step),
                    [2, 2, 3, 2][phase] + 2 * u64::from(applicable)
                );
                assert!(reads.is_empty());
            }
        }
    }
    // A guarded false result must stop before a later active unavailable read.
    let refused = [
        Op::Literal(Atom::Bool(false)),
        Op::ObserveWhen(0, Observation::PreRoot, Atom::Bool(false)),
    ];
    let later = [Op::Observe(Observation::InitialRoot)];
    let mut laws = base(&truth);
    laws[0].program = Program {
        nodes: &refused,
        root: 1,
    };
    laws[4].program = Program {
        nodes: &later,
        root: 0,
    };
    let (result, used, diagnostics, reads) =
        evaluate(&laws, &[], &frame_genesis(), limits()).into_parts();
    assert_eq!(result, Err(Failure::Violated));
    assert_eq!(used.used(Resource::Step), 2);
    assert_eq!(
        diagnostics,
        vec![Diagnostic {
            id: 10,
            verdict: Verdict::Refused(Failure::Violated)
        }]
    );
    assert!(reads.is_empty());
}

fn frame_genesis() -> Frame<'static> {
    Frame::Genesis {
        initial: RootView::Record(&[]),
    }
}

#[test]
fn guarded_true_missing_payload_and_wrong_type_never_use_default() {
    let fields = [Field {
        id: 140,
        value: Atom::I128(i128::MAX),
    }];
    let delivery = [Delivery {
        ordinal: u32::MAX,
        channel: 300,
        destination: Atom::Text(b"warehouse"),
        payload: fields.to_vec(),
        idempotency: Atom::Bytes(b"id"),
    }];
    for present in [false, true] {
        let mut c = candidate(Class::Accept, &[]);
        c.outbox = if present { &delivery } else { &[] };
        let frame = transition(&[], &c);
        for observation in [
            Observation::OutboxPayload(0, 140),
            Observation::OutboxPayload(0, 141),
        ] {
            let nodes = [
                Op::Literal(Atom::Bool(true)),
                Op::ObserveWhen(0, observation, Atom::Bool(true)),
            ];
            let mut meter = meter::new(limits());
            let mut reads = Vec::new();
            // Present numeric payload is returned unchanged, hence not a Bool predicate.
            // Missing payload refuses; neither path may use the Bool(true) default.
            assert_eq!(
                predicate::evaluate(
                    &Program {
                        nodes: &nodes,
                        root: 1
                    },
                    &frame,
                    60,
                    &mut meter,
                    &mut reads
                ),
                Err(Failure::Undefined)
            );
            assert_eq!(meter.used.counters, [1, 0, 0, 0, 0, 0, 0, 2]);
            assert_eq!(
                reads,
                [ReadAttempt {
                    law: 60,
                    node: 1,
                    observation,
                    permitted: true
                }]
            );
        }
    }
}

#[test]
fn migrated_account_full_i128_arithmetic_grid() {
    let values = [
        i128::MIN,
        i128::MIN + 1,
        -901,
        -900,
        -1,
        0,
        1,
        2,
        3,
        899,
        900,
        901,
        i64::MAX as i128 + 1,
        i128::MAX - 900,
        i128::MAX - 1,
        i128::MAX,
    ];
    for x in values {
        for y in values {
            let actual = match atoms::binary(0, Atom::I128(x), Atom::I128(y)) {
                Ok(Atom::I128(n)) => Some(n),
                Err(Failure::Undefined) => None,
                _ => panic!("unexpected wide arithmetic result"),
            };
            assert_eq!(actual, x.checked_add(y));
            assert_eq!(atoms::equal(Atom::I128(x), Atom::I128(y)), x == y);
            assert!(
                matches!(atoms::binary(3,Atom::I128(x),Atom::I128(y)),Ok(Atom::Bool(v)) if v==(x<y))
            );
        }
    }
}

#[test]
fn s4_shared_observation_mixed_attempt_trace_write_and_effect_lanes() {
    let attempts = [
        Attempt::Effect(false, 17, true),
        Attempt::Candidate(false),
        Attempt::Write(65535, true),
        Attempt::Effect(true, 99, false),
        Attempt::Write(0, false),
        Attempt::Candidate(true),
        Attempt::Effect(false, 4, false),
    ];
    let mut candidate = candidate(Class::Accept, &[]);
    candidate.attempts = &attempts;
    let frame = transition(&[], &candidate);
    assert!(matches!(
        frame::observe(&frame, Observation::WriteLength),
        Some(Atom::U128(4))
    ));
    assert!(matches!(
        frame::observe(&frame, Observation::EffectAttemptLength),
        Some(Atom::U128(3))
    ));
    // Independent literal oracles; never derived by filtering the actual trace.
    let writes = [
        (0i128, 0i128, false),
        (1, 65535, true),
        (1, 0, false),
        (0, 0, true),
    ];
    for (i, (source, id, permitted)) in writes.iter().enumerate() {
        assert!(
            matches!(frame::observe(&frame,Observation::WriteSource(i)),Some(Atom::I128(v)) if v==*source)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::WriteId(i)),Some(Atom::I128(v)) if v==*id)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::WritePermitted(i)),Some(Atom::Bool(v)) if v==*permitted)
        );
    }
    let effects = [(2i128, 17i128, true), (3, 99, false), (2, 4, false)];
    for (i, (source, id, permitted)) in effects.iter().enumerate() {
        assert!(
            matches!(frame::observe(&frame,Observation::EffectAttemptSource(i)),Some(Atom::I128(v)) if v==*source)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::EffectAttemptId(i)),Some(Atom::I128(v)) if v==*id)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::EffectAttemptPermitted(i)),Some(Atom::Bool(v)) if v==*permitted)
        );
    }
    let write_selectors: [fn(usize) -> Observation; 3] = [
        Observation::WriteSource,
        Observation::WriteId,
        Observation::WritePermitted,
    ];
    for selector in write_selectors {
        assert!(frame::observe(&frame, selector(writes.len())).is_none());
        assert!(frame::observe(&frame, selector(usize::MAX)).is_none());
    }
    let effect_selectors: [fn(usize) -> Observation; 3] = [
        Observation::EffectAttemptSource,
        Observation::EffectAttemptId,
        Observation::EffectAttemptPermitted,
    ];
    for selector in effect_selectors {
        assert!(frame::observe(&frame, selector(effects.len())).is_none());
        assert!(frame::observe(&frame, selector(usize::MAX)).is_none());
    }
}

#[test]
fn s4_shared_observation_empty_attempt_trace_index_zero_is_none() {
    let candidate = candidate(Class::Accept, &[]);
    let frame = transition(&[], &candidate);
    assert!(matches!(
        frame::observe(&frame, Observation::WriteLength),
        Some(Atom::U128(0))
    ));
    assert!(matches!(
        frame::observe(&frame, Observation::EffectAttemptLength),
        Some(Atom::U128(0))
    ));
    let selectors: [fn(usize) -> Observation; 9] = [
        Observation::WriteSource,
        Observation::WriteId,
        Observation::WritePermitted,
        Observation::EffectAttemptSource,
        Observation::EffectAttemptId,
        Observation::EffectAttemptPermitted,
        Observation::ReadSource,
        Observation::ReadId,
        Observation::ReadPermitted,
    ];
    for selector in selectors {
        assert!(frame::observe(&frame, selector(0)).is_none());
    }
}

#[test]
fn s4_shared_observation_read_trace_root_field_ids_and_denied_retention() {
    let reads = [
        TraceRead {
            source: 0,
            selector: Selector::Root,
            permitted: true,
        },
        TraceRead {
            source: 1,
            selector: Selector::Field(65535),
            permitted: false,
        },
        TraceRead {
            source: 2,
            selector: Selector::Field(0),
            permitted: true,
        },
    ];
    let mut candidate = candidate(Class::Accept, &[]);
    candidate.reads = &reads;
    let frame = transition(&[], &candidate);
    assert!(matches!(
        frame::observe(&frame, Observation::ReadLength),
        Some(Atom::U128(3))
    ));
    // Independent literal oracle: (source, stable selector id, permitted).
    let expected = [(0i128, 65536i128, true), (1, 65535, false), (2, 0, true)];
    for (i, (source, id, permitted)) in expected.iter().enumerate() {
        assert!(
            matches!(frame::observe(&frame,Observation::ReadSource(i)),Some(Atom::I128(v)) if v==*source)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::ReadId(i)),Some(Atom::I128(v)) if v==*id)
        );
        assert!(
            matches!(frame::observe(&frame,Observation::ReadPermitted(i)),Some(Atom::Bool(v)) if v==*permitted)
        );
    }
    let read_selectors: [fn(usize) -> Observation; 3] = [
        Observation::ReadSource,
        Observation::ReadId,
        Observation::ReadPermitted,
    ];
    for selector in read_selectors {
        assert!(frame::observe(&frame, selector(expected.len())).is_none());
        assert!(frame::observe(&frame, selector(usize::MAX)).is_none());
    }
}
