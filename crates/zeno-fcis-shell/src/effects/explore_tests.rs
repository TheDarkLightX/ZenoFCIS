use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

use zeno_fcis_codec::Hash32;

use super::driver::Binding;
use super::explore::{self, Exploration, Layers, Order, Refusal, Report, Scenario};
use super::handler::{self, Handled, Layer, Stack, Terminal, Unbound};
use super::laws::Laws;
use super::port::{Code, CommitMode, Fault, OpId, Operation, Outcome, Response};
use super::program::{CommitInvocation, CommitPolicy, CommitProgram, DeliveryProgram, Program};
use super::record::{self, ChainError, Ended, Entry, Manifest, ReplayError, Transcript};
use super::registry::Port;
use super::simulation::{self, Durable, Profile, Script, Simulation, World};

const BUDGET: u64 = 5_000_000;
const STACKS: [Layers; 3] = [Layers::Bare, Layers::Record, Layers::IdentityRecord];

fn explored(scenario: Scenario, retries: u8, layers: Layers, order: Order) -> Report {
    match explore::explore(scenario, Profile::frozen(retries), layers, order, BUDGET) {
        Ok(report) => {
            assert_eq!(
                report.exploration,
                Exploration::Complete,
                "{scenario:?} retries {retries} {layers:?} {order:?}: {:?}",
                report.stats
            );
            report
        }
        Err(violation) => match violation.law {
            Some(law) => panic!(
                "{scenario:?} retries {retries} {layers:?}: law {} failed: {} at {:?}",
                law.law, law.detail, violation.schedule
            ),
            None => panic!(
                "{scenario:?} retries {retries} {layers:?}: {} at {:?}",
                violation.what, violation.schedule
            ),
        },
    }
}

fn witnesses(laws: &Laws) -> String {
    let mut text = String::new();
    for (branch, count) in &laws.witnesses {
        let _ = write!(text, "{branch}={count} ");
    }
    text
}

fn stack(layers: Layers) -> Stack {
    let manifest = Manifest::current(
        vec!["record", "simulation"],
        Vec::new(),
        Hash32::ZERO,
        Vec::new(),
    );
    match layers {
        Layers::Bare => Stack::default(),
        Layers::Record => Stack {
            layers: vec![Layer::Record(Transcript::new(manifest))],
        },
        Layers::IdentityRecord => Stack {
            layers: vec![Layer::Identity, Layer::Record(Transcript::new(manifest))],
        },
    }
}

/// The nine laws for each handler stack: every request of both scenarios at
/// retry budget zero, and the probe requests from every store either
/// reaches, under every choice. Every required branch must be witnessed.
#[test]
fn simulation_laws() {
    for layers in STACKS {
        let mut laws = Laws::default();
        let mut stores: Vec<Durable> = Vec::new();
        for scenario in [Scenario::Commit, Scenario::Delivery] {
            let report = explored(scenario, 0, layers, Order::Depth);
            laws.merge(&report.laws);
            for store in report.stores {
                if !stores.contains(&store) {
                    stores.push(store);
                }
            }
        }
        for store in &stores {
            for profile in [Profile::frozen(0), Profile::frozen(2)] {
                if let Err(failure) = laws.probe(&profile, store, &stack(layers)) {
                    panic!("{layers:?}: law {} failed: {}", failure.law, failure.detail);
                }
            }
        }
        let missing = laws.unwitnessed();
        assert!(
            missing.is_empty(),
            "{layers:?}: unwitnessed {missing:?}; {}",
            witnesses(&laws)
        );
        std::println!(
            "{layers:?}: {} law checks over {} stores; {}",
            laws.checks,
            stores.len(),
            witnesses(&laws)
        );
    }
}

/// Both scenarios at retry budgets zero and one, recorded, with every
/// transcript replayed handler-free: complete, with every invariant and law
/// holding. At budget zero, breadth-first reaches the same states and final
/// states; the ignored test below compares them at every budget.
#[test]
fn explore_frozen_profile() {
    for scenario in [Scenario::Commit, Scenario::Delivery] {
        for retries in 0..=1 {
            let depth = explored(scenario, retries, Layers::Record, Order::Depth);
            if retries == 0 {
                let breadth = explored(scenario, retries, Layers::Bare, Order::Breadth);
                assert_eq!(depth.stats.states, breadth.stats.states, "{scenario:?}");
                assert_eq!(depth.finals, breadth.finals, "{scenario:?}");
            }
            let tally = depth.stats.tally;
            assert!(depth.stats.replays > 0 && tally.crashes > 0 && tally.restarts > 0);
            assert!(scenario == Scenario::Commit || tally.received > 0);
            assert!(retries == 0 || tally.late > 0, "no late reply was offered");
            std::println!("{scenario:?} retries {retries}: {:?}", depth.stats);
        }
    }
}

/// The whole frozen profile, retry budgets zero to two, behind an identity
/// forwarder and a record layer. Run with `--ignored`: it takes minutes in a
/// debug build.
#[test]
#[ignore = "full frozen profile at every retry budget; minutes in a debug build"]
fn explore_frozen_profile_all_budgets() {
    for scenario in [Scenario::Commit, Scenario::Delivery] {
        for retries in 0..=2 {
            let depth = explored(scenario, retries, Layers::IdentityRecord, Order::Depth);
            let breadth = explored(scenario, retries, Layers::Bare, Order::Breadth);
            assert_eq!(depth.stats.states, breadth.stats.states);
            assert_eq!(depth.finals, breadth.finals);
            std::println!(
                "{scenario:?} retries {retries}: {:?}; {} law checks; breadth-first {:?}",
                depth.stats,
                depth.laws.checks,
                breadth.stats
            );
        }
    }
}

/// A lost reply to a delivery's first send, offered after the resend was
/// requested: same port, store and subject, but an older ordinal.
#[test]
fn late_replies_refused() {
    let report = explored(Scenario::Delivery, 1, Layers::Bare, Order::Depth);
    assert!(report.stats.tally.late > 0, "no late reply was offered");
}

#[test]
fn exploration_outcomes() {
    let small = explore::explore(
        Scenario::Commit,
        Profile::frozen(1),
        Layers::Bare,
        Order::Depth,
        10,
    );
    match small {
        Ok(report) => assert!(
            matches!(report.exploration, Exploration::Incomplete(frontier) if frontier > 0),
            "{:?}",
            report.exploration
        ),
        Err(violation) => panic!("{violation:?}"),
    }
    let mut malformed = Profile::frozen(2);
    malformed.retries = 3;
    let refused = explore::explore(
        Scenario::Commit,
        malformed,
        Layers::Bare,
        Order::Depth,
        BUDGET,
    );
    assert_eq!(
        refused.map(|report| report.exploration),
        Ok(Exploration::Refused(Refusal::Profile))
    );
    // A recorded choice out of range refuses; a short schedule suspends.
    let mut script = Script::new(vec![9]);
    assert_eq!(script.choose(3), None);
    assert!(script.malformed);
    let mut script = Script::new(Vec::new());
    assert_eq!(script.choose(3), None);
    assert_eq!(script.needed, Some(3));
}

#[test]
fn stack_binding() {
    assert_eq!(
        handler::bind(simulation::STORE_PORTS, DeliveryProgram::PORTS),
        Err(Unbound::Uncovered(Port::Send)),
        "uncovered port bound"
    );
    assert_eq!(
        handler::bind(simulation::STORE_PORTS, CommitProgram::PORTS),
        Ok(())
    );
    assert_eq!(
        handler::bind(simulation::ALL_PORTS, DeliveryProgram::PORTS),
        Ok(())
    );
    // A terminal also refuses a port it does not cover.
    let mut store_only = Simulation::new(
        World::new(Durable::genesis()),
        Profile::frozen(0),
        simulation::STORE_PORTS,
        Script::fault_free(),
    );
    let clock = Operation::Clock;
    assert_eq!(
        store_only.handle(&Binding::of(OpId::first(1), &clock), &clock),
        Handled::Answer(Response::Clock(Err(Fault::Io)))
    );
}

/// A recorded run in which the commit applies with its reply lost, and the
/// program reopens, reads history and replays.
fn recovery_transcript() -> (Transcript, Vec<u8>, u64) {
    let profile = Profile::frozen(1);
    // The first actor steps and its reply is lost; then the first option.
    let mut schedule = vec![0, 3];
    loop {
        match explore::next_choice(Scenario::Commit, profile, Layers::Record, &schedule) {
            Ok(Some(_)) => schedule.push(0),
            Ok(None) => break,
            Err(violation) => panic!("{violation:?}"),
        }
    }
    let Some(all) = explore::transcripts(Scenario::Commit, profile, Layers::Record, &schedule)
    else {
        panic!("schedule did not finish");
    };
    let explore::Recorded {
        transcript,
        start,
        epoch,
        recover,
        ended,
    } = all[0].clone();
    assert!(!recover);
    let mut asked = Vec::new();
    for link in &transcript.links {
        if let Entry::Request(_, operation) = &link.entry {
            asked.push(operation.port());
        }
    }
    assert_eq!(
        asked,
        [Port::Commit, Port::Reopen, Port::ReadHistory, Port::Commit]
    );
    assert!(matches!(ended, Ended::Done(_)));
    (transcript, start, epoch)
}

fn replay(transcript: &Transcript, start: &[u8], epoch: u64) -> Result<Ended, ReplayError> {
    let invocation = CommitInvocation {
        epoch,
        store: simulation::store(1),
        key: Hash32::new([0x11; 32]),
        publication: simulation::publication(&[0], 1),
        mode: CommitMode::At(0),
    };
    let first = CommitProgram::start(invocation, CommitPolicy { retries: 1 });
    record::replay::<CommitProgram>(transcript, start, epoch, first, |outcome| {
        super::codec::response(&Response::Commit(outcome.clone()))
    })
}

/// Rebuilds a transcript from edited entries, with a valid chain.
fn relinked(transcript: &Transcript, entries: Vec<Entry>) -> Transcript {
    let mut rebuilt = Transcript::new(transcript.manifest.clone());
    for entry in entries {
        rebuilt.append(entry);
    }
    rebuilt
}

#[test]
fn replay_is_exact() {
    let (transcript, start, epoch) = recovery_transcript();
    let ended = replay(&transcript, &start, epoch);
    assert!(matches!(ended, Ok(Ended::Done(_))), "{ended:?}");
    let entries: Vec<Entry> = transcript
        .links
        .iter()
        .map(|link| link.entry.clone())
        .collect();
    let last = entries.len() - 1;
    let mut truncated = entries.clone();
    truncated.truncate(last - 1);
    let mut extra = entries.clone();
    extra.push(entries[last].clone());
    let mut reordered = entries.clone();
    reordered.swap(2, 4);
    let mut changed = entries.clone();
    if let Entry::Response(binding, _) = &entries[1] {
        changed[1] = Entry::Response(
            binding.clone(),
            Response::Commit(Outcome::Refused(Code::Concurrent)),
        );
    }
    let mut missing = entries.clone();
    missing.remove(3);
    let mut substituted = transcript.clone();
    substituted.manifest.program = Hash32::new([1; 32]);
    let tampered = [
        ("truncated", relinked(&transcript, truncated)),
        ("extra", relinked(&transcript, extra)),
        ("reordered", relinked(&transcript, reordered)),
        ("changed response", relinked(&transcript, changed)),
        ("missing entry", relinked(&transcript, missing)),
        (
            "substituted source",
            relinked(&substituted, entries.clone()),
        ),
    ];
    for (name, transcript) in &tampered {
        let replayed = replay(transcript, &start, epoch);
        assert!(
            replayed.is_err(),
            "tampered transcript replayed: {name} {replayed:?}"
        );
    }
    let mut other_start = start.clone();
    other_start[0] ^= 1;
    assert_eq!(
        replay(&transcript, &other_start, epoch),
        Err(ReplayError::Manifest)
    );
}

#[test]
fn transcript_chain() {
    let (transcript, _, _) = recovery_transcript();
    assert_eq!(transcript.verify(), Ok(()));
    assert!(transcript.links.len() >= 8);
    let mut changed = transcript.clone();
    changed.links[3].entry = changed.links[2].entry.clone();
    let mut relabelled = transcript.clone();
    relabelled.links[5].previous = relabelled.links[3].digest;
    let mut remanifested = transcript.clone();
    remanifested.manifest.schedule.push(0);
    let mut dropped = transcript.clone();
    dropped.links.remove(4);
    for (name, broken, at) in [
        ("changed entry", changed, 3),
        ("wrong predecessor", relabelled, 5),
        ("changed manifest", remanifested, 0),
        ("dropped link", dropped, 4),
    ] {
        assert_eq!(
            broken.verify(),
            Err(ChainError::Link(at)),
            "broken link verified: {name}"
        );
    }
}
