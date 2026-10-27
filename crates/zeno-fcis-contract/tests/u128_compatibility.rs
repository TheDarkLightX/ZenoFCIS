//! Exact legacy /2 and /3 authoring, bytes, outcomes and refusal-work snapshot.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use serde_json::json;
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer},
    },
    rules::Rules,
    schema, schema_commitment,
};

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

#[test]
fn u128_legacy_snapshot() {
    use std::fmt::Write;
    use zeno_fcis_synthesis::finite::v2_composition as c;
    let mut text = String::new();
    for typed in [false, true] {
        let source = project(if typed { Some(["1 scale e0"; 4]) } else { None });
        let spec = zeno_fcis_spec::elaborate_project(
            zeno_fcis_spec::parse_project(&source, Default::default()).unwrap(),
            Default::default(),
        )
        .unwrap();
        writeln!(
            &mut text,
            "SPEC {typed} {:?}",
            spec.canonical_bytes().unwrap()
        )
        .unwrap();
        for expr in [
            "command.101.120 + command.101.121",
            "command.101.120 - command.101.121",
            "command.101.120 * command.101.121",
            "div_floor(command.101.120, command.101.122)",
            "div_ceil(command.101.120, command.101.122)",
            "mul_div_floor(command.101.120, command.101.121, command.101.122)",
            "mul_div_ceil(command.101.120, command.101.121, command.101.122)",
            "signed_mul_div_floor(command.101.120, command.101.121, command.101.122)",
            "repeat_mul_div_floor(command.101.120, command.101.121, command.101.122, 0)",
            "repeat_mul_div_floor(command.101.120, command.101.121, command.101.122, 2)",
            "choose(true, command.101.120, div_floor(1,0))",
        ] {
            with_contract(&source, &rules(expr, typed), |contract, s| {
                writeln!(
                    &mut text,
                    "CASE {typed} {expr}\nSCHEMA {s:?}\nPOLICY {:?}\nPROGRAM {:?}\nBUDGET {:?}",
                    policy::encode(contract, s).unwrap(),
                    zeno_fcis_contract::internals::program_bytes(&contract.program().unwrap())
                        .unwrap(),
                    contract.budgets
                )
                .unwrap();
                let f = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
                policy::with_authority(contract, s, |a| {
                    for input in [
                        [0, 0, 0, 0, 0],
                        [0, 1, 1, 1, 0],
                        [0, HIGH, HIGH, HIGH, 0],
                        [0, HIGH, 1, 0, 0],
                        [0, LOW, -1, -1, 0],
                        [0, -1, 1, 2, 0],
                        [0, 1001, 3, 4, 0],
                        [0, 1, 0, 0, 0],
                        [0, 1, 1, -1, 0],
                    ] {
                        let frames = f.frame(&input);
                        let e = a.evaluate(c::Raw {
                            state: &frames[0],
                            command: &frames[1],
                            context: &frames[2],
                        });
                        writeln!(
                            &mut text,
                            "INPUT {input:?} OUTCOME {:?} USAGE {:?}",
                            evaluate::evaluate(a, &f, &input),
                            e.usage()
                        )
                        .unwrap();
                    }
                })
                .unwrap();
            });
        }
    }
    // Frozen from HEAD 8fcd0c5 plus the accepted W25b patch. The independent
    // review compared these exact bytes for 22 contracts and 198 evaluations.
    assert_eq!(text.len(), 619849);
    assert_eq!(
        zeno_fcis_crypto::RustCryptoSha256::hash(text.as_bytes()).as_bytes(),
        &[
            0x1a, 0xab, 0x0d, 0x27, 0x45, 0xe4, 0xc3, 0x53, 0xfc, 0x1d, 0x3c, 0xb1, 0x5e, 0x53,
            0x58, 0xb7, 0x85, 0x2e, 0xd5, 0xf7, 0xda, 0xf2, 0x36, 0x30, 0x8a, 0x80, 0xc2, 0x7f,
            0xd7, 0x72, 0xdf, 0xcc
        ]
    );
    if let Ok(path) = std::env::var("U128_COMPAT_SNAPSHOT") {
        std::fs::write(path, &text).unwrap();
    }
    println!(
        "22 legacy contracts, 198 evaluations and {} snapshot bytes match the accepted base",
        text.len()
    );
}
