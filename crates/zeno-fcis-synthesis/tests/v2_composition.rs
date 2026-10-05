//! Original-wire integration oracles. Fixture graphs are subjects, never expected outputs.
#[path = "../../zeno-fcis-cli/templates/account-lockout/src/v2_contract.rs"]
#[allow(dead_code, unreachable_pub)]
mod account_contract;
use c::{
    Assignment, Atom, Binding, Branch, Channel, Class, DeliveryPlan, Descriptor, Domain, Expr,
    Field, PayloadField, Raw, Reason, Schema, Selector, Source, TypedField,
};
use zeno_fcis_synthesis::finite::{
    self, Domain as ScalarDomain, Op as ScalarOp, Program as ScalarOwned,
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2InputVariant as Variant,
    V2Resource as Resource, V2ScalarProgram as ScalarProgram, v2_composition as c, v2_laws as l,
};

fn descriptor_copy<'a>(d: &Descriptor<'a>) -> Descriptor<'a> {
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
fn limits() -> finite::V2Limits {
    let mut x = finite::v2_zero_limits();
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
        x = x.with_limit(r, u64::MAX);
    }
    x
}
fn leaf(type_id: u32, ids: &[u16], sum: bool) -> InputLeaf {
    let variants = ids
        .iter()
        .enumerate()
        .map(|(code, &id)| Variant {
            id,
            code: code as i64,
        })
        .collect();
    if sum {
        InputLeaf::Sum {
            type_id,
            min: 0,
            max: ids.len() as i64 - 1,
            variants,
        }
    } else {
        InputLeaf::Enum {
            type_id,
            min: 0,
            max: ids.len() as i64 - 1,
            variants,
        }
    }
}
fn int(id: u16, min: i64, max: i64) -> InputField {
    InputField {
        id,
        leaf: InputLeaf::I128 { min, max },
    }
}
fn atom_bytes(a: Atom<'_>) -> Vec<u8> {
    match a {
        Atom::Bool(v) => vec![if v { 2 } else { 1 }],
        Atom::I128(v) => {
            let mut b = vec![4];
            b.extend(v.to_be_bytes());
            b
        }
        Atom::U128(v) => {
            let mut b = vec![3];
            b.extend(v.to_be_bytes());
            b
        }
        Atom::Enum { type_id, variant } | Atom::Sum { type_id, variant } => {
            let mut b = vec![if matches!(a, Atom::Sum { .. }) { 10 } else { 7 }];
            b.extend(type_id.to_be_bytes());
            b.extend(variant.to_be_bytes());
            if matches!(a, Atom::Sum { .. }) {
                b.push(0);
            }
            b
        }
        _ => panic!("not an admitted test input leaf"),
    }
}
fn record(fields: &[Field<'_>]) -> Vec<u8> {
    let mut b = vec![9];
    b.extend((fields.len() as u32).to_be_bytes());
    for f in fields {
        b.extend(f.id.to_be_bytes());
        b.extend(atom_bytes(f.value));
    }
    b
}
fn fi(id: u16, v: i128) -> Field<'static> {
    Field {
        id,
        value: Atom::I128(v),
    }
}
fn fe(id: u16, t: u32, v: u16, sum: bool) -> Field<'static> {
    Field {
        id,
        value: if sum {
            Atom::Sum {
                type_id: t,
                variant: v,
            }
        } else {
            Atom::Enum {
                type_id: t,
                variant: v,
            }
        },
    }
}
fn assert_original_sum_field(bytes: &[u8], field_id: u16, type_id: u32, variant: u16) {
    use zeno_fcis_value::ValueRef;
    let decoded = zeno_fcis_codec::decode_value(bytes, Default::default())
        .unwrap_or_else(|error| panic!("original template record must decode: {error:?}"));
    let ValueRef::Record(fields) = decoded.view() else {
        panic!("original template wire must be a record");
    };
    let field = fields
        .iter()
        .find(|field| field.id() == field_id)
        .unwrap_or_else(|| panic!("original template field must exist"));
    assert!(
        matches!(field.value().view(), ValueRef::Sum {
        type_id: actual_type, variant: actual_variant, payload: None,
    } if actual_type == type_id && actual_variant == variant),
        "project data type must use the original payload-free sum wire"
    );
}
fn bind(source: Source, id: u16) -> Binding {
    Binding {
        source,
        selector: Selector::Field(id),
    }
}
fn ad(field: u16, output: usize, min: i128, max: i128) -> Assignment<'static> {
    Assignment {
        field,
        value: Expr::Output(output),
        domain: Domain::I128 { min, max },
    }
}
fn branch<'a>(
    code: i128,
    class: Class,
    reason: Option<u32>,
    assignments: &'a [Assignment<'a>],
    outbox: &'a [DeliveryPlan<'a>],
) -> Branch<'a> {
    Branch {
        code,
        class,
        reason,
        assignments,
        effects: &[],
        outbox,
    }
}
fn template_graph(bytes: &[u8]) -> ScalarOwned {
    use zeno_fcis_value::{Value, ValueRef};
    fn tuple(v: &Value) -> &[Value] {
        let ValueRef::Tuple(x) = v.view() else {
            panic!("artifact tuple")
        };
        x
    }
    fn number(v: &Value) -> i64 {
        let ValueRef::I128(x) = v.view() else {
            panic!("artifact integer")
        };
        i64::try_from(x)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"))
    }
    fn domains(v: &Value) -> Vec<ScalarDomain> {
        tuple(v)
            .iter()
            .map(|v| {
                let a = tuple(v);
                if a[0] == Value::boolean(true) {
                    ScalarDomain::Bool
                } else {
                    ScalarDomain::Int {
                        min: number(&a[1]),
                        max: number(&a[2]),
                    }
                }
            })
            .collect()
    }
    let value = zeno_fcis_codec::decode_value(bytes, Default::default())
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
    let v = tuple(&value);
    assert_eq!(v[0].view(), ValueRef::Text(finite::PROFILE));
    let schema = tuple(&v[1]);
    let nodes = tuple(&v[2])
        .iter()
        .map(|v| {
            let a = tuple(v);
            let p = |i: usize| {
                u16::try_from(number(&a[i])).unwrap_or_else(|error| {
                    panic!("unexpected composition oracle refusal: {error:?}")
                })
            };
            match number(&a[0]) {
                0 => ScalarOp::Input(p(1)),
                1 => ScalarOp::Int(number(&a[1])),
                2 => ScalarOp::Bool(number(&a[1]) == 1),
                3 => ScalarOp::Add(p(1), p(2)),
                4 => ScalarOp::Sub(p(1), p(2)),
                5 => ScalarOp::Eq(p(1), p(2)),
                6 => ScalarOp::Lt(p(1), p(2)),
                7 => ScalarOp::And(p(1), p(2)),
                8 => ScalarOp::Not(p(1)),
                9 => ScalarOp::Select(p(1), p(2), p(3)),
                _ => panic!("artifact op"),
            }
        })
        .collect();
    let roots = tuple(&v[3])
        .iter()
        .map(|v| {
            let ValueRef::U128(x) = v.view() else {
                panic!("artifact root")
            };
            u16::try_from(x)
                .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"))
        })
        .collect();
    ScalarOwned::try_new(domains(&schema[0]), domains(&schema[1]), nodes, roots)
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"))
}
#[derive(Default)]
struct Predicate {
    nodes: Vec<l::Op<'static>>,
}
impl Predicate {
    fn push(&mut self, n: l::Op<'static>) -> usize {
        let i = self.nodes.len();
        self.nodes.push(n);
        i
    }
    fn num(&mut self, o: l::Observation) -> usize {
        let i = self.push(l::Op::Observe(o));
        self.push(l::Op::ToI128(i))
    }
    fn n(&mut self, n: i128) -> usize {
        self.push(l::Op::Literal(l::Atom::I128(n)))
    }
    fn eq(&mut self, a: usize, b: usize) -> usize {
        self.push(l::Op::Eq(a, b))
    }
    fn and(&mut self, a: usize, b: usize) -> usize {
        self.push(l::Op::And(a, b))
    }
    fn not(&mut self, a: usize) -> usize {
        self.push(l::Op::Not(a))
    }
    fn implies(&mut self, a: usize, b: usize) -> usize {
        let b = self.not(b);
        let both = self.and(a, b);
        self.not(both)
    }
    fn le(&mut self, a: usize, b: usize) -> usize {
        let x = self.push(l::Op::Lt(b, a));
        self.not(x)
    }
    fn add(&mut self, a: usize, b: usize) -> usize {
        self.push(l::Op::Add(a, b))
    }
    fn all(&mut self, values: &[usize]) -> usize {
        values
            .iter()
            .copied()
            .reduce(|a, b| self.and(a, b))
            .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
    }
    fn equal_number(&mut self, o: l::Observation, n: i128) -> usize {
        let a = self.num(o);
        let n = self.n(n);
        self.eq(a, n)
    }
    fn same(&mut self, a: l::Observation, b: l::Observation) -> usize {
        let a = self.num(a);
        let b = self.num(b);
        self.eq(a, b)
    }
    fn exact(&mut self, o: l::Observation, v: l::Atom<'static>) -> usize {
        let a = self.push(l::Op::Observe(o));
        let b = self.push(l::Op::Literal(v));
        self.eq(a, b)
    }
}
struct LawData {
    id: u32,
    kind: l::Kind,
    scope: l::Scope,
    genesis: bool,
    nodes: Vec<l::Op<'static>>,
    root: usize,
}
fn ld(
    id: u32,
    kind: l::Kind,
    scope: l::Scope,
    genesis: bool,
    p: Predicate,
    root: usize,
) -> LawData {
    LawData {
        id,
        kind,
        scope,
        genesis,
        nodes: p.nodes,
        root,
    }
}
fn law_refs(data: &[LawData]) -> Vec<l::Law<'_>> {
    data.iter()
        .map(|x| l::Law {
            id: x.id,
            kind: x.kind,
            scope: x.scope,
            genesis: x.genesis,
            program: l::Program {
                nodes: &x.nodes,
                root: x.root,
            },
        })
        .collect()
}
fn law_copy<'a>(law: &l::Law<'a>) -> l::Law<'a> {
    l::Law {
        id: law.id,
        kind: law.kind,
        scope: law.scope,
        genesis: law.genesis,
        program: l::Program {
            nodes: law.program.nodes,
            root: law.program.root,
        },
    }
}
fn tiny_laws() -> Vec<LawData> {
    use l::Observation as O;
    use l::{Kind as K, Scope as S};
    let mut data = Vec::new();
    for (id, kind, scope, genesis) in [
        (10, K::StateInvariant, S::Committing, true),
        (20, K::RejectNoAuthority, S::Reject, false),
        (30, K::CommittedFailureEffects, S::CommittedFailure, false),
        (40, K::DecisionConformance, S::Always, false),
    ] {
        let mut p = Predicate::default();
        let root = p.push(l::Op::Literal(l::Atom::Bool(true)));
        data.push(ld(id, kind, scope, genesis, p, root));
    }
    let mut p = Predicate::default();
    let root = p.exact(
        O::Initial(0),
        l::Atom::Enum {
            type_id: 700,
            variant: 71,
        },
    );
    data.push(ld(50, K::InitialCondition, S::Always, true, p, root));
    let mut p = Predicate::default();
    let mut clauses = Vec::new();
    for (o, a) in [
        (
            O::Pre(0),
            l::Atom::Enum {
                type_id: 700,
                variant: 71,
            },
        ),
        (
            O::CommandRoot,
            l::Atom::Sum {
                type_id: 800,
                variant: 81,
            },
        ),
        (O::ContextRoot, l::Atom::Bool(true)),
        (
            O::Post(0),
            l::Atom::Enum {
                type_id: 700,
                variant: 72,
            },
        ),
        (
            O::PatchBefore(0),
            l::Atom::Enum {
                type_id: 700,
                variant: 71,
            },
        ),
        (
            O::PatchAfter(0),
            l::Atom::Enum {
                type_id: 700,
                variant: 72,
            },
        ),
        (O::EffectDestination(0), l::Atom::Text(b"recipient")),
        (O::OutboxDestination(0), l::Atom::Text(b"recipient")),
        (
            O::EffectPayload(0, 0),
            l::Atom::Sum {
                type_id: 800,
                variant: 81,
            },
        ),
        (
            O::OutboxPayload(0, 1),
            l::Atom::Enum {
                type_id: 700,
                variant: 71,
            },
        ),
        (O::OutboxPayload(0, 2), l::Atom::Bytes(&[0xff, 0x80])),
        (O::EffectIdempotency(0), l::Atom::U128(u128::MAX)),
        (O::OutboxIdempotency(0), l::Atom::U128(u128::MAX)),
    ] {
        clauses.push(p.exact(o, a));
    }
    for (o, n) in [
        (O::EffectOrdinal(0), 9),
        (O::OutboxOrdinal(0), u32::MAX as i128),
        (O::EffectChannel(0), 300),
        (O::OutboxChannel(0), 300),
        (O::ReadLength, 3),
        (O::ReadSource(0), 0),
        (O::ReadId(0), 0),
        (O::ReadSource(1), 1),
        (O::ReadId(1), 65536),
        (O::ReadSource(2), 2),
        (O::ReadId(2), 65536),
        (O::WriteLength, 2),
        (O::WriteSource(0), 0),
        (O::WriteId(0), 0),
        (O::WriteSource(1), 1),
        (O::WriteId(1), 0),
        (O::EffectAttemptLength, 2),
        (O::EffectAttemptSource(0), 2),
        (O::EffectAttemptId(0), 9),
        (O::EffectAttemptSource(1), 3),
        (O::EffectAttemptId(1), u32::MAX as i128),
        (O::Usage(Resource::Candidate), 1),
        (O::Usage(Resource::Write), 1),
        (O::Usage(Resource::Effect), 2),
        (O::Usage(Resource::Step), 5),
    ] {
        clauses.push(p.equal_number(o, n));
    }
    let root = p.all(&clauses);
    data.push(ld(
        60,
        K::AuthoritySubjectRecipient,
        S::Accept,
        false,
        p,
        root,
    ));
    data
}
fn with_tiny(run: impl FnOnce(&Descriptor<'_>)) {
    const STATE_IDS: [u16; 2] = [71, 72];
    const CMD_IDS: [u16; 2] = [81, 82];
    let state = [InputField {
        id: 0,
        leaf: InputLeaf::Enum {
            type_id: 700,
            min: 0,
            max: 1,
            variants: vec![Variant { id: 72, code: 1 }, Variant { id: 71, code: 0 }],
        },
    }];
    let command = InputLeaf::Sum {
        type_id: 800,
        min: 0,
        max: 1,
        variants: vec![Variant { id: 82, code: 1 }, Variant { id: 81, code: 0 }],
    };
    let context = InputLeaf::Bool;
    let inputs = [
        ScalarDomain::Bool,
        ScalarDomain::Int { min: 0, max: 1 },
        ScalarDomain::Int { min: 0, max: 1 },
    ];
    let scalar_outputs = [
        ScalarDomain::Int { min: 0, max: 0 },
        ScalarDomain::Int { min: 0, max: 1 },
    ];
    let nodes = [
        ScalarOp::Int(0),
        ScalarOp::Int(1),
        ScalarOp::Input(0),
        ScalarOp::Input(1),
        ScalarOp::Input(2),
    ];
    let roots = [0, 1];
    let output_types = [
        InputLeaf::I128 { min: 0, max: 0 },
        InputLeaf::Enum {
            type_id: 700,
            min: 0,
            max: 1,
            variants: vec![Variant { id: 72, code: 1 }, Variant { id: 71, code: 0 }],
        },
    ];
    let bindings = [
        Binding {
            source: Source::Context,
            selector: Selector::Root,
        },
        bind(Source::State, 0),
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
    ];
    let assignments = [Assignment {
        field: 0,
        value: Expr::Output(1),
        domain: Domain::Enum {
            type_id: 700,
            variants: &STATE_IDS,
        },
    }];
    let payload = [
        PayloadField {
            field: 0,
            value: Expr::Root(Source::Command),
        },
        PayloadField {
            field: 1,
            value: Expr::Input(Source::State, 0),
        },
        PayloadField {
            field: 2,
            value: Expr::Constant(Atom::Bytes(&[0xff, 0x80])),
        },
    ];
    let make = |ordinal| DeliveryPlan {
        ordinal,
        channel: 300,
        when: Expr::Root(Source::Context),
        destination: Expr::Constant(Atom::Text(b"recipient")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(u128::MAX)),
    };
    let effects = [make(9)];
    let outbox = [make(u32::MAX)];
    let branches = [Branch {
        code: 0,
        class: Class::Accept,
        reason: None,
        assignments: &assignments,
        effects: &effects,
        outbox: &outbox,
    }];
    let fields = [
        TypedField {
            field: 0,
            domain: Domain::Sum {
                type_id: 800,
                variants: &CMD_IDS,
            },
        },
        TypedField {
            field: 1,
            domain: Domain::Enum {
                type_id: 700,
                variants: &STATE_IDS,
            },
        },
        TypedField {
            field: 2,
            domain: Domain::Bytes,
        },
    ];
    let channels = [Channel {
        id: 300,
        destination: Domain::Text,
        payload: &fields,
        idempotency: Domain::U128 {
            min: 0,
            max: u128::MAX,
        },
    }];
    let data = tiny_laws();
    let laws = law_refs(&data);
    let required = [10, 20, 30, 40, 50, 60];
    let descriptor = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Leaf(&context),
        program: ScalarProgram {
            inputs: &inputs,
            outputs: &scalar_outputs,
            nodes: &nodes,
            roots: &roots,
        },
        bindings: &bindings,
        output_types: &output_types,
        decision_output: 0,
        branches: &branches,
        reasons: &[],
        channels: &channels,
        laws: &laws,
        required: &required,
        limits: limits(),
    };
    run(&descriptor)
}
fn tiny_wire() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        record(&[fe(0, 700, 71, false)]),
        atom_bytes(Atom::Sum {
            type_id: 800,
            variant: 81,
        }),
        atom_bytes(Atom::Bool(true)),
    )
}
#[test]
fn permuted_maps_scalar_roots_and_complete_law_frame() {
    with_tiny(|d| {
        let core = c::bind(d)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let (s, cmd, x) = tiny_wire();
        let out = core.execute(Raw {
            state: &s,
            command: &cmd,
            context: &x,
        });
        let candidate = out
            .result()
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        assert_eq!(candidate.pre(), [fe(0, 700, 71, false)]);
        assert_eq!(candidate.post(), [fe(0, 700, 72, false)]);
        assert_eq!(
            candidate.patch()[0].before,
            Atom::Enum {
                type_id: 700,
                variant: 71
            }
        );
        assert_eq!(
            candidate.patch()[0].after,
            Atom::Enum {
                type_id: 700,
                variant: 72
            }
        );
        assert_eq!(candidate.effects()[0].ordinal, 9);
        assert_eq!(candidate.outbox()[0].ordinal, u32::MAX);
        assert_eq!(
            candidate.outbox()[0].payload,
            [
                fe(0, 800, 81, true),
                fe(1, 700, 71, false),
                Field {
                    id: 2,
                    value: Atom::Bytes(&[0xff, 0x80])
                }
            ]
        );
        assert_eq!(
            out.reads()
                .iter()
                .map(|a| (a.source, a.selector, a.permitted))
                .collect::<Vec<_>>(),
            [
                (0, Selector::Field(0), true),
                (1, Selector::Root, true),
                (2, Selector::Root, true)
            ]
        );
        assert_eq!(
            out.diagnostics()
                .last()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .verdict,
            l::Verdict::Satisfied
        );
        assert!(matches!(core.genesis(&s).result(), Ok(c) if c.class() == Class::Accept));
        let changed = record(&[fe(0, 700, 72, false)]);
        assert!(matches!(
            core.genesis(&changed).result(),
            Err(c::Failure::Law(l::Failure::Violated))
        ));
    });
}
#[test]
fn malformed_roots_refuse_with_exact_ingress_prefix() {
    with_tiny(|d| {
        let core = c::bind(d)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let (s, cmd, x) = tiny_wire();
        let mut bad = Vec::new();
        bad.push(Vec::new());
        let mut b = cmd.clone();
        b.push(0);
        bad.push(b);
        let mut b = cmd.clone();
        b[0] = 7;
        bad.push(b);
        let mut b = cmd.clone();
        b[4] ^= 1;
        bad.push(b);
        let mut b = cmd.clone();
        b[6] = 99;
        bad.push(b);
        let mut b = cmd.clone();
        b[7] = 1;
        bad.push(b);
        bad.push(vec![10]);
        for b in bad {
            let out = core.execute(Raw {
                state: &s,
                command: &b,
                context: &x,
            });
            assert!(matches!(out.result(), Err(c::Failure::Ingress(1, _))));
            assert_eq!(out.raw().command, b);
            assert_eq!(out.reads().len(), 2);
            assert_eq!(
                out.reads()[1],
                c::ReadAttempt {
                    source: 1,
                    selector: Selector::Root,
                    permitted: true
                }
            );
            assert_eq!(
                out.usage().used(Resource::Byte),
                s.len() as u64 + b.len() as u64
            );
            assert_eq!(out.usage().used(Resource::Read), 2);
            assert_eq!(out.usage().used(Resource::Step), 0);
            assert!(out.ingress_usage().is_none());
            assert!(out.decision_attempts().is_empty());
            assert!(out.diagnostics().is_empty());
        }
        let altered = atom_bytes(Atom::Sum {
            type_id: 800,
            variant: 82,
        });
        let out = core.execute(Raw {
            state: &s,
            command: &altered,
            context: &x,
        });
        assert!(matches!(
            out.result(),
            Err(c::Failure::Law(l::Failure::Violated))
        ));
        assert!(out.decision_usage().is_some());
        assert_eq!(
            out.diagnostics()
                .last()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .id,
            60
        );
        assert!(
            out.law_reads()
                .last()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .permitted
        );
    });
}
#[test]
fn closed_metadata_checks_unused_declarations_and_root_aliases() {
    with_tiny(|d| {
        let check = |changed: &Descriptor<'_>| {
            assert!(matches!(c::bind(changed), Err(c::Failure::Metadata)))
        };
        let mut bindings = d.bindings.to_vec();
        bindings[2].selector = Selector::Field(0);
        check(&Descriptor {
            bindings: &bindings,
            ..descriptor_copy(d)
        });
        bindings[2] = bindings[0];
        check(&Descriptor {
            bindings: &bindings,
            ..descriptor_copy(d)
        });
        let mut branches = d.branches.to_vec();
        branches.push(Branch {
            code: 999,
            ..branches[0]
        });
        check(&Descriptor {
            branches: &branches,
            ..descriptor_copy(d)
        });
        let assignments = [Assignment {
            domain: Domain::Enum {
                type_id: 700,
                variants: &[71, 71],
            },
            ..d.branches[0].assignments[0]
        }];
        let branches = [Branch {
            assignments: &assignments,
            ..d.branches[0]
        }];
        check(&Descriptor {
            branches: &branches,
            ..descriptor_copy(d)
        });
        let mut channels = d.channels.to_vec();
        channels.push(Channel {
            id: 301,
            destination: Domain::Enum {
                type_id: 900,
                variants: &[1, 1],
            },
            ..channels[0]
        });
        check(&Descriptor {
            channels: &channels,
            ..descriptor_copy(d)
        });
        let plans = [DeliveryPlan {
            destination: Expr::Constant(Atom::Text(&[0x80])),
            ..d.branches[0].effects[0]
        }];
        let branches = [Branch {
            effects: &plans,
            ..d.branches[0]
        }];
        check(&Descriptor {
            branches: &branches,
            ..descriptor_copy(d)
        });
        let plans = [DeliveryPlan {
            channel: 12345,
            ..d.branches[0].effects[0]
        }];
        let branches = [Branch {
            effects: &plans,
            ..d.branches[0]
        }];
        check(&Descriptor {
            branches: &branches,
            ..descriptor_copy(d)
        });
        let mut nodes = d.program.nodes.to_vec();
        nodes.push(ScalarOp::Input(99));
        check(&Descriptor {
            program: ScalarProgram {
                nodes: &nodes,
                ..ScalarProgram {
                    inputs: d.program.inputs,
                    outputs: d.program.outputs,
                    nodes: d.program.nodes,
                    roots: d.program.roots,
                }
            },
            ..descriptor_copy(d)
        });
        let mut laws: Vec<_> = d.laws.iter().map(law_copy).collect();
        laws.push(law_copy(&d.laws[0]));
        check(&Descriptor {
            laws: &laws,
            ..descriptor_copy(d)
        });
        check(&Descriptor {
            required: &[10, 20, 30, 40, 50, 60, 99],
            ..descriptor_copy(d)
        });
        let reasons = [Reason {
            id: 999,
            class: Class::Accept,
        }];
        check(&Descriptor {
            reasons: &reasons,
            ..descriptor_copy(d)
        });
    });
}
#[test]
fn every_transition_budget_cut_preserves_denied_prefix_and_candidate_custody() {
    with_tiny(|d| {
        let (s, cmd, x) = tiny_wire();
        let raw = Raw {
            state: &s,
            command: &cmd,
            context: &x,
        };
        let baseline_core = c::bind(d)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let baseline = baseline_core.execute(raw);
        assert!(baseline.result().is_ok());
        for r in [
            Resource::Byte,
            Resource::Read,
            Resource::Step,
            Resource::Candidate,
            Resource::Write,
            Resource::Effect,
        ] {
            let total = baseline.usage().used(r);
            assert!(total > 0);
            for cut in 0..total {
                let changed = Descriptor {
                    limits: d.limits.with_limit(r, cut),
                    ..descriptor_copy(d)
                };
                let core = c::bind(&changed).unwrap_or_else(|error| {
                    panic!("unexpected composition oracle refusal: {error:?}")
                });
                let out = core.execute(raw);
                assert!(out.result().is_err(), "{r:?} {cut}/{total}");
                assert!(out.usage().used(r) <= cut);
                assert_eq!(out.raw().state, s);
                assert_eq!(out.raw().command, cmd);
                assert_eq!(out.raw().context, x);
                assert!(out.reads().len() <= baseline.reads().len());
                assert_eq!(
                    &out.reads()[..out
                        .reads()
                        .iter()
                        .position(|a| !a.permitted)
                        .unwrap_or(out.reads().len())],
                    &baseline.reads()[..out
                        .reads()
                        .iter()
                        .position(|a| !a.permitted)
                        .unwrap_or(out.reads().len())]
                );
                if r == Resource::Read && cut < 3 {
                    assert_eq!(out.reads().len(), cut as usize + 1);
                    assert!(
                        !out.reads()
                            .last()
                            .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                            .permitted
                    );
                    assert!(out.ingress_usage().is_none());
                }
                if r == Resource::Candidate {
                    assert_eq!(out.decision_attempts(), [c::Attempt::Candidate(false)]);
                    assert!(out.diagnostics().is_empty());
                }
                if r == Resource::Write {
                    assert_eq!(
                        out.decision_attempts(),
                        [c::Attempt::Candidate(true), c::Attempt::Write(0, false)]
                    );
                    assert!(out.diagnostics().is_empty());
                }
                if r == Resource::Effect {
                    assert!(!matches!(
                        out.decision_attempts().last(),
                        Some(c::Attempt::Effect(_, _, true))
                    ));
                    assert!(out.diagnostics().is_empty());
                }
                if !out.diagnostics().is_empty() {
                    assert!(out.decision_usage().is_some());
                    assert!(matches!(
                        out.diagnostics()
                            .last()
                            .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                            .verdict,
                        l::Verdict::Refused(l::Failure::Budget(_))
                    ));
                }
            }
            let exact = Descriptor {
                limits: d.limits.with_limit(r, total),
                ..descriptor_copy(d)
            };
            let core = c::bind(&exact)
                .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
            assert!(core.execute(raw).result().is_ok());
        }
    });
}
#[test]
fn all_header_budget_cuts_and_short_frames_share_the_payload_meter() {
    with_tiny(|d| {
        let f = framing();
        let (s, cmd, x) = tiny_wire();
        let s = envelope(&s, &f.state);
        let cmd = envelope(&cmd, &f.command);
        let x = envelope(&x, &f.context);
        let raw = Raw {
            state: &s,
            command: &cmd,
            context: &x,
        };
        for cut in 0..144 {
            let changed = Descriptor {
                limits: d.limits.with_limit(Resource::Byte, cut),
                ..descriptor_copy(d)
            };
            let core = c::bind(&changed)
                .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
            let out = core.frame(c::Kind::Transition, raw, &f);
            let source = cut / 48;
            assert!(
                matches!(out.result(),Err(c::Failure::Frame(n,c::FrameFailure::Budget(e)))if n==source as u8&&e.resource==Resource::Byte)
            );
            assert_eq!(out.usage().used(Resource::Byte), source * 48);
            assert!(out.reads().is_empty());
        }
        for source in 0..3 {
            for n in 0..48 {
                let mut frames = [s.clone(), cmd.clone(), x.clone()];
                frames[source].truncate(n);
                let core = c::bind(d).unwrap_or_else(|error| {
                    panic!("unexpected composition oracle refusal: {error:?}")
                });
                let out = core.frame(
                    c::Kind::Transition,
                    Raw {
                        state: &frames[0],
                        command: &frames[1],
                        context: &frames[2],
                    },
                    &f,
                );
                assert!(
                    matches!(out.result(),Err(c::Failure::Frame(i,c::FrameFailure::Envelope(_)))if i==source as u8)
                );
                assert_eq!(
                    out.usage().used(Resource::Byte),
                    source as u64 * 48 + n as u64
                );
                assert!(out.reads().is_empty());
            }
        }
        let changed = Descriptor {
            limits: d.limits.with_limit(Resource::Byte, 144),
            ..descriptor_copy(d)
        };
        let core = c::bind(&changed)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let out = core.frame(c::Kind::Transition, raw, &f);
        assert!(matches!(out.result(), Err(c::Failure::Ingress(0, _))));
        assert_eq!(out.usage().used(Resource::Byte), 144);
        assert!(out.reads().is_empty());
    });
}
fn laws(which: u8) -> Vec<LawData> {
    use l::Observation as O;
    use l::{Kind as K, Scope as S};
    let mut result = Vec::new();
    let mut p = Predicate::default();
    let invariant = if which == 0 {
        let a = p.num(O::Post(110));
        let b = p.num(O::Post(111));
        let z = p.n(0);
        let five = p.n(5);
        let e = [p.le(z, a), p.le(a, five), p.le(z, b), p.le(b, five)];
        p.all(&e)
    } else if which == 1 {
        let status = p.num(O::Post(120));
        let low = p.n(161);
        let high = p.n(164);
        let in_range = p.le(low, status);
        let below = p.le(status, high);
        let active = p.and(in_range, below);
        let attempts = p.num(O::Post(121));
        let one = p.n(1);
        let nonzero = p.le(one, attempts);
        p.implies(active, nonzero)
    } else {
        let until = p.num(O::Post(111));
        let seen = p.num(O::Post(112));
        let failed = p.num(O::Post(110));
        let lock = p.n(900);
        let upper = p.add(seen, lock);
        let bounded = p.le(until, upper);
        let active = p.push(l::Op::Lt(seen, until));
        let z = p.n(0);
        let reset = p.eq(failed, z);
        let clause = p.implies(active, reset);
        p.and(bounded, clause)
    };
    result.push(ld(
        500,
        K::StateInvariant,
        S::Committing,
        true,
        p,
        invariant,
    ));
    if which == 0 {
        let mut p = Predicate::default();
        let action = p.num(O::Command(120));
        let qty = p.num(O::Command(121));
        let a = p.num(O::Pre(110));
        let r = p.num(O::Pre(111));
        let na = p.num(O::Post(110));
        let nr = p.num(O::Post(111));
        let old = p.add(a, r);
        let new = p.add(na, nr);
        let mut clauses = Vec::new();
        for id in 150..=153 {
            let n = p.n(id);
            let chosen = p.eq(action, n);
            let equality = match id {
                150 | 151 => p.eq(new, old),
                152 => {
                    let shipped = p.add(new, qty);
                    p.eq(shipped, old)
                }
                _ => {
                    let restocked = p.add(old, qty);
                    p.eq(new, restocked)
                }
            };
            clauses.push(p.implies(chosen, equality));
        }
        let root = p.all(&clauses);
        result.push(ld(501, K::AssetConservation, S::Accept, false, p, root));
        let mut p = Predicate::default();
        let action = p.num(O::Command(120));
        let q = p.num(O::Command(121));
        let a = p.num(O::Pre(110));
        let r = p.num(O::Pre(111));
        let na = p.num(O::Post(110));
        let nr = p.num(O::Post(111));
        let auth = p.equal_number(O::Context(130), 1);
        let mut clauses = vec![auth];
        for id in 150..=153 {
            let n = p.n(id);
            let chosen = p.eq(action, n);
            let aq = p.add(a, q);
            let rq = p.add(r, q);
            let naq = p.add(na, q);
            let nrq = p.add(nr, q);
            let (x, y) = match id {
                150 => (p.eq(naq, a), p.eq(nr, rq)),
                151 => (p.eq(na, aq), p.eq(nrq, r)),
                152 => (p.eq(na, a), p.eq(nrq, r)),
                _ => (p.eq(na, aq), p.eq(nr, r)),
            };
            let movement = p.and(x, y);
            clauses.push(p.implies(chosen, movement));
        }
        let root = p.all(&clauses);
        result.push(ld(
            502,
            K::AuthoritySubjectRecipient,
            S::Accept,
            false,
            p,
            root,
        ));
    }
    if which == 2 {
        let mut p = Predicate::default();
        let command = p.num(O::CommandRoot);
        let bad = p.n(121);
        let is_failed = p.eq(command, bad);
        let no_failure = p.not(is_failed);
        let success = p.n(120);
        let chosen = p.eq(command, success);
        let now = p.num(O::Context(130));
        let seen = p.num(O::Pre(112));
        let until = p.num(O::Pre(111));
        let zero = p.equal_number(O::Post(110), 0);
        let deadline = p.same(O::Post(111), O::Pre(111));
        let clock = p.same(O::Post(112), O::Context(130));
        let e = [p.le(seen, now), p.le(until, now), zero, deadline, clock];
        let body = p.all(&e);
        let clause = p.implies(chosen, body);
        let root = p.and(no_failure, clause);
        result.push(ld(
            501,
            K::AuthoritySubjectRecipient,
            S::Accept,
            false,
            p,
            root,
        ));
        let mut p = Predicate::default();
        let admin = p.equal_number(O::CommandRoot, 122);
        let auth = p.equal_number(O::Context(131), 1);
        let now = p.num(O::Context(130));
        let seen = p.num(O::Pre(112));
        let monotone = p.le(seen, now);
        let cleared = p.equal_number(O::Post(110), 0);
        let unlocked = p.equal_number(O::Post(111), 0);
        let clock = p.same(O::Post(112), O::Context(130));
        let body = p.all(&[auth, monotone, cleared, unlocked, clock]);
        let root = p.implies(admin, body);
        result.push(ld(
            502,
            K::AuthoritySubjectRecipient,
            S::Accept,
            false,
            p,
            root,
        ));
        let mut p = Predicate::default();
        let cmd = p.equal_number(O::CommandRoot, 121);
        let now = p.num(O::Context(130));
        let seen = p.num(O::Pre(112));
        let until = p.num(O::Pre(111));
        let clock = p.same(O::Post(112), O::Context(130));
        let f = p.num(O::Pre(110));
        let two = p.n(2);
        let ordinary = p.push(l::Op::Lt(f, two));
        let third = p.eq(f, two);
        let one = p.n(1);
        let increment = p.add(f, one);
        let next = p.num(O::Post(110));
        let incremented = p.eq(next, increment);
        let unchanged = p.same(O::Post(111), O::Pre(111));
        let ordinary_body = p.and(incremented, unchanged);
        let ordinary_clause = p.implies(ordinary, ordinary_body);
        let reset = p.equal_number(O::Post(110), 0);
        let lock = p.n(900);
        let deadline = p.add(now, lock);
        let post_until = p.num(O::Post(111));
        let exact_deadline = p.eq(post_until, deadline);
        let third_body = p.and(reset, exact_deadline);
        let third_clause = p.implies(third, third_body);
        let e = [
            cmd,
            p.le(seen, now),
            p.le(until, now),
            clock,
            ordinary_clause,
            third_clause,
        ];
        let root = p.all(&e);
        result.push(ld(
            503,
            K::AuthoritySubjectRecipient,
            S::CommittedFailure,
            false,
            p,
            root,
        ));
    }
    let mut p = Predicate::default();
    let zeros = [
        O::PostLength,
        O::PatchLength,
        O::EffectLength,
        O::OutboxLength,
    ]
    .map(|o| p.equal_number(o, 0));
    let root = p.all(&zeros);
    result.push(ld(900, K::RejectNoAuthority, S::Reject, false, p, root));
    let mut p = Predicate::default();
    let root = p.push(l::Op::Observe(O::HasReason));
    result.push(ld(
        901,
        K::CommittedFailureEffects,
        S::CommittedFailure,
        false,
        p,
        root,
    ));
    let mut p = Predicate::default();
    let accept = p.equal_number(O::Class, 0);
    let expected = p.not(accept);
    let reason = p.push(l::Op::Observe(O::HasReason));
    let root = p.eq(expected, reason);
    result.push(ld(902, K::DecisionConformance, S::Always, false, p, root));
    let mut p = Predicate::default();
    let init = if which == 1 {
        let placed = p.equal_number(O::Initial(120), 160);
        let zero = p.equal_number(O::Initial(121), 0);
        p.and(placed, zero)
    } else {
        let a = p.equal_number(O::Initial(110), 0);
        let b = p.equal_number(O::Initial(111), 0);
        let mut root = p.and(a, b);
        if which == 2 {
            let c = p.equal_number(O::Initial(112), 0);
            root = p.and(root, c);
        }
        root
    };
    result.push(ld(903, K::InitialCondition, S::Always, true, p, init));
    result
}

fn with_account(run: impl FnOnce(&Descriptor<'_>)) {
    const KINDS: [u16; 2] = [150, 151];
    let state = [
        int(110, 0, 2),
        int(111, 0, 4102445700),
        int(112, 0, 4102444800),
    ];
    let command = InputLeaf::Sum {
        type_id: 101,
        min: 120,
        max: 122,
        variants: (120..=122)
            .map(|id| Variant {
                id,
                code: id as i64,
            })
            .collect(),
    };
    let context = [
        int(130, 0, 4102444800),
        InputField {
            id: 131,
            leaf: InputLeaf::Bool,
        },
    ];
    let bindings = [
        bind(Source::State, 110),
        bind(Source::State, 111),
        bind(Source::State, 112),
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        bind(Source::Context, 130),
        bind(Source::Context, 131),
    ];
    let outputs = [
        InputLeaf::I128 { min: 0, max: 6 },
        InputLeaf::I128 {
            min: i64::MIN,
            max: i64::MAX,
        },
        InputLeaf::I128 {
            min: i64::MIN,
            max: i64::MAX,
        },
    ];
    let assign = |field, value, max| Assignment {
        field,
        value,
        domain: Domain::I128 { min: 0, max },
    };
    let seen = assign(112, Expr::Input(Source::Context, 130), 4102444800);
    let zero = assign(110, Expr::Constant(Atom::I128(0)), 2);
    let keep = assign(111, Expr::Input(Source::State, 111), 4102445700);
    let success = [zero, keep, seen];
    let third = [zero, assign(111, Expr::Output(1), 4102445700), seen];
    let failed = [assign(110, Expr::Output(2), 2), keep, seen];
    let admin = [
        zero,
        assign(111, Expr::Constant(Atom::I128(0)), 4102445700),
        seen,
    ];
    let locked_payload = [
        PayloadField {
            field: 140,
            value: Expr::Constant(Atom::Sum {
                type_id: 109,
                variant: 150,
            }),
        },
        PayloadField {
            field: 141,
            value: Expr::Output(1),
        },
    ];
    let unlock_payload = [
        PayloadField {
            field: 140,
            value: Expr::Constant(Atom::Sum {
                type_id: 109,
                variant: 151,
            }),
        },
        PayloadField {
            field: 141,
            value: Expr::Constant(Atom::I128(0)),
        },
    ];
    let alert = DeliveryPlan {
        ordinal: 0,
        channel: 300,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"security-team")),
        payload: &locked_payload,
        idempotency: Expr::Input(Source::Context, 130),
    };
    let locked = [alert];
    let unlocked = [DeliveryPlan {
        payload: &unlock_payload,
        ..alert
    }];
    let branches = [
        branch(0, Class::Reject, Some(200), &[], &[]),
        branch(1, Class::Reject, Some(201), &[], &[]),
        branch(2, Class::Reject, Some(202), &[], &[]),
        branch(3, Class::Accept, None, &success, &[]),
        branch(4, Class::CommittedFailure, Some(203), &third, &locked),
        branch(5, Class::CommittedFailure, Some(203), &failed, &[]),
        branch(6, Class::Accept, None, &admin, &unlocked),
    ];
    let reasons = [
        Reason {
            id: 200,
            class: Class::Reject,
        },
        Reason {
            id: 201,
            class: Class::Reject,
        },
        Reason {
            id: 202,
            class: Class::Reject,
        },
        Reason {
            id: 203,
            class: Class::CommittedFailure,
        },
    ];
    let types = [
        TypedField {
            field: 140,
            domain: Domain::Sum {
                type_id: 109,
                variants: &KINDS,
            },
        },
        TypedField {
            field: 141,
            domain: Domain::I128 {
                min: 0,
                max: 4102445700,
            },
        },
    ];
    let channels = [Channel {
        id: 300,
        destination: Domain::Text,
        payload: &types,
        idempotency: Domain::I128 {
            min: 0,
            max: 4102444800,
        },
    }];
    let data = laws(2);
    let law_refs = law_refs(&data);
    let required: Vec<_> = law_refs.iter().map(|x| x.id).collect();
    let actual = account_contract::Contract::new();
    let d = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Record(&context),
        program: actual.descriptor().program,
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &law_refs,
        required: &required,
        limits: limits(),
    };
    run(&d)
}
type AccountTuple = (i128, i128, i128, u16, i128, bool);
fn account_wire(x: AccountTuple) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (
        record(&[fi(110, x.0), fi(111, x.1), fi(112, x.2)]),
        atom_bytes(Atom::Sum {
            type_id: 101,
            variant: x.3,
        }),
        record(&[
            fi(130, x.4),
            Field {
                id: 131,
                value: Atom::Bool(x.5),
            },
        ]),
    )
}
type AccountOutcome = (Class, Option<u32>, [Field<'static>; 3], Option<(u16, i128)>);
fn account_oracle(x: AccountTuple) -> AccountOutcome {
    let (f, u, s, cmd, now, admin) = x;
    let (class, reason, nf, nu, ns, alert) = if now < s {
        (Class::Reject, Some(200), f, u, s, None)
    } else if (cmd == 120 || cmd == 121) && now < u {
        (Class::Reject, Some(201), f, u, s, None)
    } else if cmd == 122 && !admin {
        (Class::Reject, Some(202), f, u, s, None)
    } else if cmd == 120 {
        (Class::Accept, None, 0, u, now, None)
    } else if cmd == 122 {
        (Class::Accept, None, 0, 0, now, Some((151, 0)))
    } else if f == 2 {
        (
            Class::CommittedFailure,
            Some(203),
            0,
            now + 900,
            now,
            Some((150, now + 900)),
        )
    } else {
        (Class::CommittedFailure, Some(203), f + 1, u, now, None)
    };
    (
        class,
        reason,
        [fi(110, nf), fi(111, nu), fi(112, ns)],
        alert,
    )
}
fn check_account(core: &c::BoundCore<'_>, d: &Descriptor<'_>, x: AccountTuple) {
    let (s, cmd, ctx) = account_wire(x);
    let out = core.execute(Raw {
        state: &s,
        command: &cmd,
        context: &ctx,
    });
    let (class, reason, post, alert) = account_oracle(x);
    let pre = [fi(110, x.0), fi(111, x.1), fi(112, x.2)];
    let candidate = out
        .result()
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
    check_candidate(candidate, &pre, class, reason, &post);
    assert_eq!(out.reads().len(), 6);
    assert_eq!(
        (out.reads()[3].source, out.reads()[3].selector),
        (1, Selector::Root)
    );
    assert_eq!(candidate.outbox().len(), usize::from(alert.is_some()));
    if let Some((kind, until)) = alert {
        let a = &candidate.outbox()[0];
        assert_eq!(
            (a.ordinal, a.channel, a.destination, a.idempotency),
            (0, 300, Atom::Text(b"security-team"), Atom::I128(x.4))
        );
        assert_eq!(a.payload, [fe(140, 109, kind, true), fi(141, until)]);
        assert_original_sum_field(&record(&a.payload), 140, 109, kind);
    }
    report_costs(&out, d, class, false);
}
#[test]
fn account_full_boundaries_14580_and_all_20_retained_examples() {
    with_account(|d| {
        let core = c::bind(d)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let times = [
            0, 1, 899, 900, 901, 4102443899, 4102443900, 4102444799, 4102444800,
        ];
        let deadlines = [
            0, 1, 899, 900, 901, 4102444799, 4102444800, 4102444801, 4102445699, 4102445700,
        ];
        let mut checked = 0;
        for f in 0..=2 {
            for u in deadlines {
                for s in times {
                    for now in times {
                        for cmd in 120..=122 {
                            for admin in [false, true] {
                                check_account(&core, d, (f, u, s, cmd, now, admin));
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 14580);
        let examples = include_str!(
            "../../zeno-fcis-cli/templates/account-lockout/tests/decision-examples.txt"
        );
        let mut checked = 0;
        for line in examples
            .lines()
            .filter(|s| !s.starts_with('#') && !s.trim().is_empty())
        {
            let v: Vec<i128> = line
                .split('|')
                .next()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .split_whitespace()
                .map(|v| {
                    v.parse().unwrap_or_else(|error| {
                        panic!("unexpected composition oracle refusal: {error:?}")
                    })
                })
                .collect();
            check_account(&core, d, (v[0], v[1], v[2], v[3] as u16, v[4], v[5] != 0));
            let expected = account_oracle((v[0], v[1], v[2], v[3] as u16, v[4], v[5] != 0));
            let authored: Vec<_> = line
                .split('|')
                .nth(1)
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .split_whitespace()
                .collect();
            assert_eq!(
                authored[0],
                match expected.0 {
                    Class::Accept => "accept",
                    Class::Reject => "reject",
                    Class::CommittedFailure => "failure",
                    _ => panic!("unsupported future decision class in the retained account oracle"),
                }
            );
            assert_eq!(
                authored[1],
                expected
                    .1
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "-".into())
            );
            for (i, f) in expected.2.iter().enumerate() {
                let Atom::I128(value) = f.value else { panic!() };
                assert_eq!(
                    authored[i + 2]
                        .parse::<i128>()
                        .unwrap_or_else(|error| panic!(
                            "unexpected composition oracle refusal: {error:?}"
                        )),
                    value
                );
            }
            let authored_alert = line
                .split('|')
                .nth(2)
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .trim();
            assert_eq!(
                authored_alert,
                expected
                    .3
                    .map(|(kind, until)| format!("300 {kind} {until}"))
                    .unwrap_or_else(|| "-".into())
            );
            checked += 1;
        }
        assert_eq!(checked, 20);
        check_account(&core, d, (0, 0, 0, 120, 0, false));
        check_account(&core, d, (0, 0, 1, 120, 0, false));
    });
}

fn framing() -> c::Framing {
    let make = |root| c::FrameBinding {
        root,
        schema: [0xa7; 32],
        max_bytes: 4096,
    };
    c::Framing {
        state: make(100),
        command: make(101),
        context: make(102),
    }
}
fn envelope(payload: &[u8], binding: &c::FrameBinding) -> Vec<u8> {
    let mut b = b"ZFCISV1\0".to_vec();
    b.extend(binding.root.to_be_bytes());
    b.extend(binding.schema);
    b.extend((payload.len() as u32).to_be_bytes());
    b.extend(payload);
    b
}
#[test]
fn original_envelopes_share_meter_and_preserve_headers_on_refusal() {
    with_account(|d| {
        let f = framing();
        let (s, cx, x) = account_wire((2, 0, 0, 121, 4102444800, false));
        let s = envelope(&s, &f.state);
        let cx = envelope(&cx, &f.command);
        let x = envelope(&x, &f.context);
        let core = c::bind(d)
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        let out = core.frame(
            c::Kind::Transition,
            Raw {
                state: &s,
                command: &cx,
                context: &x,
            },
            &f,
        );
        assert_eq!(out.raw().state, s);
        assert_eq!(out.raw().command, cx);
        assert_eq!(out.raw().context, x);
        assert_eq!(out.usage().used(Resource::Byte), 241);
        assert_eq!(
            out.ingress_usage()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .used(Resource::Byte),
            241
        );
        assert_eq!(
            out.ingress_usage()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .used(Resource::Step),
            0
        );
        assert_eq!(
            out.result()
                .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"))
                .post()[1]
                .value,
            Atom::I128(4102445700)
        );
        for source in 0..3 {
            let mut frames = [s.clone(), cx.clone(), x.clone()];
            frames[source][8] ^= 1;
            let out = core.frame(
                c::Kind::Transition,
                Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                },
                &f,
            );
            assert!(
                matches!(out.result(),Err(c::Failure::Frame(n,c::FrameFailure::Envelope(_)))if n==source as u8)
            );
            assert_eq!(out.usage().used(Resource::Byte), 48 * (source as u64 + 1));
            assert_eq!(out.usage().used(Resource::Read), 0);
            assert!(out.reads().is_empty());
            assert!(out.diagnostics().is_empty());
            assert!(out.ingress_usage().is_none());
        }
        let (s, _, _) = account_wire((0, 0, 0, 120, 0, false));
        let original = envelope(&s, &f.state);
        let genesis = core.frame(
            c::Kind::Genesis,
            Raw {
                state: &original,
                command: &[],
                context: &[],
            },
            &f,
        );
        assert!(matches!(genesis.result(), Ok(c) if c.class() == Class::Accept));
        assert_eq!(genesis.raw().state, original);
        assert_eq!(genesis.usage().used(Resource::Byte), 110);
    });
}
fn check_candidate(
    c: &c::Candidate<'_>,
    pre: &[Field<'_>],
    class: Class,
    reason: Option<u32>,
    post: &[Field<'_>],
) {
    assert_eq!((c.class(), c.reason()), (class, reason));
    assert_eq!(c.pre(), pre);
    assert_eq!(c.post(), if class == Class::Reject { &[] } else { post });
    assert!(c.effects().is_empty());
    let changed: Vec<_> = pre
        .iter()
        .zip(post)
        .filter(|(a, b)| a.value != b.value)
        .collect();
    if class == Class::Reject {
        assert!(c.patch().is_empty());
        assert!(c.outbox().is_empty());
    } else {
        assert_eq!(c.patch().len(), changed.len());
        for (p, (a, b)) in c.patch().iter().zip(changed) {
            assert_eq!((p.field, p.before, p.after), (a.id, a.value, b.value));
        }
    }
}
fn report_costs(out: &c::Outcome<'_>, d: &Descriptor<'_>, class: Class, law_refusal: bool) {
    let ingress = out
        .ingress_usage()
        .unwrap_or_else(|| panic!("missing composition oracle fixture/result"));
    let decision = out
        .decision_usage()
        .unwrap_or_else(|| panic!("missing composition oracle fixture/result"));
    let fact_steps = 0;
    assert_eq!(ingress.used(Resource::Step), fact_steps);
    assert_eq!(
        decision.used(Resource::Step),
        fact_steps + d.program.nodes.len() as u64
    );
    assert_eq!(decision.used(Resource::Candidate), 1);
    assert_eq!(
        decision.used(Resource::Write),
        if class == Class::Reject {
            0
        } else {
            match d.state {
                Schema::Record(f) => f.len() as u64,
                _ => panic!(),
            }
        }
    );
    assert_eq!(out.decision_attempts()[0], c::Attempt::Candidate(true));
    let node_cost: u64 = out
        .diagnostics()
        .iter()
        .filter_map(|a| match a.verdict {
            l::Verdict::Satisfied | l::Verdict::Refused(l::Failure::Violated) => Some(
                d.laws
                    .iter()
                    .find(|x| x.id == a.id)
                    .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                    .program
                    .nodes
                    .len() as u64,
            ),
            l::Verdict::Skipped => None,
            _ => panic!("unexpected technical refusal"),
        })
        .sum();
    assert_eq!(
        out.usage().used(Resource::Step),
        decision.used(Resource::Step) + node_cost
    );
    assert_eq!(
        out.usage().used(Resource::Read),
        ingress.used(Resource::Read) + out.law_reads().len() as u64
    );
    assert_eq!(out.result().is_err(), law_refusal);
    assert_eq!(
        out.raw().state.len() as u64
            + out.raw().command.len() as u64
            + out.raw().context.len() as u64,
        out.usage().used(Resource::Byte)
    );
}

#[test]
fn original_inventory_864_complete_decisions_and_law_reports() {
    let graph = template_graph(include_bytes!(
        "../../zeno-fcis-cli/templates/inventory-reservation/synthesized/program.zcve"
    ));
    assert_eq!(graph.inputs()[3], ScalarDomain::Int { min: 1, max: 3 });
    let state = [int(110, 0, 5), int(111, 0, 5)];
    let command = [
        InputField {
            id: 120,
            leaf: leaf(107, &[150, 151, 152, 153], true),
        },
        int(121, 1, 3),
    ];
    let context = [InputField {
        id: 130,
        leaf: InputLeaf::Bool,
    }];
    let bindings = [
        bind(Source::State, 110),
        bind(Source::State, 111),
        bind(Source::Command, 120),
        bind(Source::Command, 121),
        bind(Source::Context, 130),
    ];
    let outputs = [
        InputLeaf::I128 { min: 0, max: 4 },
        InputLeaf::I128 { min: 0, max: 5 },
        InputLeaf::I128 { min: 0, max: 5 },
        InputLeaf::Bool,
    ];
    let assignments = [ad(110, 1, 0, 5), ad(111, 2, 0, 5)];
    let payload = [PayloadField {
        field: 140,
        value: Expr::Input(Source::Command, 121),
    }];
    let deliveries = [DeliveryPlan {
        ordinal: 0,
        channel: 300,
        when: Expr::Output(3),
        destination: Expr::Constant(Atom::Text(b"warehouse")),
        payload: &payload,
        idempotency: Expr::Input(Source::Command, 121),
    }];
    let branches = [
        branch(0, Class::Reject, Some(201), &[], &[]),
        branch(1, Class::Reject, Some(202), &[], &[]),
        branch(2, Class::Reject, Some(203), &[], &[]),
        branch(3, Class::Accept, None, &assignments, &deliveries),
        branch(4, Class::Reject, Some(200), &[], &[]),
    ];
    let reasons = [200, 201, 202, 203].map(|id| Reason {
        id,
        class: Class::Reject,
    });
    let payload_types = [TypedField {
        field: 140,
        domain: Domain::I128 { min: 1, max: 3 },
    }];
    let channels = [Channel {
        id: 300,
        destination: Domain::Text,
        payload: &payload_types,
        idempotency: Domain::I128 { min: 1, max: 3 },
    }];
    let law_data = laws(0);
    let law_refs = law_refs(&law_data);
    let required: Vec<_> = law_refs.iter().map(|x| x.id).collect();
    let descriptor = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Record(&command),
        context: Schema::Record(&context),
        program: ScalarProgram {
            inputs: graph.inputs(),
            outputs: graph.outputs(),
            nodes: graph.nodes(),
            roots: graph.roots(),
        },
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &law_refs,
        required: &required,
        limits: limits(),
    };
    let core = c::bind(&descriptor)
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
    let mut checked = 0;
    for available in 0..=5 {
        for reserved in 0..=5 {
            for action in 150..=153 {
                for quantity in 1..=3 {
                    for authorized in [false, true] {
                        let sufficient = if action == 150 {
                            available >= quantity
                        } else if action == 151 || action == 152 {
                            reserved >= quantity
                        } else {
                            true
                        };
                        let capacity = if action == 150 {
                            reserved + quantity <= 5
                        } else if action == 151 || action == 153 {
                            available + quantity <= 5
                        } else {
                            true
                        };
                        let reason = if !authorized {
                            Some(200)
                        } else if !sufficient {
                            Some(if action == 150 { 201 } else { 202 })
                        } else if !capacity {
                            Some(203)
                        } else {
                            None
                        };
                        let class = if reason.is_none() {
                            Class::Accept
                        } else {
                            Class::Reject
                        };
                        let (a, r) = if class == Class::Accept {
                            match action {
                                150 => (available - quantity, reserved + quantity),
                                151 => (available + quantity, reserved - quantity),
                                152 => (available, reserved - quantity),
                                _ => (available + quantity, reserved),
                            }
                        } else {
                            (available, reserved)
                        };
                        let pre = [fi(110, available), fi(111, reserved)];
                        let post = [fi(110, a), fi(111, r)];
                        let s = record(&pre);
                        let cmd = record(&[fe(120, 107, action, true), fi(121, quantity)]);
                        assert_original_sum_field(&cmd, 120, 107, action);
                        let ctx = record(&[Field {
                            id: 130,
                            value: Atom::Bool(authorized),
                        }]);
                        let out = core.execute(Raw {
                            state: &s,
                            command: &cmd,
                            context: &ctx,
                        });
                        let candidate = out.result().unwrap_or_else(|error| {
                            panic!("unexpected composition oracle refusal: {error:?}")
                        });
                        check_candidate(candidate, &pre, class, reason, &post);
                        assert_eq!(
                            candidate.outbox().len(),
                            usize::from(class == Class::Accept && action == 152)
                        );
                        if let Some(d) = candidate.outbox().first() {
                            assert_eq!(
                                (d.ordinal, d.channel, d.destination, d.idempotency),
                                (0, 300, Atom::Text(b"warehouse"), Atom::I128(quantity))
                            );
                            assert_eq!(d.payload, [fi(140, quantity)]);
                        }
                        report_costs(&out, &descriptor, class, false);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!(checked, 864);
    for n in [0, 1] {
        let s = record(&[fi(110, n), fi(111, 0)]);
        let out = core.genesis(&s);
        assert_eq!(out.result().is_ok(), n == 0);
        assert_eq!(out.raw().state, s);
        assert_eq!(
            out.ingress_usage()
                .unwrap_or_else(|| panic!("missing composition oracle fixture/result"))
                .used(Resource::Byte),
            43
        );
    }
}

#[test]
fn original_order_all_1728_including_288_unlawful_pre_states() {
    const STATUSES: [u16; 6] = [160, 161, 162, 163, 164, 165];
    const PAYMENT: [u16; 2] = [175, 176];
    let graph = template_graph(include_bytes!(
        "../../zeno-fcis-cli/templates/order-fulfillment/synthesized/program.zcve"
    ));
    let state = [
        InputField {
            id: 120,
            leaf: leaf(105, &STATUSES, true),
        },
        int(121, 0, 3),
    ];
    let command = [
        InputField {
            id: 125,
            leaf: leaf(111, &[150, 151, 152, 153, 154, 155], true),
        },
        int(126, 0, 3),
    ];
    let context = [InputField {
        id: 130,
        leaf: leaf(107, &[170, 171, 172], true),
    }];
    let bindings = [
        bind(Source::Command, 125),
        bind(Source::State, 120),
        bind(Source::State, 121),
        bind(Source::Command, 126),
        bind(Source::Context, 130),
    ];
    let outputs = [
        InputLeaf::I128 { min: 0, max: 10 },
        leaf(105, &STATUSES, true),
        InputLeaf::I128 { min: 0, max: 3 },
    ];
    let assignments = [
        Assignment {
            field: 120,
            value: Expr::Output(1),
            domain: Domain::Sum {
                type_id: 105,
                variants: &STATUSES,
            },
        },
        ad(121, 2, 0, 3),
    ];
    let capture_payload = [
        PayloadField {
            field: 140,
            value: Expr::Output(2),
        },
        PayloadField {
            field: 141,
            value: Expr::Constant(Atom::Sum {
                type_id: 108,
                variant: 175,
            }),
        },
    ];
    let void_payload = [
        PayloadField {
            field: 140,
            value: Expr::Output(2),
        },
        PayloadField {
            field: 141,
            value: Expr::Constant(Atom::Sum {
                type_id: 108,
                variant: 176,
            }),
        },
    ];
    let ship_payload = [PayloadField {
        field: 145,
        value: Expr::Output(2),
    }];
    let payment = DeliveryPlan {
        ordinal: 0,
        channel: 300,
        when: Expr::Constant(Atom::Bool(true)),
        destination: Expr::Constant(Atom::Text(b"payment-provider")),
        payload: &capture_payload,
        idempotency: Expr::Output(2),
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
    let mut branches = Vec::new();
    for code in 0..4 {
        branches.push(branch(
            code,
            Class::Reject,
            Some(200 + code as u32),
            &[],
            &[],
        ));
    }
    for code in 4..=10 {
        branches.push(branch(
            code,
            if code == 6 {
                Class::CommittedFailure
            } else {
                Class::Accept
            },
            if code == 6 { Some(204) } else { None },
            &assignments,
            match code {
                4 => &capture,
                5 => &shipping,
                10 => &void,
                _ => &[],
            },
        ));
    }
    let reasons: Vec<_> = (200..=204)
        .map(|id| Reason {
            id,
            class: if id == 204 {
                Class::CommittedFailure
            } else {
                Class::Reject
            },
        })
        .collect();
    let pay_types = [
        TypedField {
            field: 140,
            domain: Domain::I128 { min: 0, max: 3 },
        },
        TypedField {
            field: 141,
            domain: Domain::Sum {
                type_id: 108,
                variants: &PAYMENT,
            },
        },
    ];
    let ship_types = [TypedField {
        field: 145,
        domain: Domain::I128 { min: 0, max: 3 },
    }];
    let channels = [
        Channel {
            id: 300,
            destination: Domain::Text,
            payload: &pay_types,
            idempotency: Domain::I128 { min: 0, max: 3 },
        },
        Channel {
            id: 301,
            destination: Domain::Text,
            payload: &ship_types,
            idempotency: Domain::I128 { min: 0, max: 3 },
        },
    ];
    let data = laws(1);
    let law_refs = law_refs(&data);
    let required: Vec<_> = law_refs.iter().map(|x| x.id).collect();
    let descriptor = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Record(&command),
        context: Schema::Record(&context),
        program: ScalarProgram {
            inputs: graph.inputs(),
            outputs: graph.outputs(),
            nodes: graph.nodes(),
            roots: graph.roots(),
        },
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &law_refs,
        required: &required,
        limits: limits(),
    };
    let core = c::bind(&descriptor)
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
    let (mut checked, mut unlawful, mut law_refused) = (0, 0, 0);
    for status in 160..=165 {
        for attempts in 0..=3 {
            for action in 150..=155 {
                for callback in 0..=3 {
                    for caller in 170..=172 {
                        unlawful += usize::from((161..=164).contains(&status) && attempts == 0);
                        let required = match action {
                            150 | 155 => 170,
                            151 | 152 => 171,
                            _ => 172,
                        };
                        let (class, reason, next, n, request) = if caller != required {
                            (Class::Reject, Some(200), status, attempts, None)
                        } else {
                            match (action, status) {
                                (151 | 152, 161) if callback != attempts => {
                                    (Class::Reject, Some(202), status, attempts, None)
                                }
                                (150, 160) if attempts == 3 => {
                                    (Class::Reject, Some(203), status, attempts, None)
                                }
                                (150, 160) => {
                                    (Class::Accept, None, 161, attempts + 1, Some((300, 175)))
                                }
                                (151, 161) => (Class::Accept, None, 162, attempts, Some((301, 0))),
                                (152, 161) => {
                                    (Class::CommittedFailure, Some(204), 160, attempts, None)
                                }
                                (153, 162) => (Class::Accept, None, 163, attempts, None),
                                (154, 163) => (Class::Accept, None, 164, attempts, None),
                                (155, 160) => (Class::Accept, None, 165, attempts, None),
                                (155, 161) => {
                                    (Class::Accept, None, 165, attempts, Some((300, 176)))
                                }
                                _ => (Class::Reject, Some(201), status, attempts, None),
                            }
                        };
                        let pre = [fe(120, 105, status, true), fi(121, attempts)];
                        let post = [fe(120, 105, next, true), fi(121, n)];
                        let s = record(&pre);
                        let cmd = record(&[fe(125, 111, action, true), fi(126, callback)]);
                        let ctx = record(&[fe(130, 107, caller, true)]);
                        assert_original_sum_field(&s, 120, 105, status);
                        assert_original_sum_field(&cmd, 125, 111, action);
                        assert_original_sum_field(&ctx, 130, 107, caller);
                        let out = core.execute(Raw {
                            state: &s,
                            command: &cmd,
                            context: &ctx,
                        });
                        let refuses =
                            class != Class::Reject && (161..=164).contains(&next) && n == 0;
                        if refuses {
                            assert!(matches!(
                                out.result(),
                                Err(c::Failure::Law(l::Failure::Violated))
                            ));
                            law_refused += 1;
                        } else {
                            let candidate = out.result().unwrap_or_else(|error| {
                                panic!("unexpected composition oracle refusal: {error:?}")
                            });
                            check_candidate(candidate, &pre, class, reason, &post);
                            assert_eq!(candidate.outbox().len(), usize::from(request.is_some()));
                            if let Some((channel, kind)) = request {
                                let d = &candidate.outbox()[0];
                                if channel == 300 {
                                    assert_original_sum_field(&record(&d.payload), 141, 108, kind);
                                }
                                assert_eq!(
                                    (d.ordinal, d.channel, d.idempotency),
                                    (0, channel, Atom::I128(n))
                                );
                                assert_eq!(
                                    d.destination,
                                    Atom::Text(if channel == 300 {
                                        b"payment-provider"
                                    } else {
                                        b"carrier"
                                    })
                                );
                                assert_eq!(
                                    d.payload,
                                    if channel == 300 {
                                        vec![fi(140, n), fe(141, 108, kind, true)]
                                    } else {
                                        vec![fi(145, n)]
                                    }
                                );
                            }
                        }
                        report_costs(&out, &descriptor, class, refuses);
                        checked += 1;
                    }
                }
            }
        }
    }
    assert_eq!((checked, unlawful), (1728, 288));
    assert!(law_refused > 0);
}

fn durable_laws() -> Vec<LawData> {
    use l::{Kind as K, Observation as O, Scope as S};
    let mut data = Vec::new();
    let mut p = Predicate::default();
    let zero = p.n(0);
    let three = p.n(3);
    let count = p.num(O::Post(110));
    let failures = p.num(O::Post(111));
    let checks = [
        p.le(zero, count),
        p.le(count, three),
        p.le(zero, failures),
        p.le(failures, three),
    ];
    let root = p.all(&checks);
    data.push(ld(500, K::StateInvariant, S::Committing, true, p, root));
    for (id, scope, variant, changed, same) in [
        (501, S::Accept, 120, 110, 111),
        (502, S::CommittedFailure, 121, 111, 110),
    ] {
        let mut p = Predicate::default();
        let allowed = p.equal_number(O::ContextRoot, 1);
        let command = p.exact(
            O::CommandRoot,
            l::Atom::Sum {
                type_id: 101,
                variant,
            },
        );
        let pre = p.num(O::Pre(changed));
        let one = p.n(1);
        let next = p.add(pre, one);
        let post = p.num(O::Post(changed));
        let incremented = p.eq(post, next);
        let retained = p.same(O::Pre(same), O::Post(same));
        let no_effects = p.equal_number(O::EffectLength, 0);
        let count = p.equal_number(O::OutboxLength, 1);
        let channel = p.equal_number(O::OutboxChannel(0), 300);
        let ordinal = p.equal_number(O::OutboxOrdinal(0), 0);
        let destination = p.exact(O::OutboxDestination(0), l::Atom::Text(b"local-observer"));
        let first = p.same(O::OutboxPayload(0, 112), O::Post(110));
        let second = p.same(O::OutboxPayload(0, 113), O::Post(111));
        let root = p.all(&[
            allowed,
            command,
            incremented,
            retained,
            no_effects,
            count,
            channel,
            ordinal,
            destination,
            first,
            second,
        ]);
        data.push(ld(id, K::AuthoritySubjectRecipient, scope, false, p, root));
    }
    let mut p = Predicate::default();
    let checks = [
        O::PostLength,
        O::PatchLength,
        O::EffectLength,
        O::OutboxLength,
    ]
    .map(|o| p.equal_number(o, 0));
    let root = p.all(&checks);
    data.push(ld(503, K::RejectNoAuthority, S::Reject, false, p, root));
    let mut p = Predicate::default();
    let root = p.equal_number(O::Reason, 202);
    data.push(ld(
        504,
        K::CommittedFailureEffects,
        S::CommittedFailure,
        false,
        p,
        root,
    ));
    let mut p = Predicate::default();
    let accept = p.equal_number(O::Class, 0);
    let has_reason = p.push(l::Op::Observe(O::HasReason));
    let expected = p.not(accept);
    let root = p.eq(expected, has_reason);
    data.push(ld(505, K::DecisionConformance, S::Always, false, p, root));
    let mut p = Predicate::default();
    let a = p.equal_number(O::Initial(110), 0);
    let b = p.equal_number(O::Initial(111), 0);
    let root = p.and(a, b);
    data.push(ld(506, K::InitialCondition, S::Always, true, p, root));
    data
}

#[test]
fn durable_counter_original_sum_bool_64_tuples_and_12_retained_examples() {
    let graph = template_graph(include_bytes!(
        "../../zeno-fcis-cli/templates/durable-counter/synthesized/program.zcve"
    ));
    let state = [int(110, 0, 3), int(111, 0, 3)];
    let command = leaf(101, &[120, 121], true);
    let context = InputLeaf::Bool;
    let bindings = [
        bind(Source::State, 110),
        bind(Source::State, 111),
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        Binding {
            source: Source::Context,
            selector: Selector::Root,
        },
    ];
    let outputs = [
        InputLeaf::I128 { min: 0, max: 3 },
        InputLeaf::I128 { min: 0, max: 3 },
        InputLeaf::I128 { min: 0, max: 3 },
        InputLeaf::Bool,
        InputLeaf::I128 { min: 0, max: 3 },
        InputLeaf::I128 { min: 0, max: 3 },
    ];
    let assignments = [ad(110, 1, 0, 3), ad(111, 2, 0, 3)];
    let payload = [
        PayloadField {
            field: 112,
            value: Expr::Output(4),
        },
        PayloadField {
            field: 113,
            value: Expr::Output(5),
        },
    ];
    let outbox = [DeliveryPlan {
        ordinal: 0,
        channel: 300,
        when: Expr::Output(3),
        destination: Expr::Constant(Atom::Text(b"local-observer")),
        payload: &payload,
        idempotency: Expr::Constant(Atom::U128(0)),
    }];
    let branches = [
        branch(0, Class::Reject, Some(200), &[], &[]),
        branch(1, Class::Reject, Some(201), &[], &[]),
        branch(2, Class::Accept, None, &assignments, &outbox),
        branch(3, Class::CommittedFailure, Some(202), &assignments, &outbox),
    ];
    let reasons = [
        Reason {
            id: 200,
            class: Class::Reject,
        },
        Reason {
            id: 201,
            class: Class::Reject,
        },
        Reason {
            id: 202,
            class: Class::CommittedFailure,
        },
    ];
    let fields = [
        TypedField {
            field: 112,
            domain: Domain::I128 { min: 0, max: 3 },
        },
        TypedField {
            field: 113,
            domain: Domain::I128 { min: 0, max: 3 },
        },
    ];
    let channels = [Channel {
        id: 300,
        destination: Domain::Text,
        payload: &fields,
        idempotency: Domain::U128 { min: 0, max: 0 },
    }];
    let data = durable_laws();
    let laws = law_refs(&data);
    let required: Vec<_> = laws.iter().map(|l| l.id).collect();
    let d = Descriptor {
        state: Schema::Record(&state),
        command: Schema::Leaf(&command),
        context: Schema::Leaf(&context),
        program: ScalarProgram {
            inputs: graph.inputs(),
            outputs: graph.outputs(),
            nodes: graph.nodes(),
            roots: graph.roots(),
        },
        bindings: &bindings,
        output_types: &outputs,
        decision_output: 0,
        branches: &branches,
        reasons: &reasons,
        channels: &channels,
        laws: &laws,
        required: &required,
        limits: limits(),
    };
    let core = c::bind(&d)
        .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
    let check = |count: i128, failures: i128, variant: u16, allowed: bool| {
        // Raw policy oracle is separate from the unchanged synthesized graph.
        let full = if variant == 120 {
            count == 3
        } else {
            failures == 3
        };
        let (class, reason, next, failed) = if !allowed {
            (Class::Reject, Some(200), count, failures)
        } else if full {
            (Class::Reject, Some(201), count, failures)
        } else if variant == 120 {
            (Class::Accept, None, count + 1, failures)
        } else {
            (Class::CommittedFailure, Some(202), count, failures + 1)
        };
        let pre = [fi(110, count), fi(111, failures)];
        let post = [fi(110, next), fi(111, failed)];
        let s = record(&pre);
        let command = atom_bytes(Atom::Sum {
            type_id: 101,
            variant,
        });
        let context = atom_bytes(Atom::Bool(allowed));
        let out = core.execute(Raw {
            state: &s,
            command: &command,
            context: &context,
        });
        let candidate = out
            .result()
            .unwrap_or_else(|error| panic!("unexpected composition oracle refusal: {error:?}"));
        check_candidate(candidate, &pre, class, reason, &post);
        assert_eq!(
            candidate.outbox().len(),
            usize::from(class != Class::Reject)
        );
        if let Some(delivery) = candidate.outbox().first() {
            assert_eq!(
                (
                    delivery.ordinal,
                    delivery.channel,
                    delivery.destination,
                    delivery.idempotency
                ),
                (0, 300, Atom::Text(b"local-observer"), Atom::U128(0))
            );
            assert_eq!(delivery.payload, [fi(112, next), fi(113, failed)]);
        }
        assert_eq!(
            out.reads()
                .iter()
                .map(|r| (r.source, r.selector))
                .collect::<Vec<_>>(),
            [
                (0, Selector::Field(110)),
                (0, Selector::Field(111)),
                (1, Selector::Root),
                (2, Selector::Root)
            ]
        );
        report_costs(&out, &d, class, false);
        (class, reason, next, failed)
    };
    let mut checked = 0;
    for count in 0..=3 {
        for failures in 0..=3 {
            for variant in [120, 121] {
                for allowed in [false, true] {
                    check(count, failures, variant, allowed);
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 64);
    let examples =
        include_str!("../../zeno-fcis-cli/templates/durable-counter/tests/decision-examples.txt");
    let mut checked = 0;
    for line in examples
        .lines()
        .filter(|s| !s.starts_with('#') && !s.trim().is_empty())
    {
        let parts: Vec<_> = line.split('|').collect();
        let v: Vec<i128> = parts[0]
            .split_whitespace()
            .map(|v| {
                v.parse().unwrap_or_else(|error| {
                    panic!("unexpected composition oracle refusal: {error:?}")
                })
            })
            .collect();
        let (class, reason, next, failed) = check(v[0], v[1], v[2] as u16, v[3] != 0);
        let expected = format!(
            "{} {} {next} {failed}",
            match class {
                Class::Accept => "accept",
                Class::Reject => "reject",
                Class::CommittedFailure => "failure",
                _ => panic!("unsupported future decision class in the original oracle"),
            },
            reason.map(|r| r.to_string()).unwrap_or_else(|| "-".into())
        );
        assert_eq!(parts[1].trim(), expected);
        assert_eq!(
            parts[2].trim(),
            if class == Class::Reject {
                "-".into()
            } else {
                format!("300 {next} {failed}")
            }
        );
        checked += 1;
    }
    assert_eq!(checked, 12);
    for count in 0..=3 {
        for failures in 0..=3 {
            let s = record(&[fi(110, count), fi(111, failures)]);
            assert_eq!(
                core.genesis(&s).result().is_ok(),
                count == 0 && failures == 0
            );
        }
    }
}
