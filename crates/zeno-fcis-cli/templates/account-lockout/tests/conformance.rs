//! Checks the executed application against the decision examples and against
//! a reference model of the README's rules on a grid of inputs.
//!
//! Every decision runs through the application: schema admission, the
//! authority, the hand-written program in `src/program.rs`, the law checker,
//! the committed patch, and the outbox plan. Fields are read by the numeric IDs
//! in `project.zeno`, not through the generated name bindings, so a binding
//! that swapped two fields fails here.
//!
//! The reference model restates the README's rules. Its author also wrote the
//! program, so it catches binding and adapter errors, not a misreading shared
//! by both. The examples file, once reviewed by the project's owner, is the
//! check on that.
//!
//! On every committed decision, the invariant of claim 600 is evaluated
//! before and after, through the law checker's own observer.

use account_lockout::{
    Authority, authority, bindings::GeneratedProject, context, generated::*,
    laws::state_observations, profile,
};
use std::collections::BTreeSet;
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
const LOGIN_SUCCEEDED: u16 = 120;
const LOGIN_FAILED: u16 = 121;
const ADMIN_UNLOCK: u16 = 122;
const LOCKED: u16 = 150;
const UNLOCKED: u16 = 151;
/// The inductive claim in `project.zeno`: law 500 over the account before a
/// decision.
const LOCK_STATE_STAYS_CONSISTENT: u32 = 600;
/// The latest time a request may carry, as `project.zeno` declares it.
const LAST_TIME: i128 = 4_102_444_800;
/// The grid's decisions that commit, on each of which the invariant is
/// evaluated before and after.
const COMMITTED_GRID_DECISIONS: usize = 295;

/// Fields 110 failed_attempts, 111 locked_until, and 112 last_seen.
type State = (i128, i128, i128);

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept`, `reject`, or `failure`.
    kind: &'static str,
    /// The rejection or committed-failure reason.
    reason: Option<u32>,
    /// The state after the decision.
    post: State,
    /// Channel, alert kind (field 140), and deadline (field 141) of each alert.
    alerts: Vec<(u32, u16, i128)>,
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

fn command(id: u16) -> AccountCommand {
    match id {
        LOGIN_SUCCEEDED => AccountCommand::LoginSucceeded,
        LOGIN_FAILED => AccountCommand::LoginFailed,
        ADMIN_UNLOCK => AccountCommand::AdminUnlock,
        other => panic!("unknown command variant {other}"),
    }
}

fn state((failed, until, seen): State) -> Account {
    Account {
        failed_attempts: Attempts(failed),
        locked_until: LockDeadline(until),
        last_seen: UnixTime(seen),
    }
}

/// Runs one request through the application from an admitted pre-state.
fn observe(authority: &Authority, pre: State, command_id: u16, now: i128, admin: bool) -> Outcome {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&state(pre), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each field at its numeric ID.
    assert_eq!(
        (
            int(&pre_value, 110),
            int(&pre_value, 111),
            int(&pre_value, 112)
        ),
        pre
    );
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(&command(command_id), limits)
        .unwrap();
    assert_eq!(
        variant(admitted_command.admitted().value().value()),
        command_id
    );
    let admitted_context = project
        .admit_context::<RustCryptoSha256>(&context(now, admin), limits)
        .unwrap();
    let context_value = admitted_context.admitted().value().value();
    assert_eq!(int(context_value, 130), now);
    assert_eq!(record_field(context_value, 131), &Value::Bool(admin));
    let replay = format!("conformance {pre:?} {command_id} {now} {admin}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            admitted_context.admitted().clone(),
            profile::digest("example/account-lockout/principal", b"conformance"),
            profile::digest("example/account-lockout/authentication", b"conformance"),
            profile::digest("example/account-lockout/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/account-lockout/state", 1).unwrap();
    macro_rules! committed {
        ($kind:expr, $reason:expr, $candidate:expr) => {{
            let bundle = $candidate.bundle();
            assert!(bundle.commit_plan().effects().is_empty());
            let applied = bundle
                .patch()
                .apply::<RustCryptoSha256>(&pre_value, domain)
                .unwrap();
            let post = applied.state();
            Outcome {
                kind: $kind,
                reason: $reason,
                post: (int(post, 110), int(post, 111), int(post, 112)),
                alerts: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| {
                        (
                            entry.channel(),
                            variant(record_field(entry.payload(), 140)),
                            int(entry.payload(), 141),
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
            post: pre,
            alerts: Vec::new(),
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

/// The README's rules, restated independently of `src/program.rs`.
fn model(pre: State, command_id: u16, now: i128, admin: bool) -> Outcome {
    let (failed, until, seen) = pre;
    let reject = |reason| Outcome {
        kind: "reject",
        reason: Some(reason),
        post: pre,
        alerts: Vec::new(),
    };
    if now < seen {
        return reject(200);
    }
    match command_id {
        LOGIN_SUCCEEDED | LOGIN_FAILED if now < until => reject(201),
        LOGIN_SUCCEEDED => Outcome {
            kind: "accept",
            reason: None,
            post: (0, until, now),
            alerts: Vec::new(),
        },
        LOGIN_FAILED if failed == 2 => Outcome {
            kind: "failure",
            reason: Some(203),
            post: (0, now + 900, now),
            alerts: vec![(300, LOCKED, now + 900)],
        },
        LOGIN_FAILED => Outcome {
            kind: "failure",
            reason: Some(203),
            post: (failed + 1, until, now),
            alerts: Vec::new(),
        },
        ADMIN_UNLOCK if !admin => reject(202),
        ADMIN_UNLOCK => Outcome {
            kind: "accept",
            reason: None,
            post: (0, 0, now),
            alerts: vec![(300, UNLOCKED, 0)],
        },
        other => panic!("unknown command variant {other}"),
    }
}

struct NoPredicates;
impl PredicateProvider for NoPredicates {
    fn evaluate(&self, _: &Identifier, _: &[i128]) -> Option<bool> {
        None
    }
}

/// The invariant of claim 600, over the account before and after a decision.
/// Its induction step is attested over the declared ranges; the tests
/// evaluate it on each committed decision's actual transition, as the law
/// checker observes it.
struct Invariant {
    before: RelExpr,
    after: RelExpr,
}

impl Invariant {
    fn load() -> Self {
        let spec = profile::project();
        let claim = spec
            .claim(StableId::new(LOCK_STATE_STAYS_CONSISTENT).unwrap())
            .expect("claim 600 is declared");
        let ClaimFormula::Relational(before) = claim.formula() else {
            panic!("claim 600 states a relational invariant");
        };
        let after =
            invariant_at(before, ProjectionRoot::Post).expect("an invariant over pre. paths");
        Self {
            before: before.clone(),
            after,
        }
    }

    /// Checks the invariant before and after a decision that committed, and
    /// returns whether it did.
    fn holds_across(&self, pre: State, outcome: &Outcome) -> bool {
        if outcome.kind == "reject" {
            return false;
        }
        let mut observations = state_observations(ProjectionRoot::Pre, &state(pre)).unwrap();
        observations
            .extend(state_observations(ProjectionRoot::Post, &state(outcome.post)).unwrap());
        let step = TraceStep::try_new(observations).unwrap();
        for (when, formula) in [("before", &self.before), ("after", &self.after)] {
            assert_eq!(
                evaluate_relational(
                    formula,
                    EvaluationContext::new(&step, &NoPredicates, EvalLimits::default())
                ),
                EvalOutcome::True,
                "claim 600 {when} {pre:?} {outcome:?}"
            );
        }
        true
    }
}

/// Reachable states (law 500 holds), with request times just around each
/// deadline, every command, and both administrator flags.
fn grid() -> Vec<(State, u16, i128, bool)> {
    let mut states = BTreeSet::new();
    for seen in [0, 1000, LAST_TIME - 900] {
        for until in [0, seen, seen + 1, seen + 450, seen + 900] {
            for failed in 0..=2 {
                if until <= seen || failed == 0 {
                    states.insert((failed, until, seen));
                }
            }
        }
    }
    let mut inputs = Vec::new();
    for (failed, until, seen) in states {
        let times: BTreeSet<i128> = [seen - 1, seen, until - 1, until, until + 1, seen + 5000]
            .into_iter()
            .filter(|time| (0..=LAST_TIME).contains(time))
            .collect();
        for now in times {
            for command_id in [LOGIN_SUCCEEDED, LOGIN_FAILED, ADMIN_UNLOCK] {
                for admin in [false, true] {
                    inputs.push(((failed, until, seen), command_id, now, admin));
                }
            }
        }
    }
    inputs
}

#[test]
fn every_grid_input_matches_the_reference_model() {
    let authority = authority().unwrap();
    let invariant = Invariant::load();
    let inputs = grid();
    assert_eq!(inputs.len(), 606);
    let mut committed = 0;
    for (pre, command_id, now, admin) in inputs {
        let observed = observe(&authority, pre, command_id, now, admin);
        assert_eq!(
            observed,
            model(pre, command_id, now, admin),
            "input {pre:?} command {command_id} now {now} admin {admin}"
        );
        committed += usize::from(invariant.holds_across(pre, &observed));
    }
    assert_eq!(committed, COMMITTED_GRID_DECISIONS);
}

/// Parses `pre.110 pre.111 pre.112 command now admin | outcome reason
/// post.110 post.111 post.112 | alert`, where the alert is `-` or
/// `channel kind deadline`.
fn parse_example(line: &str) -> ((State, u16, i128, bool), Outcome) {
    let parts: Vec<&str> = line.split('|').map(str::trim).collect();
    let [input, decision, alert] = parts.as_slice() else {
        panic!("three columns expected: {line}");
    };
    let input: Vec<i128> = input
        .split_whitespace()
        .map(|word| word.parse().unwrap())
        .collect();
    let [failed, until, seen, command_id, now, admin] = input.as_slice() else {
        panic!("six inputs expected: {line}");
    };
    let decision: Vec<&str> = decision.split_whitespace().collect();
    let [kind, reason, p110, p111, p112] = decision.as_slice() else {
        panic!("five decision fields expected: {line}");
    };
    let kind = match *kind {
        "accept" => "accept",
        "reject" => "reject",
        "failure" => "failure",
        other => panic!("unknown outcome {other}"),
    };
    let reason = (*reason != "-").then(|| reason.parse().unwrap());
    let alerts = if *alert == "-" {
        Vec::new()
    } else {
        let alert: Vec<i128> = alert
            .split_whitespace()
            .map(|word| word.parse().unwrap())
            .collect();
        let [channel, alert_kind, deadline] = alert.as_slice() else {
            panic!("three alert fields expected: {line}");
        };
        vec![(
            u32::try_from(*channel).unwrap(),
            u16::try_from(*alert_kind).unwrap(),
            *deadline,
        )]
    };
    let admin = match admin {
        0 => false,
        1 => true,
        other => panic!("admin must be 0 or 1, got {other}"),
    };
    (
        (
            (*failed, *until, *seen),
            u16::try_from(*command_id).unwrap(),
            *now,
            admin,
        ),
        Outcome {
            kind,
            reason,
            post: (
                p110.parse().unwrap(),
                p111.parse().unwrap(),
                p112.parse().unwrap(),
            ),
            alerts,
        },
    )
}

#[test]
fn decision_examples_match_the_executed_application() {
    let authority = authority().unwrap();
    let invariant = Invariant::load();
    let mut covered = BTreeSet::new();
    let (mut count, mut committed) = (0, 0);
    for line in EXAMPLES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let ((pre, command_id, now, admin), expected) = parse_example(line);
        let observed = observe(&authority, pre, command_id, now, admin);
        assert_eq!(observed, expected, "example: {line}");
        covered.insert((expected.kind, expected.reason));
        count += 1;
        committed += usize::from(invariant.holds_across(pre, &observed));
    }
    assert_eq!(count, 20);
    assert_eq!(committed, 12);
    assert_eq!(
        covered,
        BTreeSet::from([
            ("accept", None),
            ("failure", Some(203)),
            ("reject", Some(200)),
            ("reject", Some(201)),
            ("reject", Some(202)),
        ])
    );
}

#[test]
fn schema_admission_enforces_the_declared_bounds() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for now in [0, LAST_TIME] {
        assert!(
            project
                .admit_context::<RustCryptoSha256>(&context(now, false), limits)
                .is_ok()
        );
    }
    for now in [-1, LAST_TIME + 1] {
        assert!(
            project
                .admit_context::<RustCryptoSha256>(&context(now, false), limits)
                .is_err(),
            "time {now} must be refused before any decision"
        );
    }
    assert!(
        project
            .admit_root::<RustCryptoSha256>(&state((2, LAST_TIME + 900, LAST_TIME)), limits)
            .is_ok()
    );
    for bad in [(3, 0, 0), (-1, 0, 0), (0, LAST_TIME + 901, 0), (0, 0, -1)] {
        assert!(
            project
                .admit_root::<RustCryptoSha256>(&state(bad), limits)
                .is_err(),
            "state {bad:?} must be refused"
        );
    }
}

#[test]
fn only_the_zero_account_is_an_accepted_genesis() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    for other in [(1, 0, 0), (0, 900, 0), (0, 0, 1)] {
        let root = project
            .admit_root::<RustCryptoSha256>(&state(other), limits)
            .unwrap();
        assert!(authority.authorize_genesis(root).is_err(), "{other:?}");
    }
    let root = project
        .admit_root::<RustCryptoSha256>(&state((0, 0, 0)), limits)
        .unwrap();
    assert!(authority.authorize_genesis(root).is_ok());
}
