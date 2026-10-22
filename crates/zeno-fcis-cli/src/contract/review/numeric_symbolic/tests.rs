use super::*;
#[path = "lemma_tests.rs"]
mod lemma_tests;

fn answers(cvc5: Answer, z3: Answer) -> Answers {
    Answers {
        cvc5,
        z3,
        millis: [None, None],
    }
}
fn unsat() -> Answer {
    Answer::Unsat { proof_output: true }
}
fn project(max: &str, unsigned: bool) -> String {
    let kind = if unsigned { "u128" } else { "int" };
    let zero = if unsigned { "u128(0)" } else { "0" };
    format!(
        "zeno 1; project 1 numeric_scale; namespace 2 core; type 100 state State; type 101 command Command; type 102 context Context; type 105 {kind} Amount in 0..={max}; field 110 100 balance 105; field 120 101 amount 105; reason 300 Cancel precedence 0; component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }} merge [400]; law 500 bounded on commit, genesis = post.100.110 >= {zero}; law 501 exact on accept = post.100.110 == command.101.120;"
    )
}
fn rules(unsigned: bool, post: &str) -> String {
    json!({"schema":if unsigned{"zeno-fcis/template-declarative-policy/4"}else{"zeno-fcis/template-declarative-policy/3"},"template":"numeric-scale","roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"102":["Bool"]},"variables":{},"cases":[{"when":false,"class":"Reject","reason":300,"post":{},"outbox":[]},{"when":true,"class":"Accept","reason":null,"post":{"110":post},"outbox":[]}],"genesis":{"110":if unsigned{json!("u128(0)")}else{json!(0)}},"law_kinds":{"500":"StateInvariant","501":"FeeAndRounding"}}).to_string()
}
fn with_contract<T>(p: &str, r: &str, f: impl FnOnce(&Contract<'_>, &[u8]) -> T) -> T {
    let rules = Rules::read(r).unwrap_or_else(|error| panic!("{error:?}"));
    let d = Declarations::read(p, &rules.leaf_bindings).unwrap_or_else(|error| panic!("{error:?}"));
    let s = schema::encode(&d).unwrap_or_else(|error| panic!("{error:?}"));
    let c = Contract::build(
        &d,
        &rules,
        schema_commitment(&s).unwrap_or_else(|error| panic!("{error:?}")),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    f(&c, &s)
}
#[test]
fn single_solver_cannot_pass() {
    for (cvc, z3) in [
        (unsat(), Answer::NotConfigured),
        (Answer::NotConfigured, unsat()),
    ] {
        assert!(matches!(
            judge(&answers(cvc, z3), |_| Ok(json!({}))),
            Verdict::Unknown(_)
        ));
    }
    assert!(matches!(
        judge(&answers(unsat(), unsat()), |_| Ok(json!({}))),
        Verdict::Holds
    ));
}
#[test]
fn timeout_and_unknown_cannot_pass() {
    for answer in [Answer::Unknown, Answer::Failed("timeout".into())] {
        for a in [
            answers(unsat(), answer.clone()),
            answers(answer.clone(), unsat()),
        ] {
            assert!(matches!(judge(&a, |_| Ok(json!({}))), Verdict::Unknown(_)));
        }
    }
}
#[test]
fn disagreement_and_unreplayed_models_are_unknown() {
    let sat = Answer::Sat(BTreeMap::new());
    assert!(matches!(
        judge(&answers(sat.clone(), unsat()), |_| Ok(json!({}))),
        Verdict::Unknown(_)
    ));
    assert!(matches!(
        judge(&answers(sat.clone(), sat), |_| Err("bad witness".into())),
        Verdict::Unknown(_)
    ));
}
#[test]
fn in_cap_contract_stays_exhaustive() {
    let p = project("1", false);
    let r = rules(false, "command.101.120");
    assert!(!over_cap(&p, &r, 8).unwrap_or_else(|error| panic!("{error:?}")));
    assert!(over_cap(&p, &r, 7).unwrap_or_else(|error| panic!("{error:?}")));
    let report = super::super::review(
        ReviewSources {
            project: &p,
            rules: &r,
            examples: None,
        },
        8,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.summary().construction, "full-domain");
}
#[test]
fn full_unsigned_domain_uses_lossless_limb_models() {
    let p = project(&u128::MAX.to_string(), true);
    let r = rules(true, "command.101.120");
    with_contract(&p, &r, |c, _| {
        let row = decode(
            c,
            &BTreeMap::from([
                ("x0h".into(), u64::MAX.into()),
                ("x0l".into(), u64::MAX.into()),
                ("x1h".into(), 1 << 63),
                ("x1l".into(), 1),
                ("x2".into(), 1),
            ]),
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(row[0], Atom::U128(u128::MAX));
        assert_eq!(row[1], Atom::U128((1 << 127) + 1));
        assert!(size(c).is_none());
    });
}
#[test]
fn encoding_mismatch_refuses_symbolic_mode() {
    let p = project("3", false);
    let r = rules(false, "command.101.120");
    let mut solver = |_: &Query<'_>| {
        answers(
            Answer::Sat(BTreeMap::from([
                ("x0".into(), 0),
                ("x1".into(), 0),
                ("x2".into(), 0),
            ])),
            Answer::Sat(BTreeMap::from([
                ("x0".into(), 0),
                ("x1".into(), 0),
                ("x2".into(), 0),
            ])),
        )
    };
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r,
            examples: None,
        },
        &mut solver,
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "UNKNOWN");
    assert_eq!(result["encoding_guard"]["status"], "UNKNOWN");
    assert!(result.get("obligations").is_none());
}

/// Real solvers are mandatory for this explicit qualification test group.
fn pinned() -> crate::symbolic_command::Solvers {
    crate::symbolic_command::Solvers::from_environment()
        .unwrap_or_else(|error| panic!("both pinned solvers must run: {error}"))
}
#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_full_width_unsigned_laws() {
    let p = project(&u128::MAX.to_string(), true);
    let r = rules(true, "command.101.120");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r,
            examples: None,
        },
        &mut |q| solvers.solve(q),
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "solver-checked", "{result}");
    assert_eq!(result["evidence"], MODE);
}
#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_overflow_boundary_guard_matches_native() {
    let p = project(&i128::MAX.to_string(), false);
    let r = rules(false, "pre.100.110 + command.101.120");
    let solvers = pinned();
    with_contract(&p, &r, |c, s| {
        let e = Encoding::new(c, "p", Script::default(), None)
            .unwrap_or_else(|error| panic!("{error:?}"));
        let result =
            guard(c, s, &e, &mut |q| solvers.solve(q)).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(result["status"], "holds", "{result}");
    });
}
#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_planted_high_range_law_has_native_witness() {
    let p = project("1000000000000000000000000", true).replace(
        "post.100.110 == command.101.120",
        "post.100.110 < u128(999999999999999999999999)",
    );
    let r = rules(true, "command.101.120");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r,
            examples: None,
        },
        &mut |q| solvers.solve(q),
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "refuted", "{result}");
    let broken = result["obligations"]
        .as_array()
        .unwrap_or_else(|| panic!("expected array"))
        .iter()
        .find(|r| r["law"] == 501 && r["status"] == "refuted")
        .unwrap_or_else(|| panic!("expected refuted obligation"));
    assert_eq!(broken["counterexample"]["authority"]["law"], 501);
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_corrupt_encoding_is_refused() {
    let p = project("1000000000000000000000000", true);
    let r = rules(true, "command.101.120");
    let solvers = pinned();
    with_contract(&p, &r, |c, s| {
        let mut e = Encoding::new(c, "p", Script::default(), None)
            .unwrap_or_else(|error| panic!("{error:?}"));
        e.cases[1].constructed = Term::Bool(false);
        let result =
            guard(c, s, &e, &mut |q| solvers.solve(q)).unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(result["status"], "UNKNOWN", "{result}");
    });
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_state_and_transition_lemmas() {
    let p = project("1000000000000000000000000", true);
    let r = rules(true, "command.101.120");
    let solvers = pinned();
    for (transition, formula) in [
        (false, "post.100.110 >= u128(0)"),
        (true, "post.100.110 == command.101.120"),
    ] {
        let result = lemma::check(&p, &r, &[formula.into()], transition, &mut |q| {
            solvers.solve(q)
        })
        .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(result["status"], "solver-checked", "{result}");
    }
    let result = lemma::check(
        &p,
        &r,
        &["post.100.110 <= u128(1)".into()],
        false,
        &mut |q| solvers.solve(q),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "not-inductive", "{result}");
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_signed_operator_parity() {
    let p = project("100", false)
        .replace("in 0..=100", "in -100..=100")
        .replace("post.100.110 >= 0", "post.100.110 >= -100");
    let solvers = pinned();
    for expr in [
        "pre.100.110 - command.101.120",
        "pre.100.110 * command.101.120",
        "div_floor(pre.100.110, command.101.120)",
        "div_ceil(pre.100.110, command.101.120)",
        "signed_mul_div_floor(pre.100.110, command.101.120, 3)",
        "mul_div_ceil(pre.100.110, command.101.120, 3)",
    ] {
        let r = rules(false, expr);
        with_contract(&p, &r, |c, s| {
            let e = Encoding::new(c, "p", Script::default(), None)
                .unwrap_or_else(|error| panic!("{error:?}"));
            let result = guard(c, s, &e, &mut |q| solvers.solve(q))
                .unwrap_or_else(|error| panic!("{error:?}"));
            assert_eq!(result["status"], "holds", "{expr}: {result}");
        });
    }
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_swap_laws() {
    let p = include_str!("../../../../tests/fixtures/numeric-scale/project.zeno");
    let r = include_str!("../../../../tests/fixtures/numeric-scale/v2/policy.json");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: p,
            rules: r,
            examples: None,
        },
        &mut |q| solvers.solve(q),
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "solver-checked", "{result}");
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_law_overflow_above_two_to_100_has_native_witness() {
    let max = 1u128 << 120;
    let p = project(&max.to_string(), true).replace(
        "post.100.110 == command.101.120",
        "post.100.110 * u128(256) >= post.100.110",
    );
    let r = rules(true, "command.101.120");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r,
            examples: None,
        },
        &mut |q| solvers.solve(q),
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "refuted", "{result}");
    let broken = result["obligations"]
        .as_array()
        .unwrap_or_else(|| panic!("expected array"))
        .iter()
        .find(|r| r["law"] == 501 && r["status"] == "refuted")
        .unwrap_or_else(|| panic!("expected refuted obligation"));
    assert_eq!(broken["counterexample"]["authority"]["law"], 501);
    assert_eq!(
        broken["counterexample"]["values"][1]["value"],
        max.to_string()
    );
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_mutants_have_native_witnesses() {
    let p = project("1000000000000000000000000", true);
    let mut r: Json = serde_json::from_str(&rules(true, "command.101.120"))
        .unwrap_or_else(|error| panic!("{error:?}"));
    r["cases"][0]["when"] = json!("command.101.120 == u128(0)");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r.to_string(),
            examples: None,
        },
        &mut |q| solvers.solve(q),
        true,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "solver-checked", "{result}");
    let mutants = result["mutants"]
        .as_array()
        .unwrap_or_else(|| panic!("expected array"));
    assert!(!mutants.is_empty());
    assert!(
        mutants.iter().any(|m| m["status"] == "refuted"
            && m["counterexample"]["input"]["authority"]
                != m["counterexample"]["mutant_authority"]),
        "{result}"
    );
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_numeric_review_checks_unlawful_declared_pre_states() {
    let p = project("1000000000000000000000000", true)
        .replace("post.100.110 >= u128(0)", "post.100.110 >= u128(1)")
        .replace(
            "post.100.110 == command.101.120",
            "post.100.110 == pre.100.110",
        );
    let mut r: Json = serde_json::from_str(&rules(true, "pre.100.110"))
        .unwrap_or_else(|error| panic!("{error:?}"));
    r["genesis"]["110"] = json!("u128(1)");
    let solvers = pinned();
    let result = review(
        ReviewSources {
            project: &p,
            rules: &r.to_string(),
            examples: None,
        },
        &mut |q| solvers.solve(q),
        false,
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(result["status"], "refuted", "{result}");
    let failed = result["obligations"]
        .as_array()
        .unwrap_or_else(|| panic!("expected array"))
        .iter()
        .find(|o| o["law"] == 500 && o["status"] == "refuted")
        .unwrap_or_else(|| panic!("expected refuted obligation"));
    assert_eq!(failed["counterexample"]["values"][0]["value"], "0");
    assert_eq!(failed["counterexample"]["authority"]["law"], 500);
}
