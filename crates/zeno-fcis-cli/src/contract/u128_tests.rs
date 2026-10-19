//! Typed core commands and consumer binding witnesses, including high-bit finite domains.
use super::*;
use serde_json::{Value, json};
const H: u128 = 1u128 << 127;
fn project(min: u128, max: u128) -> String {
    format!(
        "zeno 1; project 1 unsigned_review; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 u128 Amount in {min}..={max}; field 110 100 result 105; field 120 101 amount 105; reason 300 Cancel precedence 0; component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400]; law 500 bounded on commit, genesis = post.100.110 >= u128({min}); law 501 reject_false on reject = false;"
    )
}
fn rules(min: u128) -> String {
    json!({"schema":"zeno-fcis/template-declarative-policy/4","template":"unsigned-review","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":false,"class":"Reject","reason":300,"post":{},"outbox":[]},{"when":true,"class":"Accept","reason":null,"post":{"110":"command.101.120"},"outbox":[]}],"genesis":{"110":format!("u128({min})")},"law_kinds":{"500":"StateInvariant","501":"FeeAndRounding"}}).to_string()
}
fn sources<'a>(p: &'a str, r: &'a str) -> ContractSources<'a> {
    ContractSources {
        project: p,
        rules: r,
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    }
}
fn review_sources<'a>(p: &'a str, r: &'a str, e: Option<&'a str>) -> review::ReviewSources<'a> {
    review::ReviewSources {
        project: p,
        rules: r,
        examples: e,
    }
}
#[test]
fn u128_generate_and_review_keep_high_bit_values() {
    let p = project(H, H + 1);
    let r = rules(H);
    let g = generate_contract(sources(&p, &r)).unwrap();
    assert!(g.program().is_none());
    assert!(g.source().contains("NumericDescriptor"));
    let e = format!("{H} {} 0 | accept - {} | -\n", H + 1, H + 1);
    let one = review::review(review_sources(&p, &r, Some(&e)), 100).unwrap();
    let two = review::review(review_sources(&p, &r, Some(&e)), 100).unwrap();
    assert_eq!(one.packet(), two.packet());
    let packet: Value = serde_json::from_str(one.packet()).unwrap();
    assert_eq!(packet["summary"]["examples"]["disagreements"], 0);
    assert_eq!(packet["summary"]["inputs"]["count"], 8);
    assert!(one.packet().contains(&(H + 1).to_string()));
}
#[test]
fn u128_finite_gaps_proposals_and_limits() {
    let p = project(H, H + 1);
    let r = rules(H);
    let settings = review::gaps::Options {
        mutants: true,
        max_tuples: 100,
        max_work: 100000,
        proposals: None,
    };
    let run = |options| review::review_with_gaps(review_sources(&p, &r, None), options).unwrap();
    let baseline = run(settings);
    let packet: Value = serde_json::from_str(baseline.packet()).unwrap();
    assert_eq!(packet["gaps"]["admissible"]["full"]["undetermined"], 8);
    let proposal = "law 600 exact on commit = post.100.110 == command.101.120;";
    let more = run(review::gaps::Options {
        proposals: Some(proposal),
        ..settings
    });
    let packet: Value = serde_json::from_str(more.packet()).unwrap();
    assert_eq!(
        packet["gaps"]["proposals"]["after"]["admissible"]["full"]["determined"],
        8
    );
    let alarm = run(review::gaps::Options {
        proposals: Some("law 600 false_alarm on commit = post.100.110 == u128(0);"),
        ..settings
    });
    assert!(alarm.packet().contains("proposal-false-alarm"));
    let over = run(review::gaps::Options {
        max_work: 1,
        ..settings
    });
    assert!(over.packet().contains("work-exceeds-limit"));
}
#[test]
fn u128_lemma_state_transition_and_inconclusive() {
    let p = project(H, H + 1);
    let r = rules(H);
    let formulas = [
        format!("post.100.110 >= u128({H})"),
        "post.100.110 == u128(0)".into(),
    ];
    let review::lemma::Lemmas::Judged(result) =
        review::lemma::judge(&p, &r, &formulas, 100).unwrap()
    else {
        panic!("finite domain")
    };
    assert_eq!(result[0]["verdict"], "inductive");
    assert_eq!(result[1]["verdict"], "false");
    let review::lemma::Lemmas::Judged(result) =
        review::lemma::judge_transitions(&p, &r, &["post.100.110 == command.101.120".into()], 100)
            .unwrap()
    else {
        panic!("finite domain")
    };
    assert_eq!(result[0]["verdict"], "checked");
    assert!(matches!(
        review::lemma::judge(&project(0, u128::MAX), &rules(0), &formulas, 100).unwrap(),
        review::lemma::Lemmas::Inconclusive(_)
    ));
}
#[test]
fn u128_tau_and_signed_program_binding_refuse_even_low_values() {
    for (min, max) in [(0, 1), (H, H + 1)] {
        let p = project(min, max);
        let r = rules(min);
        let err = tau::export(&p, &r, 100, 1).unwrap_err();
        assert_eq!(err.reason(), "U128SubjectUnsupported");
        let parsed = Rules::read(&r).unwrap();
        let d = Declarations::read(&p, &parsed.leaf_bindings).unwrap();
        let bytes = schema::encode(&d).unwrap();
        let c = Contract::build(&d, &parsed, schema_commitment(&bytes).unwrap()).unwrap();
        assert_eq!(c.program().unwrap_err().reason(), "U128SubjectUnsupported");
        assert_eq!(
            review::domain::positions(&d).unwrap_err().reason(),
            "U128SubjectUnsupported"
        );
    }
}
#[test]
fn u128_review_preserves_existing_sum_variants() {
    let p = project(H, H + 1)
        + " type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 121 101 mode 109;";
    let r = rules(H);
    let result = review::review(review_sources(&p, &r, None), 100).unwrap();
    let packet: Value = serde_json::from_str(result.packet()).unwrap();
    assert_eq!(packet["summary"]["inputs"]["count"], 16);
    assert_eq!(packet["domain_size"], "16");
}
#[test]
fn u128_symbolic_refuses_before_solver_and_sqlite_migration_before_mapping() {
    for min in [0, H] {
        let p = project(min, min + 1);
        let r = rules(min);
        let mut solve =
            |_: &crate::symbolic::verdict::Query<'_>| panic!("unsigned subject reached a solver");
        let error = symbolic::check(
            symbolic::SymbolicSources {
                project: &p,
                rules: &r,
                strengthening: None,
            },
            100,
            &mut solve,
        )
        .err()
        .unwrap();
        assert_eq!(error.reason(), "U128SubjectUnsupported");
        let r = Rules::read(&r).unwrap();
        let d = Declarations::read(&p, &r.leaf_bindings).unwrap();
        let file = rules::read_migration(
            r#"{"schema":"zeno-fcis/migration/1","state":{"110":{"from":110}}}"#,
            "migration",
        )
        .unwrap();
        let e = migration::compile(&file, &d, &d, "migration")
            .err()
            .unwrap();
        assert_eq!(e.reason(), "U128SubjectUnsupported");
    }
}
#[test]
fn u128_search_optimizer_and_loop_doors_share_a_refusing_signed_json_binding() {
    use crate::neural_loop::program_json::{ProgramJsonError, program_from_json};
    for max in [1u128, u128::MAX] {
        let subject = json!({"inputs":[{"kind":"U128","min":"0","max":max.to_string()}],"outputs":[{"kind":"Int","min":"0","max":"1"}],"nodes":[["Int","0"]],"roots":[0]});
        assert_eq!(
            program_from_json(&subject).unwrap_err(),
            ProgramJsonError::U128SubjectUnsupported
        );
    }
}
#[test]
fn u128_new_scaffolds_a_native_typed_application() {
    use std::path::Path;
    let root = Path::new("/tmp/sol-u128").join(format!("generated-u128-{}", std::process::id()));
    let input = root.join("input");
    let app = root.join("app");
    std::fs::create_dir_all(input.join("v2")).unwrap();
    std::fs::create_dir_all(input.join("tests")).unwrap();
    std::fs::create_dir_all(&app).unwrap();
    let project = project(H, H + 1).replace(" law 501 reject_false on reject = false;", "")
        + " type 108 bool Flag; field 111 100 flag 108; type 109 data Mode; variant 130 109 First none; variant 131 109 Second none; field 121 101 mode 109;";
    let mut rules: Value = serde_json::from_str(&rules(H)).unwrap();
    rules["cases"][0]["when"] = json!("context.102");
    rules["leaf_bindings"]["108"] = json!(["Bool"]);
    rules["genesis"]["111"] = json!(false);
    rules["cases"][1]["post"]["111"] = json!(format!(
        "choose(context.102, false, products_ge(command.101.120, u128({}), command.101.120, u128({})))",
        u128::MAX,
        u128::MAX
    ));
    rules["cases"][1]["post"]["110"] = json!(format!(
        "mul_div_floor(command.101.120, u128({}), u128({})) + to_u128(to_i128(u128(0)))",
        u128::MAX,
        u128::MAX
    ));
    rules["law_kinds"].as_object_mut().unwrap().remove("501");
    std::fs::write(input.join("project.zeno"), project).unwrap();
    std::fs::write(input.join("v2/policy.json"), rules.to_string()).unwrap();
    std::fs::write(
        input.join("tests/decision-examples.txt"),
        format!(
            "{H} 0 {} 131 0 | accept - {} 1 | -\n{H} 1 {} 130 1 | reject 300 {H} 1 | -\n",
            H + 1,
            H + 1,
            H + 1
        ),
    )
    .unwrap();
    let tree = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    assert_eq!(
        crate::contract_files::scaffold(&app, &input, Some(tree)),
        crate::OK
    );
    assert!(
        std::fs::read_to_string(app.join("src/lib.rs"))
            .unwrap()
            .contains("NumericDescriptor")
    );
    std::fs::write(
        "/tmp/sol-u128/generated-app-path.txt",
        app.to_str().unwrap(),
    )
    .unwrap();
}
#[test]
fn u128_reject_examples_require_the_unchanged_pre_state() {
    let p = project(H, H + 1).replace(" law 501 reject_false on reject = false;", "");
    let mut r: Value = serde_json::from_str(&rules(H)).unwrap();
    r["cases"][0]["when"] = json!("context.102");
    r["law_kinds"].as_object_mut().unwrap().remove("501");
    let r = r.to_string();
    for (post, disagrees) in [(H, 0), (H + 1, 1)] {
        let examples = format!("{H} {} 1 | reject 300 {post} | -\n", H + 1);
        let report = review::review(review_sources(&p, &r, Some(&examples)), 100).unwrap();
        let packet: Value = serde_json::from_str(report.packet()).unwrap();
        assert_eq!(packet["summary"]["examples"]["disagreements"], disagrees);
    }
}
#[test]
fn u128_law_refusal_findings_keep_full_width_human_witnesses() {
    let p = project(H, H + 1);
    let mut r: Value = serde_json::from_str(&rules(H)).unwrap();
    r["cases"][0]["when"] = json!("context.102");
    let r = r.to_string();
    let result = review::review(review_sources(&p, &r, None), 100).unwrap();
    let lines = result.law_refusal_lines();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("4 inputs"));
    assert!(lines[0].contains(&H.to_string()));
    let packet: Value = serde_json::from_str(result.packet()).unwrap();
    assert_eq!(
        packet["law_refusals"][0]["input"][0]["value"],
        H.to_string()
    );
    assert_eq!(packet["summary"]["law_refusal_findings"], 1);
}

#[test]
fn u128_policy4_signed_only_build_checks_unused_bindings() {
    let p = project(0, 1)
        .replace("type 105 u128", "type 105 int")
        .replace(">= u128(0)", ">= 0");
    let mut r: Value = serde_json::from_str(&rules(0)).unwrap();
    r["genesis"]["110"] = json!(0);
    for expression in ["choose(true, u128(0), 1)", "to_u128(context.102)"] {
        r["variables"] = json!({"bad":expression});
        let text = r.to_string();
        let error = generate_contract(sources(&p, &text)).unwrap_err();
        assert!(
            error.reason().contains("KindMismatch"),
            "{expression}: {error}"
        );
    }
    r["variables"] = json!({});
    let text = r.to_string();
    let generated = generate_contract(sources(&p, &text)).unwrap();
    assert!(generated.program().is_none());
    assert!(generated.source().contains("NumericDescriptor"));
    assert!(review::review(review_sources(&p, &text, None), 100).is_ok());
}
