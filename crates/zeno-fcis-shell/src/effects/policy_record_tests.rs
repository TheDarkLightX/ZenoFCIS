use alloc::{vec, vec::Vec};
use zeno_fcis_codec::Hash32;

use super::driver::{Driver, Reply};
use super::handler::{Handled, Layer, Stack};
use super::policy::{Monitor, Policy, Verdict};
use super::port::{Outcome, Response, Sent};
use super::program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram};
use super::record::{Ended, Entry, Manifest, ReplayError, Transcript};
use super::registry::REGISTRY;
use super::simulation::{self, Durable, Profile, Script, Simulation, World};
use super::{codec, laws, record};

fn first() -> super::program::Step<super::program::DeliveryContinuation, u64> {
    let profile = Profile::frozen(0);
    DeliveryProgram::start(
        DeliveryInvocation {
            epoch: 1,
            store: simulation::store(1),
        },
        DeliveryPolicy {
            retries: 0,
            backoff: profile.backoff,
            jitter: profile.jitter,
        },
    )
}

fn encode(outcome: &Outcome<u64>) -> Vec<u8> {
    codec::outcome(
        outcome,
        match outcome {
            Outcome::Ok(n) => Some(*n),
            _ => None,
        },
    )
}

fn run() -> Transcript {
    let mut driver =
        Driver::<DeliveryProgram>::start(REGISTRY, 1, first()).unwrap_or_else(|e| panic!("{e:?}"));
    let mut stack = Stack {
        layers: vec![
            Layer::Policy(Monitor::new(Policy::AtMostOne { capacity: 2 })),
            Layer::Policy(Monitor::new(Policy::allowlist(vec![b"receiver".to_vec()]))),
            Layer::Record(Transcript::new(Manifest::current(
                vec!["policy", "policy", "record", "simulation"],
                vec![],
                Hash32::ZERO,
                vec![],
            ))),
        ],
    };
    let durable = laws::committed(&Durable::genesis(), Hash32::new([1; 32]), 1);
    let mut terminal = Simulation::new(
        World::new(durable),
        Profile::frozen(0),
        simulation::ALL_PORTS,
        Script::fault_free(),
    );
    for _ in 0..100 {
        if let Some(outcome) = driver.outcome() {
            stack.done(&encode(outcome));
            return stack.transcripts()[0].clone();
        }
        let (binding, operation) = driver
            .request()
            .unwrap_or_else(|| panic!("missing request"));
        let binding = binding.clone();
        match stack.handle(&mut terminal, &binding, operation) {
            Handled::Answer(response) => driver
                .reply(Reply { binding, response })
                .unwrap_or_else(|e| panic!("{e:?}")),
            other => panic!("{other:?}"),
        }
    }
    panic!("run bound");
}

fn replay(transcript: &Transcript) -> Result<Ended, ReplayError> {
    record::replay::<DeliveryProgram>(transcript, &[], 1, first(), encode)
}

fn rechain(original: &Transcript, entries: Vec<Entry>) -> Transcript {
    let mut result = Transcript::new(original.manifest.clone());
    for entry in entries {
        result.append(entry);
    }
    result
}

#[test]
fn policy_verdicts_are_recomputed() {
    let transcript = run();
    assert_eq!(transcript.manifest.policies.len(), 2);
    assert!(
        transcript
            .links
            .iter()
            .any(|l| matches!(&l.entry, Entry::Policy(_, c)
        if c.decisions.len() == 2 && c.decisions.iter().all(|d| d.verdict == Verdict::Allow)))
    );
    assert!(matches!(replay(&transcript), Ok(Ended::Done(_))));
    let mut entries: Vec<_> = transcript.links.iter().map(|l| l.entry.clone()).collect();
    let changed = entries
        .iter_mut()
        .find_map(|e| match e {
            Entry::Policy(_, check) => check.decisions.first_mut(),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing check"));
    changed.proposed.push(99);
    let tampered = rechain(&transcript, entries);
    assert_eq!(tampered.verify(), Ok(()));
    assert!(
        matches!(replay(&tampered), Err(ReplayError::Mismatch(_))),
        "a rehashed invented policy successor replayed"
    );
}

#[test]
fn policy_records_cannot_be_omitted_or_added() {
    let transcript = run();
    let mut entries: Vec<_> = transcript.links.iter().map(|l| l.entry.clone()).collect();
    entries.retain(|e| !matches!(e, Entry::Policy(_, _)));
    assert!(
        replay(&rechain(&transcript, entries)).is_err(),
        "policy check omitted"
    );
    let mut substituted = transcript.clone();
    substituted.manifest.policies[0] = Monitor::new(Policy::AtMostOne { capacity: 0 });
    let entries = substituted.links.iter().map(|l| l.entry.clone()).collect();
    assert!(
        replay(&rechain(&substituted, entries)).is_err(),
        "policy substituted"
    );
    let mut missing = transcript.clone();
    missing.manifest.policies.clear();
    assert_eq!(
        replay(&missing),
        Err(ReplayError::Chain(record::ChainError::Link(0)))
    );
    let entries = missing.links.iter().map(|l| l.entry.clone()).collect();
    assert_eq!(
        replay(&rechain(&missing, entries)),
        Err(ReplayError::Manifest)
    );
}

#[test]
fn recorded_refusal_does_not_invent_a_terminal_response() {
    let transcript = run();
    let mut entries: Vec<_> = transcript.links.iter().map(|l| l.entry.clone()).collect();
    // A terminal may itself refuse; replacing a *policy* verdict without
    // changing the input state is detected independently of the hash chain.
    for entry in &mut entries {
        if let Entry::Policy(_, check) = entry {
            check.decisions[0].verdict = Verdict::Reject(super::port::Code::PolicyDuplicate);
            break;
        }
    }
    assert!(replay(&rechain(&transcript, entries)).is_err());
    assert!(
        transcript
            .links
            .iter()
            .any(|l| matches!(l.entry, Entry::Response(_, Response::Send(Sent::Sent(_)))))
    );
}
