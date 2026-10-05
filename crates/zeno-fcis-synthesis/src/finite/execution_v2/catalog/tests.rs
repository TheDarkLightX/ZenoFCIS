//! Independent original wire fixtures and complete construction refusals.
use super::super::super::evaluation::{Domain as SD, Op};
use super::super::{
    InputField, InputLeaf, InputVariant, Resource, ScalarProgram, laws as l, zero_limits,
};
use super::*;
use alloc::{vec, vec::Vec};
use composition::{
    Assignment, Atom, Binding, Branch, Channel, Class, DeliveryPlan, Domain, Expr, FrameBinding,
    PayloadField, Schema, Selector, Source, TypedField,
};
use schema::{Definition, Description, Field, Kind, Variant};

// Deliberately uses primitive standard big-endian encoders, never the schema matcher.
fn wire(d: &Description<'_>) -> Vec<u8> {
    fn name(out: &mut Vec<u8>, b: &[u8]) {
        out.extend((b.len() as u16).to_be_bytes());
        out.extend(b);
    }
    let mut b = b"ZFCISSCHEMA1\0".to_vec();
    name(&mut b, d.profile);
    b.extend(d.version.to_be_bytes());
    b.extend(d.root.to_be_bytes());
    b.extend((d.definitions.len() as u32).to_be_bytes());
    for t in d.definitions {
        b.extend(t.id.to_be_bytes());
        name(&mut b, t.name);
        match t.kind {
            Kind::Unit => b.push(0),
            Kind::Bool => b.push(1),
            Kind::U128 { min, max } => {
                b.push(2);
                b.extend(min.to_be_bytes());
                b.extend(max.to_be_bytes());
            }
            Kind::I128 { min, max } => {
                b.push(3);
                b.extend(min.to_be_bytes());
                b.extend(max.to_be_bytes());
            }
            Kind::Bytes { min, max } | Kind::Text { min, max } => {
                b.push(if matches!(t.kind, Kind::Bytes { .. }) {
                    4
                } else {
                    5
                });
                b.extend(min.to_be_bytes());
                b.extend(max.to_be_bytes());
            }
            Kind::Enum(v) | Kind::Sum(v) => {
                let sum = matches!(t.kind, Kind::Sum(_));
                b.push(if sum { 9 } else { 6 });
                b.extend((v.len() as u32).to_be_bytes());
                for v in v {
                    b.extend(v.id.to_be_bytes());
                    name(&mut b, v.name);
                    if sum {
                        b.push(0);
                    }
                }
            }
            Kind::Record(fs) => {
                b.push(8);
                b.extend((fs.len() as u32).to_be_bytes());
                for f in fs {
                    b.extend(f.id.to_be_bytes());
                    name(&mut b, f.name);
                    b.extend(f.type_id.to_be_bytes());
                }
            }
            Kind::Tuple(items) => {
                b.push(7);
                b.extend((items.len() as u32).to_be_bytes());
                for id in items {
                    b.extend(id.to_be_bytes());
                }
            }
            Kind::SumPayload(variants) => {
                b.push(9);
                b.extend((variants.len() as u32).to_be_bytes());
                for variant in variants {
                    b.extend(variant.id.to_be_bytes());
                    name(&mut b, variant.name);
                    match variant.payload {
                        None => b.push(0),
                        Some(id) => {
                            b.push(1);
                            b.extend(id.to_be_bytes());
                        }
                    }
                }
            }
            Kind::Vector { element, min, max } => {
                b.push(10);
                b.extend(element.to_be_bytes());
                b.extend(min.to_be_bytes());
                b.extend(max.to_be_bytes());
            }
            Kind::Map {
                key,
                value,
                min,
                max,
            } => {
                b.push(11);
                b.extend(key.to_be_bytes());
                b.extend(value.to_be_bytes());
                b.extend(min.to_be_bytes());
                b.extend(max.to_be_bytes());
            }
        }
    }
    b
}
fn schema_limits() -> schema::Limits {
    schema::Limits {
        bytes: u64::MAX,
        types: u32::MAX,
        fields: u32::MAX,
        variants: u32::MAX,
    }
}
fn fixture(run: impl FnOnce(&Description<'_>, &Descriptor<'_>, &Framing, &[(u32, u32, u32)])) {
    let variants = [
        Variant { id: 0, name: b"A" },
        Variant {
            id: u16::MAX,
            name: b"Z",
        },
    ];
    let state_fields = [Field {
        id: 0,
        name: b"Count",
        type_id: 1,
    }];
    let payload_fields = [Field {
        id: u16::MAX,
        name: b"Data",
        type_id: 4,
    }];
    let defs = [
        Definition {
            id: 0,
            name: b"State",
            kind: Kind::Record(&state_fields),
        },
        Definition {
            id: 1,
            name: b"Count",
            kind: Kind::I128 { min: -5, max: 5 },
        },
        Definition {
            id: 2,
            name: b"Cmd",
            kind: Kind::Sum(&variants),
        },
        Definition {
            id: 3,
            name: b"Flag",
            kind: Kind::Bool,
        },
        Definition {
            id: 4,
            name: b"Bytes",
            kind: Kind::Bytes { min: 1, max: 3 },
        },
        Definition {
            id: 5,
            name: b"Destination",
            kind: Kind::Text { min: 1, max: 3 },
        },
        Definition {
            id: 6,
            name: b"Payload",
            kind: Kind::Record(&payload_fields),
        },
        Definition {
            id: 7,
            name: b"UnusedSigned",
            kind: Kind::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
        },
        Definition {
            id: u32::MAX,
            name: b"UnusedUnsigned",
            kind: Kind::U128 {
                min: 0,
                max: u128::MAX,
            },
        },
    ];
    let description = Description {
        profile: b"Catalog",
        version: 0,
        root: 0,
        definitions: &defs,
    };
    let state = [InputField {
        id: 0,
        leaf: InputLeaf::I128 { min: -5, max: 5 },
    }];
    let command = InputLeaf::Sum {
        type_id: 2,
        min: -2,
        max: -1,
        variants: vec![
            InputVariant {
                id: u16::MAX,
                code: -2,
            },
            InputVariant { id: 0, code: -1 },
        ],
    };
    let context = InputLeaf::Bool;
    let bindings = [
        Binding {
            source: Source::State,
            selector: Selector::Field(0),
        },
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        Binding {
            source: Source::Context,
            selector: Selector::Root,
        },
    ];
    let outputs = [InputLeaf::I128 { min: 0, max: 1 }];
    let inputs = [
        SD::Int { min: -5, max: 5 },
        SD::Int { min: -2, max: -1 },
        SD::Bool,
    ];
    let program = ScalarProgram {
        inputs: &inputs,
        outputs: &[SD::Int { min: 0, max: 1 }],
        nodes: &[Op::Int(0)],
        roots: &[0],
    };
    let assignments = [Assignment {
        field: 0,
        value: Expr::Input(Source::State, 0),
        domain: Domain::I128 { min: -5, max: 5 },
    }];
    let payload = [PayloadField {
        field: u16::MAX,
        value: Expr::Constant(Atom::Bytes(b"abc")),
    }];
    let plans = [DeliveryPlan {
        ordinal: 0,
        channel: u32::MAX,
        when: Expr::Constant(Atom::Bool(false)),
        destination: Expr::Constant(Atom::Text(b"dst")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(u128::MAX)),
    }];
    let branches = [
        Branch {
            code: 0,
            class: Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &[],
            outbox: &[],
        },
        Branch {
            code: 1,
            class: Class::Accept,
            reason: None,
            assignments: &assignments,
            effects: &plans,
            outbox: &plans,
        },
    ];
    let fields = [TypedField {
        field: u16::MAX,
        domain: Domain::Bytes,
    }];
    let channels = [Channel {
        id: u32::MAX,
        destination: Domain::Text,
        payload: &fields,
        idempotency: Domain::U128 {
            min: 0,
            max: u128::MAX,
        },
    }];
    let nodes = [l::Op::Literal(l::Atom::Bool(true))];
    let laws = [
        (l::Kind::StateInvariant, l::Scope::Committing, true),
        (l::Kind::RejectNoAuthority, l::Scope::Reject, false),
        (
            l::Kind::CommittedFailureEffects,
            l::Scope::CommittedFailure,
            false,
        ),
        (l::Kind::DecisionConformance, l::Scope::Always, false),
        (l::Kind::InitialCondition, l::Scope::Always, true),
    ]
    .into_iter()
    .enumerate()
    .map(|(i, (kind, scope, genesis))| l::Law {
        id: i as u32 + 1,
        kind,
        scope,
        genesis,
        program: l::Program {
            nodes: &nodes,
            root: 0,
        },
    })
    .collect::<Vec<_>>();
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
    let d = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Leaf(&context),
        program,
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &[],
        channels: &channels,
        laws: &laws,
        required: &[1, 2, 3, 4, 5],
        limits,
    };
    let f = Framing {
        state: FrameBinding {
            root: 0,
            schema: [7; 32],
            max_bytes: 1000,
        },
        command: FrameBinding {
            root: 2,
            schema: [8; 32],
            max_bytes: 1000,
        },
        context: FrameBinding {
            root: 3,
            schema: [9; 32],
            max_bytes: 1000,
        },
    };
    run(&description, &d, &f, &[(u32::MAX, 5, 6)]);
}
fn copy<'a>(d: &Descriptor<'a>) -> Descriptor<'a> {
    Descriptor {
        state: d.state,
        command: d.command,
        context: d.context,
        program: ScalarProgram {
            inputs: d.program.inputs,
            outputs: d.program.outputs,
            nodes: d.program.nodes,
            roots: d.program.roots,
        },
        bindings: d.bindings,
        output_types: d.output_types,
        decision_output: d.decision_output,
        branches: d.branches,
        reasons: d.reasons,
        channels: d.channels,
        laws: d.laws,
        required: d.required,
        limits: d.limits,
    }
}
#[test]
fn original_wire_generic_ids_roots_and_private_borrowed_custody() {
    fixture(|description, d, f, links| {
        let bytes = wire(description);
        let checked = schema::admit(&bytes, description, schema_limits()).unwrap_or_else(|error| panic!("original_wire_generic_ids_roots_and_private_borrowed_custody fixture failed: {error:?}"));
        let policy = authority::policy_bytes(d, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in original_wire_generic_ids_roots_and_private_borrowed_custody"));
        let bound = bind(&checked, &policy, d, f, links, policy.len() as u64).unwrap_or_else(|error| panic!("original_wire_generic_ids_roots_and_private_borrowed_custody fixture failed: {error:?}"));
        assert!(core::ptr::eq(bound.original_schema(), bytes.as_slice()));
        assert!(core::ptr::eq(bound.original_contract(), policy.as_slice()));
        assert!(core::ptr::eq(bound.descriptor(), d));
        assert!(core::ptr::eq(bound.framing(), f));
        assert!(core::ptr::eq(bound.channel_roots(), links));
        assert_eq!(bound.roots(), (0, 2, 3));
        assert_eq!(
            bind(&checked, &policy, d, f, links, policy.len() as u64 - 1).err().unwrap_or_else(|| panic!("expected refusal in original_wire_generic_ids_roots_and_private_borrowed_custody")),
            Failure::Size
        );
        for i in 0..policy.len() {
            let mut wrong = policy.clone();
            wrong[i] ^= 1;
            assert_eq!(
                bind(&checked, &wrong, d, f, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in original_wire_generic_ids_roots_and_private_borrowed_custody")),
                Failure::Policy
            );
        }
        let mut trailing = policy.clone();
        trailing.push(0);
        assert_eq!(
            bind(&checked, &trailing, d, f, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in original_wire_generic_ids_roots_and_private_borrowed_custody")),
            Failure::Policy
        );
        for end in 0..bytes.len() {
            assert!(schema::admit(&bytes[..end], description, schema_limits()).is_err());
        }
    });
}
#[test]
fn complete_original_bounds_and_unused_definitions_remain_bound() {
    fixture(|description, d, f, links| {
        let bytes = wire(description);
        let policy = authority::policy_bytes(d, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in complete_original_bounds_and_unused_definitions_remain_bound"));
        for (index, kind, expected) in [
            (1, Kind::I128 { min: -5, max: 6 }, Failure::Roots),
            (
                1,
                Kind::I128 {
                    min: i128::MIN,
                    max: i128::MAX,
                },
                Failure::Roots,
            ),
            (
                7,
                Kind::I128 {
                    min: i128::MIN,
                    max: i128::MAX - 1,
                },
                Failure::Policy,
            ),
        ] {
            let mut defs = description.definitions.to_vec();
            defs[index].kind = kind;
            let changed = Description {
                definitions: &defs,
                ..*description
            };
            let bytes = wire(&changed);
            let checked = schema::admit(&bytes, &changed, schema_limits()).unwrap_or_else(|error| panic!("complete_original_bounds_and_unused_definitions_remain_bound fixture failed: {error:?}"));
            assert_eq!(
                bind(&checked, &policy, d, f, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in complete_original_bounds_and_unused_definitions_remain_bound")),
                expected
            );
        }
    });
}
#[test]
fn independent_roots_variant_sets_and_full_links_are_required() {
    fixture(|description, d, f, links| {
        let bytes = wire(description);
        let checked = schema::admit(&bytes, description, schema_limits()).unwrap_or_else(|error| panic!("independent_roots_variant_sets_and_full_links_are_required fixture failed: {error:?}"));
        let policy = authority::policy_bytes(d, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in independent_roots_variant_sets_and_full_links_are_required"));
        for source in 0..3 {
            let mut changed = *f;
            match source {
                0 => changed.state.root = 1,
                1 => changed.command.root = 3,
                _ => changed.context.root = 2,
            };
            assert_eq!(
                bind(&checked, &policy, d, &changed, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in independent_roots_variant_sets_and_full_links_are_required")),
                Failure::Roots
            );
        }
        // An equally shaped record at another type ID cannot replace the
        // original declared state root, even with a freshly matching policy.
        let mut alternate_defs = description.definitions.to_vec();
        let last = alternate_defs.len() - 1;
        alternate_defs[last].kind = alternate_defs[0].kind;
        let alternate_description = Description {
            definitions: &alternate_defs,
            ..*description
        };
        let alternate_bytes = wire(&alternate_description);
        let alternate_checked =
            schema::admit(&alternate_bytes, &alternate_description, schema_limits()).unwrap_or_else(|error| panic!("independent_roots_variant_sets_and_full_links_are_required fixture failed: {error:?}"));
        let alternate_frame = Framing {
            state: FrameBinding {
                root: u32::MAX,
                ..f.state
            },
            ..*f
        };
        let alternate_policy =
            authority::policy_bytes(d, &alternate_bytes, &alternate_frame, links).unwrap_or_else(|| panic!("missing fixture value in independent_roots_variant_sets_and_full_links_are_required"));
        assert_eq!(
            bind(
                &alternate_checked,
                &alternate_policy,
                d,
                &alternate_frame,
                links,
                u64::MAX,
            )
            .err()
            .unwrap_or_else(|| panic!(
                "expected refusal in independent_roots_variant_sets_and_full_links_are_required"
            )),
            Failure::Roots
        );
        for changed in [
            vec![],
            vec![(u32::MAX, 5, 6), (u32::MAX, 5, 6)],
            vec![(1, 5, 6)],
            vec![(u32::MAX, 4, 6)],
            vec![(u32::MAX, 5, 0)],
        ] {
            assert_eq!(
                bind(&checked, &policy, d, f, &changed, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in independent_roots_variant_sets_and_full_links_are_required")),
                Failure::Channels
            );
        }
        let command = InputLeaf::Sum {
            type_id: 2,
            min: -2,
            max: -1,
            variants: vec![
                InputVariant { id: 1, code: -2 },
                InputVariant {
                    id: u16::MAX,
                    code: -1,
                },
            ],
        };
        let changed = Descriptor {
            command: Schema::Leaf(&command),
            ..copy(d)
        };
        assert_eq!(
            bind(&checked, &policy, &changed, f, links, u64::MAX)
                .err()
                .unwrap_or_else(|| panic!(
                    "expected refusal in independent_roots_variant_sets_and_full_links_are_required"
                )),
            Failure::Roots
        );
        let bindings = [
            Binding {
                source: Source::State,
                selector: Selector::Field(0),
            },
            Binding {
                source: Source::Command,
                selector: Selector::Field(0),
            },
            Binding {
                source: Source::Context,
                selector: Selector::Root,
            },
        ];
        let alias = Descriptor {
            bindings: &bindings,
            ..copy(d)
        };
        assert_eq!(
            bind(&checked, &policy, &alias, f, links, u64::MAX)
                .err()
                .unwrap_or_else(|| panic!(
                    "expected refusal in independent_roots_variant_sets_and_full_links_are_required"
                )),
            Failure::Descriptor
        );
    });
}
#[test]
fn every_unused_branch_and_disabled_plan_keeps_original_length_bounds() {
    fixture(|description, d, f, links| {
        let bytes = wire(description);
        let checked = schema::admit(&bytes, description, schema_limits()).unwrap_or_else(|error| panic!("every_unused_branch_and_disabled_plan_keeps_original_length_bounds fixture failed: {error:?}"));
        for raw in [b"".as_slice(), b"toolong".as_slice()] {
            let fields = [PayloadField {
                field: u16::MAX,
                value: Expr::Constant(Atom::Bytes(raw)),
            }];
            let plans = [DeliveryPlan {
                payload: &fields,
                ..d.branches[1].effects[0]
            }];
            let branches = [
                d.branches[0],
                Branch {
                    effects: &plans,
                    ..d.branches[1]
                },
            ];
            let changed = Descriptor {
                branches: &branches,
                ..copy(d)
            };
            let policy = authority::policy_bytes(&changed, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in every_unused_branch_and_disabled_plan_keeps_original_length_bounds"));
            assert_eq!(
                bind(&checked, &policy, &changed, f, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in every_unused_branch_and_disabled_plan_keeps_original_length_bounds")),
                Failure::Deliveries
            );
        }
        for raw in [b"".as_slice(), b"long".as_slice(), &[0x80]] {
            let plans = [DeliveryPlan {
                destination: Expr::Constant(Atom::Text(raw)),
                ..d.branches[1].outbox[0]
            }];
            let branches = [
                d.branches[0],
                Branch {
                    outbox: &plans,
                    ..d.branches[1]
                },
            ];
            let changed = Descriptor {
                branches: &branches,
                ..copy(d)
            };
            let policy = authority::policy_bytes(&changed, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in every_unused_branch_and_disabled_plan_keeps_original_length_bounds"));
            let expected = if raw == [0x80] {
                Failure::Descriptor
            } else {
                Failure::Deliveries
            };
            assert_eq!(
                bind(&checked, &policy, &changed, f, links, u64::MAX).err().unwrap_or_else(|| panic!("expected refusal in every_unused_branch_and_disabled_plan_keeps_original_length_bounds")),
                expected
            );
        }
    });
}

// Independent fixture policy encoder: explicit wire fields and standard primitive
// conversions. It does not call any production policy projection or serializer.
struct Wire(Vec<u8>);
impl Wire {
    fn words(&mut self, x: &[u128]) {
        for n in x {
            self.0.push(0);
            self.0.extend(n.to_be_bytes());
        }
    }
    fn bytes(&mut self, x: &[u8]) {
        self.0.push(1);
        self.0.extend((x.len() as u128).to_be_bytes());
        self.0.extend(x);
    }
}
fn expected_fixture_policy(schema: &[u8], f: &Framing) -> Vec<u8> {
    let mut framed = Wire(Vec::new());
    framed.words(&[0x5a465232, 1]);
    for frame in [&f.state, &f.command, &f.context] {
        framed.words(&[frame.root as u128]);
        framed.bytes(&frame.schema);
        framed.words(&[frame.max_bytes as u128]);
    }
    let mut w = Wire(Vec::new());
    w.words(&[0x5a504f32, 1]);
    w.bytes(schema);
    w.bytes(&framed.0);
    let neg5 = (-5i128) as u128;
    let neg2 = (-2i128) as u128;
    let neg1 = (-1i128) as u128;
    let max16 = u16::MAX as u128;
    let max32 = u32::MAX as u128;
    w.words(&[1, max32, 5, 6]); // complete links
    w.words(&[1, 1, 0, 0, neg5, 5]); // state schema
    w.words(&[0, 3, 2, neg2, neg1, 2, max16, neg2, 0, neg1, 0, 1]); // original Sum command and Bool context
    w.words(&[3, 1, neg5, 5, 1, neg2, neg1, 0, 1, 1, 0, 1, 1, 1, 0, 1, 0]); // graph domains, node, output root
    w.words(&[3, 0, 1, 0, 1, 0, 2, 0, 1, 0, 0, 1, 0]); // complete bindings, output type, branch output
    w.words(&[2]);
    for code in [0, 1] {
        w.words(&[code, 0, 0, 1, 0, 0, 0, 0, 1, neg5, 5]);
        for _ in 0..2 {
            w.words(&[code]);
            if code == 1 {
                w.words(&[0, max32, 2, 0, 0, 2, 6]);
                w.bytes(b"dst");
                w.words(&[1, max16, 2, 5]);
                w.bytes(b"abc");
                w.words(&[2, 2, u128::MAX]);
            }
        }
    }
    w.words(&[0, 1, max32, 6, 1, max16, 5, 2, 0, u128::MAX]); // reasons and channel
    w.words(&[5]);
    for (id, kind, scope, genesis) in [
        (1, 0, 4, 1),
        (2, 6, 2, 0),
        (3, 7, 3, 0),
        (4, 8, 0, 0),
        (5, 9, 0, 1),
    ] {
        w.words(&[id, kind, scope, genesis, 1, 0, 0, 1, 0]);
    }
    w.words(&[5, 1, 2, 3, 4, 5]);
    w.words(&[u64::MAX as u128; 8]);
    w.0
}
#[test]
fn independently_encoded_policy_covers_all_fixture_fields() {
    fixture(|description, d, f, links| {
        let bytes = wire(description);
        let checked = schema::admit(&bytes, description, schema_limits()).unwrap_or_else(|error| {
            panic!(
                "independently_encoded_policy_covers_all_fixture_fields fixture failed: {error:?}"
            )
        });
        let expected = expected_fixture_policy(&bytes, f);
        assert_eq!(
            authority::policy_bytes(d, &bytes, f, links).unwrap_or_else(|| panic!(
                "missing fixture value in independently_encoded_policy_covers_all_fixture_fields"
            )),
            expected
        );
        assert!(bind(&checked, &expected, d, f, links, u64::MAX).is_ok());
    });
}
#[test]
fn empty_state_records_and_permuted_complete_variant_codes_are_legal() {
    fixture(|description, d, f, links| {
        let mut defs = description.definitions.to_vec();
        defs[0].kind = Kind::Record(&[]);
        let description = Description {
            definitions: &defs,
            ..*description
        };
        let bytes = wire(&description);
        let checked = schema::admit(&bytes, &description, schema_limits()).unwrap_or_else(|error| panic!("empty_state_records_and_permuted_complete_variant_codes_are_legal fixture failed: {error:?}"));
        let branches = d
            .branches
            .iter()
            .map(|b| Branch {
                assignments: &[],
                ..*b
            })
            .collect::<Vec<_>>();
        let d = Descriptor {
            state: Schema::Record(&[]),
            bindings: &d.bindings[1..],
            program: ScalarProgram {
                inputs: &d.program.inputs[1..],
                outputs: d.program.outputs,
                nodes: d.program.nodes,
                roots: d.program.roots,
            },
            branches: &branches,
            ..copy(d)
        };
        for entries in [
            vec![
                InputVariant { id: 0, code: -2 },
                InputVariant {
                    id: u16::MAX,
                    code: -1,
                },
            ],
            vec![
                InputVariant {
                    id: u16::MAX,
                    code: -1,
                },
                InputVariant { id: 0, code: -2 },
            ],
        ] {
            let command = InputLeaf::Sum {
                type_id: 2,
                min: -2,
                max: -1,
                variants: entries,
            };
            let changed = Descriptor {
                command: Schema::Leaf(&command),
                ..copy(&d)
            };
            let policy = authority::policy_bytes(&changed, &bytes, f, links).unwrap_or_else(|| panic!("missing fixture value in empty_state_records_and_permuted_complete_variant_codes_are_legal"));
            assert!(bind(&checked, &policy, &changed, f, links, u64::MAX).is_ok());
        }
    });
}

#[test]
fn exact_channel_numeric_and_enum_sum_domains() {
    let variants = [
        Variant { id: 0, name: b"A" },
        Variant {
            id: u16::MAX,
            name: b"Z",
        },
    ];
    let payload = [Field {
        id: 0,
        name: b"Value",
        type_id: 1,
    }];
    for kind in [
        Kind::U128 {
            min: 0,
            max: u128::MAX,
        },
        Kind::I128 {
            min: i128::MIN,
            max: i128::MAX,
        },
        Kind::Enum(&variants),
        Kind::Sum(&variants),
    ] {
        let defs = [
            Definition {
                id: 0,
                name: b"Payload",
                kind: Kind::Record(&payload),
            },
            Definition {
                id: 1,
                name: b"Value",
                kind,
            },
        ];
        let correct = match kind {
            Kind::U128 { min, max } => Domain::U128 { min, max },
            Kind::I128 { min, max } => Domain::I128 { min, max },
            Kind::Enum(_) => Domain::Enum {
                type_id: 1,
                variants: &[u16::MAX, 0],
            },
            _ => Domain::Sum {
                type_id: 1,
                variants: &[u16::MAX, 0],
            },
        };
        let wrong = match kind {
            Kind::U128 { min, max } => Domain::U128 { min, max: max - 1 },
            Kind::I128 { min, max } => Domain::I128 { min: min + 1, max },
            Kind::Enum(_) => Domain::Enum {
                type_id: 1,
                variants: &[0, 1],
            },
            _ => Domain::Enum {
                type_id: 1,
                variants: &[u16::MAX, 0],
            },
        };
        let fields = [TypedField {
            field: 0,
            domain: correct,
        }];
        let links = [(1, 1, 0)];
        let channels = [Channel {
            id: 1,
            destination: correct,
            payload: &fields,
            idempotency: Domain::Bool,
        }];
        assert!(matching::channels(&defs, &channels, &links));
        let changed = [Channel {
            destination: wrong,
            ..channels[0]
        }];
        assert!(!matching::channels(&defs, &changed, &links));
        let fields = [TypedField {
            field: 0,
            domain: wrong,
        }];
        let changed = [Channel {
            payload: &fields,
            ..channels[0]
        }];
        assert!(!matching::channels(&defs, &changed, &links));
    }
}

// Test compatibility adapter exercises the sole public constructor in every case.
fn bind<'a>(
    checked: &schema::Checked<'a>,
    contract: &'a [u8],
    d: &'a Descriptor<'a>,
    f: &'a Framing,
    links: &'a [(u32, u32, u32)],
    max: u64,
) -> Result<BoundCatalog<'a>, Failure> {
    bind_original(
        checked.original(),
        checked.description(),
        Limits {
            schema: schema_limits(),
            contract_bytes: max,
        },
        contract,
        d,
        f,
        links,
    )
}
#[test]
fn public_original_constructor_proves_schema_admission_before_correspondence() {
    fixture(|description, d, f, links| {
        let original = wire(description);
        let policy = expected_fixture_policy(&original, f);
        for end in 0..original.len() {
            assert_eq!(
                bind_original(
                    &original[..end],
                    description,
                    Limits {
                        schema: schema_limits(),
                        contract_bytes: u64::MAX
                    },
                    &policy,
                    d,
                    f,
                    links,
                )
                .err().unwrap_or_else(|| panic!("expected refusal in public_original_constructor_proves_schema_admission_before_correspondence")),
                Failure::Schema(schema::Failure::Encoding)
            );
        }
        let mut changed = description.definitions.to_vec();
        changed[0].name = b"";
        let bad = Description {
            definitions: &changed,
            ..*description
        };
        let raw = wire(&bad);
        assert_eq!(
            bind_original(
                &raw,
                &bad,
                Limits {
                    schema: schema_limits(),
                    contract_bytes: u64::MAX
                },
                &policy,
                d,
                f,
                links
            )
            .err().unwrap_or_else(|| panic!("expected refusal in public_original_constructor_proves_schema_admission_before_correspondence")),
            Failure::Schema(schema::Failure::Metadata)
        );
    });
}
