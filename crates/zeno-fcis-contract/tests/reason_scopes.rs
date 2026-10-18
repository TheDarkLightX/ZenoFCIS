//! Strict necessary conditions, independent scalar relations and native totality.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, LawSet, Outcome, Proposed},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::v2_laws as l;

fn source(family: &str, c: i128) -> (String, String) {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core-components")
        .join(family);
    let read = |name| {
        std::fs::read_to_string(base.join(name))
            .unwrap()
            .replace("${C}", &c.to_string())
            .replace("${INSTANCE}", &format!("c{c}"))
    };
    (read("project.zeno.in"), read("policy.json.in"))
}
fn with(
    project: &str,
    text: &str,
    run: impl FnOnce(&zeno_fcis_synthesis::finite::v2_authority::Authority<'_>, &Framer),
) {
    let rules = Rules::read(text).unwrap();
    let declarations = Declarations::read(project, &rules.leaf_bindings).unwrap();
    let schema = schema::encode(&declarations).unwrap();
    let contract =
        Contract::build(&declarations, &rules, schema_commitment(&schema).unwrap()).unwrap();
    let positions = domain::positions(&declarations).unwrap();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(&contract, &schema, |a| run(a, &framer)).unwrap();
}
#[test]
fn complete_scalar_relations_cover_176_and_568_rows_without_991() {
    let mut totals = [0, 0];
    for (family_index, family) in ["bounded-counter", "consumable-budget"].iter().enumerate() {
        for c in 1..=8 {
            let (project, rules) = source(family, c);
            with(&project, &rules, |authority, framer| {
                let ids: Vec<_> = authority
                    .descriptor()
                    .laws
                    .iter()
                    .filter(|law| law.id < 900)
                    .map(|law| law.id)
                    .collect();
                assert!(!ids.contains(&991));
                let laws = LawSet::new(authority.descriptor(), &ids);
                let commands: Vec<_> = if family_index == 0 {
                    vec![150, 151]
                } else {
                    (0..=c).collect()
                };
                for pre in 0..=c {
                    for command in &commands {
                        for auth in 0..=1 {
                            let tuple = [pre, *command, auth];
                            totals[family_index] += 1;
                            // Frozen meaning, independent of producer cases and law graphs.
                            let reason = if auth == 0 {
                                Some(200)
                            } else if family_index == 0 {
                                if *command == 150 && pre == c {
                                    Some(201)
                                } else if *command == 151 && pre == 0 {
                                    Some(202)
                                } else {
                                    None
                                }
                            } else if *command > pre {
                                Some(201)
                            } else {
                                None
                            };
                            let class = if reason.is_some() {
                                Class::Reject
                            } else {
                                Class::Accept
                            };
                            let post = if reason.is_some() {
                                pre
                            } else if family_index == 1 {
                                pre - command
                            } else if *command == 150 {
                                pre + 1
                            } else {
                                pre - 1
                            };
                            let Outcome::Decision(native) =
                                evaluate::evaluate(authority, framer, &tuple)
                            else {
                                panic!("technical refusal: {family} C={c} {tuple:?}")
                            };
                            assert_eq!((native.class, native.reason), (class, reason));
                            if class == Class::Accept {
                                assert_eq!(native.post[0].1.number(), Some(post));
                            } else {
                                assert!(native.post.is_empty());
                            }
                            let mut count = 0;
                            for r in if family_index == 0 {
                                vec![200, 201, 202]
                            } else {
                                vec![200, 201]
                            } {
                                let p = Proposed {
                                    class: Class::Reject,
                                    reason: Some(r),
                                    post: &[],
                                };
                                let allowed = laws.on_transition(framer, &tuple, &p).0;
                                assert_eq!(allowed, class == Class::Reject && reason == Some(r));
                                count += usize::from(allowed);
                                for next in 0..=c {
                                    assert!(
                                        !laws
                                            .on_transition(
                                                framer,
                                                &tuple,
                                                &Proposed {
                                                    class: Class::CommittedFailure,
                                                    reason: Some(r),
                                                    post: &[next]
                                                }
                                            )
                                            .0
                                    );
                                }
                            }
                            for next in 0..=c {
                                let allowed = laws
                                    .on_transition(
                                        framer,
                                        &tuple,
                                        &Proposed {
                                            class: Class::Accept,
                                            reason: None,
                                            post: &[next],
                                        },
                                    )
                                    .0;
                                assert_eq!(allowed, class == Class::Accept && post == next);
                                count += usize::from(allowed);
                            }
                            assert_eq!(count, 1);
                        }
                    }
                }
            });
        }
    }
    assert_eq!(totals, [176, 568]);
}
#[test]
fn reason_predicates_are_strict_input_only_conjoined_and_bound() {
    let (project, rules) = source("bounded-counter", 1);
    let mut r: serde_json::Value = serde_json::from_str(&rules).unwrap();
    r["law_kinds"]["550"] = "AuthoritySubjectRecipient".into();
    let scoped = format!("{project}\nlaw 550 additional on reject:200 = div_floor(1, 0) == 0;\n");
    with(&scoped, &r.to_string(), |a, f| {
        let selected = LawSet::new(a.descriptor(), &[550]);
        let (_, d) = selected.on_transition(
            f,
            &[0, 150, 1],
            &Proposed {
                class: Class::Accept,
                reason: None,
                post: &[1],
            },
        );
        assert_eq!(d[0].verdict, l::Verdict::Skipped);
        let (_, d) = selected.on_transition(
            f,
            &[0, 150, 0],
            &Proposed {
                class: Class::Reject,
                reason: Some(200),
                post: &[],
            },
        );
        assert_eq!(d[0].verdict, l::Verdict::Refused(l::Failure::Undefined));
    });
    for (scope, predicate) in [
        ("reject:999", "true"),
        ("accept:200", "true"),
        ("reject:200, genesis", "true"),
        ("reject:200", "post.100.110 == 0"),
        ("reject:200", "decision_reason == 200"),
    ] {
        let p = format!("{project}\nlaw 550 invalid on {scope} = {predicate};\n");
        let rules = Rules::read(&r.to_string()).unwrap();
        let result = Declarations::read(&p, &rules.leaf_bindings).and_then(|d| {
            let s = schema::encode(&d)?;
            Contract::build(&d, &rules, schema_commitment(&s)?).map(|_| ())
        });
        assert!(result.is_err(), "admitted {scope}: {predicate}");
    }
    let contradictory =
        format!("{project}\nlaw 550 contradiction on reject:200 = context.102.130 == 1;\n");
    with(&contradictory, &r.to_string(), |a, f| {
        for ids in [&[520, 550][..], &[550, 520][..]] {
            assert!(
                !LawSet::new(a.descriptor(), ids)
                    .on_transition(
                        f,
                        &[0, 150, 0],
                        &Proposed {
                            class: Class::Reject,
                            reason: Some(200),
                            post: &[]
                        }
                    )
                    .0
            );
        }
    });
    // Changing producer class cannot reinterpret an existing Reject binding.
    r["cases"][0]["class"] = "CommittedFailure".into();
    let rules = Rules::read(&r.to_string()).unwrap();
    let d = Declarations::read(&project, &rules.leaf_bindings).unwrap();
    let s = schema::encode(&d).unwrap();
    assert!(Contract::build(&d, &rules, schema_commitment(&s).unwrap()).is_err());
}

#[test]
fn disabled_reason_and_outcome_checks_admit_their_assigned_faults() {
    let (project, rules) = source("bounded-counter", 2);
    with(&project, &rules, |a, f| {
        let ids: Vec<_> = a
            .descriptor()
            .laws
            .iter()
            .filter(|law| law.id < 900)
            .map(|law| law.id)
            .collect();
        let wrong_reason = Proposed {
            class: Class::Reject,
            reason: Some(201),
            post: &[],
        };
        let full = LawSet::new(a.descriptor(), &ids);
        assert!(!full.on_transition(f, &[0, 150, 0], &wrong_reason).0);
        let disabled: Vec<_> = ids.iter().copied().filter(|id| *id != 521).collect();
        assert!(
            LawSet::new(a.descriptor(), &disabled)
                .on_transition(f, &[0, 150, 0], &wrong_reason)
                .0
        );
        let wrong_movement = Proposed {
            class: Class::Accept,
            reason: None,
            post: &[0],
        };
        assert!(!full.on_transition(f, &[0, 150, 1], &wrong_movement).0);
        let disabled: Vec<_> = ids.iter().copied().filter(|id| *id != 502).collect();
        assert!(
            LawSet::new(a.descriptor(), &disabled)
                .on_transition(f, &[0, 150, 1], &wrong_movement)
                .0
        );
    });
}
