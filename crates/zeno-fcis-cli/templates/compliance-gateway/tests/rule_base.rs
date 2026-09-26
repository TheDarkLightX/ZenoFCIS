//! Checks the rule base itself: that it is consistent, total, and live, that
//! the synthesized step decides every input exactly as the rule base does,
//! and that a conflicting, incomplete, or shadowed rule base is refused.
//!
//! `src/rules.rs` evaluates `rules.txt` directly, without the contract in
//! `synthesis.json` or the selected program, so agreement here is a check on
//! the contract's derivation as much as on the synthesis.

use compliance_gateway::{
    rules::{Feature, RULE_BASE, Rule, RuleBase, RuleBaseError, Verdict},
    synthesized,
};

/// The strikes after a blocked transfer, as the README states them.
fn struck(strikes: i64) -> i64 {
    (strikes + 1).min(3)
}

#[test]
fn the_rule_base_is_consistent_and_total() {
    let base = RuleBase::parse(RULE_BASE).unwrap();
    assert_eq!(base.check(), Ok(()));
    assert_eq!(base.rules().len(), 12);
    let names: Vec<&str> = base.features().iter().map(Feature::name).collect();
    assert_eq!(
        names,
        [
            "strikes",
            "identity_tier",
            "region",
            "amount_band",
            "counterparty_risk"
        ]
    );
    assert_eq!(base.inputs().len(), 720);
    // The blocking rules come first, so their committed-failure reasons
    // 210 to 214 follow the order of the file.
    let blocking: Vec<usize> = (0..base.rules().len())
        .filter(|rule| base.rules()[*rule].verdict() == Verdict::Block)
        .collect();
    assert_eq!(blocking, [0, 1, 2, 3, 4]);
    assert_eq!(base.block_ordinal(4), Some(4));
    assert_eq!(base.block_ordinal(5), None);
    // Two rules share priority 80. The check above shows that no transfer
    // matches both; this shows that the pair is really there.
    let at_80: Vec<&str> = base
        .rules()
        .iter()
        .filter(|rule| rule.priority() == 80)
        .map(Rule::name)
        .collect();
    assert_eq!(at_80, ["restricted_large", "unverified_large"]);
    // The default rule matches everything, at the lowest priority.
    let last = base.rules().last().unwrap();
    assert_eq!(
        (last.name(), last.priority(), last.verdict()),
        ("default_allow", 0, Verdict::Allow)
    );
}

#[test]
fn every_input_decides_the_same_in_the_rule_base_and_the_synthesized_step() {
    let base = RuleBase::load(RULE_BASE).unwrap();
    let inputs = base.inputs();
    assert_eq!(inputs.len(), 720);
    let mut verdicts = [0; 3];
    for input in inputs {
        let fired = base.decide(&input).unwrap();
        let expected_strikes = match fired.verdict {
            Verdict::Block => struck(input[0]),
            Verdict::Allow | Verdict::Hold => input[0],
        };
        for reviewer in 0..=1 {
            let mut screening = input.clone();
            screening.extend([0, reviewer]);
            assert_eq!(
                synthesized::transition(&screening),
                Some([
                    fired.verdict.code(),
                    i64::try_from(fired.rule).unwrap(),
                    expected_strikes
                ]),
                "screening {screening:?}"
            );
            let mut reinstatement = input.clone();
            reinstatement.extend([1, reviewer]);
            let code = if reviewer == 0 {
                3
            } else if input[0] == 0 {
                4
            } else {
                5
            };
            let post = if code == 5 { 0 } else { input[0] };
            assert_eq!(
                synthesized::transition(&reinstatement),
                Some([code, 11, post]),
                "reinstatement {reinstatement:?}"
            );
        }
        verdicts[usize::try_from(fired.verdict.code()).unwrap()] += 1;
    }
    // Every verdict occurs.
    assert!(verdicts.iter().all(|count| *count > 0), "{verdicts:?}");
}

#[test]
fn the_synthesized_step_refuses_inputs_outside_the_domain() {
    assert!(synthesized::transition(&[0, 0, 0, 0, 0, 0, 0]).is_some());
    for outside in [
        [4, 0, 0, 0, 0, 0, 0],
        [-1, 0, 0, 0, 0, 0, 0],
        [0, 4, 0, 0, 0, 0, 0],
        [0, 0, 3, 0, 0, 0, 0],
        [0, 0, 0, 5, 0, 0, 0],
        [0, 0, 0, 0, 3, 0, 0],
        [0, 0, 0, 0, 0, 2, 0],
        [0, 0, 0, 0, 0, 0, 2],
    ] {
        assert!(synthesized::transition(&outside).is_none(), "{outside:?}");
    }
    assert!(synthesized::transition(&[0, 0, 0, 0, 0, 0]).is_none());
    assert!(synthesized::transition(&[0, 0, 0, 0, 0, 0, 0, 0]).is_none());
}

#[test]
fn a_conflicting_or_incomplete_rule_base_is_refused() {
    // Two rules of the same priority with different conclusions on some
    // transfer: a restricted transfer of band 1 or 2 matches both.
    let conflicting = "\
feature amount_band 0..2
feature region allowed restricted
rule large priority 10 when amount_band >= 1 then hold
rule restricted priority 10 when region == restricted then block
rule default priority 0 when always then allow
";
    assert_eq!(
        RuleBase::load(conflicting),
        Err(RuleBaseError::Conflict {
            input: vec![1, 1],
            first: "large".into(),
            second: "restricted".into(),
        })
    );
    // The same conclusion does not make it consistent: the decision would
    // still not name one rule.
    let same_conclusion = conflicting.replace("then block", "then hold");
    assert!(matches!(
        RuleBase::load(&same_conclusion),
        Err(RuleBaseError::Conflict { .. })
    ));
    // Different priorities resolve it.
    let resolved =
        conflicting.replace("rule restricted priority 10", "rule restricted priority 20");
    let base = RuleBase::load(&resolved).unwrap();
    assert_eq!(base.decide(&[1, 1]).unwrap().rule, 1);
    assert_eq!(base.decide(&[1, 0]).unwrap().rule, 0);
    // Without the default rule, a small allowed transfer matches nothing.
    let incomplete = resolved.replace("rule default priority 0 when always then allow\n", "");
    assert_eq!(
        RuleBase::load(&incomplete),
        Err(RuleBaseError::Uncovered { input: vec![0, 0] })
    );
    // A rule that a higher priority always covers, or whose conditions never
    // hold together, decides nothing and is refused.
    for shadowed in [
        "rule shadowed priority 5 when amount_band >= 1 then allow\n",
        "rule shadowed priority 30 when amount_band == 0 and amount_band == 2 then block\n",
    ] {
        assert_eq!(
            RuleBase::load(&format!("{resolved}{shadowed}")),
            Err(RuleBaseError::NeverFires {
                rule: "shadowed".into()
            })
        );
    }
    // The shipped rule base with one more rule at an existing priority: the
    // new rule and `high_risk_counterparty` both match a high-risk transfer of
    // band 2 or more.
    let shipped_plus_one = format!(
        "{RULE_BASE}rule block_high_risk priority 70 when counterparty_risk == high and amount_band >= 2 then block\n"
    );
    match RuleBase::load(&shipped_plus_one) {
        Err(RuleBaseError::Conflict { first, second, .. }) => {
            assert_eq!(
                (first.as_str(), second.as_str()),
                ("high_risk_counterparty", "block_high_risk")
            );
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    // The shipped rule base without its default rule leaves transfers
    // uncovered.
    let shipped_minus_default = RULE_BASE
        .lines()
        .filter(|line| !line.starts_with("rule default_allow"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        RuleBase::load(&shipped_minus_default),
        Err(RuleBaseError::Uncovered {
            input: vec![0, 0, 0, 0, 0]
        })
    );
}

#[test]
fn malformed_rule_bases_are_refused_at_their_line() {
    let header = "feature amount_band 0..2\nfeature region allowed restricted\n";
    for (body, problem) in [
        (
            "rule r priority 1 when amount_bands >= 1 then hold\n",
            "a condition names an undeclared feature",
        ),
        (
            "rule r priority 1 when amount_band >= 3 then hold\n",
            "a condition names a value outside the feature's range",
        ),
        (
            "rule r priority 1 when region < restricted then hold\n",
            "an enumerated feature accepts == and != only",
        ),
        (
            "rule r priority 1 when region == elsewhere then hold\n",
            "a condition names an undeclared value",
        ),
        (
            "rule r priority 1 when always then escalate\n",
            "a verdict is allow, hold, or block",
        ),
        (
            "rule r priority soon when always then allow\n",
            "a priority must be an integer",
        ),
        (
            "rule r priority 1 when amount_band >= 1 region == allowed then hold\n",
            "conditions are joined by `and`",
        ),
        (
            "rule r priority 1 when always then allow\nrule r priority 2 when always then hold\n",
            "a rule is declared twice",
        ),
        (
            "rule r priority 1 when always then allow\nfeature late 0..1\n",
            "features come before rules",
        ),
        ("verdict allow\n", "a line must declare a feature or a rule"),
    ] {
        let text = format!("{header}{body}");
        let line = 2 + body.lines().count();
        assert_eq!(
            RuleBase::parse(&text),
            Err(RuleBaseError::Syntax { line, problem }),
            "{body}"
        );
    }
    assert_eq!(
        RuleBase::parse("feature huge 0..70000\n"),
        Err(RuleBaseError::Syntax {
            line: 1,
            problem: "the features have more than 65536 inputs"
        })
    );
}
