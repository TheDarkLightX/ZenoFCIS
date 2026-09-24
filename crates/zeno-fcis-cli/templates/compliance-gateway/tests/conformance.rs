//! Checks the executed application against the rule base and against
//! decision examples drafted from the README.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the adapter in `src/program.rs`, the law checker, the committed
//! patch, and the outbox plan. Fields are read by the numeric IDs in
//! `project.zeno`, not through the generated name bindings, so a binding that
//! swapped two fields fails here.
//!
//! The reference model evaluates `rules.txt` through `src/rules.rs` for a
//! screening and restates the README's rules for a reinstatement. It never
//! calls the synthesized step: `tests/rule_base.rs` compares the step with the
//! rule base on every input, and `tools/check_synthesis.py` in the library
//! does so again with a separate Python evaluator. The examples file checks
//! the running application against the README.

use compliance_gateway::{
    Authority, authority,
    bindings::GeneratedProject,
    context,
    generated::*,
    laws::trace_step,
    profile,
    rules::{RULE_BASE, RuleBase, Verdict},
};
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_spec::{
    ClaimFormula, EvalLimits, EvalOutcome, EvaluationContext, Identifier, PredicateProvider,
    ProjectionRoot, RelExpr, StableId, TraceStep, evaluate_relational, invariant_at,
};
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
/// The inductive claim in `project.zeno`: the strikes stay within their bounds.
const STRIKES_STAY_IN_BOUNDS: u32 = 600;
const SCREEN: u16 = 150;
const REINSTATE: u16 = 151;
const REGIONS: [u16; 3] = [160, 161, 162];
const RISKS: [u16; 3] = [165, 166, 167];
/// The variant of `RuleId` (type 114) for the first rule of `rules.txt`.
const FIRST_RULE: u16 = 170;
/// The committed-failure reason of the first blocking rule of `rules.txt`.
const FIRST_BLOCK_REASON: u32 = 210;
/// The most strikes an account can carry.
const FROZEN: i128 = 3;

/// One request: the strikes (field 120); the action, region, amount band,
/// and counterparty risk (fields 130 to 133); the identity tier and the
/// reviewer flag (fields 140 and 141).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Input {
    strikes: i128,
    action: u16,
    region: u16,
    band: i128,
    risk: u16,
    tier: i128,
    reviewer: bool,
}

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept`, `reject`, or `failure`.
    kind: &'static str,
    /// The rejection or committed-failure reason.
    reason: Option<u32>,
    /// The strikes after the decision.
    post: i128,
    /// Channel, rule variant (field 145 or 147), and the amount band (146)
    /// or strikes (148) of each queued notice.
    notices: Vec<(u32, u16, i128)>,
}

fn record_field(value: &Value, id: u16) -> &Value {
    let Value::Record(fields) = value else {
        panic!("record expected, got {value:?}");
    };
    fields
        .iter()
        .find(|field| field.id() == id)
        .unwrap_or_else(|| panic!("field {id} missing from {value:?}"))
        .value()
}

fn int(value: &Value, id: u16) -> i128 {
    match record_field(value, id) {
        Value::I128(value) => *value,
        other => panic!("field {id} holds {other:?}"),
    }
}

/// The variant ID of an enum or sum value.
fn variant(value: &Value) -> u16 {
    match value {
        Value::Enum { variant, .. } | Value::Sum { variant, .. } => *variant,
        other => panic!("variant expected, got {other:?}"),
    }
}

fn command(input: Input) -> GatewayCommand {
    GatewayCommand {
        action: match input.action {
            SCREEN => GatewayAction::Screen,
            REINSTATE => GatewayAction::Reinstate,
            other => panic!("unknown action variant {other}"),
        },
        region: match input.region {
            160 => Region::Allowed,
            161 => Region::Restricted,
            162 => Region::Sanctioned,
            other => panic!("unknown region variant {other}"),
        },
        amount_band: AmountBand(input.band),
        counterparty_risk: match input.risk {
            165 => CounterpartyRisk::Low,
            166 => CounterpartyRisk::Medium,
            167 => CounterpartyRisk::High,
            other => panic!("unknown risk variant {other}"),
        },
    }
}

fn standing(strikes: i128) -> Standing {
    Standing {
        strikes: Strikes(strikes),
    }
}

/// Runs one request through the application from an admitted pre-state.
fn observe(authority: &Authority, project: &GeneratedProject, input: Input) -> Outcome {
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&standing(input.strikes), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field at its numeric ID.
    assert_eq!(int(&pre_value, 120), input.strikes);
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(&command(input), limits)
        .unwrap();
    let command_value = admitted_command.admitted().value().value();
    assert_eq!(variant(record_field(command_value, 130)), input.action);
    assert_eq!(variant(record_field(command_value, 131)), input.region);
    assert_eq!(int(command_value, 132), input.band);
    assert_eq!(variant(record_field(command_value, 133)), input.risk);
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(&context(input.tier, input.reviewer), limits)
        .unwrap();
    let context_value = admitted_context.admitted().value().value();
    assert_eq!(int(context_value, 140), input.tier);
    assert_eq!(
        record_field(context_value, 141),
        &Value::Bool(input.reviewer)
    );
    let replay = format!("conformance {input:?}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/compliance-gateway/principal", b"conformance"),
            profile::digest("example/compliance-gateway/authentication", b"conformance"),
            profile::digest("example/compliance-gateway/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/compliance-gateway/state", 1).unwrap();
    macro_rules! committed {
        ($kind:expr, $reason:expr, $candidate:expr) => {{
            let bundle = $candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            Outcome {
                kind: $kind,
                reason: $reason,
                post: int(applied.state(), 120),
                notices: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| {
                        let (rule, extra) = match entry.channel() {
                            300 => (145, 146),
                            301 => (147, 148),
                            other => panic!("unknown channel {other}"),
                        };
                        (
                            entry.channel(),
                            variant(record_field(entry.payload(), rule)),
                            int(entry.payload(), extra),
                        )
                    })
                    .collect(),
            }
        }};
    }
    match authority.execute(witness).unwrap() {
        Decision::Reject(reject) => Outcome {
            kind: "reject",
            reason: Some(reject.reason().rejection().reason_id().get()),
            post: input.strikes,
            notices: Vec::new(),
        },
        Decision::Accept(accepted) => {
            let candidate = accepted.into_candidate();
            committed!("accept", None, candidate)
        }
        Decision::CommittedFailure(failed) => {
            let (candidate, reason) = failed.into_parts();
            committed!("failure", Some(reason.get()), candidate)
        }
    }
}

/// The features of a screening, in the order `rules.txt` declares them.
fn features(input: Input) -> [i64; 5] {
    [
        i64::try_from(input.strikes).unwrap(),
        i64::try_from(input.tier).unwrap(),
        i64::from(input.region - REGIONS[0]),
        i64::try_from(input.band).unwrap(),
        i64::from(input.risk - RISKS[0]),
    ]
}

/// The README's rules: a reinstatement restated here, and a screening decided
/// by the rule base.
fn model(rules: &RuleBase, input: Input) -> Outcome {
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: input.strikes,
        notices: Vec::new(),
    };
    if input.action == REINSTATE {
        if !input.reviewer {
            return reject(200);
        }
        if input.strikes == 0 {
            return reject(201);
        }
        return Outcome {
            kind: "accept",
            reason: None,
            post: 0,
            notices: Vec::new(),
        };
    }
    let fired = rules
        .decide(&features(input))
        .expect("the rule base is total");
    let rule = FIRST_RULE + u16::try_from(fired.rule).unwrap();
    match fired.verdict {
        Verdict::Allow => Outcome {
            kind: "accept",
            reason: None,
            post: input.strikes,
            notices: Vec::new(),
        },
        Verdict::Hold => Outcome {
            kind: "accept",
            reason: None,
            post: input.strikes,
            notices: vec![(300, rule, input.band)],
        },
        Verdict::Block => {
            let post = (input.strikes + 1).min(FROZEN);
            let ordinal = rules.block_ordinal(fired.rule).unwrap();
            Outcome {
                kind: "failure",
                reason: Some(FIRST_BLOCK_REASON + u32::try_from(ordinal).unwrap()),
                post,
                notices: vec![(301, rule, post)],
            }
        }
    }
}

fn every_input() -> Vec<Input> {
    let mut inputs = Vec::new();
    for strikes in 0..=FROZEN {
        for action in [SCREEN, REINSTATE] {
            for region in REGIONS {
                for band in 0..=4 {
                    for risk in RISKS {
                        for tier in 0..=3 {
                            for reviewer in [false, true] {
                                inputs.push(Input {
                                    strikes,
                                    action,
                                    region,
                                    band,
                                    risk,
                                    tier,
                                    reviewer,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    inputs
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

/// Whether a formula of `project.zeno` is true of one decision's observations.
fn holds(formula: &RelExpr, step: &TraceStep) -> bool {
    matches!(
        evaluate_relational(
            formula,
            EvaluationContext::new(step, &NoPredicates, EvalLimits::default())
        ),
        EvalOutcome::True
    )
}

/// The invariant of claim 600, over the state before and after a decision.
fn invariant(spec: &zeno_fcis_spec::ProjectSpec) -> (RelExpr, RelExpr) {
    let claim = spec
        .claim(StableId::new(STRIKES_STAY_IN_BOUNDS).unwrap())
        .expect("claim 600 is declared");
    let ClaimFormula::Relational(before) = claim.formula() else {
        panic!("claim 600 states a relational invariant");
    };
    let after = invariant_at(before, ProjectionRoot::Post).expect("an invariant over pre. paths");
    (before.clone(), after)
}

#[test]
fn every_admitted_input_matches_the_rule_base_and_keeps_the_invariant() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let spec = profile::project();
    let rules = RuleBase::load(RULE_BASE).unwrap();
    let inputs = every_input();
    assert_eq!(inputs.len(), 2880);
    let mut kinds = BTreeSet::new();
    let (before, after) = invariant(&spec);
    let mut exercised: BTreeMap<&str, usize> = BTreeMap::new();
    for input in inputs {
        let observed = observe(&authority, &project, input);
        assert_eq!(observed, model(&rules, input), "input {input:?}");
        kinds.insert((observed.kind, observed.reason));
        if observed.kind == "reject" {
            continue;
        }
        // Claim 600's step is attested for every integer; here the invariant
        // is evaluated on each committed decision's actual transition, as the
        // law checker observes it, before and after.
        let step = trace_step(
            &standing(input.strikes),
            &standing(observed.post),
            &command(input),
            &context(input.tier, input.reviewer),
        )
        .unwrap();
        assert!(holds(&before, &step), "invariant before {input:?}");
        assert!(holds(&after, &step), "invariant after {input:?}");
        *exercised.entry(observed.kind).or_insert(0) += 1;
    }
    // Every reason and every verdict is reached from some admitted input.
    assert_eq!(
        kinds,
        BTreeSet::from([
            ("accept", None),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("failure", Some(210)),
            ("failure", Some(211)),
            ("failure", Some(212)),
            ("failure", Some(213)),
            ("failure", Some(214)),
        ])
    );
    // Both kinds of committing decision exercise the invariant.
    assert_eq!(
        exercised.keys().copied().collect::<Vec<_>>(),
        ["accept", "failure"]
    );
}

#[test]
fn schema_admission_matches_the_finite_domain() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for strikes in -1..=4 {
        let admitted = project
            .admit_root::<RustCryptoSha256>(&standing(strikes), limits)
            .is_ok();
        assert_eq!(
            admitted,
            (0..=FROZEN).contains(&strikes),
            "strikes {strikes}"
        );
    }
    let input = |band, tier| Input {
        strikes: 0,
        action: SCREEN,
        region: 160,
        band,
        risk: 165,
        tier,
        reviewer: false,
    };
    for band in -1..=5 {
        let admitted = project
            .admit_command::<RustCryptoSha256>(&command(input(band, 0)), limits)
            .is_ok();
        assert_eq!(admitted, (0..=4).contains(&band), "band {band}");
    }
    for tier in -1..=4 {
        let admitted = project
            .admit_context::<RustCryptoSha256>(&context(tier, false), limits)
            .is_ok();
        assert_eq!(admitted, (0..=3).contains(&tier), "tier {tier}");
    }
}

#[test]
fn genesis_has_no_strikes_and_reaches_every_standing() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let other = project
        .admit_root::<RustCryptoSha256>(&standing(1), limits)
        .unwrap();
    assert!(authority.authorize_genesis(other).is_err());
    let mut seen = BTreeSet::from([0]);
    let mut pending = vec![0];
    while let Some(strikes) = pending.pop() {
        let mut inputs = vec![Input {
            strikes,
            action: REINSTATE,
            region: 160,
            band: 0,
            risk: 165,
            tier: 3,
            reviewer: true,
        }];
        for region in REGIONS {
            for band in 0..=4 {
                for risk in RISKS {
                    for tier in 0..=3 {
                        inputs.push(Input {
                            strikes,
                            action: SCREEN,
                            region,
                            band,
                            risk,
                            tier,
                            reviewer: false,
                        });
                    }
                }
            }
        }
        for input in inputs {
            let outcome = observe(&authority, &project, input);
            if outcome.kind != "reject" && seen.insert(outcome.post) {
                pending.push(outcome.post);
            }
        }
    }
    assert_eq!(seen, BTreeSet::from([0, 1, 2, 3]));
}

/// Converts `sanctioned_region` to `SanctionedRegion`.
fn camel_case(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

#[test]
fn rule_declarations_match_the_rule_base() {
    let rules = RuleBase::load(RULE_BASE).unwrap();
    let project = profile::project();
    // One `RuleId` variant per rule, in the order of the file.
    let variants: Vec<(u32, &str)> = project
        .variants()
        .iter()
        .filter(|variant| variant.owner().get() == 114)
        .map(|variant| (variant.id().get(), variant.name().as_str()))
        .collect();
    let expected: Vec<(u32, String)> = rules
        .rules()
        .iter()
        .enumerate()
        .map(|(index, rule)| {
            (
                u32::from(FIRST_RULE) + u32::try_from(index).unwrap(),
                camel_case(rule.name()),
            )
        })
        .collect();
    assert_eq!(
        variants,
        expected
            .iter()
            .map(|(id, name)| (*id, name.as_str()))
            .collect::<Vec<_>>()
    );
    // One committed-failure reason per blocking rule, in the same order.
    let reasons: Vec<(u32, &str)> = project
        .reasons()
        .iter()
        .filter(|reason| reason.id().get() >= FIRST_BLOCK_REASON)
        .map(|reason| (reason.id().get(), reason.name().as_str()))
        .collect();
    let blocking: Vec<(u32, &str)> = rules
        .rules()
        .iter()
        .filter(|rule| rule.verdict() == Verdict::Block)
        .enumerate()
        .map(|(ordinal, rule)| {
            (
                FIRST_BLOCK_REASON + u32::try_from(ordinal).unwrap(),
                rule.name(),
            )
        })
        .collect();
    assert_eq!(reasons, blocking);
    // The enumerated features list their values in the order of the variants.
    let names = |owner: u32| -> Vec<String> {
        project
            .variants()
            .iter()
            .filter(|variant| variant.owner().get() == owner)
            .map(|variant| variant.name().as_str().to_ascii_lowercase())
            .collect()
    };
    assert_eq!(rules.features()[2].values(), names(112));
    assert_eq!(rules.features()[4].values(), names(113));
}

/// Parses `pre.120 action region band risk tier reviewer | outcome reason
/// post.120 | notice`, where the notice is `-` or `channel rule extra`.
fn parse_example(line: &str) -> (Input, Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [input, decision, notice] = parts.as_slice() else {
        panic!("three columns expected: {line}");
    };
    let input: Vec<i128> = input
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let [strikes, action, region, band, risk, tier, reviewer] = input.as_slice() else {
        panic!("seven inputs expected: {line}");
    };
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, post] = decision.as_slice() else {
        panic!("three decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        "failure" => "failure",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let notices = if *notice == "-" {
        Vec::new()
    } else {
        let notice: Vec<i128> = notice
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        let [channel, rule, extra] = notice.as_slice() else {
            panic!("three notice fields expected: {line}");
        };
        vec![(
            u32::try_from(*channel).unwrap(),
            u16::try_from(*rule).unwrap(),
            *extra,
        )]
    };
    let reviewer = match reviewer {
        0 => false,
        1 => true,
        other => panic!("reviewer must be 0 or 1, got {other}"),
    };
    (
        Input {
            strikes: *strikes,
            action: u16::try_from(*action).unwrap(),
            region: u16::try_from(*region).unwrap(),
            band: *band,
            risk: u16::try_from(*risk).unwrap(),
            tier: *tier,
            reviewer,
        },
        Outcome {
            kind,
            reason,
            post: post.parse().unwrap(),
            notices,
        },
    )
}

#[test]
fn decision_examples_match_the_executed_application_and_reach_every_rule() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let rules = RuleBase::load(RULE_BASE).unwrap();
    let mut covered = BTreeSet::new();
    let mut fired = BTreeSet::new();
    let mut count = 0;
    for line in EXAMPLES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (input, expected) = parse_example(line);
        assert_eq!(
            observe(&authority, &project, input),
            expected,
            "example: {line}"
        );
        covered.insert((expected.kind, expected.reason));
        if input.action == SCREEN {
            fired.insert(rules.decide(&features(input)).unwrap().rule);
        }
        count += 1;
    }
    assert_eq!(count, 24);
    // Every rule of the rule base decides at least one example.
    assert_eq!(fired, (0..rules.rules().len()).collect());
    assert_eq!(
        covered,
        BTreeSet::from([
            ("accept", None),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("failure", Some(210)),
            ("failure", Some(211)),
            ("failure", Some(212)),
            ("failure", Some(213)),
            ("failure", Some(214)),
        ])
    );
}
