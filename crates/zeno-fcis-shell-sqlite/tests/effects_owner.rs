//! Actual SQLite, authority, durable reservation and transport observations.
#[allow(dead_code, unused_imports)]
mod sim;

use rusqlite::{Connection, OpenFlags};
use sim::{
    database::{self, Files},
    fixture, key, model, ok,
};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};
use zeno_fcis_codec::{CommitmentHasher, RustCryptoSha256};
use zeno_fcis_shell::effects::{
    durable,
    policy::Policy,
    port::{Code, Delivery, Fault, Report, Sent},
};
use zeno_fcis_shell_sqlite::{
    effects::{
        dispatcher::{Configuration, Error, Limits, Refusal, TrustedDispatcher},
        production::{AdapterProvenance, Transport},
    },
    v2::{self, Lineage, V2SqliteShell, relay},
};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Success,
    Unknown,
    NotSent,
    WrongReport,
    Panic,
}

#[derive(Clone, Debug)]
struct Observation {
    delivery: Delivery,
    reservations: Vec<u8>,
}
type Calls = Rc<RefCell<Vec<Observation>>>;

struct Wire {
    calls: Calls,
    reservations: PathBuf,
    mode: Mode,
}
impl Transport for Wire {
    fn provenance(&self) -> Option<AdapterProvenance> {
        Some(AdapterProvenance {
            source: RustCryptoSha256::hash(include_bytes!("effects_owner.rs")),
            configuration: vec![self.mode as u8],
            seed: 0,
        })
    }
    fn send(&mut self, delivery: &Delivery) -> Sent {
        // Independent connection observes the committed reservation before the
        // fake transport reports anything, including panic or Unknown.
        self.calls.borrow_mut().push(Observation {
            delivery: delivery.clone(),
            reservations: checkpoint(&self.reservations),
        });
        match self.mode {
            Mode::Success => Sent::Sent(Report {
                delivery_id: delivery.id,
                payload_sha256: delivery.payload_sha256,
            }),
            Mode::Unknown => Sent::Unknown(Fault::Io),
            Mode::NotSent => Sent::NotSent(Code::Interrupted),
            Mode::WrongReport => Sent::Sent(Report {
                delivery_id: key(999),
                payload_sha256: delivery.payload_sha256,
            }),
            Mode::Panic => panic!("transport lost control after possible invocation"),
        }
    }
}

fn wire(files: &Files, calls: &Calls, mode: Mode) -> Wire {
    Wire {
        calls: Rc::clone(calls),
        reservations: files.receiver.clone(),
        mode,
    }
}
fn configuration() -> Configuration {
    Configuration {
        deployment: key(10),
        tenant: key(20),
        epoch: 30,
        limits: Limits {
            grants: 4,
            intent_bytes: 16_384,
            dispatches: 16,
        },
    }
}
fn policies() -> [Policy; 1] {
    [Policy::AtMostOne { capacity: 2 }]
}
fn checkpoint(path: &Path) -> Vec<u8> {
    ok(ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    ))
    .query_row(
        "SELECT bytes FROM effect_reservation WHERE id = 1",
        [],
        |r| r.get(0),
    ))
}
fn with_lineage(run: impl FnOnce(&Lineage<'_, '_>)) {
    fixture::with_catalog(|catalog| run(&ok(Lineage::bind(&[catalog], &[]))));
}
fn seed(files: &Files, lineage: &Lineage<'_, '_>) -> Vec<Delivery> {
    let authority = &lineage.authorities()[0];
    let mut db = sim::create(files, authority);
    let input = fixture::input(authority, key(1), [0, 0], 120);
    assert!(matches!(
        sim::commit(&mut db, authority, &input, 0),
        model::Outcome::Committed(1, _)
    ));
    ok(db.export_pending())
        .iter()
        .map(|d| Delivery {
            id: d.delivery_id(),
            version: d.version(),
            lane: d.lane(),
            ordinal: d.ordinal(),
            destination: d.destination().to_vec(),
            payload: d.payload().to_vec(),
            payload_sha256: relay::payload_sha256(d.payload()),
        })
        .collect()
}
fn create<'a, 'p>(
    files: &Files,
    lineage: &'a Lineage<'a, 'p>,
    calls: &Calls,
    mode: Mode,
) -> TrustedDispatcher<'a, 'p, Wire> {
    ok(TrustedDispatcher::create(
        &files.store,
        &files.receiver,
        lineage,
        wire(files, calls, mode),
        configuration(),
        &policies(),
    ))
}
fn reopen<'a, 'p>(
    files: &Files,
    lineage: &'a Lineage<'a, 'p>,
    calls: &Calls,
) -> TrustedDispatcher<'a, 'p, Wire> {
    let mut config = configuration();
    config.epoch += 1;
    ok(TrustedDispatcher::open(
        &files.store,
        &files.receiver,
        lineage,
        wire(files, calls, Mode::Success),
        config,
        &policies(),
    ))
}
fn refused(result: Result<Sent, Error>, expected: Refusal) {
    assert!(
        matches!(result, Err(Error::Refused(reason)) if reason == expected),
        "{result:?}"
    );
}

#[test]
fn valid_dispatch_uses_exact_original_bytes_after_durable_reservation() {
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut owner = create(&files, lineage, &calls, Mode::Success);
        let handle = ok(owner.grant(key(40), pending[0].id));
        let before = database::snapshot(&files.store);
        let initial_reservations = checkpoint(&files.receiver);
        let sent = ok(owner.session(key(20), key(40)).dispatch(&handle));
        assert_eq!(
            sent,
            Sent::Sent(Report {
                delivery_id: pending[0].id,
                payload_sha256: pending[0].payload_sha256
            })
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(calls.borrow()[0].delivery, pending[0]);
        assert_ne!(calls.borrow()[0].reservations, initial_reservations);
        assert_eq!(calls.borrow()[0].reservations, checkpoint(&files.receiver));
        assert_eq!(
            database::snapshot(&files.store),
            before,
            "dispatch is not acknowledgement"
        );
        let reserved = checkpoint(&files.receiver);
        assert_eq!(
            ok(owner.session(key(20), key(40)).dispatch(&handle)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(checkpoint(&files.receiver), reserved);
    });
}

#[test]
fn wrong_principal_tenant_and_same_genesis_owner_have_no_effect() {
    with_lineage(|lineage| {
        let first = Files::new();
        let second = Files::new();
        let pending = seed(&first, lineage);
        assert_eq!(seed(&second, lineage), pending);
        let calls = Calls::default();
        let mut owner = create(&first, lineage, &calls, Mode::Success);
        let mut other = create(&second, lineage, &calls, Mode::Success);
        let handle = ok(owner.grant(key(40), pending[0].id));
        // Same principal, intent bytes and owner-local grant number. Only the
        // private owner incarnation distinguishes these capabilities.
        let _other_handle = ok(other.grant(key(40), pending[0].id));
        let before = database::snapshot(&first.store);
        let reserved = checkpoint(&first.receiver);
        let other_reserved = checkpoint(&second.receiver);
        refused(
            owner.session(key(21), key(41)).dispatch(&handle),
            Refusal::Tenant,
        );
        refused(
            owner.session(key(20), key(41)).dispatch(&handle),
            Refusal::Principal,
        );
        refused(
            other.session(key(21), key(41)).dispatch(&handle),
            Refusal::Owner,
        );
        refused(
            other.session(key(20), key(40)).dispatch(&handle),
            Refusal::Owner,
        );
        assert!(calls.borrow().is_empty());
        assert_eq!(database::snapshot(&first.store), before);
        assert_eq!(checkpoint(&first.receiver), reserved);
        assert_eq!(checkpoint(&second.receiver), other_reserved);
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&handle),
            Ok(Sent::Sent(_))
        ));
    });
}

#[test]
fn revocation_and_replacement_never_revive_old_handles_or_erase_attempts() {
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut owner = create(&files, lineage, &calls, Mode::Unknown);
        let old = ok(owner.grant(key(40), pending[0].id));
        assert!(ok(owner.revoke(&old)));
        assert!(!ok(owner.revoke(&old)));
        let current = ok(owner.grant(key(40), pending[0].id));
        refused(
            owner.session(key(20), key(40)).dispatch(&old),
            Refusal::Grant,
        );
        assert_eq!(
            ok(owner.session(key(20), key(40)).dispatch(&current)),
            Sent::Unknown(Fault::Io)
        );
        let reserved = checkpoint(&files.receiver);
        assert!(ok(owner.revoke(&current)));
        let replacement = ok(owner.grant(key(41), pending[0].id));
        refused(
            owner.session(key(20), key(40)).dispatch(&current),
            Refusal::Grant,
        );
        assert_eq!(
            ok(owner.session(key(20), key(41)).dispatch(&replacement)),
            Sent::Refused(Code::PolicyDuplicate)
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(checkpoint(&files.receiver), reserved);
    });
}

#[test]
fn restart_regrant_preserves_uncertain_attempts_and_invalidates_old_handles() {
    with_lineage(|lineage| {
        for mode in [Mode::Unknown, Mode::WrongReport, Mode::Panic] {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, mode);
            let old = ok(owner.grant(key(40), pending[0].id));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                owner.session(key(20), key(40)).dispatch(&old)
            }));
            match mode {
                Mode::Unknown => assert!(matches!(result, Ok(Ok(Sent::Unknown(Fault::Io))))),
                Mode::WrongReport => assert!(matches!(result, Ok(Err(Error::Reply)))),
                Mode::Panic => assert!(result.is_err()),
                Mode::Success | Mode::NotSent => unreachable!(),
            }
            if !matches!(mode, Mode::Unknown) {
                assert!(matches!(
                    owner.session(key(20), key(40)).dispatch(&old),
                    Err(Error::Stopped)
                ));
            }
            let reserved = checkpoint(&files.receiver);
            drop(owner);
            let mut restored = reopen(&files, lineage, &calls);
            refused(
                restored.session(key(20), key(40)).dispatch(&old),
                Refusal::Owner,
            );
            let current = ok(restored.grant(key(40), pending[0].id));
            assert_eq!(
                ok(restored.session(key(20), key(40)).dispatch(&current)),
                Sent::Refused(Code::PolicyDuplicate)
            );
            assert_eq!(checkpoint(&files.receiver), reserved);
            assert_eq!(calls.borrow().len(), 1);
            // Independent unattempted work remains useful after recovery.
            let fresh = ok(restored.grant(key(40), pending[1].id));
            assert!(matches!(
                restored.session(key(20), key(40)).dispatch(&fresh),
                Ok(Sent::Sent(_))
            ));
            assert_eq!(calls.borrow().len(), 2);
        }
    });
}

#[test]
fn owner_refuses_external_store_changes_and_acknowledged_or_unknown_intents() {
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut owner = create(&files, lineage, &calls, Mode::Success);
        assert!(matches!(
            owner.grant(key(40), key(99)),
            Err(Error::Refused(Refusal::Pending))
        ));
        let handle = ok(owner.grant(key(40), pending[0].id));
        let reserved = checkpoint(&files.receiver);
        // Hostile writer ignores the advisory file lock. Actual SQLite change
        // detection must stop this owner before a reservation or transport call.
        let mut foreign = ok(V2SqliteShell::open(&files.store, &lineage.authorities()[0]));
        ok(relay::acknowledge(
            &mut foreign,
            pending[0].id,
            pending[0].payload_sha256,
        ));
        drop(foreign);
        let acknowledged = database::snapshot(&files.store);
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&handle),
            Err(Error::Store(v2::Error::History))
        ));
        assert!(calls.borrow().is_empty());
        assert_eq!(checkpoint(&files.receiver), reserved);
        assert_eq!(database::snapshot(&files.store), acknowledged);
        drop(owner);
        let mut restored = reopen(&files, lineage, &calls);
        assert!(matches!(
            restored.grant(key(40), pending[0].id),
            Err(Error::Refused(Refusal::Pending))
        ));
    });
}

#[test]
fn policy_denial_and_resource_limits_preserve_successful_work() {
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut config = configuration();
        config.limits.grants = 1;
        config.limits.dispatches = 1;
        let mut owner = ok(TrustedDispatcher::create(
            &files.store,
            &files.receiver,
            lineage,
            wire(&files, &calls, Mode::Success),
            config,
            &policies(),
        ));
        let old = ok(owner.grant(key(40), pending[0].id));
        assert!(matches!(
            owner.grant(key(40), pending[1].id),
            Err(Error::Capacity)
        ));
        assert!(ok(owner.revoke(&old)));
        let current = ok(owner.grant(key(40), pending[1].id));
        refused(
            owner.session(key(20), key(40)).dispatch(&old),
            Refusal::Grant,
        );
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&current),
            Ok(Sent::Sent(_))
        ));
        let reserved = checkpoint(&files.receiver);
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&current),
            Err(Error::Capacity)
        ));
        assert_eq!(checkpoint(&files.receiver), reserved);
        assert_eq!(calls.borrow().len(), 1);
        drop(owner);
        config.limits.intent_bytes = 1;
        let mut owner = ok(TrustedDispatcher::open(
            &files.store,
            &files.receiver,
            lineage,
            wire(&files, &calls, Mode::Success),
            config,
            &policies(),
        ));
        assert!(matches!(
            owner.grant(key(40), pending[0].id),
            Err(Error::Capacity)
        ));
        assert_eq!(checkpoint(&files.receiver), reserved);
    });
    with_lineage(|lineage| {
        for allowed in [false, true] {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut recipient = pending[0].destination.clone();
            if !allowed {
                recipient.push(0);
            }
            let policies = [
                Policy::AtMostOne { capacity: 2 },
                Policy::Allowlist(vec![recipient]),
            ];
            let mut owner = ok(TrustedDispatcher::create(
                &files.store,
                &files.receiver,
                lineage,
                wire(&files, &calls, Mode::Success),
                configuration(),
                &policies,
            ));
            let handle = ok(owner.grant(key(40), pending[0].id));
            let reserved = checkpoint(&files.receiver);
            let result = ok(owner.session(key(20), key(40)).dispatch(&handle));
            if allowed {
                assert!(matches!(result, Sent::Sent(_)));
                assert_eq!(calls.borrow().len(), 1);
                assert_eq!(calls.borrow()[0].delivery, pending[0]);
            } else {
                assert_eq!(result, Sent::Refused(Code::PolicyRecipient));
                assert_eq!(checkpoint(&files.receiver), reserved);
                assert!(calls.borrow().is_empty());
            }
        }
    });
}

#[test]
fn missing_mismatched_or_already_owned_storage_never_starts_a_new_lifetime() {
    with_lineage(|lineage| {
        let files = Files::new();
        seed(&files, lineage);
        let calls = Calls::default();
        assert!(matches!(
            TrustedDispatcher::open(
                &files.store,
                &files.receiver,
                lineage,
                wire(&files, &calls, Mode::Success),
                configuration(),
                &policies()
            ),
            Err(Error::Reservation(_))
        ));
        assert!(!files.receiver.exists());
        let owner = create(&files, lineage, &calls, Mode::Success);
        assert!(matches!(
            TrustedDispatcher::open(
                &files.store,
                &files.receiver,
                lineage,
                wire(&files, &calls, Mode::Success),
                configuration(),
                &policies()
            ),
            Err(Error::Store(v2::Error::Concurrent))
        ));
        let reserved = checkpoint(&files.receiver);
        drop(owner);
        for index in 0..3 {
            let mut config = configuration();
            let mut policy = policies();
            match index {
                0 => config.tenant = key(21),
                1 => config.deployment = key(11),
                _ => policy[0] = Policy::AtMostOne { capacity: 3 },
            }
            assert!(matches!(
                TrustedDispatcher::open(
                    &files.store,
                    &files.receiver,
                    lineage,
                    wire(&files, &calls, Mode::Success),
                    config,
                    &policy
                ),
                Err(Error::Reservation(durable::Error::Binding))
            ));
            assert_eq!(checkpoint(&files.receiver), reserved);
        }
        assert!(matches!(
            TrustedDispatcher::create(
                &files.store,
                &files.receiver,
                lineage,
                wire(&files, &calls, Mode::Success),
                configuration(),
                &policies()
            ),
            Err(Error::Reservation(_))
        ));
        assert_eq!(checkpoint(&files.receiver), reserved);
        assert!(calls.borrow().is_empty());
        let _restored = reopen(&files, lineage, &calls);
    });
}

#[test]
fn concurrent_reservation_write_stops_dispatch_without_invoking_transport() {
    with_lineage(|lineage| {
        let files = Files::new();
        let pending = seed(&files, lineage);
        let calls = Calls::default();
        let mut owner = create(&files, lineage, &calls, Mode::Success);
        let handle = ok(owner.grant(key(40), pending[0].id));
        let mut changed = checkpoint(&files.receiver);
        changed.push(9);
        ok(ok(Connection::open(&files.receiver)).execute(
            "UPDATE effect_reservation SET bytes = ?1 WHERE id = 1",
            [&changed],
        ));
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&handle),
            Err(Error::Reservation(durable::Error::Conflict))
        ));
        assert!(matches!(
            owner.session(key(20), key(40)).dispatch(&handle),
            Err(Error::Stopped)
        ));
        assert_eq!(checkpoint(&files.receiver), changed);
        assert!(calls.borrow().is_empty());
        drop(owner);
        assert!(matches!(
            TrustedDispatcher::open(
                &files.store,
                &files.receiver,
                lineage,
                wire(&files, &calls, Mode::Success),
                configuration(),
                &policies()
            ),
            Err(Error::Reservation(durable::Error::Corrupt))
        ));
    });
}

mod worker_channel {
    use super::*;
    use zeno_fcis_shell_sqlite::effects::channel::{self, Channel, Error as ChannelError, Status};

    #[test]
    fn malformed_frames_and_slots_stop_before_touching_real_state() {
        with_lineage(|lineage| {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, Mode::Success);
            let handles = [ok(owner.grant(key(40), pending[0].id))];
            let before = database::snapshot(&files.store);
            let reserved = checkpoint(&files.receiver);
            let valid = channel::request(0, 0);
            let mut cases: Vec<_> = (0..16)
                .map(|length| (valid[..length].to_vec(), ChannelError::Format))
                .collect();
            let mut trailing = valid.to_vec();
            trailing.push(0);
            cases.push((trailing, ChannelError::Format));
            for index in 0..4 {
                let mut wrong_version = valid;
                wrong_version[index] ^= 1;
                cases.push((wrong_version.to_vec(), ChannelError::Format));
            }
            for sequence in [1, 2, u32::MAX as u64, u64::MAX] {
                cases.push((
                    channel::request(sequence, 0).to_vec(),
                    ChannelError::Sequence,
                ));
            }
            for slot in [1, 63, 64, u32::MAX] {
                cases.push((channel::request(0, slot).to_vec(), ChannelError::Slot));
            }
            for (frame, reason) in cases {
                let mut channel = ok(Channel::new(key(20), key(40), &handles, 4));
                assert_eq!(channel.dispatch(&mut owner, &frame), Err(reason));
                assert_eq!(
                    channel.dispatch(&mut owner, &valid),
                    Err(ChannelError::Stopped)
                );
                assert!(calls.borrow().is_empty());
                assert_eq!(database::snapshot(&files.store), before);
                assert_eq!(checkpoint(&files.receiver), reserved);
            }
        });
    }

    #[test]
    fn valid_wire_dispatch_preserves_original_bytes_and_rejects_replay() {
        with_lineage(|lineage| {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, Mode::Success);
            let handles = [ok(owner.grant(key(40), pending[0].id))];
            let mut channel = ok(Channel::new(key(20), key(40), &handles, 4));
            // Literal independent wire example, not the implementation's encoder.
            let request = *b"ZFW1\0\0\0\0\0\0\0\0\0\0\0\0";
            let reply = ok(channel.dispatch(&mut owner, &request));
            assert_eq!(reply.sequence, 0);
            assert_eq!(reply.status, Status::Sent);
            assert_eq!(reply.encode(), *b"ZFR1\0\0\0\0\0\0\0\0\0\0\0\0");
            assert_eq!(calls.borrow().len(), 1);
            assert_eq!(calls.borrow()[0].delivery, pending[0]);
            let reserved = checkpoint(&files.receiver);
            assert_eq!(
                channel.dispatch(&mut owner, &request),
                Err(ChannelError::Sequence)
            );
            assert_eq!(
                channel.dispatch(&mut owner, &channel::request(1, 0)),
                Err(ChannelError::Stopped)
            );
            // A fresh connection is not authority to forget a send reservation.
            let mut replacement = ok(Channel::new(key(20), key(40), &handles, 4));
            assert_eq!(
                ok(replacement.dispatch(&mut owner, &request)).status,
                Status::PolicyRefused
            );
            assert_eq!(calls.borrow().len(), 1);
            assert_eq!(checkpoint(&files.receiver), reserved);
        });
    }

    #[test]
    fn identity_and_revocation_are_rechecked_on_every_wire_dispatch() {
        with_lineage(|lineage| {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, Mode::Success);
            let handles = [
                ok(owner.grant(key(40), pending[0].id)),
                ok(owner.grant(key(40), pending[1].id)),
            ];
            let reserved = checkpoint(&files.receiver);
            for (tenant, principal, reason) in [
                (key(21), key(40), Refusal::Tenant),
                (key(20), key(41), Refusal::Principal),
            ] {
                let mut channel = ok(Channel::new(tenant, principal, &handles, 4));
                assert_eq!(
                    ok(channel.dispatch(&mut owner, &channel::request(0, 0))).status,
                    Status::DispatcherRefused(reason)
                );
                assert!(calls.borrow().is_empty());
                assert_eq!(checkpoint(&files.receiver), reserved);
            }
            let mut channel = ok(Channel::new(key(20), key(40), &handles, 4));
            assert_eq!(
                ok(channel.dispatch(&mut owner, &channel::request(0, 0))).status,
                Status::Sent
            );
            assert!(ok(owner.revoke(&handles[1])));
            let _replacement = ok(owner.grant(key(40), pending[1].id));
            let reply = ok(channel.dispatch(&mut owner, &channel::request(1, 1)));
            assert_eq!(reply.status, Status::DispatcherRefused(Refusal::Grant));
            assert_eq!(reply.encode(), *b"ZFR1\x01\0\0\0\0\0\0\0\x04\x04\0\0");
            assert_eq!(calls.borrow().len(), 1);
        });
    }

    #[test]
    fn unknown_not_sent_and_owner_failure_remain_distinct() {
        with_lineage(|lineage| {
            for (mode, expected) in [
                (Mode::Unknown, Status::Unknown),
                (Mode::NotSent, Status::NotSent),
                (Mode::WrongReport, Status::DispatcherUnavailable),
            ] {
                let files = Files::new();
                let pending = seed(&files, lineage);
                let calls = Calls::default();
                let mut owner = create(&files, lineage, &calls, mode);
                let handles = [ok(owner.grant(key(40), pending[0].id))];
                let mut channel = ok(Channel::new(key(20), key(40), &handles, 4));
                let first = ok(channel.dispatch(&mut owner, &channel::request(0, 0)));
                assert_eq!(first.status, expected);
                let second = channel.dispatch(&mut owner, &channel::request(1, 0));
                if expected == Status::DispatcherUnavailable {
                    assert_eq!(second, Err(ChannelError::Stopped));
                } else {
                    assert_eq!(ok(second).status, Status::PolicyRefused);
                }
                assert_eq!(calls.borrow().len(), 1);
                drop(owner);
                let mut owner = reopen(&files, lineage, &calls);
                let handles = [ok(owner.grant(key(40), pending[0].id))];
                let mut replacement = ok(Channel::new(key(20), key(40), &handles, 4));
                assert_eq!(
                    ok(replacement.dispatch(&mut owner, &channel::request(0, 0))).status,
                    Status::PolicyRefused
                );
                assert_eq!(calls.borrow().len(), 1);
            }
        });
    }

    #[test]
    fn unwind_after_invocation_stops_the_wire_channel() {
        with_lineage(|lineage| {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, Mode::Panic);
            let handles = [ok(owner.grant(key(40), pending[0].id))];
            let mut channel = ok(Channel::new(key(20), key(40), &handles, 4));
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                channel.dispatch(&mut owner, &channel::request(0, 0))
            }));
            assert!(result.is_err());
            assert_eq!(calls.borrow().len(), 1);
            assert_eq!(
                channel.dispatch(&mut owner, &channel::request(1, 0)),
                Err(ChannelError::Stopped)
            );
        });
    }

    #[test]
    fn bounds_do_not_reset_reservations_or_block_independent_channels() {
        with_lineage(|lineage| {
            let files = Files::new();
            let pending = seed(&files, lineage);
            let calls = Calls::default();
            let mut owner = create(&files, lineage, &calls, Mode::Success);
            let handles = [
                ok(owner.grant(key(40), pending[0].id)),
                ok(owner.grant(key(40), pending[1].id)),
            ];
            for limit in [0, channel::MAX_REQUESTS + 1, u32::MAX] {
                assert!(matches!(
                    Channel::new(key(20), key(40), &handles, limit),
                    Err(ChannelError::Configuration)
                ));
            }
            let mut empty = ok(Channel::new(key(20), key(40), &[], 1));
            assert_eq!(
                empty.dispatch(&mut owner, &channel::request(0, 0)),
                Err(ChannelError::Slot)
            );
            let mut first = ok(Channel::new(key(20), key(40), &handles, 1));
            assert_eq!(
                ok(first.dispatch(&mut owner, &channel::request(0, 0))).status,
                Status::Sent
            );
            let reserved = checkpoint(&files.receiver);
            assert_eq!(
                first.dispatch(&mut owner, &channel::request(1, 1)),
                Err(ChannelError::Limit)
            );
            assert_eq!(checkpoint(&files.receiver), reserved);
            let mut second = ok(Channel::new(key(20), key(40), &handles, 1));
            assert_eq!(
                ok(second.dispatch(&mut owner, &channel::request(0, 1))).status,
                Status::Sent
            );
            assert_eq!(calls.borrow().len(), 2);
            assert_eq!(calls.borrow()[1].delivery, pending[1]);
        });
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "native_worker/tests.rs"]
mod native_worker_process;
