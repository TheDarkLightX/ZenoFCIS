//! D6/D10 authoring, binding, arithmetic and eager-prefix regressions.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::json;
use std::collections::BTreeMap;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_contract::internals::review::{
    domain,
    evaluate::{self, Framer, Outcome, RefusalClass},
};
use zeno_fcis_contract::internals::{
    declarations::Declarations, expr, graph::ScalarOp, model::Contract, policy, rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

const LOW: i128 = i128::MIN;
const HIGH: i128 = i128::MAX;

fn project(units: Option<[&str; 4]>) -> String {
    let mut source = "zeno 1; project 1 quantity_arithmetic; namespace 2 core;\ntype 100 state State; type 101 command Command; type 102 context Context;\n".to_owned();
    for (i, name) in ["Result", "Left", "Right", "Denominator"]
        .iter()
        .enumerate()
    {
        let suffix = units.map(|u| format!(" unit {}", u[i])).unwrap_or_default();
        source.push_str(&format!(
            "type {} int {name} in {LOW}..={HIGH}{suffix};\n",
            105 + i
        ));
    }
    source.push_str("field 110 100 result 105; field 120 101 left 106; field 121 101 right 107; field 122 101 divisor 108;\ncomponent 400 arithmetic { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; } merge [400];\n");
    source.push_str(&format!(
        "law 500 bounded_state on commit, genesis = post.100.110 >= {LOW};\n"
    ));
    source
}

fn rules(expression: &str, typed: bool) -> serde_json::Value {
    json!({"schema":if typed {"zeno-fcis/template-declarative-policy/3"} else {"zeno-fcis/template-declarative-policy/2"},"template":"quantity-arithmetic","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":true,"class":"Accept","reason":null,"post":{"110":expression},"outbox":[]}],"genesis":{"110":0},"law_kinds":{"500":"StateInvariant"}})
}

fn with_contract(
    source: &str,
    rules: &serde_json::Value,
    check: impl FnOnce(&Contract<'_>, &[u8]),
) {
    let rules = Rules::read(&rules.to_string()).unwrap();
    let d = Declarations::read(source, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&d).unwrap();
    let contract = Contract::build(&d, &rules, schema_commitment(&schema).unwrap()).unwrap();
    check(&contract, &schema);
}

fn refusal(source: &str, rules: &serde_json::Value) -> String {
    let r = Rules::read(&rules.to_string()).unwrap();
    let d = match Declarations::read(source, &r.leaf_bindings) {
        Ok(d) => d,
        Err(e) => return e.to_string(),
    };
    let s = schema::encode(&d).unwrap();
    match Contract::build(&d, &r, schema_commitment(&s).unwrap()) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("unexpected admission"),
    }
}

fn answer(c: &Contract<'_>, s: &[u8], input: &[i128]) -> Outcome {
    let f = Framer::new(&domain::positions(c.declarations).unwrap(), c);
    policy::with_authority(c, s, |a| evaluate::evaluate(a, &f, input)).unwrap()
}

fn number(outcome: Outcome) -> i128 {
    match outcome {
        Outcome::Decision(d) => d.post[0].1.number().unwrap(),
        other => panic!("unexpected {other:?}"),
    }
}

fn example(name: &str) -> i128 {
    include_str!("test-data/w25/examples.tsv")
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('\t')?;
            (key == name).then(|| value.parse().unwrap())
        })
        .unwrap()
}

fn compare(actual: i128, expected: i128, name: &str) {
    println!("W25_WITNESS {name} admitted actual={actual} expected={expected}");
    if std::env::var_os("W25_DISABLE_COMPARISON").is_none() {
        assert_eq!(actual, expected, "W25_ORACLE_MISMATCH {name}");
    }
}

#[test]
fn w25_bps_oracle() {
    let source = project(Some([
        "USD/COLL scale e8",
        "USD/COLL scale e8",
        "1 scale e4",
        "1 scale e0",
    ]));
    for rounding in ["floor", "ceil"] {
        let expected = example(&format!("bps_{rounding}"));
        let expr = format!("mul_div_{rounding}(command.101.120, command.101.121, 1, e8)");
        with_contract(&source, &rules(&expr, true), |c, s| {
            compare(
                number(answer(c, s, &[0, 123456789, 25, 1, 0])),
                expected,
                "bps-denominator",
            );
            assert_eq!(
                c.nodes
                    .iter()
                    .filter(|n| matches!(n, ScalarOp::MulDiv(..)))
                    .count(),
                1
            );
            let p = c.program().unwrap();
            assert_eq!(
                import_program(&p.value().unwrap().canonical_bytes().unwrap()).unwrap(),
                p
            );
        });
    }
}

#[test]
fn w25_chain_prefix_oracle() {
    let source = project(Some([
        "POINT scale e0",
        "POINT scale e0",
        "ms scale e0",
        "ms scale e0",
    ]));
    for p in 0..=16 {
        let expected = example(&format!("chain_{p}"));
        let expr =
            format!("repeat_mul_div_floor(command.101.120, command.101.121, command.101.122, {p})");
        with_contract(&source, &rules(&expr, true), |c, s| {
            compare(
                number(answer(c, s, &[0, 1001, 225000, 300000, 0])),
                expected,
                "chain-prefix",
            )
        });
    }
}

#[test]
fn big_integer_oracle_covers_signs_extrema_and_every_repeat_count() {
    let lines = include_str!("test-data/w25/oracle.tsv").lines().skip(1);
    let mut groups = BTreeMap::<(String, u8), Vec<(Vec<i128>, Option<i128>)>>::new();
    for line in lines {
        let v: Vec<_> = line.split('\t').collect();
        groups
            .entry((v[0].to_owned(), v[4].parse().unwrap()))
            .or_default()
            .push((
                vec![
                    0,
                    v[1].parse().unwrap(),
                    v[2].parse().unwrap(),
                    v[3].parse().unwrap(),
                    0,
                ],
                v[5].parse().ok(),
            ));
    }
    for ((kind, p), vectors) in groups {
        let function = match kind.as_str() {
            "signed" => "signed_mul_div_floor",
            "floor" => "mul_div_floor",
            "ceil" => "mul_div_ceil",
            "repeat" => "repeat_mul_div_floor",
            _ => "repeat_signed_mul_div_floor",
        };
        let count = if kind.starts_with("repeat") {
            format!(", {p}")
        } else {
            String::new()
        };
        let expr = format!("{function}(command.101.120, command.101.121, command.101.122{count})");
        with_contract(&project(None), &rules(&expr, false), |c, s| {
            let f = Framer::new(&domain::positions(c.declarations).unwrap(), c);
            policy::with_authority(c,s,|a| {
                for (input,expected) in &vectors {
                    let actual=evaluate::evaluate(a,&f,input);
                    match expected {
                        Some(n)=>assert_eq!(number(actual),*n,"{kind} {p} {input:?}"),
                        None=>assert!(matches!(actual,Outcome::Refused(ref r) if r.class==RefusalClass::Arithmetic),"{kind} {p} {input:?}: {actual:?}"),
                    }
                }
            }).unwrap();
        });
    }
}

#[test]
fn quantity_admission_checks_rules_laws_selects_assignments_effects_and_metadata() {
    let units = [
        "USD scale e8",
        "USD scale e8",
        "COLL scale e8",
        "1 scale e0",
    ];
    let source = project(Some(units));
    for expr in [
        "command.101.120 + command.101.121",
        "command.101.120 - command.101.121",
        "choose(true, command.101.120, command.101.121)",
        "choose(command.101.120 == command.101.121, command.101.120, command.101.120)",
        "command.101.121",
        "mul_div_floor(command.101.120, command.101.121, 1)",
    ] {
        assert!(
            refusal(&source, &rules(expr, true)).contains("mismatch"),
            "{expr}"
        );
    }
    for other in [
        "USD scale e7",
        "SHARE scale e8",
        "USD/ETH scale e8",
        "USD/year scale e8",
    ] {
        let s = project(Some([units[0], units[1], other, units[3]]));
        assert!(
            refusal(&s, &rules("command.101.120 == command.101.121", true)).contains("mismatch")
        );
    }
    let mut r = rules("command.101.120", true);
    r["variables"] = json!({"unused":"command.101.120 + command.101.121"});
    assert!(refusal(&source, &r).contains("mismatch"));
    assert!(refusal(&source, &rules("command.101.120", false)).contains("policy/3"));
    let boolean = format!("{source} type 109 bool Flag; field 111 100 flag 109;");
    let mut r = rules("command.101.120", true);
    r["leaf_bindings"]["109"] = json!(["Bool"]);
    r["genesis"]["111"] = json!(true);
    r["cases"][0]["post"]["111"] = json!("-0");
    assert!(refusal(&boolean, &r).contains("scalar kind mismatch"));
    let missing = source.replace(" unit COLL scale e8", "");
    assert!(refusal(&missing, &rules("command.101.120", true)).contains("every integer"));
    for suffix in [
        "unit USD",
        "unit USD scale e39",
        "scale e8",
        "unit 0 scale e8",
    ] {
        let s = source.replace("unit COLL scale e8", suffix);
        let parsed = zeno_fcis_spec::parse_project(&s, Default::default());
        assert!(parsed.is_err(), "{suffix}");
    }
    let law = format!("{source} law 501 bad_units on accept = post.100.110 == command.101.121;");
    let mut r = rules("command.101.120", true);
    r["law_kinds"]["501"] = json!("FeeAndRounding");
    assert!(refusal(&law, &r).contains("mismatch"));
    let effects = format!(
        "{source} type 200 destination Destination; type 201 payload Payload; field 202 201 amount 107; channel 300 transfer destination 200 payload 201;"
    );
    let mut r = rules("command.101.120", true);
    r["leaf_bindings"]["200"] = json!(["Text", 1, 8]);
    r["cases"][0]["outbox"] = json!([{"ordinal":0,"channel":300,"destination":"a","payload":{"202":"command.101.120"},"idempotency_ordinal":0}]);
    assert!(refusal(&effects, &r).contains("mismatch"));
    // Equal nominal expressions have equal schema identities; changing a scale
    // or asset changes the identity even when all raw bounds stay identical.
    let bindings = BTreeMap::from([(102, zeno_fcis_contract::internals::declarations::Leaf::Bool)]);
    let encode = |s: &str| schema::encode(&Declarations::read(s, &bindings).unwrap()).unwrap();
    let canonical = project(Some([
        "USD*COLL/COLL scale e8",
        units[1],
        units[2],
        units[3],
    ]));
    assert_eq!(encode(&source), encode(&canonical));
    assert_ne!(
        encode(&source),
        encode(&source.replace("USD scale e8", "USD scale e7"))
    );
    assert_ne!(
        encode(&source),
        encode(&source.replace("COLL scale e8", "ETH scale e8"))
    );
}

#[test]
fn scaling_cancellation_and_checked_bounds_are_exact() {
    for (left, right, denom, target, expected, expression) in [
        (
            "USD scale e8",
            "1 scale e4",
            "1 scale e0",
            "USD scale e8",
            308641,
            "mul_div_floor(command.101.120, command.101.121, 1, e8)",
        ),
        (
            "USD scale e8",
            "1 scale e0",
            "1 scale e0",
            "USD scale e9",
            1234567890,
            "mul_div_floor(command.101.120, 1, 1, e9)",
        ),
        (
            "USD scale e0",
            "1 scale e0",
            "1 scale e4",
            "USD scale e0",
            1234567890000,
            "mul_div_floor(command.101.120, 1, command.101.122, e0)",
        ),
        (
            "USD scale e0",
            "1 scale e0",
            "1 scale e0",
            "USD scale e0",
            123456789,
            "mul_div_floor(command.101.120, 1, 1, e0)",
        ),
    ] {
        let source = project(Some([target, left, right, denom]));
        with_contract(&source, &rules(expression, true), |c, s| {
            assert_eq!(number(answer(c, s, &[0, 123456789, 25, 1, 0])), expected)
        });
    }
    let source = project(Some([
        "USD scale e0",
        "USD scale e8",
        "1 scale e4",
        "1 scale e0",
    ]));
    let expression = "mul_div_floor(command.101.120, command.101.121, command.101.122, e0)";
    assert!(refusal(&source, &rules(expression, true)).contains("denominator fit"));
    let bound = source.replace(
        &format!("Denominator in {LOW}..={HIGH}"),
        "Denominator in 1..=1000000",
    );
    with_contract(&bound, &rules(expression, true), |c, s| {
        assert_eq!(number(answer(c, s, &[0, 123456789, 25, 1, 0])), 0)
    });
    let huge = project(Some([
        "USD scale e38",
        "USD scale e0",
        "1 scale e0",
        "1 scale e38",
    ]));
    assert!(
        refusal(
            &huge,
            &rules(
                "mul_div_floor(command.101.120, command.101.121, command.101.122, e38)",
                true
            )
        )
        .contains("exponent exceeds 38")
    );
    let scale_up = project(Some([
        "USD scale e1",
        "USD scale e0",
        "1 scale e0",
        "1 scale e0",
    ]));
    assert!(
        refusal(
            &scale_up,
            &rules(
                "mul_div_floor(command.101.120, command.101.121, 1, e1)",
                true
            )
        )
        .contains("numerator fit")
    );
}

#[test]
fn bounded_bindings_mixed_rounding_graph_caps_and_eager_refusal_retry() {
    for p in ["-1", "17", "command.101.122", "1+1"] {
        assert!(expr::parse(&format!("repeat_mul_div_floor(1, 2, 1, {p})")).is_err());
    }
    let source = project(None);
    let mut r = rules("v16", false);
    r["schema"] = json!("zeno-fcis/template-declarative-policy/3");
    let mut variables = serde_json::Map::new();
    variables.insert("v0".into(), json!("command.101.120"));
    for i in 1..=16 {
        variables.insert(
            format!("v{i}"),
            json!(format!("mul_div_floor(v{}, 1, 1)", i - 1)),
        );
    }
    r["variables"] = variables.into();
    with_contract(&source, &r, |c, s| {
        assert_eq!(number(answer(c, s, &[0, 11, 1, 1, 0])), 11)
    });
    r["variables"]["v17"] = json!("mul_div_floor(v16, 1, 1)");
    r["cases"][0]["post"]["110"] = json!("v17");
    assert!(refusal(&source, &r).contains("exceeds 16"));
    for (expr, expected) in [
        (
            "mul_div_ceil(mul_div_floor(command.101.120, 3, 4), 3, 4)",
            563,
        ),
        (
            "mul_div_floor(mul_div_ceil(command.101.120, 3, 4), 3, 4)",
            563,
        ),
        ("repeat_mul_div_floor(command.101.120, 1, 0, 0)", 1001),
    ] {
        with_contract(&source, &rules(expr, false), |c, s| {
            assert_eq!(number(answer(c, s, &[0, 1001, 1, 1, 0])), expected)
        });
    }
    for expr in [
        "repeat_mul_div_floor(command.101.120, div_floor(1,0), 0, 0)",
        "choose(false, mul_div_floor(command.101.120, 2, 1), 0)",
        "mul_div_floor(mul_div_floor(command.101.120,2,1),0,1)",
    ] {
        with_contract(&source, &rules(expr, false), |c, s| {
            let first = answer(c, s, &[0, HIGH, 1, 1, 0]);
            assert!(matches!(first,Outcome::Refused(ref r) if r.class==RefusalClass::Arithmetic));
            assert_eq!(first, answer(c, s, &[0, HIGH, 1, 1, 0]));
        });
    }
    // Independent chains retain the total graph cap rather than sharing a
    // cumulative 16-step budget.
    let mut expressions = (1..=240)
        .map(|n| format!("repeat_mul_div_floor(command.101.120, {n}, 100, 16)"))
        .collect::<Vec<_>>();
    while expressions.len() > 1 {
        expressions = expressions
            .chunks(2)
            .map(|v| {
                if v.len() == 2 {
                    format!("({} + {})", v[0], v[1])
                } else {
                    v[0].clone()
                }
            })
            .collect();
    }
    let expression = expressions.pop().unwrap();
    assert!(refusal(&source, &rules(&expression, false)).contains("program nodes exceed"));
}

#[test]
fn expanded_graph_usage_and_first_failure_survive_retry() {
    use zeno_fcis_synthesis::finite::{V2ExecutionFailure, V2Resource as R, v2_zero_limits};
    let limits = v2_zero_limits()
        .with_limit(R::Read, 1000)
        .with_limit(R::Write, 1000)
        .with_limit(R::Step, 1000);
    for p in 0..=16 {
        let expr =
            format!("repeat_mul_div_floor(command.101.120, command.101.121, command.101.122, {p})");
        with_contract(&project(None), &rules(&expr, false), |c, s| {
            let program = c.program().unwrap();
            let input = [0, 1001, 225000, 300000, 0];
            let evaluation = program.execute_v2(&input, limits);
            assert_eq!(evaluation.usage().used(R::Step), c.nodes.len() as u64);
            let short =
                program.execute_v2(&input, limits.with_limit(R::Step, c.nodes.len() as u64 - 1));
            assert!(matches!(
                short.into_parts().0,
                Err(V2ExecutionFailure::Budget(_))
            ));
            let frames = Framer::new(&domain::positions(c.declarations).unwrap(), c).frame(&input);
            policy::with_authority(c, s, |authority| {
                use zeno_fcis_synthesis::finite::v2_composition::Raw;
                let first = authority.evaluate(Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                });
                let second = authority.evaluate(Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                });
                assert_eq!(first.decision_usage(), second.decision_usage());
                assert_eq!(
                    first.decision_usage().unwrap().used(R::Step),
                    c.nodes.len() as u64
                );
                assert!(first.result().is_ok());
            })
            .unwrap();
        });
    }
    with_contract(
        &project(None),
        &rules("repeat_mul_div_floor(command.101.120, 2, 1, 2)", false),
        |c, s| {
            let input = [0, HIGH, 0, 0, 0];
            let p = c.program().unwrap();
            let first = p.execute_v2(&input, limits);
            assert_eq!(first, p.execute_v2(&input, limits));
            let fail = c
                .nodes
                .iter()
                .position(|n| matches!(n, ScalarOp::MulDiv(..)))
                .unwrap();
            assert_eq!(first.usage().used(R::Step), fail as u64 + 1);
            assert!(matches!(
                first.into_parts().0,
                Err(V2ExecutionFailure::Arithmetic)
            ));
            let f = Framer::new(&domain::positions(c.declarations).unwrap(), c);
            let frames = f.frame(&input);
            let before = frames.clone();
            policy::with_authority(c, s, |a| {
                use zeno_fcis_synthesis::finite::v2_composition::Raw;
                let one = a.evaluate(Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                });
                let two = a.evaluate(Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                });
                assert!(one.result().is_err());
                assert_eq!(one.usage(), two.usage());
                assert!(one.decision_usage().is_none());
                assert_eq!(one.usage().used(R::Step), fail as u64 + 1);
            })
            .unwrap();
            assert_eq!(frames, before);
        },
    );
}

#[test]
fn lowered_graphs_replay_in_generated_rust_python_javascript() {
    use std::process::Command;
    use zeno_fcis_synthesis::finite::emit::{
        JavaScriptEmitter, PythonEmitter, RustEmitter, TargetEmitter,
    };
    let directory = std::env::temp_dir().join(format!("w25-emitter-replay-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (id, source, expr, input, expected, typed) in [
        (
            "bps",
            project(Some([
                "USD/COLL scale e8",
                "USD/COLL scale e8",
                "1 scale e4",
                "1 scale e0",
            ])),
            "mul_div_floor(command.101.120, command.101.121, 1, e8)",
            [0, 123456789, 25, 1, 0],
            308641,
            true,
        ),
        (
            "chain",
            project(None),
            "repeat_mul_div_floor(command.101.120, command.101.121, command.101.122, 2)",
            [0, 1001, 225000, 300000, 0],
            562,
            false,
        ),
        (
            "signed",
            project(None),
            "repeat_signed_mul_div_floor(command.101.120, command.101.121, command.101.122, 2)",
            [0, -1001, 3, 4, 0],
            -564,
            false,
        ),
    ] {
        with_contract(&source, &rules(expr, typed), |c, s| {
            assert_eq!(number(answer(c, s, &input)), expected);
            let p = c.program().unwrap();
            let values = p.evaluate(&input).unwrap();
            let tokens = input
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            let result = values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" ");
            let py = directory.join(format!("{id}.py"));
            std::fs::write(
                &py,
                format!(
                    "{}\nassert transition(tuple(map(int, '{}'.split()))) == {:?}\n",
                    PythonEmitter.emit(&p).unwrap(),
                    tokens,
                    values
                )
                .replace(
                    &format!("== {values:?}"),
                    &format!(
                        "== {}",
                        format!(
                            "({})",
                            values
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    ),
                ),
            )
            .unwrap();
            let js = directory.join(format!("{id}.mjs"));
            std::fs::write(&js,format!("{}\nif (transition('{tokens}') !== '{result}') throw Error('replay mismatch');\n",JavaScriptEmitter.emit(&p).unwrap())).unwrap();
            let rs = directory.join(format!("{id}.rs"));
            std::fs::write(
                &rs,
                format!(
                    "{}\nfn main() {{ assert_eq!(transition(&{:?}), Some({:?})); }}\n",
                    RustEmitter.emit(&p).unwrap(),
                    input,
                    values
                ),
            )
            .unwrap();
            let binary = directory.join(id);
            for (tool, args) in [
                ("python3", vec![py.to_str().unwrap()]),
                ("node", vec![js.to_str().unwrap()]),
                (
                    "rustc",
                    vec![
                        "+1.97.1",
                        rs.to_str().unwrap(),
                        "-o",
                        binary.to_str().unwrap(),
                    ],
                ),
            ] {
                let output = Command::new(tool).args(args).output().unwrap();
                assert!(
                    output.status.success(),
                    "{tool}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            assert!(Command::new(binary).status().unwrap().success());
        });
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn independent_rational_oracle_covers_every_scale_and_exact_scaled_bounds() {
    for line in include_str!("test-data/w25/scales.tsv").lines() {
        let v: Vec<_> = line.split('\t').collect();
        let n = |i: usize| v[i].parse::<i128>().unwrap();
        let (a, b, d) = (n(2), n(3), n(4));
        let (p, t, j, q) = (n(5), n(6), n(7), n(8));
        let units = [
            format!("USD scale e{q}"),
            format!("USD scale e{p}"),
            format!("1 scale e{t}"),
            format!("1 scale e{j}"),
        ];
        let mut source = project(Some([&units[0], &units[1], &units[2], &units[3]]));
        for (name, value) in [("Left", a), ("Right", b), ("Denominator", d)] {
            source = source.replace(
                &format!("{name} in {LOW}..={HIGH}"),
                &format!("{name} in {value}..={value}"),
            );
        }
        let function = if n(0) == 1 {
            "signed_mul_div_floor"
        } else if n(1) == 1 {
            "mul_div_ceil"
        } else {
            "mul_div_floor"
        };
        let expression =
            format!("{function}(command.101.120, command.101.121, command.101.122, e{q})");
        with_contract(&source, &rules(&expression, true), |c, s| {
            let actual = answer(c, s, &[0, a, b, d, 0]);
            if let Ok(expected) = v[9].parse::<i128>() {
                assert_eq!(number(actual), expected, "{line}");
            } else {
                assert!(
                    matches!(actual,Outcome::Refused(ref r) if r.class==RefusalClass::Arithmetic),
                    "{line}: {actual:?}"
                );
            }
        });
    }
}

#[test]
fn typed_authoring_preserves_raw_three_argument_scales_and_law_lowering() {
    let units = [
        "USD/COLL scale e12",
        "USD/COLL scale e8",
        "1 scale e4",
        "1 scale e0",
    ];
    let source = project(Some(units));
    let expression = "mul_div_floor(command.101.120, command.101.121, 1)";
    with_contract(&source, &rules(expression, true), |c, s| {
        assert_eq!(number(answer(c, s, &[0, 123456789, 25, 1, 0])), 3086419725)
    });
    let scaled = source.replace("USD/COLL scale e12", "USD/COLL scale e8");
    assert!(refusal(&scaled, &rules(expression, true)).contains("mismatch"));
    let source = format!(
        "{scaled} law 501 scaled_fee on accept = post.100.110 == mul_div_floor(command.101.120, command.101.121, 1, e8);"
    );
    let mut r = rules(
        "mul_div_floor(command.101.120, command.101.121, 1, e8)",
        true,
    );
    r["law_kinds"]["501"] = json!("FeeAndRounding");
    with_contract(&source, &r, |c, s| {
        assert_eq!(number(answer(c, s, &[0, 123456789, 25, 1, 0])), 308641)
    });
    let raw = expr::parse("mul_div_floor(1, 2, 1, e8)").unwrap();
    assert_eq!(expr::parse(&expr::render(&raw)).unwrap(), raw);
    let repeat = expr::parse("repeat_signed_mul_div_floor(1, -2, 1, 16)").unwrap();
    assert_eq!(expr::parse(&expr::render(&repeat)).unwrap(), repeat);
}

#[test]
fn delivery_aggregate_cannot_borrow_one_payloads_fit_bound() {
    let maximum = HIGH / 10;
    let source = project(Some([
        "1 scale e0",
        "USD scale e1",
        "1 scale e0",
        "USD scale e0",
    ]))
    .replace(
        &format!("Denominator in {LOW}..={HIGH}"),
        &format!("Denominator in 0..={maximum}"),
    );
    let source = format!(
        "{source} type 200 destination Destination; type 201 payload Payload; field 202 201 amount 108; channel 300 transfer destination 200 payload 201; law 501 ratio on accept = post.100.110 == mul_div_floor(command.101.120, 1, outbox.201.202, e0);"
    );
    let mut r = rules("0", true);
    r["leaf_bindings"]["200"] = json!(["Text", 1, 8]);
    r["law_kinds"]["501"] = json!("FeeAndRounding");
    r["delivery_laws"] = json!({"501":{"max_deliveries":2,"observations":{"outbox.201.202":{"channel":300,"field":202}}}});
    r["cases"][0]["outbox"] = json!([
        {"ordinal":0,"channel":300,"destination":"a","payload":{"202":maximum.to_string()},"idempotency_ordinal":0},
        {"ordinal":1,"channel":300,"destination":"a","payload":{"202":maximum.to_string()},"idempotency_ordinal":1}
    ]);
    assert_eq!(example("aggregate_denominator_floor"), 0);
    assert!((2 * maximum).checked_mul(10).is_none());
    assert!(refusal(&source, &r).contains("denominator fit"));
    // An ordinary command field really is checked against this range.
    let parsed = Rules::read(&r.to_string()).unwrap();
    let declarations = Declarations::read(&source, &parsed.leaf_bindings).unwrap();
    let ordinary = expr::parse("mul_div_floor(command.101.120, 1, command.101.122, e0)").unwrap();
    assert!(zeno_fcis_contract::internals::quantity::lower(&declarations, &ordinary).is_ok());
    // Preserve lawful aggregate use where no scaled temporary is required.
    let unscaled = source.replace("USD scale e1", "USD scale e0");
    r["cases"][0]["post"]["110"] = json!(5);
    with_contract(&unscaled, &r, |c, s| {
        assert_eq!(number(answer(c, s, &[0, HIGH, 1, 0, 0])), 5)
    });
}

#[test]
fn mixed_rounding_prefixes_match_independent_rationals() {
    let source = format!(
        "{} field 111 100 prefix 105;",
        project(Some([
            "POINT scale e0",
            "POINT scale e0",
            "1 scale e0",
            "1 scale e0"
        ]))
    );
    for (x, a, d) in [(2, 1, 2), (1001, 3, 4)] {
        for (first, second) in [("floor", "ceil"), ("ceil", "floor")] {
            let key = format!("mixed_{x}_{a}_{d}_{first}_{second}");
            let mut r = rules("second", true);
            r["variables"] = json!({"first":format!("mul_div_{first}(command.101.120, {a}, {d})"),"second":format!("mul_div_{second}(first, {a}, {d})")});
            r["genesis"]["111"] = json!(0);
            r["cases"][0]["post"]["111"] = json!("first");
            with_contract(&source, &r, |c, s| {
                let Outcome::Decision(actual) = answer(c, s, &[0, 0, x, 0, 0, 0]) else {
                    panic!("mixed recurrence refused")
                };
                assert_eq!(
                    actual.post[0].1.number(),
                    Some(example(&format!("{key}_final")))
                );
                assert_eq!(
                    actual.post[1].1.number(),
                    Some(example(&format!("{key}_prefix")))
                );
            });
        }
    }
}

fn alias_rules(depth: usize, multiply: bool) -> serde_json::Value {
    let mut r = rules(&format!("v{depth}"), false);
    r["variables"]["v0"] = json!("command.101.120");
    for i in 1..=depth {
        r["variables"][format!("v{i}")] = json!(if multiply {
            format!("mul_div_floor(v{}, 1, 1)", i - 1)
        } else {
            format!("v{}", i - 1)
        });
    }
    r
}

#[test]
fn historical_alias_chain() {
    let source = project(None);
    with_contract(&source, &alias_rules(256, false), |c, s| {
        assert_eq!(c.nodes.len(), 1, "exact-base alias witness shape");
        assert_eq!(number(answer(c, s, &[0, 11, 1, 1, 0])), 11);
    });
}

#[test]
fn historical_three_argument_chain() {
    let source = project(None);
    with_contract(&source, &alias_rules(17, true), |c, s| {
        assert_eq!(c.nodes.len(), 20, "exact-base fused-chain witness shape");
        assert_eq!(number(answer(c, s, &[0, 11, 1, 1, 0])), 11);
    });
}

#[test]
fn new_arithmetic_limits_follow_policy_version_and_syntax() {
    let source = project(None);
    for (r, reason) in [
        (
            alias_rules(256, false),
            "expanded expression nests too deeply",
        ),
        (
            alias_rules(17, true),
            "dependent mul-div chain exceeds 16 steps",
        ),
    ] {
        let mut versioned = r.clone();
        versioned["schema"] = json!("zeno-fcis/template-declarative-policy/3");
        assert!(refusal(&source, &versioned).contains(reason));
        // Even an unused new form opts the whole policy into its limits.
        for feature in [
            "repeat_mul_div_floor(1, 1, 1, 0)",
            "repeat_signed_mul_div_floor(1, 1, 1, 0)",
        ] {
            let mut extended = r.clone();
            extended["variables"]["new_feature"] = json!(feature);
            let actual = refusal(&source, &extended);
            assert!(actual.contains(reason), "{feature}: {actual}");
        }
        let mut new_post = r.clone();
        let historical = r["cases"][0]["post"]["110"].as_str().unwrap();
        new_post["cases"][0]["post"]["110"] = json!(format!(
            "choose(true, {historical}, repeat_mul_div_floor(1, 1, 1, 0))"
        ));
        assert!(refusal(&source, &new_post).contains(reason));
        let new_law = source.replace(
            &format!("post.100.110 >= {LOW}"),
            "post.100.110 >= repeat_mul_div_floor(0, 1, 1, 0)",
        );
        assert!(refusal(&new_law, &r).contains(reason));
    }
    // Scaled calls still need quantity declarations; opting into limits
    // cannot make an untyped scaled call admissible.
    assert!(
        refusal(&source, &rules("mul_div_floor(1, 1, 1, e0)", false))
            .contains("scaled call requires declared units and scales")
    );
    // Work limits are versioned too; the historical graph reuses each sum.
    let mut wide = rules("v15", false);
    wide["variables"]["v0"] = json!("command.101.120");
    for i in 1..=15 {
        wide["variables"][format!("v{i}")] = json!(format!("v{} + v{}", i - 1, i - 1));
    }
    with_contract(&source, &wide, |c, s| {
        assert_eq!(number(answer(c, s, &[0, 11, 1, 1, 0])), 11 * (1 << 15));
    });
    wide["schema"] = json!("zeno-fcis/template-declarative-policy/3");
    assert!(refusal(&source, &wide).contains("admission work cap"));
    let mut positive = rules("command.101.120", false);
    positive["schema"] = json!("zeno-fcis/template-declarative-policy/3");
    with_contract(&source, &positive, |c, s| {
        assert_eq!(number(answer(c, s, &[0, 11, 1, 1, 0])), 11);
    });
}
