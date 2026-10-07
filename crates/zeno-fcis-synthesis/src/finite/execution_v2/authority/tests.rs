use super::super::{Resource, decision::*, laws, meter, zero_limits};
use super::{
    candidate::candidate_bytes,
    canonical::{Part, encode, encoded_size, exact},
    observations::*,
};
use alloc::{vec, vec::Vec};
#[path = "sealing_tests.rs"]
pub(super) mod sealing;

// Independent wire oracle: standard-library conversion, with no production projection.
fn w(out: &mut Vec<u8>, v: u128) {
    out.push(0);
    out.extend_from_slice(&v.to_be_bytes());
}
fn b(out: &mut Vec<u8>, v: &[u8]) {
    out.push(1);
    out.extend_from_slice(&(v.len() as u128).to_be_bytes());
    out.extend_from_slice(v);
}
fn int(out: &mut Vec<u8>, v: i128) {
    w(out, 1);
    w(out, v as u128);
}
fn text(out: &mut Vec<u8>, v: &[u8]) {
    w(out, 6);
    b(out, v);
}
pub(super) fn generous() -> super::super::Limits {
    let mut limits = zero_limits();
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
        limits = limits.with_limit(r, u64::MAX);
    }
    limits
}
#[test]
fn optional_stage_usage_keeps_absence_distinct_from_zero_and_full_counters() {
    let mut parts = Vec::new();
    append_optional_usage(&mut parts, None);
    let mut absent = Vec::new();
    w(&mut absent, 0);
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in optional_stage_usage_keeps_absence_distinct_from_zero_and_full_counters")),absent);
    let mut meter = meter::new(generous());
    parts.clear();
    append_optional_usage(&mut parts, Some(meter.used));
    let mut zero = Vec::new();
    w(&mut zero, 1);
    for _ in 0..8 {
        w(&mut zero, 0);
    }
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in optional_stage_usage_keeps_absence_distinct_from_zero_and_full_counters")),zero);
    assert_ne!(zero, absent);
    for (i, r) in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .into_iter()
    .enumerate()
    {
        meter.charge(r,(i+1) as u64).unwrap_or_else(|error| panic!("optional_stage_usage_keeps_absence_distinct_from_zero_and_full_counters fixture failed: {error:?}"));
    }
    parts.clear();
    append_optional_usage(&mut parts, Some(meter.used));
    let mut expected = Vec::new();
    w(&mut expected, 1);
    for i in 1..=8 {
        w(&mut expected, i);
    }
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in optional_stage_usage_keeps_absence_distinct_from_zero_and_full_counters")),expected);
}

#[test]
fn primitive_encoding_matches_standard_bytes_and_full_equality() {
    let payload: Vec<u8> = (0..=255).collect();
    for value in [
        0,
        1,
        255,
        256,
        u64::MAX as u128,
        i128::MAX as u128,
        1u128 << 127,
        u128::MAX,
    ] {
        for bytes in [&[][..], &payload[..1], &payload[..]] {
            let parts = [Part::Word(value), Part::Bytes(bytes), Part::Word(value)];
            let mut expected = Vec::new();
            w(&mut expected, value);
            b(&mut expected, bytes);
            w(&mut expected, value);
            let actual=encode(&parts).unwrap_or_else(|| panic!("missing fixture value in primitive_encoding_matches_standard_bytes_and_full_equality"));
            assert_eq!(actual, expected);
            assert_eq!(encoded_size(&parts), Some(expected.len()));
            assert!(exact(&actual, &expected));
            assert!(!exact(&actual, &expected[..expected.len() - 1]));
            for i in 0..actual.len() {
                let mut changed = actual.clone();
                changed[i] ^= 1;
                assert!(!exact(&actual, &changed), "byte {i}");
            }
        }
    }
    assert_eq!(encode(&[]), Some(vec![]));
}

#[test]
fn complete_three_class_oracle_preserves_patch_and_order() {
    let pre = [
        Field {
            id: 0,
            value: Atom::I128(i128::MIN),
        },
        Field {
            id: u16::MAX,
            value: Atom::Bool(true),
        },
    ];
    let assignments = [
        Assignment {
            field: 0,
            value: Expr::Constant(Atom::I128(i128::MAX)),
            domain: Domain::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        Assignment {
            field: u16::MAX,
            value: Expr::Constant(Atom::Bool(true)),
            domain: Domain::Bool,
        },
    ];
    let payload = [PayloadField {
        field: u16::MAX,
        value: Expr::Constant(Atom::I128(-1)),
    }];
    let effects = [DeliveryPlan {
        ordinal: 3,
        channel: u32::MAX,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"effect-dest")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(u128::MAX)),
    }];
    let outbox = [DeliveryPlan {
        ordinal: 8,
        channel: 7,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"outbox-dest")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::Bytes(b"key")),
    }];
    for (class, tag, reason) in [
        (Class::Accept, 0, None),
        (Class::Reject, 1, Some(9)),
        (Class::CommittedFailure, 2, Some(u32::MAX)),
    ] {
        let reject = class == Class::Reject;
        let branch = [Branch {
            code: -7,
            class,
            reason,
            assignments: if reject { &[] } else { &assignments },
            effects: if reject { &[] } else { &effects },
            outbox: if reject { &[] } else { &outbox },
        }];
        let mut meter = meter::new(generous());
        let mut attempts = Vec::new();
        let candidate = construct(
            Inputs {
                state: &pre,
                command: RootView::Record(&[]),
                context: RootView::Record(&[]),
            },
            &[],
            -7,
            &branch,
            &mut meter,
            &mut attempts,
        )
        .unwrap_or_else(|error| {
            panic!(
                "complete_three_class_oracle_preserves_patch_and_order fixture failed: {error:?}"
            )
        });
        let actual = candidate_bytes(&candidate).unwrap_or_else(|| {
            panic!("missing fixture value in complete_three_class_oracle_preserves_patch_and_order")
        });
        let mut expected = Vec::new();
        w(&mut expected, 0x5a434432);
        w(&mut expected, 1);
        w(&mut expected, tag);
        if let Some(reason) = reason {
            w(&mut expected, 1);
            w(&mut expected, reason as u128);
        } else {
            w(&mut expected, 0);
        }
        // Complete original state.
        w(&mut expected, 2);
        w(&mut expected, 0);
        int(&mut expected, i128::MIN);
        w(&mut expected, u16::MAX as u128);
        w(&mut expected, 0);
        w(&mut expected, 1);
        if reject {
            for _ in 0..4 {
                w(&mut expected, 0);
            }
        } else {
            w(&mut expected, 2);
            w(&mut expected, 0);
            int(&mut expected, i128::MAX);
            w(&mut expected, u16::MAX as u128);
            w(&mut expected, 0);
            w(&mut expected, 1);
            w(&mut expected, 1);
            w(&mut expected, 0);
            int(&mut expected, i128::MIN);
            int(&mut expected, i128::MAX);
            w(&mut expected, 1);
            w(&mut expected, 3);
            w(&mut expected, u32::MAX as u128);
            text(&mut expected, b"effect-dest");
            w(&mut expected, 1);
            w(&mut expected, u16::MAX as u128);
            int(&mut expected, -1);
            w(&mut expected, 2);
            w(&mut expected, u128::MAX);
            w(&mut expected, 1);
            w(&mut expected, 8);
            w(&mut expected, 7);
            text(&mut expected, b"outbox-dest");
            w(&mut expected, 1);
            w(&mut expected, u16::MAX as u128);
            int(&mut expected, -1);
            w(&mut expected, 5);
            b(&mut expected, b"key");
        }
        assert_eq!(actual, expected);
        assert_eq!(candidate.pre(), &pre);
    }
}

#[test]
fn atom_types_and_all_counter_positions_are_distinct() {
    let values = [
        Atom::Bool(false),
        Atom::I128(0),
        Atom::U128(0),
        Atom::Enum {
            type_id: u32::MAX,
            variant: u16::MAX,
        },
        Atom::Sum {
            type_id: u32::MAX,
            variant: u16::MAX,
        },
        Atom::Bytes(b"same"),
        Atom::Text(b"same"),
    ];
    let mut encodings = Vec::new();
    for value in values {
        let mut parts = Vec::new();
        super::candidate::atom(&mut parts, value);
        encodings.push(encode(&parts).unwrap_or_else(|| {
            panic!("missing fixture value in atom_types_and_all_counter_positions_are_distinct")
        }));
    }
    for i in 0..encodings.len() {
        for j in 0..i {
            assert_ne!(encodings[i], encodings[j]);
        }
    }
    let mut meter = meter::new(generous());
    for (i, r) in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .into_iter()
    .enumerate()
    {
        meter.charge(r, (i + 1) as u64).unwrap_or_else(|error| {
            panic!("atom_types_and_all_counter_positions_are_distinct fixture failed: {error:?}")
        });
    }
    let mut parts = Vec::new();
    append_usage(&mut parts, meter.used);
    let mut expected = Vec::new();
    for i in 1..=8 {
        w(&mut expected, i);
    }
    assert_eq!(
        encode(&parts).unwrap_or_else(|| panic!(
            "missing fixture value in atom_types_and_all_counter_positions_are_distinct"
        )),
        expected
    );
}

#[test]
fn exact_diagnostics_and_attempts_include_refusal_details() {
    let error = super::super::MeterFailure {
        resource: Resource::Step,
        limit: 9,
        attempted: u64::MAX,
        overflow: true,
    };
    let diagnostics = [
        laws::Diagnostic {
            id: 1,
            verdict: laws::Verdict::Skipped,
        },
        laws::Diagnostic {
            id: 2,
            verdict: laws::Verdict::Satisfied,
        },
        laws::Diagnostic {
            id: 3,
            verdict: laws::Verdict::Refused(laws::Failure::Budget(error)),
        },
    ];
    let mut parts = Vec::new();
    append_diagnostics(&mut parts, &diagnostics);
    let mut expected = Vec::new();
    for v in [3, 1, 0, 2, 1, 3, 2, 4, 7, 9, u64::MAX as u128, 1] {
        w(&mut expected, v);
    }
    assert_eq!(
        encode(&parts).unwrap_or_else(|| panic!(
            "missing fixture value in exact_diagnostics_and_attempts_include_refusal_details"
        )),
        expected
    );
    let reads = [laws::ReadAttempt {
        law: u32::MAX,
        node: usize::MAX,
        observation: laws::Observation::OutboxPayload(usize::MAX, u16::MAX),
        permitted: false,
    }];
    parts.clear();
    append_law_reads(&mut parts, &reads);
    expected.clear();
    for v in [
        1,
        u32::MAX as u128,
        usize::MAX as u128,
        28,
        usize::MAX as u128,
        u16::MAX as u128,
        0,
    ] {
        w(&mut expected, v);
    }
    assert_eq!(
        encode(&parts).unwrap_or_else(|| panic!(
            "missing fixture value in exact_diagnostics_and_attempts_include_refusal_details"
        )),
        expected
    );
    parts.clear();
    append_decision_attempts(
        &mut parts,
        &[
            Attempt::Candidate(true),
            Attempt::Write(u16::MAX, false),
            Attempt::Effect(true, u32::MAX, false),
        ],
    );
    expected.clear();
    for v in [3, 0, 1, 1, u16::MAX as u128, 0, 2, 1, u32::MAX as u128, 0] {
        w(&mut expected, v);
    }
    assert_eq!(
        encode(&parts).unwrap_or_else(|| panic!(
            "missing fixture value in exact_diagnostics_and_attempts_include_refusal_details"
        )),
        expected
    );
}

#[test]
fn identity_and_subject_bind_every_original_component_and_genesis_kind() {
    use super::framing::*;
    let identity=identity_bytes(b"contract",b"evaluator").unwrap_or_else(|| panic!("missing fixture value in identity_and_subject_bind_every_original_component_and_genesis_kind"));
    let actual=subject_bytes(Kind::Transition,&identity,b"state",b"command",b"context",b"artifact").unwrap_or_else(|| panic!("missing fixture value in identity_and_subject_bind_every_original_component_and_genesis_kind"));
    let mut expected = Vec::new();
    for v in [0x5a525032, 1, 1] {
        w(&mut expected, v);
    }
    for bytes in [&identity[..], b"state", b"command", b"context", b"artifact"] {
        b(&mut expected, bytes);
    }
    assert_eq!(actual, expected);
    for (kind, id, s, c, x, a) in [
        (
            Kind::Genesis,
            &identity[..],
            b"state" as &[u8],
            b"command" as &[u8],
            b"context" as &[u8],
            b"artifact" as &[u8],
        ),
        (
            Kind::Transition,
            &identity[..],
            b"State",
            b"command",
            b"context",
            b"artifact",
        ),
        (
            Kind::Transition,
            &identity[..],
            b"state",
            b"Command",
            b"context",
            b"artifact",
        ),
        (
            Kind::Transition,
            &identity[..],
            b"state",
            b"command",
            b"Context",
            b"artifact",
        ),
        (
            Kind::Transition,
            &identity[..],
            b"state",
            b"command",
            b"context",
            b"Artifact",
        ),
        (
            Kind::Transition,
            b"other",
            b"state",
            b"command",
            b"context",
            b"artifact",
        ),
    ] {
        assert!(!exact(&actual,&subject_bytes(kind,id,s,c,x,a).unwrap_or_else(|| panic!("missing fixture value in identity_and_subject_bind_every_original_component_and_genesis_kind"))));
    }
    assert_ne!(identity,identity_bytes(b"Contract",b"evaluator").unwrap_or_else(|| panic!("missing fixture value in identity_and_subject_bind_every_original_component_and_genesis_kind")));
    assert_ne!(identity,identity_bytes(b"contract",b"Evaluator").unwrap_or_else(|| panic!("missing fixture value in identity_and_subject_bind_every_original_component_and_genesis_kind")));
    assert_ne!(identity_bytes(b"ab", b"c"), identity_bytes(b"a", b"bc"));
}

#[test]
fn evaluator_digest_binds_every_byte() {
    let original = super::EVALUATOR;
    let expected = super::framing::identity_bytes(b"policy", &original);
    for index in 0..original.len() {
        let mut changed = original;
        changed[index] ^= 1;
        assert_ne!(
            expected,
            super::framing::identity_bytes(b"policy", &changed)
        );
    }
}

#[test]
fn complete_schema_metadata_preserves_ranges_ids_codes_and_limits() {
    use super::super::{InputField, InputLeaf, InputVariant};
    use super::metadata;
    let fields = [
        InputField {
            id: 0,
            leaf: InputLeaf::I128 {
                min: i64::MIN,
                max: i64::MAX,
            },
        },
        InputField {
            id: 1,
            leaf: InputLeaf::Bool,
        },
        InputField {
            id: 2,
            leaf: InputLeaf::Enum {
                type_id: 0,
                min: -1,
                max: 0,
                variants: vec![
                    InputVariant {
                        id: u16::MAX,
                        code: -1,
                    },
                    InputVariant { id: 0, code: 0 },
                ],
            },
        },
        InputField {
            id: u16::MAX,
            leaf: InputLeaf::Sum {
                type_id: u32::MAX,
                min: i64::MIN,
                max: i64::MIN + 1,
                variants: vec![
                    InputVariant {
                        id: 0,
                        code: i64::MIN + 1,
                    },
                    InputVariant {
                        id: u16::MAX,
                        code: i64::MIN,
                    },
                ],
            },
        },
    ];
    let mut parts = Vec::new();
    metadata::input_fields(&mut parts, &fields);
    let mut expected = Vec::new();
    for value in [
        4,
        0,
        0,
        i64::MIN as i128 as u128,
        i64::MAX as u128,
        1,
        1,
        2,
        2,
        0,
        (-1i128) as u128,
        0,
        2,
        u16::MAX as u128,
        (-1i128) as u128,
        0,
        0,
        u16::MAX as u128,
        3,
        u32::MAX as u128,
        i64::MIN as i128 as u128,
        (i64::MIN + 1) as i128 as u128,
        2,
        0,
        (i64::MIN + 1) as i128 as u128,
        u16::MAX as u128,
        i64::MIN as i128 as u128,
    ] {
        w(&mut expected, value);
    }
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in complete_schema_metadata_preserves_ranges_ids_codes_and_limits")),expected);
    let mut limits = zero_limits();
    for (i, r) in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
        Resource::Step,
    ]
    .into_iter()
    .enumerate()
    {
        limits = limits.with_limit(r, (i + 1) as u64);
    }
    parts.clear();
    metadata::limits(&mut parts, &limits);
    expected.clear();
    for value in 1..=8 {
        w(&mut expected, value);
    }
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in complete_schema_metadata_preserves_ranges_ids_codes_and_limits")),expected);
}

#[test]
fn complete_branch_metadata_keeps_reasons_domains_and_delivery_expressions() {
    use super::metadata;
    let assignments = [Assignment {
        field: u16::MAX,
        value: Expr::Output(usize::MAX),
        domain: Domain::I128 {
            min: i128::MIN,
            max: i128::MAX,
        },
    }];
    let payload = [PayloadField {
        field: u16::MAX,
        value: Expr::Input(Source::Context, u16::MAX),
    }];
    let effects = [DeliveryPlan {
        ordinal: u32::MAX,
        channel: 7,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"destination")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(u128::MAX)),
    }];
    for (class, tag, reason) in [
        (Class::Accept, 0, None),
        (Class::Reject, 1, Some(1)),
        (Class::CommittedFailure, 2, Some(u32::MAX)),
    ] {
        let committing = class != Class::Reject;
        let branches = [Branch {
            code: i128::MIN,
            class,
            reason,
            assignments: if committing { &assignments } else { &[] },
            effects: if committing { &effects } else { &[] },
            outbox: &[],
        }];
        let mut parts = Vec::new();
        metadata::branches(&mut parts, &branches);
        let mut expected = Vec::new();
        for value in [1, i128::MIN as u128, tag] {
            w(&mut expected, value);
        }
        if let Some(reason) = reason {
            w(&mut expected, 1);
            w(&mut expected, reason as u128);
        } else {
            w(&mut expected, 0);
        }
        if committing {
            for value in [
                1,
                u16::MAX as u128,
                1,
                usize::MAX as u128,
                1,
                i128::MIN as u128,
                i128::MAX as u128,
                1,
                u32::MAX as u128,
                7,
                2,
                0,
                1,
                2,
                6,
            ] {
                w(&mut expected, value);
            }
            b(&mut expected, b"destination");
            for value in [
                1,
                u16::MAX as u128,
                0,
                2,
                u16::MAX as u128,
                2,
                2,
                u128::MAX,
                0,
            ] {
                w(&mut expected, value);
            }
        } else {
            for _ in 0..3 {
                w(&mut expected, 0);
            }
        }
        assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in complete_branch_metadata_keeps_reasons_domains_and_delivery_expressions")),expected);
    }
}

#[test]
fn complete_scalar_graph_metadata_keeps_all_nodes_domains_and_root_order() {
    use super::super::super::evaluation::{Domain as D, Op};
    use super::metadata;
    let inputs = [
        D::Bool,
        D::Int {
            min: i64::MIN,
            max: i64::MAX,
        },
    ];
    let outputs = [D::Int { min: -2, max: 4 }, D::Bool];
    let nodes = [
        Op::Input(u16::MAX),
        Op::Int(i64::MIN),
        Op::Bool(true),
        Op::Add(0, u16::MAX),
        Op::Sub(u16::MAX, 1),
        Op::Eq(0, 1),
        Op::Lt(1, 0),
        Op::And(0, 1),
        Op::Not(u16::MAX),
        Op::Select(u16::MAX, 0, u16::MAX),
    ];
    let roots = [u16::MAX, 0];
    let program = super::super::ScalarProgram {
        inputs: &inputs,
        outputs: &outputs,
        nodes: &nodes,
        roots: &roots,
    };
    let mut parts = Vec::new();
    metadata::program(&mut parts, &program);
    let mut expected = Vec::new();
    for value in [
        2,
        0,
        1,
        i64::MIN as i128 as u128,
        i64::MAX as u128,
        2,
        1,
        (-2i128) as u128,
        4,
        0,
        10,
        0,
        u16::MAX as u128,
        1,
        i64::MIN as i128 as u128,
        2,
        1,
        3,
        0,
        u16::MAX as u128,
        4,
        u16::MAX as u128,
        1,
        5,
        0,
        1,
        6,
        1,
        0,
        7,
        0,
        1,
        8,
        u16::MAX as u128,
        9,
        u16::MAX as u128,
        0,
        u16::MAX as u128,
        2,
        u16::MAX as u128,
        0,
    ] {
        w(&mut expected, value);
    }
    assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in complete_scalar_graph_metadata_keeps_all_nodes_domains_and_root_order")),expected);
}

#[test]
fn complete_law_metadata_keeps_scope_genesis_rounding_observations_and_every_node() {
    use super::metadata;
    let kinds = [
        laws::Kind::StateInvariant,
        laws::Kind::AssetConservation,
        laws::Kind::MintBurnAuthorization,
        laws::Kind::DebitCreditEffectEquality,
        laws::Kind::FeeAndRounding,
        laws::Kind::AuthoritySubjectRecipient,
        laws::Kind::RejectNoAuthority,
        laws::Kind::CommittedFailureEffects,
        laws::Kind::DecisionConformance,
        laws::Kind::InitialCondition,
    ];
    let scopes = [
        laws::Scope::Always,
        laws::Scope::Accept,
        laws::Scope::Reject,
        laws::Scope::CommittedFailure,
        laws::Scope::Committing,
    ];
    let nodes = [
        laws::Op::Literal(laws::Atom::Bool(true)),
        laws::Op::Observe(laws::Observation::OutboxPayload(usize::MAX, u16::MAX)),
        laws::Op::Add(0, usize::MAX),
        laws::Op::Sub(usize::MAX, 1),
        laws::Op::Mul(usize::MAX, usize::MAX),
        laws::Op::Div(laws::Division::Ceil, 1, usize::MAX),
        laws::Op::ToI128(usize::MAX),
        laws::Op::Eq(usize::MAX, 0),
        laws::Op::Lt(0, 1),
        laws::Op::And(0, 1),
        laws::Op::Not(usize::MAX),
        laws::Op::Select(usize::MAX, 0, usize::MAX),
    ];
    for (kind_index, kind) in kinds.into_iter().enumerate() {
        for (scope_index, scope) in scopes.into_iter().enumerate() {
            for genesis in [false, true] {
                let laws = [laws::Law {
                    id: u32::MAX,
                    kind,
                    scope,
                    genesis,
                    program: laws::Program {
                        nodes: &nodes,
                        root: usize::MAX,
                    },
                }];
                let mut parts = Vec::new();
                metadata::law_list(&mut parts, &laws);
                let mut expected = Vec::new();
                for value in [
                    1,
                    u32::MAX as u128,
                    kind_index as u128,
                    scope_index as u128,
                    genesis as u128,
                    12,
                    0,
                    0,
                    1,
                    1,
                    28,
                    usize::MAX as u128,
                    u16::MAX as u128,
                    2,
                    0,
                    usize::MAX as u128,
                    3,
                    usize::MAX as u128,
                    1,
                    4,
                    usize::MAX as u128,
                    usize::MAX as u128,
                    5,
                    2,
                    1,
                    usize::MAX as u128,
                    6,
                    usize::MAX as u128,
                    7,
                    usize::MAX as u128,
                    0,
                    8,
                    0,
                    1,
                    9,
                    0,
                    1,
                    10,
                    usize::MAX as u128,
                    11,
                    usize::MAX as u128,
                    0,
                    usize::MAX as u128,
                    usize::MAX as u128,
                ] {
                    w(&mut expected, value);
                }
                assert_eq!(encode(&parts).unwrap_or_else(|| panic!("missing fixture value in complete_law_metadata_keeps_scope_genesis_rounding_observations_and_every_node")),expected);
            }
        }
    }
}
