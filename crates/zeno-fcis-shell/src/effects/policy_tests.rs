use alloc::vec;
use alloc::vec::Vec;

use zeno_fcis_codec::Hash32;

use super::codec;
use super::driver::{Binding, Driver, Reply};
use super::explore::{self, Exploration, Layers, Order, Scenario};
use super::handler::{Handled, Layer, Stack, Terminal};
use super::laws;
use super::policy::{self, Monitor, Policy, State, Verdict};
use super::port::{Code, Fault, OpId, Operation, Outcome, Response, Sent};
use super::program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram};
use super::record::{self, Ended, Entry, Manifest, Transcript};
use super::registry::{Port, REGISTRY};
use super::simulation::{self, Durable, Profile, Script, Simulation, World};

fn ok<T, E: core::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{error:?}"),
    }
}

fn policies(allowed: bool) -> Vec<Policy> {
    vec![
        Policy::AtMostOne { capacity: 2 },
        Policy::allowlist(if allowed {
            vec![b"receiver".to_vec()]
        } else {
            Vec::new()
        }),
    ]
}

fn world() -> World {
    World::new(laws::committed(
        &Durable::genesis(),
        Hash32::new([1; 32]),
        1,
    ))
}

fn send() -> Operation {
    Operation::Send(simulation::store(1), world().durable.outbox[0].0.clone())
}

fn stack(policies: Vec<Policy>) -> Stack {
    let mut layers: Vec<Layer> = policies
        .into_iter()
        .map(Monitor::new)
        .map(Layer::Policy)
        .collect();
    layers.push(Layer::Record(Transcript::new(Manifest::current(
        vec!["policy", "policy", "record", "simulation"],
        Vec::new(),
        Hash32::ZERO,
        Vec::new(),
    ))));
    Stack { layers }
}

#[test]
fn policy_multiset() {
    let policy = Policy::AtMostOne { capacity: 2 };
    let initial = State::default();
    let operation = send();
    let before = initial.clone();
    let (reserved, verdict) = policy::step(&policy, &initial, &operation, None);
    assert_eq!(verdict, Verdict::Allow);
    assert_eq!(initial, before, "step mutated its input");
    for answer in [Sent::NotSent(Code::Delivery), Sent::Unknown(Fault::Io)] {
        let (observed, verdict) = policy::step(
            &policy,
            &reserved,
            &operation,
            Some(&Response::Send(answer)),
        );
        assert_eq!((observed, verdict), (reserved.clone(), Verdict::Allow));
    }
    let (_, verdict) = policy::step(&policy, &reserved, &operation, None);
    assert_eq!(
        verdict,
        Verdict::Reject(Code::PolicyDuplicate),
        "second identical send allowed"
    );
    let mut changed = operation.clone();
    if let Operation::Send(store, delivery) = &mut changed {
        store.handle = 2;
        delivery.payload.push(9);
        delivery.destination = b"other".to_vec();
    }
    assert_eq!(
        policy::step(&policy, &reserved, &changed, None).1,
        Verdict::Reject(Code::PolicyDuplicate)
    );
    if let Operation::Send(store, _) = &mut changed {
        store.identity = Hash32::new([9; 32]);
    }
    assert_eq!(
        policy::step(&policy, &reserved, &changed, None).1,
        Verdict::Allow
    );
    assert_eq!(
        policy::step(
            &Policy::AtMostOne { capacity: 0 },
            &initial,
            &operation,
            None
        )
        .1,
        Verdict::Reject(Code::PolicyCapacity)
    );
}

#[test]
fn policy_allowlist() {
    let allowed = Policy::allowlist(vec![b"receiver".to_vec(), b"other".to_vec()]);
    let equivalent = Policy::Allowlist(vec![
        b"other".to_vec(),
        b"receiver".to_vec(),
        b"other".to_vec(),
    ]);
    assert_eq!(allowed.identity(), equivalent.identity());
    let denied = Policy::allowlist(vec![b"other".to_vec()]);
    assert_ne!(allowed.identity(), denied.identity());
    let operation = send();
    assert_eq!(
        policy::step(&allowed, &State::default(), &operation, None).1,
        Verdict::Allow
    );
    assert_eq!(
        policy::step(&denied, &State::default(), &operation, None).1,
        Verdict::Reject(Code::PolicyRecipient),
        "missing allowlist check"
    );
}

#[test]
fn policy_declared_fields() {
    let one = Policy::AtMostOne { capacity: 2 };
    let allowlist = Policy::allowlist(vec![b"receiver".to_vec()]);
    assert_eq!(one.fields(), &["Store.identity", "Delivery.id"]);
    assert_eq!(allowlist.fields(), &["Delivery.destination"]);
    let operation = send();
    for policy in [one, allowlist] {
        assert_eq!(
            policy::step(&policy, &State::default(), &operation, None).1,
            Verdict::Allow,
            "undeclared field read reached policy"
        );
        let mut irrelevant = operation.clone();
        if let Operation::Send(store, delivery) = &mut irrelevant {
            store.handle = 2;
            delivery.payload = vec![99, 88];
            delivery.payload_sha256 = Hash32::ZERO;
            delivery.version = 91;
            delivery.lane = 7;
            delivery.ordinal = 8;
        }
        assert_eq!(
            policy::step(&policy, &State::default(), &operation, None),
            policy::step(&policy, &State::default(), &irrelevant, None)
        );
    }
}

struct Capture(Vec<Operation>);
impl Terminal for Capture {
    fn ports(&self) -> &'static [Port] {
        simulation::ALL_PORTS
    }
    fn handle(&mut self, _: &Binding, operation: &Operation) -> Handled {
        self.0.push(operation.clone());
        Handled::Answer(Response::Send(Sent::NotSent(Code::Delivery)))
    }
}

struct Panics;
impl Terminal for Panics {
    fn ports(&self) -> &'static [Port] {
        simulation::ALL_PORTS
    }
    fn handle(&mut self, _: &Binding, _: &Operation) -> Handled {
        panic!("terminal lost control after invocation")
    }
}

#[test]
fn policy_preserves_request() {
    let mut layers = stack(policies(true));
    let mut terminal = Capture(Vec::new());
    let operation = send();
    let binding = Binding::of(OpId::first(1), &operation);
    let _ = layers.handle(&mut terminal, &binding, &operation);
    assert_eq!(
        terminal.0,
        vec![operation.clone()],
        "policy rewrote requested operation"
    );
    let _ = layers.handle(&mut terminal, &binding, &operation);
    assert_eq!(terminal.0.len(), 1, "policy forwarded a second send");
    let mut denied = stack(policies(false));
    let bytes = denied.policy_bytes();
    let _ = denied.handle(&mut terminal, &binding, &operation);
    assert_eq!(
        denied.policy_bytes(),
        bytes,
        "a rejected request consumed a reservation"
    );
    assert_eq!(terminal.0.len(), 1, "rejected recipient was forwarded");
}

#[test]
fn policy_refusal_pending() {
    let invocation = DeliveryInvocation {
        epoch: 1,
        store: simulation::store(1),
    };
    let profile = Profile::frozen(2);
    let first = || {
        DeliveryProgram::start(
            invocation,
            DeliveryPolicy {
                retries: 2,
                backoff: profile.backoff,
                jitter: profile.jitter,
            },
        )
    };
    let mut driver = ok(Driver::<DeliveryProgram>::start(REGISTRY, 1, first()));
    let mut layers = stack(policies(false));
    let mut simulation = Simulation::new(
        world(),
        profile,
        simulation::ALL_PORTS,
        Script::fault_free(),
    );
    let before = simulation.world.clone();
    while let Some((binding, operation)) = driver.request() {
        let (binding, operation) = (binding.clone(), operation.clone());
        let handled = layers.handle(&mut simulation, &binding, &operation);
        match handled {
            Handled::Answer(response) => ok(driver.reply(Reply { binding, response })),
            _ => break,
        }
    }
    assert_eq!(
        simulation.world.durable.pending(),
        before.durable.pending(),
        "rejection silently dropped record"
    );
    assert!(
        matches!(
            driver.outcome(),
            Some(Outcome::Refused(Code::PolicyRecipient))
        ),
        "rejection has no Refused outcome: {:?}",
        driver.outcome()
    );
    assert!(simulation.world.sent.is_empty());
    let bytes = codec::outcome(
        driver
            .outcome()
            .unwrap_or_else(|| panic!("missing outcome")),
        None,
    );
    layers.done(&bytes);
    let transcript = layers.transcripts()[0];
    assert!(transcript.links.iter().any(|link| matches!(
        link.entry,
        Entry::Response(_, Response::Send(Sent::Refused(Code::PolicyRecipient)))
    )));
    assert_eq!(
        record::replay::<DeliveryProgram>(transcript, &[], 1, first(), |outcome| {
            codec::outcome(
                outcome,
                match outcome {
                    Outcome::Ok(count) => Some(*count),
                    _ => None,
                },
            )
        }),
        Ok(Ended::Done(bytes))
    );
}

#[test]
fn policy_suspend_and_crash() {
    let operation = send();
    let binding = Binding::of(OpId::first(1), &operation);
    let mut layers = stack(policies(true));
    let mut simulation = Simulation::new(
        world(),
        Profile::frozen(2),
        simulation::ALL_PORTS,
        Script::new(Vec::new()),
    );
    let before = layers.policy_bytes();
    assert_eq!(
        layers.handle(&mut simulation, &binding, &operation),
        Handled::Suspend
    );
    assert_eq!(layers.policy_bytes(), before);
    simulation.script = Script::new(vec![5]); // carried, then crash
    assert_eq!(
        layers.handle(&mut simulation, &binding, &operation),
        Handled::Crash
    );
    let mut restarted = Stack {
        layers: layers.policies(),
    };
    assert_eq!(
        restarted.handle(&mut simulation, &binding, &operation),
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
    );
    // A caller catching an unwinding terminal must not regain a send permit.
    let mut layers = stack(policies(true));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = layers.handle(&mut Panics, &binding, &operation);
        }))
        .is_err()
    );
    let mut capture = Capture(Vec::new());
    assert_eq!(
        layers.handle(&mut capture, &binding, &operation),
        Handled::Answer(Response::Send(Sent::Refused(Code::PolicyDuplicate)))
    );
    assert!(capture.0.is_empty());
}

fn explored(retries: u8, scenario: Scenario, allowed: bool, budget: u64) -> explore::Report {
    let result = explore::explore_with_policies(
        scenario,
        Profile::frozen(retries),
        Layers::IdentityRecord,
        Order::Depth,
        budget,
        &policies(allowed),
    );
    assert!(
        result.is_ok(),
        "assertion failed: policy product violation: {result:?}"
    );
    let report = result.unwrap_or_else(|_| unreachable!("checked policy product result"));
    assert_eq!(
        report.exploration,
        Exploration::Complete,
        "{scenario:?} {retries}: {:?}",
        report.stats
    );
    assert_eq!(report.premises, policy::PREMISES);
    let actions = report.stats.actions;
    let actors = if scenario == Scenario::Commit { 2 } else { 1 };
    assert_eq!(
        actions.committed + actions.refused + actions.incomplete,
        report.stats.schedules * actors,
        "an action has no final classification"
    );
    report
}

#[test]
fn policy_retry_product() {
    let report = explored(1, Scenario::Delivery, true, 100_000);
    assert!(
        report.stats.policy_refusals > 0,
        "retry never checked a policy"
    );
    assert!(report.stats.tally.restarts > 0);
    let denied = explored(0, Scenario::Delivery, false, 100_000);
    assert!(denied.stats.policy_refusals > 0);
    let incomplete = ok(explore::explore_with_policies(
        Scenario::Delivery,
        Profile::frozen(0),
        Layers::Record,
        Order::Depth,
        0,
        &policies(true),
    ));
    assert!(matches!(incomplete.exploration, Exploration::Incomplete(_)));
    assert_eq!(incomplete.premises, policy::PREMISES);
    let mut malformed = Profile::frozen(0);
    malformed.retries = 3;
    let refused = ok(explore::explore_with_policies(
        Scenario::Delivery,
        malformed,
        Layers::Record,
        Order::Depth,
        100,
        &policies(true),
    ));
    assert!(matches!(refused.exploration, Exploration::Refused(_)));
    assert_eq!(refused.premises, policy::PREMISES);
    let mut schedule = Vec::new();
    while ok(explore::next_choice_with_policies(
        Scenario::Delivery,
        Profile::frozen(0),
        Layers::Record,
        &schedule,
        &policies(false),
    ))
    .is_some()
    {
        schedule.push(0);
    }
    let recorded = explore::transcripts_with_policies(
        Scenario::Delivery,
        Profile::frozen(0),
        Layers::Record,
        &schedule,
        &policies(false),
    )
    .unwrap_or_else(|| panic!("schedule not reproduced"));
    assert!(!recorded.is_empty());
    assert!(recorded.iter().all(|run| run.premises == policy::PREMISES));
}

#[test]
#[ignore = "complete policy product on the frozen profile, budgets 0-2"]
fn policy_frozen_profile() {
    for allowed in [true, false] {
        for scenario in [Scenario::Commit, Scenario::Delivery] {
            for retries in 0..=2 {
                let report = explored(retries, scenario, allowed, 5_000_000);
                std::println!(
                    "allowed={allowed} {scenario:?} retries={retries}: {:?}; checks={}; premises={:?}",
                    report.stats,
                    report.laws.checks,
                    report.premises
                );
            }
        }
    }
}
