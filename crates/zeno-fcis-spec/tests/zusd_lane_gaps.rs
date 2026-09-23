//! Characterizes what `.zeno` version 1 can and cannot state about the
//! single-vault zUSD lane.
//!
//! Each test pins one finding in `docs/ZUSD_LANE_EXPRESSIVENESS.md`. A change
//! that closes a gap fails its test, so the record must be updated with it.

use zeno_fcis_spec::{
    DiagnosticCode, EvalLimits, EvalOutcome, EvaluationContext, Identifier, IndeterminateReason,
    Observation, PredicateProvider, ProjectLimits, ProjectSpec, ProjectionPath, ProjectionRoot,
    SourceLimits, StableId, TraceStep, elaborate_project, evaluate_relational, parse_project,
};

const LANE: &str = include_str!("data/zusd_lane_v1.zeno");
const E8: i128 = 100_000_000;
const STATE_FIELDS: usize = 32;
const DEPOSIT_COLLATERAL: i128 = 5;
// Native state field positions used below.
const PRICE: usize = 3;
const PRICE_PENDING: usize = 4;
const COLLATERAL: usize = 6;

struct NoPredicates;

impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

fn elaborate(source: &str) -> ProjectSpec {
    let parsed = parse_project(source, SourceLimits::default())
        .unwrap_or_else(|diagnostics| panic!("{diagnostics}"));
    elaborate_project(parsed, ProjectLimits::default())
        .unwrap_or_else(|diagnostics| panic!("{diagnostics}"))
}

fn id(value: u32) -> StableId {
    StableId::new(value).unwrap_or_else(|| panic!("stable id {value}"))
}

fn path(root: ProjectionRoot, ids: &[u32]) -> ProjectionPath {
    ProjectionPath::try_new(root, ids.iter().map(|value| id(*value)).collect())
        .unwrap_or_else(|| panic!("projection path {ids:?}"))
}

/// A bootstrapped vault with 10 units of collateral, 200 units of debt, and a
/// price of 60,000, at the native default parameters.
fn vault() -> [i128; STATE_FIELDS] {
    let mut state = [0; STATE_FIELDS];
    state[0] = 10; // now_epoch
    state[1] = 1; // oracle_seen
    state[2] = 10; // oracle_last_update_epoch
    state[PRICE] = 60_000 * E8;
    state[PRICE_PENDING] = 60_000 * E8;
    state[5] = 100; // max_oracle_staleness_epochs
    state[COLLATERAL] = 10 * E8;
    state[7] = 200 * E8; // debt_e8
    state[8] = 200 * E8; // free_debt_e8
    state[14] = 11_000; // mcr_bps
    state[15] = 15_000; // ccr_bps
    state[16] = 100 * E8; // min_debt_open_e8
    state[17] = 10_000_000 * E8; // max_debt_e8
    state[18] = 20_000_000 * E8; // max_debt_supply_e8
    state[19] = 20_000_000 * E8; // max_sp_coll_e8
    state[20] = 20_000_000 * E8; // max_protocol_coll_e8
    state[27] = 1_000; // borrow_fee_max_bps
    state[29] = 1_000; // redemption_fee_max_bps
    state
}

fn step(pre: &[i128; STATE_FIELDS], post: &[i128; STATE_FIELDS], amount: i128) -> TraceStep {
    let mut observations = Vec::new();
    for (index, (before, after)) in pre.iter().zip(post).enumerate() {
        let field = 110 + u32::try_from(index).unwrap_or_else(|_| unreachable!());
        observations.push(Observation::new(
            path(ProjectionRoot::Pre, &[100, field]),
            *before,
        ));
        observations.push(Observation::new(
            path(ProjectionRoot::Post, &[100, field]),
            *after,
        ));
    }
    for (field, value) in [(150, DEPOSIT_COLLATERAL), (151, amount), (152, 0)] {
        observations.push(Observation::new(
            path(ProjectionRoot::Command, &[101, field]),
            value,
        ));
    }
    TraceStep::try_new(observations).unwrap_or_else(|| panic!("duplicate observation"))
}

fn law(spec: &ProjectSpec, law_id: u32, trace: &TraceStep) -> EvalOutcome {
    let law = spec
        .laws()
        .iter()
        .find(|law| law.id() == id(law_id))
        .unwrap_or_else(|| panic!("law {law_id}"));
    let limits = EvalLimits::try_new(1_000_000, 10_000, 16).unwrap_or_else(|| unreachable!());
    evaluate_relational(
        law.formula(),
        EvaluationContext::new(trace, &NoPredicates, limits),
    )
}

/// Replaces the single law of the attempt with `formula` and parses it.
fn with_law(formula: &str) -> String {
    let declarations: Vec<&str> = LANE
        .lines()
        .filter(|line| !line.starts_with("law "))
        .collect();
    format!("{}\nlaw 900 probe = {formula};\n", declarations.join("\n"))
}

fn parse_codes(source: &str) -> Vec<DiagnosticCode> {
    match parse_project(source, SourceLimits::default()) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .diagnostics()
            .iter()
            .map(|item| item.code())
            .collect(),
    }
}

#[test]
fn zusd_lane_declares_every_state_field_and_reason_in_registry_order() {
    let spec = elaborate(LANE);
    let state_fields = spec
        .fields()
        .iter()
        .filter(|field| field.owner() == id(100))
        .count();
    assert_eq!(state_fields, STATE_FIELDS);
    assert_eq!(spec.reasons().len(), 46);
    for (rank, reason) in spec.reasons().iter().enumerate() {
        assert_eq!(
            u32::try_from(rank).ok(),
            Some(reason.precedence()),
            "{}",
            reason.name().as_str()
        );
    }
    assert_eq!(spec.laws().len(), 9);
}

#[test]
fn zusd_lane_unguarded_effect_law_admits_both_outcomes() {
    let spec = elaborate(LANE);
    let pre = vault();
    let mut accepted = pre;
    accepted[COLLATERAL] += E8;
    let mut over_credited = accepted;
    over_credited[COLLATERAL] += 1;

    // Law 510 states "the effect happened, or nothing changed" without the
    // acceptance conditions, so it admits both outcomes. That is a weak
    // example law, not a language limit: law 513 below states the conditions.
    // It still rejects a wrong effect.
    assert_eq!(
        law(&spec, 510, &step(&pre, &accepted, E8)),
        EvalOutcome::True
    );
    assert_eq!(law(&spec, 510, &step(&pre, &pre, E8)), EvalOutcome::True);
    assert_eq!(
        law(&spec, 510, &step(&pre, &over_credited, E8)),
        EvalOutcome::False
    );
}

#[test]
fn zusd_lane_solvency_law_overflows_inside_the_declared_domain() {
    let spec = elaborate(LANE);
    let realistic = vault();
    assert_eq!(
        law(&spec, 504, &step(&realistic, &realistic, E8)),
        EvalOutcome::True
    );

    // Every amount may reach 10^30. Collateral and price of 10^20 already
    // overflow the evaluator's checked i128 products.
    let mut large = realistic;
    large[PRICE] = 10_i128.pow(20);
    large[PRICE_PENDING] = 10_i128.pow(20);
    large[COLLATERAL] = 10_i128.pow(20);
    assert_eq!(
        law(&spec, 500, &step(&large, &large, E8)),
        EvalOutcome::True
    );
    assert_eq!(
        law(&spec, 504, &step(&large, &large, E8)),
        EvalOutcome::Indeterminate(IndeterminateReason::Overflow)
    );
}

#[test]
fn zeno_v1_accepts_formula_paths_that_name_no_declared_field() {
    let spec = elaborate(&with_law(
        "post.100.999 == 0 && command.101.777 == 3 && post.555.1.2.3 == 1",
    ));
    assert_eq!(spec.laws().len(), 1);
}

#[test]
fn zeno_v1_integer_literals_stop_at_the_u64_range() {
    assert!(parse_codes(&with_law("post.100.110 <= 18446744073709551615")).is_empty());
    assert!(
        parse_codes(&with_law("post.100.110 <= 18446744073709551616"))
            .contains(&DiagnosticCode::InvalidNumber)
    );
    // 10^30 is written as a product instead.
    assert!(
        parse_codes(&with_law(
            "post.100.110 <= 1000000000000000 * 1000000000000000"
        ))
        .is_empty()
    );
}

#[test]
fn zeno_v1_comparisons_cannot_start_with_a_parenthesized_scalar() {
    assert!(!parse_codes(&with_law("(post.100.110 + 1) * 2 == 4")).is_empty());
    assert!(parse_codes(&with_law("2 * (post.100.110 + 1) == 4")).is_empty());
}

/// Native outcomes of `deposit_collateral` from the pinned ZenoDEX Python
/// authority (commit e3cf1aad40e487893230ae0c55f1f0dda62f9955, `_step_python`).
struct NativeDeposit {
    case: &'static str,
    pre: [i128; STATE_FIELDS],
    amount: i128,
    accepted: bool,
}

fn native_deposits() -> Vec<NativeDeposit> {
    let mut bad_debt = vault();
    bad_debt[PRICE] = 1;
    bad_debt[PRICE_PENDING] = 1;
    let mut full = bad_debt;
    full[COLLATERAL] = 10_i128.pow(30);
    vec![
        // Accepted: collateral 10e8 becomes 11e8, and nothing else changes.
        NativeDeposit {
            case: "normal deposit",
            pre: vault(),
            amount: E8,
            accepted: true,
        },
        // "amount_e8 must be a positive int".
        NativeDeposit {
            case: "zero amount",
            pre: vault(),
            amount: 0,
            accepted: false,
        },
        // "invariant violation: inv_system_no_bad_debt".
        NativeDeposit {
            case: "bad debt after deposit",
            pre: bad_debt,
            amount: E8,
            accepted: false,
        },
        // "collateral_e8 exceeds MAX_AMOUNT_E8".
        NativeDeposit {
            case: "beyond the bound",
            pre: full,
            amount: 1,
            accepted: false,
        },
    ]
}

#[test]
fn zusd_lane_guarded_deposit_law_matches_native_outcomes() {
    // Law 513 restates the native acceptance conditions for a deposit: the
    // pre-state shape, a positive amount, the stored-amount bound, and the
    // post-state invariants. It requires the effect when they hold, and an
    // unchanged state otherwise, so it tells the two outcomes apart without
    // observing the decision.
    let spec = elaborate(LANE);
    for deposit in native_deposits() {
        // A rejected zero deposit would credit nothing, so its wrong outcome
        // credits one unit instead.
        let mut credited = deposit.pre;
        credited[COLLATERAL] += deposit.amount.max(1);
        let (native, other) = if deposit.accepted {
            (credited, deposit.pre)
        } else {
            (deposit.pre, credited)
        };
        let check = |post| law(&spec, 513, &step(&deposit.pre, post, deposit.amount));
        assert_eq!(
            check(&native),
            EvalOutcome::True,
            "{} native outcome",
            deposit.case
        );
        assert_eq!(
            check(&other),
            EvalOutcome::False,
            "{} other outcome",
            deposit.case
        );
    }
    // At a realistic price, the same guard overflows near the stored-amount
    // bound: strict connectives evaluate every conjunct (finding 2).
    let mut full = vault();
    full[COLLATERAL] = 10_i128.pow(30);
    assert_eq!(
        law(&spec, 513, &step(&full, &full, 1)),
        EvalOutcome::Indeterminate(IndeterminateReason::Overflow)
    );
}

#[test]
fn reviewer_guarded_law_distinguishes_required_deposit_from_unchanged_state() {
    // Counterexample from the independent review of 521b768, kept in its
    // form: guarding on one exact pre-state already separates the accepted
    // deposit from an unchanged state.
    let pre = vault();
    let mut accepted = pre;
    accepted[COLLATERAL] += E8;
    let mut guards: Vec<String> = pre
        .iter()
        .enumerate()
        .map(|(index, value)| format!("pre.100.{} == {value}", 110 + index))
        .collect();
    guards.push(format!("command.101.150 == {DEPOSIT_COLLATERAL}"));
    guards.push(format!("command.101.151 == {E8}"));
    let formula = format!(
        "{} -> post.100.116 == pre.100.116 + command.101.151",
        guards.join(" && ")
    );
    let spec = elaborate(&with_law(&formula));
    assert_eq!(
        law(&spec, 900, &step(&pre, &accepted, E8)),
        EvalOutcome::True
    );
    assert_eq!(law(&spec, 900, &step(&pre, &pre, E8)), EvalOutcome::False);
}
