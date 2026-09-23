//! Checks the executed application against its finite model and against
//! independently written decision examples.
//!
//! Every decision is observed through the running application: schema
//! admission, the authority, the reviewed adapter in `src/program.rs`, the law
//! checker, the committed patch, and the outbox plan. Fields are read by the
//! numeric IDs in `project.zeno`, not through the generated name bindings.

use durable_counter::{Authority, authority, bindings::GeneratedProject, generated::*, profile};
use std::collections::BTreeSet;
use zeno_fcis_codec::Domain;
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;
use zeno_fcis_value::Value;

const EXAMPLES: &str = include_str!("decision-examples.txt");
const INCREMENT: u16 = 120;
const RECORD_FAILURE: u16 = 121;

/// One decision, in the numeric IDs of `project.zeno`.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Outcome {
    /// `accept`, `reject`, or `failure`.
    kind: &'static str,
    /// The rejection or committed-failure reason.
    reason: Option<u32>,
    /// Fields 110 and 111 after the decision.
    post: (i128, i128),
    /// Channel, then payload fields 112 and 113, for each queued entry.
    notifications: Vec<(u32, i128, i128)>,
}

fn field(value: &Value, id: u16) -> i128 {
    let Value::Record(fields) = value else {
        panic!("record expected, got {value:?}");
    };
    let found = fields
        .iter()
        .find(|field| field.id() == id)
        .unwrap_or_else(|| panic!("field {id} missing from {value:?}"));
    match found.value() {
        Value::I128(value) => *value,
        other => panic!("field {id} holds {other:?}"),
    }
}

/// The variant ordinal of an enum or sum value.
fn variant(value: &Value) -> u16 {
    match value {
        Value::Enum { variant, .. } | Value::Sum { variant, .. } => *variant,
        other => panic!("variant expected, got {other:?}"),
    }
}

fn command(id: u16) -> CounterCommand {
    match id {
        INCREMENT => CounterCommand::Increment,
        RECORD_FAILURE => CounterCommand::RecordFailure,
        other => panic!("unknown command variant {other}"),
    }
}

fn state(count: i128, failures: i128) -> CounterState {
    CounterState {
        count: CounterValue(count),
        failures: CounterValue(failures),
    }
}

/// Runs one invocation through the application from an admitted pre-state.
fn observe(authority: &Authority, pre: (i128, i128), command_id: u16, allowed: bool) -> Outcome {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let limits = ValidationLimits::default();
    let root = project
        .admit_root::<RustCryptoSha256>(&state(pre.0, pre.1), limits)
        .unwrap();
    let pre_value = root.value().value().clone();
    // The typed bindings must place each counter at its numeric field ID.
    assert_eq!((field(&pre_value, 110), field(&pre_value, 111)), pre);
    let admitted_command = project
        .admit_command::<RustCryptoSha256>(&command(command_id), limits)
        .unwrap();
    assert_eq!(
        variant(admitted_command.admitted().value().value()),
        command_id
    );
    let context = project
        .admit_context::<RustCryptoSha256>(&CounterContext(allowed), limits)
        .unwrap();
    let replay = format!("conformance {pre:?} {command_id} {allowed}");
    let witness = authority
        .admit_invocation(
            root,
            admitted_command.admitted().clone(),
            context.admitted().clone(),
            profile::digest("example/durable-counter/principal", b"conformance"),
            profile::digest("example/durable-counter/authentication", b"conformance"),
            profile::digest("example/durable-counter/replay", replay.as_bytes()),
        )
        .unwrap();
    let domain = Domain::new("example/durable-counter/state", 1).unwrap();
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
                post: (field(post, 110), field(post, 111)),
                notifications: bundle
                    .outbox_plan()
                    .entries()
                    .iter()
                    .map(|entry| {
                        (
                            entry.channel(),
                            field(entry.payload(), 112),
                            field(entry.payload(), 113),
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
            notifications: Vec::new(),
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

/// The finite program's decision, mapped through the declared output table.
///
/// Output 0 is the decision code: 0 rejects with reason 200 and 1 with reason
/// 201, 2 accepts, and 3 commits failure 202. Outputs 1 and 2 are the new
/// counters, and outputs 3 to 5 are the notification flag and its payload. A
/// rejection keeps the pre-state and queues nothing.
fn model(pre: (i128, i128), command_id: u16, allowed: bool) -> Outcome {
    let output = durable_counter::synthesized::transition(&[
        i64::try_from(pre.0).unwrap(),
        i64::try_from(pre.1).unwrap(),
        i64::from(command_id == RECORD_FAILURE),
        i64::from(allowed),
    ])
    .expect("an admitted input has a model output");
    let (kind, reason) = match output[0] {
        0 => ("reject", Some(200)),
        1 => ("reject", Some(201)),
        2 => ("accept", None),
        3 => ("failure", Some(202)),
        code => panic!("undeclared decision code {code}"),
    };
    if kind == "reject" {
        assert_eq!(output[3], 0, "a rejection queues nothing");
        return Outcome {
            kind,
            reason,
            post: pre,
            notifications: Vec::new(),
        };
    }
    assert_eq!(
        output[3], 1,
        "a committing decision queues one notification"
    );
    Outcome {
        kind,
        reason,
        post: (i128::from(output[1]), i128::from(output[2])),
        notifications: vec![(300, i128::from(output[4]), i128::from(output[5]))],
    }
}

/// Every admitted pre-state, command, and context.
fn inputs() -> Vec<((i128, i128), u16, bool)> {
    let mut all = Vec::new();
    for count in 0..=3 {
        for failures in 0..=3 {
            for command_id in [INCREMENT, RECORD_FAILURE] {
                for allowed in [false, true] {
                    all.push(((count, failures), command_id, allowed));
                }
            }
        }
    }
    all
}

#[test]
fn every_admitted_input_matches_the_finite_model() {
    let authority = authority().unwrap();
    let inputs = inputs();
    assert_eq!(inputs.len(), 64);
    for (pre, command_id, allowed) in inputs {
        assert_eq!(
            observe(&authority, pre, command_id, allowed),
            model(pre, command_id, allowed),
            "pre {pre:?}, command {command_id}, allowed {allowed}"
        );
    }
}

struct Example {
    line: &'static str,
    pre: (i128, i128),
    command_id: u16,
    allowed: bool,
    expected: Outcome,
}

fn number<T: std::str::FromStr>(text: &str, line: &str) -> T {
    text.parse()
        .unwrap_or_else(|_| panic!("bad number {text:?} in {line:?}"))
}

fn examples() -> Vec<Example> {
    EXAMPLES
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let parts: Vec<Vec<&str>> = line
                .split('|')
                .map(|part| part.split_whitespace().collect())
                .collect();
            let [input, decision, notification] = parts.as_slice() else {
                panic!("three sections expected in {line:?}");
            };
            let [count, failures, command_id, context] = input.as_slice() else {
                panic!("four inputs expected in {line:?}");
            };
            let [kind, reason, post_count, post_failures] = decision.as_slice() else {
                panic!("four decision fields expected in {line:?}");
            };
            let kind = match *kind {
                "accept" => "accept",
                "reject" => "reject",
                "failure" => "failure",
                other => panic!("unknown outcome {other:?} in {line:?}"),
            };
            let notifications = match notification.as_slice() {
                ["-"] => Vec::new(),
                [channel, notified_count, notified_failures] => vec![(
                    number(channel, line),
                    number(notified_count, line),
                    number(notified_failures, line),
                )],
                _ => panic!("bad notification in {line:?}"),
            };
            Example {
                line,
                pre: (number(count, line), number(failures, line)),
                command_id: number(command_id, line),
                allowed: match *context {
                    "0" => false,
                    "1" => true,
                    other => panic!("context must be 0 or 1, got {other:?}"),
                },
                expected: Outcome {
                    kind,
                    reason: (*reason != "-").then(|| number(reason, line)),
                    post: (number(post_count, line), number(post_failures, line)),
                    notifications,
                },
            }
        })
        .collect()
}

#[test]
fn independent_examples_match_the_executed_application() {
    let authority = authority().unwrap();
    let examples = examples();
    let mut seen = BTreeSet::new();
    for example in &examples {
        assert_eq!(
            observe(&authority, example.pre, example.command_id, example.allowed),
            example.expected,
            "example {:?}",
            example.line
        );
        seen.insert((example.expected.kind, example.expected.reason));
    }
    // The examples exercise every outcome and reason.
    assert_eq!(examples.len(), 12);
    assert_eq!(seen.len(), 4, "{seen:?}");
}

#[test]
fn schema_admission_matches_the_finite_input_domain() {
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    for count in -1..=4_i128 {
        for failures in -1..=4_i128 {
            let admitted = project
                .admit_root::<RustCryptoSha256>(
                    &state(count, failures),
                    ValidationLimits::default(),
                )
                .is_ok();
            let modeled = durable_counter::synthesized::transition(&[
                i64::try_from(count).unwrap(),
                i64::try_from(failures).unwrap(),
                0,
                1,
            ])
            .is_some();
            assert_eq!(admitted, modeled, "pre ({count}, {failures})");
        }
    }
    // Commands and contexts are closed Rust types with two values each; the
    // model admits exactly the two command bits and the two contexts.
    for value in -1..=2_i64 {
        let two = (0..=1).contains(&value);
        assert_eq!(
            durable_counter::synthesized::transition(&[0, 0, value, 1]).is_some(),
            two
        );
        assert_eq!(
            durable_counter::synthesized::transition(&[0, 0, 0, value]).is_some(),
            two
        );
    }
}

#[test]
fn genesis_is_zero_and_reaches_every_admitted_state() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    let admit = |count, failures| {
        project
            .admit_root::<RustCryptoSha256>(&state(count, failures), ValidationLimits::default())
            .unwrap()
    };
    assert!(authority.authorize_genesis(admit(0, 0)).is_ok());
    for (count, failures) in [(1, 0), (0, 1), (3, 3)] {
        assert!(authority.authorize_genesis(admit(count, failures)).is_err());
    }
    // Every admitted state is reachable from genesis through executed
    // decisions, so the model's admitted inputs are reachable inputs.
    let mut reached = BTreeSet::from([(0, 0)]);
    let mut frontier = vec![(0, 0)];
    while let Some(pre) = frontier.pop() {
        for command_id in [INCREMENT, RECORD_FAILURE] {
            for allowed in [false, true] {
                let post = observe(&authority, pre, command_id, allowed).post;
                if reached.insert(post) {
                    frontier.push(post);
                }
            }
        }
    }
    assert_eq!(reached.len(), 16);
}
