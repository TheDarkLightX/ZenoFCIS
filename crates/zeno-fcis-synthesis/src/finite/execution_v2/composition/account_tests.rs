//! Migrated account raw-boundary oracle over the surviving ingress/ABI/evaluator.
use super::super::{InputVariant, ScalarProgram};
use super::*;
use crate::finite::evaluation::Op;
use alloc::{vec, vec::Vec};

pub(super) fn program() -> ScalarProgram<'static> {
    ScalarProgram {
        inputs: &[
            ScalarDomain::Int { min: 0, max: 2 },
            ScalarDomain::Int {
                min: 0,
                max: 4102445700,
            },
            ScalarDomain::Int {
                min: 0,
                max: 4102444800,
            },
            ScalarDomain::Int { min: 120, max: 122 },
            ScalarDomain::Int {
                min: 0,
                max: 4102444800,
            },
            ScalarDomain::Bool,
        ],
        outputs: &[
            ScalarDomain::Int { min: 0, max: 6 },
            ScalarDomain::Int {
                min: -9223372036854775808,
                max: 9223372036854775807,
            },
            ScalarDomain::Int {
                min: -9223372036854775808,
                max: 9223372036854775807,
            },
        ],
        nodes: &[
            Op::Input(4),
            Op::Input(2),
            Op::Lt(0, 1),
            Op::Int(0),
            Op::Input(3),
            Op::Int(122),
            Op::Eq(4, 5),
            Op::Not(6),
            Op::Input(1),
            Op::Lt(0, 8),
            Op::And(7, 9),
            Op::Int(1),
            Op::Input(5),
            Op::Not(12),
            Op::And(6, 13),
            Op::Int(2),
            Op::Int(120),
            Op::Eq(4, 16),
            Op::Int(3),
            Op::Int(121),
            Op::Eq(4, 19),
            Op::Input(0),
            Op::Eq(21, 15),
            Op::And(20, 22),
            Op::Int(4),
            Op::Int(5),
            Op::Int(6),
            Op::Select(20, 25, 26),
            Op::Select(23, 24, 27),
            Op::Select(17, 18, 28),
            Op::Select(14, 15, 29),
            Op::Select(10, 11, 30),
            Op::Select(2, 3, 31),
            Op::Int(900),
            Op::Add(0, 33),
            Op::Add(21, 11),
        ],
        roots: &[32, 34, 35],
    }
}

#[derive(Debug)]
struct Facts {
    raw: (i128, i128, i128, u16, i128, bool),
    branch: i64,
    increment: i128,
    deadline: i128,
}
impl Facts {
    fn original(&self) -> (i128, i128, i128, u16, i128, bool) {
        self.raw
    }
    fn computed(&self) -> (i128, i128) {
        (self.increment, self.deadline)
    }
}
// Test-only stage fixture. Production BoundCore and actual emitted policy are
// independently exercised by the retained 14,580-tuple public composition test.
fn execute_into(
    state: &[u8],
    command: &[u8],
    context: &[u8],
    meter: &mut Meter,
    attempts: &mut Vec<ReadAttempt>,
) -> Result<Facts, Failure> {
    let sf = [
        InputField {
            id: 110,
            leaf: InputLeaf::I128 { min: 0, max: 2 },
        },
        InputField {
            id: 111,
            leaf: InputLeaf::I128 {
                min: 0,
                max: 4102445700,
            },
        },
        InputField {
            id: 112,
            leaf: InputLeaf::I128 {
                min: 0,
                max: 4102444800,
            },
        },
    ];
    let cl = InputLeaf::Sum {
        type_id: 101,
        min: 120,
        max: 122,
        variants: vec![
            InputVariant { id: 120, code: 120 },
            InputVariant { id: 121, code: 121 },
            InputVariant { id: 122, code: 122 },
        ],
    };
    let xf = [
        InputField {
            id: 130,
            leaf: InputLeaf::I128 {
                min: 0,
                max: 4102444800,
            },
        },
        InputField {
            id: 131,
            leaf: InputLeaf::Bool,
        },
    ];
    let bindings = [
        Binding {
            source: Source::State,
            selector: Selector::Field(110),
        },
        Binding {
            source: Source::State,
            selector: Selector::Field(111),
        },
        Binding {
            source: Source::State,
            selector: Selector::Field(112),
        },
        Binding {
            source: Source::Command,
            selector: Selector::Root,
        },
        Binding {
            source: Source::Context,
            selector: Selector::Field(130),
        },
        Binding {
            source: Source::Context,
            selector: Selector::Field(131),
        },
    ];
    let p = program();
    let d = Descriptor {
        state: Schema::Record(&sf),
        command: Schema::Leaf(&cl),
        context: Schema::Record(&xf),
        program: p,
        bindings: &bindings,
        output_types: &[],
        decision_output: 0,
        branches: &[],
        reasons: &[],
        channels: &[],
        laws: &[],
        required: &[],
        limits: meter.limits,
    };
    assert!(admission::bindings(&d));
    let s =
        ingress::project(state, d.state, 0, meter, attempts).map_err(|e| Failure::Ingress(0, e))?;
    let c = ingress::project(command, d.command, 1, meter, attempts)
        .map_err(|e| Failure::Ingress(1, e))?;
    let x = ingress::project(context, d.context, 2, meter, attempts)
        .map_err(|e| Failure::Ingress(2, e))?;
    let input = producer::tuple(&d, &s, &c, &x).unwrap_or_else(|| panic!("complete ABI"));
    let mut output = Vec::new();
    let mut scratch = Vec::new();
    super::super::evaluate_into(
        d.program.inputs,
        d.program.outputs,
        d.program.nodes,
        d.program.roots,
        &input,
        meter,
        &mut scratch,
        &mut output,
    )
    .map_err(Failure::Execution)?;
    assert_eq!(output.len(), 3);
    Ok(Facts {
        raw: (
            input[0] as i128,
            input[1] as i128,
            input[2] as i128,
            input[3] as u16,
            input[4] as i128,
            input[5] != 0,
        ),
        branch: output[0],
        deadline: output[1] as i128,
        increment: output[2] as i128,
    })
}
struct Report(Result<Facts, Failure>, Usage, Vec<ReadAttempt>);
impl Report {
    fn into_parts(self) -> (Result<Facts, Failure>, Usage, Vec<ReadAttempt>) {
        (self.0, self.1, self.2)
    }
}
fn execute(s: &[u8], c: &[u8], x: &[u8], limits: Limits) -> Report {
    let mut m = super::super::meter::new(limits);
    let mut a = Vec::new();
    let result = execute_into(s, c, x, &mut m, &mut a);
    Report(result, m.used, a)
}

fn encoded_field(id: u16, n: i128) -> Vec<u8> {
    let mut b = id.to_be_bytes().to_vec();
    b.push(4);
    b.extend_from_slice(&n.to_be_bytes());
    b
}
fn record(fields: &[(u16, i128)]) -> Vec<u8> {
    let mut b = vec![9];
    b.extend_from_slice(&(fields.len() as u32).to_be_bytes());
    for &(id, n) in fields {
        b.extend(encoded_field(id, n));
    }
    b
}
fn wire(
    failed: i128,
    until: i128,
    seen: i128,
    cmd: u16,
    now: i128,
    admin: bool,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let state = record(&[(110, failed), (111, until), (112, seen)]);
    let mut command = vec![10];
    command.extend_from_slice(&101u32.to_be_bytes());
    command.extend_from_slice(&cmd.to_be_bytes());
    command.push(0);
    let mut context = vec![9, 0, 0, 0, 2];
    context.extend(encoded_field(130, now));
    context.extend_from_slice(&131u16.to_be_bytes());
    context.push(if admin { 2 } else { 1 });
    (state, command, context)
}
fn limits() -> Limits {
    super::super::zero_limits()
        .with_limit(Resource::Byte, u64::MAX)
        .with_limit(Resource::Read, u64::MAX)
        .with_limit(Resource::Step, u64::MAX)
}
fn run(input: (i128, i128, i128, u16, i128, bool)) -> Facts {
    let (a, b, c) = wire(input.0, input.1, input.2, input.3, input.4, input.5);
    let (r, u, requests) = execute(&a, &b, &c, limits()).into_parts();
    assert_eq!(u.used(Resource::Byte), (a.len() + b.len() + c.len()) as u64);
    assert_eq!(u.used(Resource::Read), 6);
    assert_eq!(u.used(Resource::Step), program().nodes.len() as u64);
    assert_eq!(
        requests
            .iter()
            .map(|x| (x.source, x.selector, x.permitted))
            .collect::<Vec<_>>(),
        vec![
            (0, Selector::Field(110), true),
            (0, Selector::Field(111), true),
            (0, Selector::Field(112), true),
            (1, Selector::Root, true),
            (2, Selector::Field(130), true),
            (2, Selector::Field(131), true)
        ]
    );
    let facts = r.unwrap_or_else(|e| panic!("expected facts: {e:?}"));
    assert_eq!(facts.original(), input);
    facts
}
// Independent complete raw rule oracle: (class,reason,post triple,alert kind/deadline).
type Expected = (u8, u16, (i128, i128, i128), Option<(u16, i128)>);
fn oracle(p: (i128, i128, i128, u16, i128, bool)) -> Expected {
    let (n, u, s, c, t, a) = p;
    if t < s {
        return (0, 200, (n, u, s), None);
    }
    if c != 122 && t < u {
        return (0, 201, (n, u, s), None);
    }
    if c == 122 && !a {
        return (0, 202, (n, u, s), None);
    }
    match c {
        120 => (1, 0, (0, u, t), None),
        121 if n < 2 => (2, 203, (n + 1, u, t), None),
        121 => (2, 203, (0, t + 900, t), Some((150, t + 900))),
        _ => (1, 0, (0, 0, t), Some((151, 0))),
    }
}
fn reconstruct(f: Facts) -> Expected {
    let (n, u, s, _, t, _) = f.original();
    let (inc, deadline) = f.computed();
    match f.branch {
        0 => (0, 200, (n, u, s), None),
        1 => (0, 201, (n, u, s), None),
        2 => (0, 202, (n, u, s), None),
        3 => (1, 0, (0, u, t), None),
        4 => (2, 203, (0, deadline, t), Some((150, deadline))),
        5 => (2, 203, (inc, u, t), None),
        6 => (1, 0, (0, 0, t), Some((151, 0))),
        _ => panic!("invalid branch"),
    }
}
#[test]
fn complete_boundary_grid_matches_independent_raw_rules() {
    let times = [
        0, 1, 899, 900, 901, 4102443899, 4102443900, 4102444799, 4102444800,
    ];
    let deadlines = [
        0, 1, 899, 900, 901, 4102444799, 4102444800, 4102444801, 4102445699, 4102445700,
    ];
    for n in 0..=2 {
        for u in deadlines {
            for s in times {
                for t in times {
                    for c in 120..=122 {
                        for a in [false, true] {
                            let input = (n, u, s, c, t, a);
                            assert_eq!(reconstruct(run(input)), oracle(input), "{input:?}");
                        }
                    }
                }
            }
        }
    }
}
#[test]
fn original_seen_distinguishes_minimal_counterexample() {
    assert_ne!(
        reconstruct(run((0, 0, 0, 120, 0, false))),
        reconstruct(run((0, 0, 1, 120, 0, false)))
    );
}
#[test]
fn schema_refuses_every_outside_boundary_without_clamping() {
    for p in [
        (i128::MIN, 0, 0, 120, 0, false),
        (3, 0, 0, 120, 0, false),
        (0, -1, 0, 120, 0, false),
        (0, 4102445701, 0, 120, 0, false),
        (0, 0, -1, 120, 0, false),
        (0, 0, i128::MAX, 120, 0, false),
        (0, 0, 0, 119, 0, false),
        (0, 0, 0, 123, 0, false),
        (0, 0, 0, 120, -1, false),
        (0, 0, 0, 120, 4102444801, false),
    ] {
        let (a, b, c) = wire(p.0, p.1, p.2, p.3, p.4, p.5);
        let (r, u, _) = execute(&a, &b, &c, limits()).into_parts();
        assert!(r.is_err());
        assert_eq!(u.used(Resource::Step), 0);
    }
}
#[test]
fn malformed_original_bytes_and_every_charge_boundary_refuse() {
    let (a, b, c) = wire(2, 900, 899, 121, 900, false);
    for source in 0..3 {
        let originals = [a.clone(), b.clone(), c.clone()];
        for end in 0..originals[source].len() {
            let mut slices = [a.as_slice(), b.as_slice(), c.as_slice()];
            slices[source] = &originals[source][..end];
            assert!(
                execute(slices[0], slices[1], slices[2], limits())
                    .into_parts()
                    .0
                    .is_err()
            );
        }
    }
    for source in 0..3 {
        let mut originals = [a.clone(), b.clone(), c.clone()];
        originals[source].push(0);
        assert!(
            execute(&originals[0], &originals[1], &originals[2], limits())
                .into_parts()
                .0
                .is_err()
        );
    }
    for quota in 0..6 {
        let (r, u, attempts) =
            execute(&a, &b, &c, limits().with_limit(Resource::Read, quota)).into_parts();
        assert!(matches!(r, Err(Failure::Ingress(_, _))));
        assert_eq!(u.used(Resource::Read), quota);
        assert_eq!(attempts.len(), quota as usize + 1);
        assert!(
            !attempts
                .last()
                .unwrap_or_else(|| panic!("expected attempt"))
                .permitted
        );
        assert_eq!(u.used(Resource::Step), 0);
    }
    for quota in 0..program().nodes.len() as u64 {
        let (r, u, attempts) =
            execute(&a, &b, &c, limits().with_limit(Resource::Step, quota)).into_parts();
        assert!(matches!(
            r,
            Err(Failure::Execution(super::super::Failure::Budget(_)))
        ));
        assert_eq!(u.used(Resource::Step), quota);
        assert_eq!(attempts.len(), 6);
    }
    for quota in [0, a.len() as u64, (a.len() + b.len()) as u64] {
        let (r, u, _) =
            execute(&a, &b, &c, limits().with_limit(Resource::Byte, quota)).into_parts();
        assert!(matches!(r, Err(Failure::Ingress(_, _))));
        assert_eq!(u.used(Resource::Byte), quota);
    }
}
#[test]
fn same_private_meter_preserves_arbitrary_prefix_and_overflow() {
    let (a, b, c) = wire(2, 0, 0, 121, 4102444800, true);
    let initial = [7, 11, 13, 17, 19, 23, 29, 31];
    let mut meter = super::super::meter::Meter {
        limits: Limits {
            counters: [u64::MAX; 8],
        },
        used: Usage { counters: initial },
    };
    let prefix = ReadAttempt {
        source: 255,
        selector: Selector::Field(0),
        permitted: false,
    };
    let mut attempts = vec![prefix];
    assert!(execute_into(&a, &b, &c, &mut meter, &mut attempts).is_ok());
    assert_eq!(attempts[0], prefix);
    assert_eq!(
        meter.used.counters,
        [
            13,
            11,
            13,
            17,
            19 + (a.len() + b.len() + c.len()) as u64,
            23,
            29,
            31 + program().nodes.len() as u64
        ]
    );
    meter.used.counters[0] = u64::MAX;
    let before = meter.used.counters;
    assert!(matches!(
        execute_into(&a, &b, &c, &mut meter, &mut attempts),
        Err(Failure::Ingress(
            0,
            ingress::Failure::Record(super::super::RecordFailure::Budget(
                super::super::MeterFailure { overflow: true, .. }
            ))
        ))
    ));
    assert_eq!(meter.used.counters[0], before[0]);
    assert_eq!(meter.used.counters[7], before[7]);
    assert!(
        !attempts
            .last()
            .unwrap_or_else(|| panic!("expected attempt"))
            .permitted
    );
}

#[test]
fn all_48_fact_classes_and_eager_numeric_extremes() {
    let mut count = 0;
    for command in 120..=122 {
        for admin in [false, true] {
            for backwards in [false, true] {
                for locked in [false, true] {
                    for third in [false, true] {
                        let input = (
                            if third { 2 } else { 0 },
                            if locked { 2 } else { 0 },
                            if backwards { 2 } else { 0 },
                            command,
                            1,
                            admin,
                        );
                        assert_eq!(reconstruct(run(input)), oracle(input));
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 48);
    for failed in 0..=2 {
        for now in [
            0, 1, 4102443899, 4102443900, 4102443901, 4102444799, 4102444800,
        ] {
            let input = (failed, 0, now, 121, now, false);
            let facts = run(input);
            assert_eq!(facts.computed(), (failed + 1, now + 900));
            assert_eq!(reconstruct(facts), oracle(input));
        }
    }
    let facts = run((2, 0, 0, 121, 4102444800, false));
    assert_eq!(facts.computed(), (3, 4102445700));
    for now in [
        4102444801,
        4102445700,
        i128::MAX - 900,
        i128::MAX - 899,
        i128::MAX,
    ] {
        let (s, c, x) = wire(0, 0, 0, 120, now, false);
        let (result, usage, _) = execute(&s, &c, &x, limits()).into_parts();
        assert!(matches!(result, Err(Failure::Ingress(2, _))));
        assert_eq!(usage.used(Resource::Step), 0);
    }
}
