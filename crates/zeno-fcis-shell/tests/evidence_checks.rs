//! Source/profile-bound continuation and manifest provenance checks.
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::{
    explore::{self, Exploration, Layers, Order, Scenario},
    policy::Policy,
    record::Manifest,
    simulation::Profile,
};

#[test]
fn resumed_search_matches_uninterrupted_closure() {
    for order in [Order::Depth, Order::Breadth] {
        for scenario in [Scenario::Commit, Scenario::Delivery] {
            for policies in [
                vec![],
                vec![
                    Policy::AtMostOne { capacity: 8 },
                    Policy::allowlist(vec![b"receiver".to_vec()]),
                ],
            ] {
                let whole = explore::explore_with_policies(
                    scenario,
                    Profile::frozen(0),
                    Layers::Record,
                    order,
                    100_000,
                    &policies,
                )
                .unwrap_or_else(|error| panic!("{error:?}"));
                assert_eq!(whole.exploration, Exploration::Complete);
                let mut resumed = explore::explore_with_policies(
                    scenario,
                    Profile::frozen(0),
                    Layers::Record,
                    order,
                    7,
                    &policies,
                )
                .unwrap_or_else(|error| panic!("{error:?}"));
                let mut first_resume = true;
                while matches!(resumed.exploration, Exploration::Incomplete(_)) {
                    let checkpoint = resumed
                        .checkpoint
                        .take()
                        .unwrap_or_else(|| panic!("missing resumable checkpoint"));
                    // Advisory report edits cannot alter the opaque retained progress.
                    resumed.stats.runs = u64::MAX;
                    // Keep the first partial resume and every final closure.
                    // Larger later Miri chunks avoid repeatedly cloning the
                    // growing checkpoint. Native tests retain every 127-step cut.
                    let chunk = if cfg!(miri) && !first_resume {
                        4096
                    } else {
                        127
                    };
                    first_resume = false;
                    resumed = explore::resume(checkpoint, chunk)
                        .unwrap_or_else(|error| panic!("{error:?}"));
                }
                assert_eq!(resumed, whole, "resumed closure differs");
            }
        }
    }
}

#[test]
fn manifest_sources_configuration_and_seed_are_bound() {
    let manifest = Manifest::current(
        vec![
            "identity",
            "policy",
            "record",
            "simulation",
            "fault",
            "replay",
        ],
        vec![],
        Hash32::ZERO,
        vec![],
    );
    assert!(
        manifest.provenance_complete(),
        "handler implementation missing"
    );
    let original = manifest.digest();
    for change in 0..3 {
        let mut changed = manifest.clone();
        match change {
            0 => changed.implementations[0].source = Hash32::ZERO,
            1 => changed.configuration.push(1),
            _ => changed.seed = 1,
        }
        assert_ne!(
            changed.digest(),
            original,
            "manifest omitted handler/configuration/seed"
        );
    }
    let mut omitted = manifest;
    omitted.implementations.clear();
    assert!(!omitted.provenance_complete());
}

#[test]
fn independent_commit_compares_every_delivery_field_and_history() {
    use zeno_fcis_shell::{
        CommitStatus,
        effects::{
            driver::Binding,
            handler::Handled,
            laws::{self, Laws},
            port::{CommitMode, CommitReceipt, OpId, Operation, Outcome, Response},
            simulation::{self, Durable, World},
        },
    };
    let before = Durable::genesis();
    let key = Hash32::new([1; 32]);
    let valid = laws::committed(&before, key, 1);
    assert_eq!(laws::history_fold(&valid.commits), Some(valid.clone()));
    let operation = Operation::Commit(
        simulation::store(1),
        key,
        simulation::publication(&before.state, 1),
        CommitMode::Unversioned,
    );
    let handled = Handled::Answer(Response::Commit(Outcome::Ok(CommitReceipt {
        version: 1,
        chain: valid.chain,
        status: CommitStatus::Committed,
    })));
    for field in 0..9 {
        let mut changed = valid.clone();
        let (delivery, acknowledged) = &mut changed.outbox[1];
        match field {
            0 => delivery.id = Hash32::ZERO,
            1 => delivery.version += 1,
            2 => delivery.lane += 1,
            3 => delivery.ordinal += 1,
            4 => delivery.destination.push(0),
            5 => delivery.payload.push(0),
            6 => delivery.payload_sha256 = Hash32::ZERO,
            7 => *acknowledged = true,
            _ => changed.outbox.reverse(),
        }
        assert!(
            Laws::default()
                .check(
                    &Profile::frozen(0),
                    &World::new(before.clone()),
                    &Binding::of(OpId::first(1), &operation),
                    &operation,
                    &handled,
                    &World::new(changed)
                )
                .is_err(),
            "outbox field {field} accepted"
        );
    }
    for field in 0..6 {
        let mut history = valid.commits.clone();
        let entry = &mut history[0];
        match field {
            0 => entry.version += 1,
            1 => entry.publication.prestate.push(0),
            2 => entry.publication.command.push(0),
            3 => entry.publication.context.push(0),
            4 => entry.publication.subject.push(0),
            _ => entry.publication.identity.push(0),
        }
        assert!(
            laws::history_fold(&history).is_none(),
            "history field {field} accepted"
        );
    }
}
