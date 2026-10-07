//! Public raw-record composition against actual canonical Value bytes.
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{
    Domain, Op, V2InputField, V2InputLeaf, V2InputVariant, V2Resource, V2ScalarProgram,
    v2_composition as c, v2_laws as l, v2_zero_limits,
};
use zeno_fcis_value::{Field, Value};

#[test]
fn canonical_original_records_preserve_binding_order_and_shared_budget() {
    let values = [
        Value::record_canonical(vec![
            Field::new(7, Value::signed(-9)),
            Field::new(32, Value::boolean(true)),
        ]),
        Value::record_canonical(vec![Field::new(7, Value::enumeration(u32::MAX, u16::MAX))]),
        Value::record_canonical(vec![Field::new(u16::MAX, Value::sum(0, 7, None))]),
    ];
    let bytes: [Vec<u8>; 3] = values.map(|value| {
        value
            .unwrap_or_else(|e| panic!("canonical record: {e}"))
            .canonical_bytes()
            .unwrap_or_else(|e| panic!("canonical bytes: {e}"))
    });
    let state = [
        V2InputField {
            id: 7,
            leaf: V2InputLeaf::I128 { min: -9, max: 9 },
        },
        V2InputField {
            id: 32,
            leaf: V2InputLeaf::Bool,
        },
    ];
    let command = [V2InputField {
        id: 7,
        leaf: V2InputLeaf::Enum {
            type_id: u32::MAX,
            min: -5,
            max: -4,
            variants: vec![
                V2InputVariant {
                    id: u16::MAX,
                    code: -4,
                },
                V2InputVariant { id: 0, code: -5 },
            ],
        },
    }];
    let context = [V2InputField {
        id: u16::MAX,
        leaf: V2InputLeaf::Sum {
            type_id: 0,
            min: 10,
            max: 11,
            variants: vec![
                V2InputVariant { id: 7, code: 11 },
                V2InputVariant {
                    id: u16::MAX,
                    code: 10,
                },
            ],
        },
    }];
    let bindings = [
        c::Binding {
            source: c::Source::Context,
            selector: c::Selector::Field(u16::MAX),
        },
        c::Binding {
            source: c::Source::State,
            selector: c::Selector::Field(32),
        },
        c::Binding {
            source: c::Source::Command,
            selector: c::Selector::Field(7),
        },
        c::Binding {
            source: c::Source::State,
            selector: c::Selector::Field(7),
        },
    ];
    let domains = [
        Domain::Int { min: 10, max: 11 },
        Domain::Bool,
        Domain::Int { min: -5, max: -4 },
        Domain::Int { min: -9, max: 9 },
    ];
    let program = V2ScalarProgram {
        inputs: &domains,
        outputs: &domains,
        nodes: &[Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)],
        roots: &[0, 1, 2, 3],
    };

    let output_types = [
        V2InputLeaf::I128 { min: 10, max: 11 },
        V2InputLeaf::Bool,
        V2InputLeaf::Enum {
            type_id: u32::MAX,
            min: -5,
            max: -4,
            variants: vec![
                V2InputVariant {
                    id: u16::MAX,
                    code: -4,
                },
                V2InputVariant { id: 0, code: -5 },
            ],
        },
        V2InputLeaf::I128 { min: -9, max: 9 },
    ];
    let assignments = [
        c::Assignment {
            field: 7,
            value: c::Expr::Output(3),
            domain: c::Domain::I128 { min: -9, max: 9 },
        },
        c::Assignment {
            field: 32,
            value: c::Expr::Output(1),
            domain: c::Domain::Bool,
        },
    ];
    let payload = [c::PayloadField {
        field: 7,
        value: c::Expr::Output(2),
    }];
    let deliveries = [c::DeliveryPlan {
        ordinal: 0,
        channel: 7,
        when: c::Expr::Constant(c::Atom::Bool(true)),
        destination: c::Expr::Constant(c::Atom::Bool(false)),
        payload: &payload,
        idempotency: c::Expr::Constant(c::Atom::Bool(true)),
    }];
    let branches = [
        c::Branch {
            code: 10,
            class: c::Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &[],
            outbox: &[],
        },
        c::Branch {
            code: 11,
            class: c::Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &deliveries,
            outbox: &[],
        },
    ];
    let payload_types = [c::TypedField {
        field: 7,
        domain: c::Domain::Enum {
            type_id: u32::MAX,
            variants: &[0, u16::MAX],
        },
    }];
    let channels = [c::Channel {
        id: 7,
        destination: c::Domain::Bool,
        payload: &payload_types,
        idempotency: c::Domain::Bool,
    }];
    // These fixture laws satisfy the complete route's mandatory families; the
    // assertions below independently check the record semantics under test.
    let predicate = [l::Op::Literal(l::Atom::Bool(true))];
    let laws: Vec<_> = [
        (1, l::Kind::StateInvariant, l::Scope::Committing, true),
        (2, l::Kind::RejectNoAuthority, l::Scope::Reject, false),
        (
            3,
            l::Kind::CommittedFailureEffects,
            l::Scope::CommittedFailure,
            false,
        ),
        (4, l::Kind::DecisionConformance, l::Scope::Always, false),
        (5, l::Kind::InitialCondition, l::Scope::Always, true),
    ]
    .into_iter()
    .map(|(id, kind, scope, genesis)| l::Law {
        id,
        kind,
        scope,
        genesis,
        program: l::Program {
            nodes: &predicate,
            root: 0,
        },
    })
    .collect();
    let total = bytes.iter().map(|b| b.len() as u64).sum();
    let limits = v2_zero_limits()
        .with_limit(V2Resource::Byte, total)
        .with_limit(V2Resource::Read, 4)
        .with_limit(V2Resource::Step, 6)
        .with_limit(V2Resource::Candidate, 1)
        .with_limit(V2Resource::Write, 2)
        .with_limit(V2Resource::Effect, 1);
    let mut descriptor = c::Descriptor {
        state: c::Schema::Record(&state),
        command: c::Schema::Record(&command),
        context: c::Schema::Record(&context),
        program,
        bindings: &bindings,
        output_types: &output_types,
        decision_output: 0,
        branches: &branches,
        reasons: &[],
        channels: &channels,
        laws: &laws,
        required: &[],
        limits,
    };
    let raw = c::Raw {
        state: &bytes[0],
        command: &bytes[1],
        context: &bytes[2],
    };
    {
        let bound = c::bind(&descriptor).unwrap_or_else(|e| panic!("composition admission: {e:?}"));
        let complete = bound.execute(raw);
        assert_eq!(complete.usage().used(V2Resource::Byte), total);
        assert_eq!(complete.usage().used(V2Resource::Read), 4);
        assert_eq!(complete.usage().used(V2Resource::Step), 6);
        assert_eq!(
            complete
                .reads()
                .iter()
                .filter(|r| r.source == 1)
                .map(|r| (r.selector, r.permitted))
                .collect::<Vec<_>>(),
            [(c::Selector::Field(7), true)]
        );
        let candidate = complete
            .result()
            .unwrap_or_else(|e| panic!("complete canonical records: {e:?}"));
        assert_eq!(
            candidate.post(),
            [
                c::Field {
                    id: 7,
                    value: c::Atom::I128(-9)
                },
                c::Field {
                    id: 32,
                    value: c::Atom::Bool(true)
                }
            ]
        );
        assert_eq!(
            candidate.effects()[0].payload,
            [c::Field {
                id: 7,
                value: c::Atom::Enum {
                    type_id: u32::MAX,
                    variant: u16::MAX
                }
            }]
        );
        // The retained four-output tuple oracle lives at the producer ABI/evaluator
        // boundary; this public route also performs a complete candidate stage
        // and the two applicable one-instruction laws (four + two steps).
        assert_eq!(complete.usage().used(V2Resource::Candidate), 1);
        assert_eq!(complete.usage().used(V2Resource::Write), 2);
        assert_eq!(complete.usage().used(V2Resource::Effect), 1);
        for resource in [V2Resource::WitnessByte, V2Resource::Depth] {
            assert_eq!(complete.usage().used(resource), 0);
        }
    }
    descriptor.limits = limits.with_limit(V2Resource::Read, 3);
    {
        let bound =
            c::bind(&descriptor).unwrap_or_else(|e| panic!("bounded read admission: {e:?}"));
        let refused = bound.execute(raw);
        assert_eq!(refused.usage().used(V2Resource::Byte), total);
        assert_eq!(refused.usage().used(V2Resource::Read), 3);
        assert_eq!(refused.usage().used(V2Resource::Step), 0);
        assert_eq!(refused.reads().iter().filter(|r| r.source == 0).count(), 2);
        assert_eq!(
            refused
                .reads()
                .iter()
                .filter(|r| r.source == 2)
                .map(|r| (r.selector, r.permitted))
                .collect::<Vec<_>>(),
            [(c::Selector::Field(u16::MAX), false)]
        );
        let Err(c::Failure::Ingress(2, refusal)) = refused.result() else {
            panic!("expected context ingress refusal");
        };
        assert_eq!(
            format!("{refusal:?}"),
            "Record(Budget(MeterFailure { resource: Read, limit: 3, attempted: 4, overflow: false }))"
        );
    }
    descriptor.limits = limits.with_limit(V2Resource::Step, 3);
    let bound = c::bind(&descriptor).unwrap_or_else(|e| panic!("bounded step admission: {e:?}"));
    let step = bound.execute(raw);
    assert_eq!(step.usage().used(V2Resource::Byte), total);
    assert_eq!(step.usage().used(V2Resource::Read), 4);
    assert_eq!(step.usage().used(V2Resource::Step), 3);
    assert!(matches!(step.result(), Err(c::Failure::Execution(_))));
}
