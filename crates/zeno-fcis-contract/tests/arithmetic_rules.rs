//! Authored arithmetic rules run through the ordinary bound library Authority.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_contract::internals::review::{
    domain,
    evaluate::{self, Framer, Outcome, RefusalClass},
};
use zeno_fcis_contract::internals::{
    declarations::Declarations, expr, model::Contract, policy, rules::Rules, schema,
    schema_commitment,
};
use zeno_fcis_synthesis::finite::{ARITHMETIC_PROFILE, Op};
use zeno_fcis_synthesis::finite_runtime::import_program;

const PROJECT: &str = include_str!("../../../examples/arithmetic-fee/project.zeno");
const RULES: &str = include_str!("../../../examples/arithmetic-fee/v2/policy.json");

#[test]
fn authored_fused_value_laws_use_existing_full_products_and_refusals() {
    let project = include_str!("../../../examples/arithmetic-fused/project.zeno");
    let rules = include_str!("../../../examples/arithmetic-fused/v2/policy.json").replace(
        "\"501\": \"FeeAndRounding\"",
        "\"501\": \"FeeAndRounding\", \"502\": \"FeeAndRounding\"",
    );
    let large = "(9223372036854775807 * 9223372036854775807)";
    for (formula, accepted) in [
        (format!("mul_div_floor({large}, 8, 8) == {large}"), true),
        (format!("mul_div_ceil({large}, 8, 8) == {large}"), true),
        ("mul_div_floor(-1, -1, 1) == 1".to_owned(), false),
        ("mul_div_ceil(1, 1, 0) == 0".to_owned(), false),
        (format!("mul_div_floor({large}, 8, 1) == 0"), false),
    ] {
        let source = format!("{project}\nlaw 502 fused_value on accept = {formula};\n");
        with_contract(&source, &rules, |contract, schema| {
            let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
            policy::with_authority(contract, schema, |authority| {
                let actual = evaluate::evaluate(authority, &framer, &[0, 1, 1, 1, 0]);
                if accepted {
                    assert!(
                        matches!(actual, Outcome::Decision(_)),
                        "{formula}: {actual:?}"
                    );
                } else {
                    assert!(
                        matches!(actual,Outcome::Refused(ref r) if r.law==Some(502)),
                        "{formula}: {actual:?}"
                    );
                }
            })
            .unwrap();
        });
    }
}

fn with_contract(project: &str, source: &str, check: impl FnOnce(&Contract<'_>, &[u8])) {
    let rules = Rules::read(source).unwrap();
    let declarations = Declarations::read(project, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    check(&contract, &schema);
}

#[test]
fn generic_fee_is_computed_by_the_authority_on_every_reduced_tuple() {
    with_contract(PROJECT, RULES, |contract, schema| {
        let p = contract.program().unwrap();
        assert_eq!(p.profile(), ARITHMETIC_PROFILE);
        assert!(p.nodes().iter().any(|op| matches!(op, Op::MulDivCeil(..))));
        assert_eq!(
            import_program(&p.value().unwrap().canonical_bytes().unwrap()).unwrap(),
            p
        );
        let positions = domain::positions(contract.declarations).unwrap();
        let framer = Framer::new(&positions, contract);
        policy::with_authority(contract, schema, |authority| {
            let mut count = 0;
            for old_fee in 0..=20 {
                for old_net in 0..=20 {
                    for amount in 0..=20 {
                        for rate in 0..=3 {
                            for context in 0..=1 {
                                let input = [old_fee, old_net, amount, rate, context];
                                let result = evaluate::evaluate(authority, &framer, &input);
                                let Outcome::Decision(decision) = result else {
                                    panic!("Authority refused {input:?}: {result:?}")
                                };
                                let expected = (amount * rate + 9) / 10;
                                assert_eq!(decision.post[0].1.number(), Some(expected as i128));
                                assert_eq!(
                                    decision.post[1].1.number(),
                                    Some((amount - expected) as i128)
                                );
                                assert!(decision.outbox.is_empty());
                                count += 1;
                            }
                        }
                    }
                }
            }
            assert_eq!(count, 74088);
        })
        .unwrap();
    });
}

#[test]
fn wrong_rounding_and_false_laws_refuse_without_a_decision_or_effects() {
    let wrong = RULES.replace("mul_div_ceil", "mul_div_floor");
    for (project, rules, law) in [
        (PROJECT, wrong.as_str(), 501u32),
        (
            &PROJECT.replace(
                "post.100.110 == div_ceil(command.101.120 * command.101.121, 10)",
                "false",
            ),
            RULES,
            501u32,
        ),
    ] {
        with_contract(project, rules, |contract, schema| {
            let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
            policy::with_authority(contract, schema, |authority| {
                let first = evaluate::evaluate(authority, &framer, &[0, 0, 1, 1, 0]);
                let Outcome::Refused(refusal) = &first else {
                    panic!("a false arithmetic law published {first:?}")
                };
                assert_eq!(refusal.class, RefusalClass::Law);
                assert_eq!(refusal.law, Some(law));
                assert_eq!(
                    first,
                    evaluate::evaluate(authority, &framer, &[0, 0, 1, 1, 0])
                );
            })
            .unwrap();
        });
    }
}

#[test]
fn ceiling_quote_and_fitting_fused_quotient_use_authored_rules() {
    for (project, rules, input, answer) in [
        (
            include_str!("../../../examples/arithmetic-quote/project.zeno"),
            include_str!("../../../examples/arithmetic-quote/v2/policy.json"),
            vec![0, 5, 2, 0],
            3,
        ),
        (
            include_str!("../../../examples/arithmetic-fused/project.zeno"),
            include_str!("../../../examples/arithmetic-fused/v2/policy.json"),
            vec![0, i64::MAX, i64::MAX, i64::MAX, 0],
            i64::MAX,
        ),
    ] {
        with_contract(project, rules, |contract, schema| {
            let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
            policy::with_authority(contract, schema, |authority| {
                let actual = evaluate::evaluate(authority, &framer, &input);
                let Outcome::Decision(decision) = actual else {
                    panic!("refused {actual:?}")
                };
                assert_eq!(decision.post[0].1.number(), Some(answer as i128));
            })
            .unwrap();
        });
    }
}

#[test]
fn product_comparison_rule_and_law_share_exact_unsigned_products() {
    let predicate =
        "products_ge(command.101.120, command.101.121, command.101.120, command.101.121)";
    let source = RULES.replace(
        "command.101.120 - fee",
        &format!("choose({predicate}, command.101.120 - fee, 0)"),
    );
    with_contract(PROJECT, &source, |contract, _| {
        assert!(
            contract
                .program()
                .unwrap()
                .nodes()
                .iter()
                .any(|op| matches!(op, Op::ProductsGe(..)))
        );
    });
    for text in [
        "mul_div_floor(a, b, d)",
        "mul_div_ceil(a, b, d)",
        "products_ge(a, b, c, d)",
        "products_ge(a, b, c, d, e, f)",
    ] {
        let ast = expr::parse(text).unwrap();
        assert_eq!(expr::parse(&expr::render(&ast)).unwrap(), ast);
    }
    for text in [
        "products_ge(1, 2, 3)",
        "mul_div_floor(1, 2)",
        "products_ge(1, 2, 3, 4, 5)",
    ] {
        assert!(expr::parse(text).is_err());
    }
}

#[test]
fn full_size_three_factor_laws_keep_the_high_product_bits() {
    let source = include_str!("../../../examples/arithmetic-fused/project.zeno");
    let rules = include_str!("../../../examples/arithmetic-fused/v2/policy.json");
    for (left, accepted) in [("command.101.120", true), ("command.101.120 - 1", false)] {
        let project = format!(
            "{source}\nlaw 502 triple_bound on accept = products_ge({left}, command.101.121, command.101.120, post.100.110, command.101.122, command.101.120);\n"
        );
        let rules = rules.replace(
            "\"501\": \"FeeAndRounding\"",
            "\"501\": \"FeeAndRounding\", \"502\": \"FeeAndRounding\"",
        );
        with_contract(&project, &rules, |contract, schema| {
            let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
            policy::with_authority(contract,schema,|authority| {
                let actual=evaluate::evaluate(authority,&framer,&[0,i64::MAX,i64::MAX,i64::MAX,0]);
                if accepted {assert!(matches!(actual,Outcome::Decision(_)));}
                else {assert!(matches!(actual,Outcome::Refused(r) if r.class==RefusalClass::Law && r.law==Some(502)));}
            }).unwrap();
        });
    }
}

#[test]
fn a_zero_multiplier_does_not_remove_the_eager_bad_subexpression() {
    let rules = RULES.replace(
        "mul_div_ceil(command.101.120, command.101.121, 10)",
        "0 * (9223372036854775807 + 1)",
    );
    with_contract(PROJECT, &rules, |contract, schema| {
        let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
        policy::with_authority(contract, schema, |authority| {
            let actual = evaluate::evaluate(authority, &framer, &[0, 0, 1, 1, 0]);
            assert!(matches!(actual,Outcome::Refused(r) if r.class==RefusalClass::Arithmetic));
        })
        .unwrap();
    });
}

#[test]
fn successful_authority_path_controls_reject_fee_stubs_and_disabled_comparison() {
    fn matches_expected(actual: &Outcome, amount: i64, rate: i64, comparison: bool) -> bool {
        if !comparison {
            return true;
        }
        match actual {
            Outcome::Decision(decision) => {
                decision.post[0].1.number() == Some(((amount * rate + 9) / 10) as i128)
                    && decision.post[1].1.number()
                        == Some((amount - (amount * rate + 9) / 10) as i128)
            }
            Outcome::Refused(_) => false,
        }
    }
    let expression = "mul_div_ceil(command.101.120, command.101.121, 10)";
    for stub in ["0", "1", "div_floor(1, 0)"] {
        let rules = RULES.replace(expression, stub);
        with_contract(PROJECT, &rules, |contract, schema| {
            let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
            policy::with_authority(contract, schema, |authority| {
                let actual = evaluate::evaluate(authority, &framer, &[0, 0, 20, 3, 0]);
                assert!(
                    !matches_expected(&actual, 20, 3, true),
                    "successful-path mutant survived"
                );
                assert!(
                    matches_expected(&actual, 20, 3, false),
                    "disabling the comparator must let the same fault survive"
                );
            })
            .unwrap();
        });
    }
}

#[test]
fn signed_minimum_literal_is_legal_but_its_division_by_minus_one_refuses() {
    let rules = RULES.replace(
        "mul_div_ceil(command.101.120, command.101.121, 10)",
        "div_floor(-9223372036854775808, -1)",
    );
    with_contract(PROJECT, &rules, |contract, schema| {
        assert!(
            contract
                .program()
                .unwrap()
                .nodes()
                .iter()
                .any(|op| matches!(op, Op::Int(i64::MIN)))
        );
        let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
        policy::with_authority(contract, schema, |authority| {
            let actual = evaluate::evaluate(authority, &framer, &[0, 0, 1, 1, 0]);
            assert!(matches!(actual,Outcome::Refused(r) if r.class==RefusalClass::Arithmetic));
        })
        .unwrap();
    });
}

#[test]
fn proposed_integer_square_root_uses_exact_rules_and_laws() {
    use zeno_fcis_contract::internals::rules::Class;
    let project = r#"zeno 1;
project 1 proposed_root;
namespace 2 core;
type 100 state Root;
type 101 command Proposal;
type 102 context Check;
type 105 int Units in 0..=9223372036854775807;
field 110 100 value 105;
field 120 101 first 105;
field 121 101 second 105;
field 122 101 root 105;
field 130 102 marker 105;
reason 200 bad_root precedence 0;
component 400 root {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 128;
}
merge [400];
law 499 root_domain on commit, genesis = 0 <= post.100.110 && post.100.110 <= 9223372036854775807;
law 500 exact_root on accept = products_ge(command.101.120, command.101.121, post.100.110, post.100.110) && !products_ge(command.101.120, command.101.121, post.100.110 + 1, post.100.110 + 1);
"#;
    // At MAX, the next public i64 factor cannot be formed. The checked rule
    // uses a fitting increment and the known upper bound MAX*MAX; the law
    // checks the exact next square through its existing i128 value expressions.
    let rules = r#"{
      "schema": "zeno-fcis/template-declarative-policy/2",
      "template": "proposed-root",
      "roots": {"state": 100, "command": 101, "context": 102},
      "leaf_bindings": {},
      "variables": {
        "a0": "command.101.120", "a1": "command.101.121",
        "s": "command.101.122",
        "upper": "s + choose(s == 9223372036854775807, 0, 1)"
      },
      "cases": [
        {"when": "products_ge(a0, a1, s, s) && (s == 9223372036854775807 || !products_ge(a0, a1, upper, upper))",
         "class": "Accept", "reason": null, "post": {"110": "s"}, "outbox": []},
        {"when": "true", "class": "Reject", "reason": 200, "post": {}, "outbox": []}
      ],
      "genesis": {"110": 0}, "law_kinds": {"499": "StateInvariant", "500": "FeeAndRounding"}
    }"#;
    with_contract(project, rules, |contract, schema| {
        assert!(
            contract
                .program()
                .unwrap()
                .nodes()
                .iter()
                .any(|op| matches!(op, Op::ProductsGe(..)))
        );
        let framer = Framer::new(&domain::positions(contract.declarations).unwrap(), contract);
        policy::with_authority(contract, schema, |authority| {
            let check = |a0: i64, a1: i64, proposed: i64| {
                let product = i128::from(a0) * i128::from(a1);
                let s = i128::from(proposed);
                let expected = s * s <= product && product < (s + 1) * (s + 1);
                let actual = evaluate::evaluate(authority, &framer, &[0, a0, a1, proposed, 0]);
                let Outcome::Decision(decision) = actual else {
                    panic!("root proposal refused {a0} {a1} {proposed}: {actual:?}");
                };
                assert_eq!(
                    decision.class,
                    if expected {
                        Class::Accept
                    } else {
                        Class::Reject
                    }
                );
                if expected {
                    assert_eq!(decision.post[0].1.number(), Some(s));
                } else {
                    assert_eq!(decision.reason, Some(200));
                    assert!(decision.post.is_empty());
                }
                assert!(decision.outbox.is_empty());
            };
            let mut count = 0;
            for a0 in 0..=20 {
                for a1 in 0..=20 {
                    for proposed in 0..=20 {
                        check(a0, a1, proposed);
                        count += 1;
                    }
                }
            }
            assert_eq!(count, 9261);
            for (a0, a1, proposed) in [
                (i64::MAX, i64::MAX, i64::MAX),
                (i64::MAX, i64::MAX, i64::MAX - 1),
                (i64::MAX, i64::MAX - 1, i64::MAX),
                (i64::MAX, 1, 3_037_000_499),
                (i64::MAX, 1, 3_037_000_500),
                (0, 0, 0),
                (0, i64::MAX, 1),
            ] {
                check(a0, a1, proposed);
            }
        })
        .unwrap();
    });
}
