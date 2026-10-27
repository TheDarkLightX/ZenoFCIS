//! Independent Python oracle: every tuple, then all-pairs property verdicts.
use serde_json::{Value, json};
use zeno_fcis_contract::{
    internals::{
        declarations::Declarations,
        model::Contract,
        policy,
        review::{
            domain,
            evaluate::{self, Framer, Outcome},
        },
        rules::Rules,
        schema, schema_commitment,
    },
    two_run::{self, Property},
};
fn normalize(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Refused(r) => json!({"class":"Refused","refusal":r.class.name(),"law":r.law}),
        Outcome::Decision(d) => json!({"class":d.class.name(),"reason":d.reason,
            "post":d.post.iter().map(|(f,v)|(f.to_string(),v.json())).collect::<serde_json::Map<_,_>>(),
            "outbox":d.outbox.iter().map(|v|json!({"ordinal":v.ordinal,"channel":v.channel,"destination":v.destination.json(),"idempotency":v.idempotency.json(),"payload":v.payload.iter().map(|(f,v)|(f.to_string(),v.json())).collect::<serde_json::Map<_,_>>()})).collect::<Vec<_>>()}),
    }
}
#[test]
fn reference_corpus_matches_every_input_and_property() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus = if let Some(path) = std::env::var_os("ZENO_TWO_RUN_CORPUS") {
        std::path::PathBuf::from(path)
    } else {
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| root.join("target"));
        let path = target.join(format!("two-run-reference-{}.jsonl", std::process::id()));
        let status = std::process::Command::new("python3")
            .args(["-B", "tools/two_run_reference.py", "--corpus"])
            .arg(&path)
            .current_dir(&root)
            .output()
            .unwrap_or_else(|e| panic!("Python oracle: {e}"));
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        path
    };
    let contents =
        std::fs::read_to_string(&corpus).unwrap_or_else(|e| panic!("{}: {e}", corpus.display()));
    let mut contracts = 0;
    let mut tuples = 0;
    let mut questions = 0;
    for line in contents.lines() {
        let record: Value = serde_json::from_str(line).unwrap_or_else(|e| panic!("{e}"));
        let name = record["name"].as_str().unwrap_or_default();
        let project = record["project"].as_str().unwrap_or_default();
        let source = record["policy"].as_str().unwrap_or_default();
        let rules = Rules::read(source).unwrap_or_else(|e| panic!("{name}: {e}"));
        let d = Declarations::read(project, &rules.leaf_bindings)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let positions = domain::positions(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
        let schema = schema::encode(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
        let c = Contract::build(
            &d,
            &rules,
            schema_commitment(&schema).unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let f = Framer::new(&positions, &c);
        policy::with_authority(&c, &schema, |a| {
            for row in record["outcomes"]
                .as_array()
                .unwrap_or_else(|| panic!("outcomes"))
            {
                let input = row["tuple"]
                    .as_array()
                    .unwrap_or_else(|| panic!("tuple"))
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .unwrap_or_default()
                            .parse::<i128>()
                            .unwrap_or_else(|e| panic!("{e}"))
                    })
                    .collect::<Vec<_>>();
                let outcome = evaluate::evaluate(a, &f, &input);
                if name == "regression:invalid-payload" {
                    assert!(matches!(&outcome, Outcome::Refused(r) if r.text == "Core(Schema)"));
                }
                let actual = normalize(&outcome);
                assert_eq!(actual, row["outcome"], "contract {name}, tuple {input:?}");
                tuples += 1;
            }
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        for q in record["questions"]
            .as_array()
            .unwrap_or_else(|| panic!("questions"))
        {
            let paths = |key: &str| {
                q[key]
                    .as_array()
                    .unwrap_or_else(|| panic!("{key}"))
                    .iter()
                    .map(|v| v.as_str().unwrap_or_default().to_owned())
                    .collect::<Vec<_>>()
            };
            let property = if q["kind"] == "commute" {
                Property::Commute {
                    first: q["first"].as_str().unwrap_or_default().into(),
                    second: q["second"].as_str().unwrap_or_default().into(),
                    observe: paths("observe"),
                    within: u128::from(q["within"].as_u64().unwrap_or(0)),
                }
            } else {
                Property::Noninterference {
                    secret: paths("secret"),
                    public: paths("public"),
                }
            };
            let result = two_run::check(project, source, &property, two_run::MAX_TUPLES)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                result.verdict().name(),
                q["verdict"].as_str().unwrap_or_default(),
                "contract {name}: {:?}",
                result.packet()
            );
            if let Some(kind) = q.get("expected_difference_kind") {
                assert_eq!(
                    result.packet()["result"]["first_difference"]["kind"],
                    *kind,
                    "contract {name}"
                );
            }
            assert_eq!(
                result.packet()["measured"]["admissible_states"],
                record["admissible_states"],
                "{name}"
            );
            questions += 1;
        }
        contracts += 1;
    }
    assert!(
        contracts >= 304,
        "need all examples and at least 300 random contracts"
    );
    println!(
        "independent reference: contracts={contracts}, input_outcomes={tuples}, property_verdicts={questions}"
    );
}
