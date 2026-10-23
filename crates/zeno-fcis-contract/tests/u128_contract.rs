//! Full-width unsigned core, refusal and original-wire regression witnesses.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use finite::{canonical_v2::output, v2_composition as c};
use serde_json::json;
use zeno_fcis_contract::internals::{
    declarations::Declarations, model::Contract, policy, rules::Rules, schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{
    self, V2IntegerWidth as Width, V2NumericAtom as Atom, V2NumericDomain as Domain,
    V2NumericOp as Op, V2NumericProgram as Program, V2Resource as Resource,
};
const M: u128 = u128::MAX;

// The reviewer's signed-only project: /4 is an explicit opt-in to strict kinds.
const SIGNED_PROJECT: &str = "zeno 1; project 1 unused_unsigned; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 int Amount in 0..=1; field 110 100 amount 105; field 120 101 amount 105; component 400 core { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; } merge [400]; law 500 bounded on commit, genesis = post.100.110 >= 0;";

fn signed_policy(version: u8) -> serde_json::Value {
    json!({"schema":format!("zeno-fcis/template-declarative-policy/{version}"),"template":"unused-unsigned","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":"command.101.120"},"outbox":[]}],"genesis":{"110":0},"law_kinds":{"500":"StateInvariant"}})
}

fn bind_signed_policy(source: &str, value: &serde_json::Value) -> Result<Vec<u8>, String> {
    let r = Rules::read(&value.to_string()).map_err(|e| e.to_string())?;
    let d = Declarations::read(source, &r.leaf_bindings).map_err(|e| e.to_string())?;
    let s = schema::encode(&d).map_err(|e| e.to_string())?;
    Contract::build(&d, &r, schema_commitment(&s).unwrap())
        .and_then(|c| policy::encode(&c, &s))
        .map_err(|e| e.to_string())
}

#[test]
fn u128_policy4_checks_every_unused_binding_with_live_kinds() {
    for (expression, reason) in [
        ("choose(true, u128(0), 1)", "KindMismatch"),
        ("to_u128(context.102)", "U128CastKindMismatch"),
        ("choose(1, 0, 1)", "boolean"),
        ("choose(true, context.102, 1)", "mismatch"),
    ] {
        let mut r = signed_policy(4);
        r["variables"]["bad"] = json!(expression);
        let result = bind_signed_policy(SIGNED_PROJECT, &r);
        assert!(result.is_err(), "policy/4 admitted unused {expression}");
        let error = result.unwrap_err();
        assert!(error.contains(reason), "{expression}: {error}");
    }
    // Quantity inference alone treats a sum code as a number; the live graph
    // must still refuse its public cast, even when this binding is unused.
    let source = format!(
        "{SIGNED_PROJECT} type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 121 101 mode 109;"
    );
    let mut r = signed_policy(4);
    r["variables"]["bad"] = json!("to_u128(command.101.121)");
    assert!(
        bind_signed_policy(&source, &r)
            .unwrap_err()
            .contains("U128CastKindMismatch")
    );
}

#[test]
fn u128_policy4_inference_uses_explicit_mode_on_signed_declarations() {
    use zeno_fcis_contract::internals::{expr, quantity};
    let r = Rules::read(&signed_policy(4).to_string()).unwrap();
    let d = Declarations::read(SIGNED_PROJECT, &r.leaf_bindings).unwrap();
    assert!(!d.unsigned());
    for expression in ["choose(true, u128(0), 1)", "to_u128(context.102)"] {
        let ast = expr::parse(expression).unwrap();
        assert!(
            quantity::lower(&d, &ast).is_ok(),
            "legacy inference changed"
        );
        assert!(quantity::lower_with_strict(&d, &ast, true).is_err());
    }
}

#[test]
fn u128_policy4_valid_unused_bindings_do_not_enter_the_decision_graph() {
    let plain = signed_policy(4);
    let mut extra = plain.clone();
    extra["variables"] = json!({
        "flag":"context.102",
        "converted":"to_u128(command.101.120)",
        "arithmetic_trap":"to_u128(-1)",
        "alias":"choose(flag, converted, u128(0))"
    });
    assert_eq!(
        bind_signed_policy(SIGNED_PROJECT, &plain).unwrap(),
        bind_signed_policy(SIGNED_PROJECT, &extra).unwrap()
    );
}

#[test]
fn u128_policy4_signed_only_opt_in_binds_the_selected_profile() {
    let mut policies = Vec::new();
    let mut identities = Vec::new();
    for version in [2, 3, 4] {
        let value = signed_policy(version);
        let r = Rules::read(&value.to_string()).unwrap();
        let d = Declarations::read(SIGNED_PROJECT, &r.leaf_bindings).unwrap();
        assert!(!d.unsigned());
        let s = schema::encode(&d).unwrap();
        let c = Contract::build(&d, &r, schema_commitment(&s).unwrap()).unwrap();
        assert_eq!(c.unsigned, version == 4);
        assert_eq!(c.program().is_ok(), version != 4);
        policies.push(policy::encode(&c, &s).unwrap());
        identities.push(if version == 4 {
            policy::with_numeric_authority(&c, &s, |a| a.identity().to_vec()).unwrap()
        } else {
            policy::with_authority(&c, &s, |a| a.identity().to_vec()).unwrap()
        });
    }
    assert_eq!(policies[0], policies[1]);
    assert_ne!(policies[0], policies[2]);
    assert_eq!(identities[0], identities[1]);
    assert_ne!(identities[0], identities[2]);
}

fn limits() -> finite::V2Limits {
    finite::v2_zero_limits()
        .with_limit(Resource::Read, 10000)
        .with_limit(Resource::Write, 10000)
        .with_limit(Resource::Step, 10000)
}
fn numeric(op: Op, input: &[Atom<'static>], signed: bool) -> Option<Atom<'static>> {
    let inputs: Vec<_> = input
        .iter()
        .map(|a| match a {
            Atom::I128(_) => Domain::I128 {
                min: i128::MIN,
                max: i128::MAX,
            },
            Atom::U128(_) => Domain::U128 { min: 0, max: M },
            _ => Domain::Bool,
        })
        .collect();
    let mut nodes: Vec<_> = (0..input.len()).map(|i| Op::Input(i as u16)).collect();
    nodes.push(op);
    let outputs = [if signed {
        Domain::I128 {
            min: i128::MIN,
            max: i128::MAX,
        }
    } else {
        Domain::U128 { min: 0, max: M }
    }];
    let roots = [(nodes.len() - 1) as u16];
    let program = Program {
        inputs: &inputs,
        outputs: &outputs,
        nodes: &nodes,
        roots: &roots,
    };
    assert!(program.admit().is_ok());
    let (result, usage) = finite::execute_numeric_v2(program, input, limits()).into_parts();
    assert_eq!(usage.used(Resource::Step), nodes.len() as u64);
    if let Err(error) = result {
        assert_eq!(error, finite::V2NumericFailure::Arithmetic);
        None
    } else {
        result.ok().map(|v| v[0])
    }
}
fn compare(actual: Option<Atom<'static>>, expected: Option<Atom<'static>>, witness: &str) {
    println!("U128_WITNESS {witness} admitted actual={actual:?} expected={expected:?}");
    if std::env::var_os("U128_DISABLE_COMPARISON").is_none() {
        assert_eq!(actual, expected, "U128_ORACLE_MISMATCH {witness}");
    }
}
#[test]
fn u128_checked_arithmetic_oracle() {
    compare(
        numeric(
            Op::Add(Width::Unsigned128, 0, 1),
            &[Atom::U128(M), Atom::U128(1)],
            false,
        ),
        None,
        "wrapping-add",
    );
    compare(
        numeric(
            Op::Add(Width::Unsigned128, 0, 1),
            &[Atom::U128(M - 1), Atom::U128(1)],
            false,
        ),
        Some(Atom::U128(M)),
        "valid-add",
    );
}
#[test]
fn u128_conversion_boundaries() {
    for n in [i128::MIN, -1, 0, 1, i128::MAX] {
        compare(
            numeric(Op::ToU128(0), &[Atom::I128(n)], false),
            u128::try_from(n).ok().map(Atom::U128),
            "signed-to-unsigned",
        );
    }
    for n in [0, 1, i128::MAX as u128, 1u128 << 127, M] {
        compare(
            numeric(Op::ToI128(0), &[Atom::U128(n)], true),
            i128::try_from(n).ok().map(Atom::I128),
            "unsigned-to-signed",
        );
    }
}
#[test]
fn u128_fused_wide_oracle() {
    compare(
        numeric(
            Op::MulDivFloor(0, 1, 2),
            &[Atom::U128(M), Atom::U128(M), Atom::U128(M)],
            false,
        ),
        Some(Atom::U128(M)),
        "narrowed-product",
    );
    compare(
        numeric(
            Op::MulDivCeil(0, 1, 2),
            &[Atom::U128(M - 1), Atom::U128(M - 1), Atom::U128(M - 2)],
            false,
        ),
        None,
        "ceil-only-overflow",
    );
}
fn project() -> String {
    format!(
        "zeno 1; project 1 unsigned_core; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 u128 Amount in 0..={M}; field 110 100 result 105; field 120 101 left 105; field 121 101 right 105; field 122 101 divisor 105; component 400 unsigned {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400]; law 500 bounded on commit, genesis = post.100.110 >= u128(0);"
    )
}
fn rules(expr: &str) -> Rules {
    Rules::read(&json!({"schema":"zeno-fcis/template-declarative-policy/4","template":"unsigned-core","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":expr},"outbox":[]}],"genesis":{"110":"u128(0)"},"law_kinds":{"500":"StateInvariant"}}).to_string()).unwrap()
}
fn record(c: &Contract<'_>, i: usize, fields: &[(u16, u128)]) -> Vec<u8> {
    let f: Vec<_> = fields
        .iter()
        .map(|(id, n)| c::Field {
            id: *id,
            value: Atom::U128(*n),
        })
        .collect();
    let b = output::encode_record(&f, c.frame_bytes[i] as usize).unwrap();
    output::encode_envelope(100 + i as u32, &c.commitment, &b, c.frame_bytes[i] as usize).unwrap()
}
#[test]
fn u128_authority_full_width_original_output() {
    let rules = rules("mul_div_floor(command.101.120, command.101.121, command.101.122)");
    let d = Declarations::read(&project(), &rules.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    let c = Contract::build(&d, &rules, schema_commitment(&s).unwrap()).unwrap();
    let state = record(&c, 0, &[(110, 0)]);
    let command = record(&c, 1, &[(120, M), (121, M), (122, M)]);
    let payload = output::encode_atom(Atom::Bool(false), c.frame_bytes[2] as usize).unwrap();
    let context =
        output::encode_envelope(102, &c.commitment, &payload, c.frame_bytes[2] as usize).unwrap();
    policy::with_numeric_authority(&c, &s, |a| {
        let result = a.evaluate(c::Raw {
            state: &state,
            command: &command,
            context: &context,
        });
        let decision = result.result().unwrap();
        assert_eq!(decision.post()[0].value, Atom::U128(M));
        assert_eq!(state, record(&c, 0, &[(110, 0)]));
    })
    .unwrap();
    assert!(policy::encode(&c, &s).is_ok());
    assert_eq!(c.program().unwrap_err().reason(), "U128SubjectUnsupported");
}

#[test]
fn independent_big_integer_oracle() {
    for line in include_str!("test-data/u128/oracle.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let v: Vec<_> = line.split('\t').collect();
        let kind = v[0];
        let a = if kind == "to_u128" {
            Atom::I128(v[1].parse().unwrap())
        } else {
            Atom::U128(v[1].parse().unwrap())
        };
        let b = Atom::U128(v[2].parse().unwrap());
        let d = Atom::U128(v[3].parse().unwrap());
        let op = match kind {
            "add" => Op::Add(Width::Unsigned128, 0, 1),
            "sub" => Op::Sub(Width::Unsigned128, 0, 1),
            "mul" => Op::Mul(Width::Unsigned128, 0, 1),
            "div_floor" => Op::DivFloor(Width::Unsigned128, 0, 2),
            "div_ceil" => Op::DivCeil(Width::Unsigned128, 0, 2),
            "floor" => Op::MulDivFloor(0, 1, 2),
            "ceil" => Op::MulDivCeil(0, 1, 2),
            "to_i128" => Op::ToI128(0),
            "to_u128" => Op::ToU128(0),
            _ => panic!("unknown oracle operation"),
        };
        let expected = if v[4] == "none" {
            None
        } else if kind == "to_i128" {
            Some(Atom::I128(v[4].parse().unwrap()))
        } else {
            Some(Atom::U128(v[4].parse().unwrap()))
        };
        assert_eq!(
            numeric(op, &[a, b, d], kind == "to_i128"),
            expected,
            "{line}"
        );
    }
}

#[test]
fn u128_kinds_eager_refusal_and_retry_usage() {
    use finite::V2NumericFailure as Failure;
    let inputs = [
        Domain::U128 { min: 0, max: M },
        Domain::U128 { min: 0, max: M },
    ];
    let outputs = [Domain::U128 { min: 0, max: M }];
    let nodes = [
        Op::Input(0),
        Op::Input(1),
        Op::Add(Width::Unsigned128, 0, 1),
        Op::U128(7),
    ];
    let roots = [3];
    let program = Program {
        inputs: &inputs,
        outputs: &outputs,
        nodes: &nodes,
        roots: &roots,
    };
    assert!(program.admit().is_ok());
    let data = [Atom::U128(M), Atom::U128(1)];
    let copy = data;
    let (first, usage) = finite::execute_numeric_v2(program, &data, limits()).into_parts();
    let (second, retry) = finite::execute_numeric_v2(program, &data, limits()).into_parts();
    assert_eq!(first, Err(Failure::Arithmetic));
    assert_eq!(first, second);
    assert_eq!(usage, retry);
    assert_eq!(usage.used(Resource::Step), 3);
    assert_eq!(usage.used(Resource::Read), 0);
    assert_eq!(copy, data);
    let kinds = [Op::Input(0), Op::I128(1), Op::Add(Width::Unsigned128, 0, 1)];
    assert!(
        Program {
            nodes: &kinds,
            roots: &[2],
            ..program
        }
        .admit()
        .is_err()
    );
    let cast = [Op::Input(0), Op::ToU128(0)];
    assert!(
        Program {
            nodes: &cast,
            roots: &[1],
            ..program
        }
        .admit()
        .is_err()
    );
}
#[test]
fn u128_parser_ranges_literal_and_version_boundaries() {
    use zeno_fcis_spec::{ProjectLimits, SourceLimits, elaborate_project, parse_project};
    let p = project();
    let spec = elaborate_project(
        parse_project(&p, SourceLimits::default()).unwrap(),
        ProjectLimits::default(),
    )
    .unwrap();
    let b = spec.canonical_bytes().unwrap();
    assert_eq!(
        &b[b"ZFCIS-PROJECT-SPEC\0".len()..][..2],
        &3u16.to_be_bytes()
    );
    for bad in ["340282366920938463463374607431768211456", "-1", "01", "+1"] {
        let p = p.replace(&format!("0..={M}"), &format!("0..={bad}"));
        assert!(parse_project(&p, SourceLimits::default()).is_err(), "{bad}")
    }
    for lit in [
        "u128(-0)",
        "u128(-1)",
        "u128(+1)",
        "u128(01)",
        "u128(340282366920938463463374607431768211456)",
    ] {
        assert!(
            zeno_fcis_contract::internals::expr::parse(lit).is_err(),
            "{lit}"
        );
    }
}
#[test]
fn u128_original_publication_replay_refuses_changed_bytes() {
    use finite::v2_authority::PublicationOutcome;
    let r = rules("command.101.120");
    let d = Declarations::read(&project(), &r.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    let c = Contract::build(&d, &r, schema_commitment(&s).unwrap()).unwrap();
    let st = record(&c, 0, &[(110, 0)]);
    let co = record(&c, 1, &[(120, M), (121, 1), (122, 1)]);
    let b = output::encode_atom(Atom::Bool(false), c.frame_bytes[2] as usize).unwrap();
    let cx = output::encode_envelope(102, &c.commitment, &b, c.frame_bytes[2] as usize).unwrap();
    policy::with_numeric_authority(&c, &s, |a| {
        let raw = c::Raw {
            state: &st,
            command: &co,
            context: &cx,
        };
        let PublicationOutcome::Commit(p) = a.publish(raw) else {
            panic!("publication refused")
        };
        assert!(
            p.poststate()
                .windows(17)
                .any(|b| b[0] == 3 && b[1..] == M.to_be_bytes())
        );
        assert!(matches!(
            a.replay_publication(raw, p.subject()),
            PublicationOutcome::Commit(_)
        ));
        let mut changed = p.subject().to_vec();
        let n = changed.len();
        changed[n - 1] ^= 1;
        let replay = a.replay_publication(raw, &changed);
        assert!(!matches!(replay, PublicationOutcome::Commit(_)));
        assert_eq!(replay.evaluation().usage(), p.evaluation().usage());
        assert!(matches!(
            a.publish_genesis(&st),
            PublicationOutcome::Commit(_)
        ));
    })
    .unwrap();
}
#[test]
fn u128_independent_384_bit_products() {
    let inputs = [Domain::U128 { min: 0, max: M }; 6];
    let outputs = [Domain::Bool];
    let nodes = [
        Op::Input(0),
        Op::Input(1),
        Op::Input(2),
        Op::Input(3),
        Op::Input(4),
        Op::Input(5),
        Op::ProductsGe([0, 1, 2], [3, 4, 5]),
    ];
    let roots = [6];
    let p = Program {
        inputs: &inputs,
        outputs: &outputs,
        nodes: &nodes,
        roots: &roots,
    };
    assert!(p.admit().is_ok());
    for line in include_str!("test-data/u128/products.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let words: Vec<_> = line.split('\t').collect();
        let input: Vec<_> = words[..6]
            .iter()
            .map(|v| Atom::U128(v.parse().unwrap()))
            .collect();
        let result = finite::execute_numeric_v2(p, &input, limits())
            .into_parts()
            .0
            .unwrap();
        assert_eq!(result, [Atom::Bool(words[6] == "1")], "{line}");
    }
}
fn scaled_project(scale: u8, left_max: u128) -> String {
    format!(
        "zeno 1; project 1 unsigned_scales; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 u128 Amount in 0..={M} unit 1 scale e{scale}; type 106 u128 Left in 0..={left_max} unit 1 scale e0; type 107 u128 Right in 0..={M} unit 1 scale e0; type 108 u128 Denom in 0..={M} unit 1 scale e0; field 110 100 result 105; field 120 101 left 106; field 121 101 right 107; field 122 101 divisor 108; component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400]; law 500 bounded on commit, genesis = post.100.110 >= u128(0);"
    )
}
#[test]
fn u128_units_all_scales_independent_rational_oracle() {
    for line in include_str!("test-data/u128/scales.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let w: Vec<_> = line.split('\t').collect();
        let q: u8 = w[0].parse().unwrap();
        let vals: Vec<u128> = w[2..].iter().map(|v| v.parse().unwrap()).collect();
        let source = scaled_project(q, vals[0]);
        let r = rules(&format!(
            "mul_div_{}(command.101.120, command.101.121, command.101.122, e{q})",
            w[1]
        ));
        let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
        let schema = schema::encode(&d).unwrap();
        let c = Contract::build(&d, &r, schema_commitment(&schema).unwrap()).unwrap();
        let st = record(&c, 0, &[(110, 0)]);
        let co = record(&c, 1, &[(120, vals[0]), (121, vals[1]), (122, vals[2])]);
        let b = output::encode_atom(Atom::Bool(false), c.frame_bytes[2] as usize).unwrap();
        let cx =
            output::encode_envelope(102, &c.commitment, &b, c.frame_bytes[2] as usize).unwrap();
        policy::with_numeric_authority(&c, &schema, |a| {
            let e = a.evaluate(c::Raw {
                state: &st,
                command: &co,
                context: &cx,
            });
            assert_eq!(
                e.result().unwrap().post()[0].value,
                Atom::U128(vals[3]),
                "{line}"
            );
        })
        .unwrap();
    }
}
#[test]
fn u128_scaled_operand_fit_is_an_admission_obligation() {
    let source = scaled_project(1, M);
    let r = rules("mul_div_floor(command.101.120, command.101.121, command.101.122, e1)");
    let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    let err = Contract::build(&d, &r, schema_commitment(&s).unwrap())
        .err()
        .unwrap();
    assert!(err.reason().contains("numerator fit"), "{err}");
}
#[test]
fn u128_mixed_sum_input_preserves_original_kind() {
    let source = project()
        + " type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 123 101 mode 109;";
    let r = rules("command.101.120");
    let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    let c = Contract::build(&d, &r, schema_commitment(&s).unwrap()).unwrap();
    let state = record(&c, 0, &[(110, 0)]);
    let fields = [
        c::Field {
            id: 120,
            value: Atom::U128(M),
        },
        c::Field {
            id: 121,
            value: Atom::U128(1),
        },
        c::Field {
            id: 122,
            value: Atom::U128(1),
        },
        c::Field {
            id: 123,
            value: Atom::Sum {
                type_id: 109,
                variant: 131,
            },
        },
    ];
    let body = output::encode_record(&fields, c.frame_bytes[1] as usize).unwrap();
    let command =
        output::encode_envelope(101, &c.commitment, &body, c.frame_bytes[1] as usize).unwrap();
    let b = output::encode_atom(Atom::Bool(false), c.frame_bytes[2] as usize).unwrap();
    let context =
        output::encode_envelope(102, &c.commitment, &b, c.frame_bytes[2] as usize).unwrap();
    policy::with_numeric_authority(&c, &s, |a| {
        let e = a.evaluate(c::Raw {
            state: &state,
            command: &command,
            context: &context,
        });
        assert_eq!(e.result().unwrap().post()[0].value, Atom::U128(M));
    })
    .unwrap();
}
#[test]
fn u128_law_kinds_do_not_coerce_boolean_observations() {
    for formula in ["context.102 == context.102", "context.102 + 1 == 2"] {
        let source = project() + &format!("law 501 context_check on accept = {formula};");
        let mut r = rules("command.101.120");
        r.law_kinds.insert(
            501,
            zeno_fcis_contract::internals::rules::LawKind::FeeAndRounding,
        );
        let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
        let s = schema::encode(&d).unwrap();
        let result = Contract::build(&d, &r, schema_commitment(&s).unwrap());
        if formula.contains('+') {
            assert!(result.is_err())
        } else {
            let c = result.unwrap();
            assert!(policy::encode(&c, &s).is_ok());
        }
    }
}
#[test]
fn u128_delivery_amounts_and_checked_sums_use_actual_outbox() {
    use std::collections::BTreeMap;
    use zeno_fcis_contract::internals::{
        declarations::Leaf,
        expr::Ast,
        rules::{Delivery, DeliveryLaw, DeliveryObservation, LawKind},
    };
    let source = project()
        + " type 200 destination Destination; type 201 payload Payload; field 202 201 amount 105; channel 300 transfer destination 200 payload 201; law 501 payout on accept = outbox.201.202 == command.101.120;";
    for deliveries in [1, 2] {
        let mut r = rules("command.101.120");
        r.leaf_bindings.insert(200, Leaf::Text { min: 1, max: 16 });
        r.law_kinds.insert(501, LawKind::FeeAndRounding);
        r.delivery_laws.insert(
            501,
            DeliveryLaw {
                max_deliveries: 2,
                observations: BTreeMap::from([(
                    "outbox.201.202".into(),
                    DeliveryObservation {
                        channel: 300,
                        field: Some(202),
                        destination: None,
                    },
                )]),
            },
        );
        r.cases[0].outbox = (0..deliveries)
            .map(|ordinal| Delivery {
                ordinal,
                channel: 300,
                destination: "sink".into(),
                payload: BTreeMap::from([(202, Ast::Name("command.101.120".into()))]),
                idempotency: u128::from(ordinal),
            })
            .collect();
        let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
        let s = schema::encode(&d).unwrap();
        let c = Contract::build(&d, &r, schema_commitment(&s).unwrap()).unwrap();
        let st = record(&c, 0, &[(110, 0)]);
        let co = record(&c, 1, &[(120, M), (121, 1), (122, 1)]);
        let b = output::encode_atom(Atom::Bool(false), c.frame_bytes[2] as usize).unwrap();
        let cx =
            output::encode_envelope(102, &c.commitment, &b, c.frame_bytes[2] as usize).unwrap();
        policy::with_numeric_authority(&c, &s, |a| {
            let raw = c::Raw {
                state: &st,
                command: &co,
                context: &cx,
            };
            let e = a.evaluate(raw);
            if deliveries == 1 {
                let v = e.result().unwrap();
                assert_eq!(v.outbox()[0].payload[0].value, Atom::U128(M));
                assert!(matches!(
                    a.publish(raw),
                    finite::v2_authority::PublicationOutcome::Commit(_)
                ));
            } else {
                assert!(e.result().is_err());
                assert!(
                    e.diagnostics().iter().any(|d| d.id == 501
                        && matches!(d.verdict, finite::v2_laws::Verdict::Refused(_)))
                );
                assert_eq!(e.usage(), a.evaluate(raw).usage());
            }
        })
        .unwrap();
    }
}
#[test]
fn u128_public_cast_does_not_accept_an_original_sum() {
    let source = project()
        + " type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 123 101 mode 109;";
    let r = rules("to_u128(command.101.123)");
    let d = Declarations::read(&source, &r.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    let e = Contract::build(&d, &r, schema_commitment(&s).unwrap())
        .err()
        .unwrap();
    assert!(e.reason().contains("CastKindMismatch"), "{e}");
}
#[test]
fn u128_table_authoring_remains_outside_the_closed_scalar_grammar() {
    for table in [
        "type 109 table Positions key 105 value 105;",
        "table 109 Positions key u128 value u128;",
    ] {
        let text = project() + table;
        assert!(zeno_fcis_spec::parse_project(&text, Default::default()).is_err());
    }
}
