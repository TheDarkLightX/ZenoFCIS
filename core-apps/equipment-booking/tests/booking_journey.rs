//! Durable journeys through generated decisions and genuine store capabilities.
#![cfg(feature = "sqlite")]

use application::{Destination, Failure, FileDestination, Outgoing, Sent};
use zeno_fcis_codec::{CanonicalEncode, Hash32};
use zeno_fcis_shell_sqlite::v2::{Error, V2SqliteShell};
use zeno_fcis_synthesis::finite::{v2_authority as a, v2_composition as c};
use zeno_fcis_value::{Field, Value};

struct LoseAcknowledgmentOnce {
    receiver: FileDestination,
    lose_once: bool,
}

impl Destination for LoseAcknowledgmentOnce {
    fn send(&mut self, outgoing: &Outgoing) -> Result<Sent, Failure> {
        let sent = self.receiver.send(outgoing)?;
        if self.lose_once {
            self.lose_once = false;
            Err(Failure::Refused(
                "lost acknowledgment after durable receipt".to_owned(),
            ))
        } else {
            Ok(sent)
        }
    }
}

type Step = ([i128; 2], [i128; 2], Option<u32>, [i128; 6], bool);

fn directory(name: &str) -> std::path::PathBuf {
    let parent = std::env::var_os("ZENO_FCIS_COMPOSITION_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let path = parent.join(format!("booking-{name}-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    path
}

fn journey(directory: &std::path::Path, name: &str, steps: &[Step], expected_intents: u64) {
    let db = directory.join(format!("{name}.sqlite"));
    let received = directory.join(format!("{name}-received.jsonl"));
    application::init(&db).unwrap();
    assert_eq!(application::current(&db).unwrap().state, [2, 0, 0, 0, 0, 0]);
    let mut intents = 0;
    let mut first_id = None;
    for (command, context, reason, post, emits) in steps {
        let before = application::current(&db).unwrap();
        let database_bytes = std::fs::read(&db).unwrap();
        let journal_bytes = std::fs::read(application::journal_path(&db)).unwrap();
        let result = application::submit(&db, command, context).unwrap();
        assert_eq!(
            result.class,
            if reason.is_some() { "Reject" } else { "Accept" }
        );
        assert_eq!(result.reason, *reason);
        assert_eq!(result.post, *post);
        assert_eq!(application::current(&db).unwrap().state, *post);
        if reason.is_some() {
            assert_eq!(application::current(&db).unwrap(), before);
            assert_eq!(std::fs::read(&db).unwrap(), database_bytes);
            assert_eq!(
                std::fs::read(application::journal_path(&db)).unwrap(),
                journal_bytes
            );
        }
        if *emits {
            intents += 1;
            assert_eq!(result.outbox, [(300, vec![(140, 2), (141, 1)])]);
            assert!(first_id.is_none(), "one booking creates one intent");
            let outgoing = application::pending(&db).unwrap().next.unwrap();
            assert_eq!((outgoing.channel, outgoing.ordinal), (300, 0));
            assert_eq!(
                (outgoing.destination_type, outgoing.payload_type),
                (103, 104)
            );
            assert_eq!(
                outgoing.destination,
                Value::text_ascii("booking".to_owned())
                    .unwrap()
                    .canonical_bytes()
                    .unwrap()
            );
            let payload = Value::record_canonical(vec![
                Field::new(140, Value::signed(2)),
                Field::new(141, Value::signed(1)),
            ])
            .unwrap();
            assert_eq!(outgoing.payload, payload.canonical_bytes().unwrap());
            first_id = Some(outgoing.id);
        } else {
            assert!(result.outbox.is_empty());
        }
        let pending = application::pending(&db).unwrap();
        assert_eq!(pending.head.pending, intents);
        assert_eq!(pending.next.map(|outgoing| outgoing.id), first_id);
    }
    assert_eq!(intents, expected_intents);
    let before_delivery = application::current(&db).unwrap().state;
    let mut receiver = LoseAcknowledgmentOnce {
        receiver: FileDestination::new(&received),
        lose_once: true,
    };
    if let Some(id) = first_id {
        assert!(application::deliver_to(&db, &mut receiver).is_err());
        assert_eq!(application::pending(&db).unwrap().next.unwrap().id, id);
        assert_eq!(
            std::fs::read_to_string(&received).unwrap().lines().count(),
            1
        );
        let delivered = application::deliver_to(&db, &mut receiver).unwrap();
        assert_eq!(delivered.deliveries, [id]);
        assert_eq!(delivered.head.pending, 0);
        assert_eq!(
            std::fs::read_to_string(&received).unwrap().lines().count(),
            1
        );
    } else {
        assert_eq!(expected_intents, 0);
        assert!(!received.exists());
    }
    assert!(
        application::deliver_to(&db, &mut receiver)
            .unwrap()
            .deliveries
            .is_empty()
    );
    assert_eq!(application::current(&db).unwrap().state, before_delivery);
    assert_eq!(application::pending(&db).unwrap().head.pending, 0);
    application::audit(&db).unwrap();
}

#[test]
fn booking_journeys_reject_without_writes_and_redeliver_once() {
    let directory = directory("journeys");
    journey(
        &directory,
        "confirmation",
        &[
            ([151, 0], [0, 1], Some(202), [2, 0, 0, 0, 0, 0], false),
            ([150, 0], [0, 0], Some(200), [2, 0, 0, 0, 0, 0], false),
            ([150, 0], [0, 1], None, [1, 1, 1, 1, 0, 0], false),
            ([152, 1], [3, 1], Some(203), [1, 1, 1, 1, 0, 0], false),
            ([151, 1], [3, 1], None, [1, 1, 2, 2, 0, 0], true),
            ([151, 1], [3, 1], None, [1, 1, 2, 2, 0, 0], false),
            ([152, 1], [4, 1], Some(205), [1, 1, 2, 2, 0, 0], false),
            ([152, 2], [4, 1], Some(202), [1, 1, 2, 2, 0, 0], false),
        ],
        1,
    );
    // These seven rows run from another real genesis, including the repeated reserve.
    journey(
        &directory,
        "timeout",
        &[
            ([150, 0], [4, 1], Some(203), [2, 0, 0, 0, 0, 0], false),
            ([150, 0], [0, 1], None, [1, 1, 1, 1, 0, 0], false),
            ([151, 1], [4, 1], Some(203), [1, 1, 1, 1, 0, 0], false),
            ([152, 1], [4, 1], None, [2, 0, 2, 3, 4, 1], false),
            ([152, 1], [4, 1], None, [2, 0, 2, 3, 4, 1], false),
            ([151, 1], [4, 1], Some(205), [2, 0, 2, 3, 4, 1], false),
            ([152, 1], [3, 1], Some(206), [2, 0, 2, 3, 4, 1], false),
        ],
        0,
    );
    println!(
        "retained equipment-booking journeys: {}",
        directory.display()
    );
}

fn publication<'a>(authority: &'a a::Authority<'_>, wire: &'a [Vec<u8>; 3]) -> a::Publication<'a> {
    match authority.publish(c::Raw {
        state: &wire[0],
        command: &wire[1],
        context: &wire[2],
    }) {
        a::PublicationOutcome::Commit(publication) => publication,
        other => panic!("expected genuine commit capability: {other:?}"),
    }
}

#[test]
fn confirm_and_timeout_candidates_share_one_store_cas() {
    let directory = directory("cas");
    let contract = application::v2_contract::Contract::new();
    let definition = contract.descriptor();
    let authority = application::authority(&definition).unwrap();
    let initial = application::state(&definition, &[2, 0, 0, 0, 0, 0]).unwrap();
    let reserve = application::wire(&definition, &[2, 0, 0, 0, 0, 0, 150, 0, 0, 1]).unwrap();
    let confirm = application::wire(&definition, &[1, 1, 1, 1, 0, 0, 151, 1, 3, 1]).unwrap();
    let timeout = application::wire(&definition, &[1, 1, 1, 1, 0, 0, 152, 1, 4, 1]).unwrap();
    for confirm_wins in [true, false] {
        let file = directory.join(if confirm_wins {
            "confirm.sqlite"
        } else {
            "timeout.sqlite"
        });
        let a::PublicationOutcome::Commit(genesis) = authority.publish_genesis(&initial) else {
            panic!("genesis must publish")
        };
        let mut first = V2SqliteShell::create(&file, &authority, genesis).unwrap();
        first
            .commit(Hash32::new([1; 32]), publication(&authority, &reserve))
            .unwrap();
        let mut second = V2SqliteShell::open(&file, &authority).unwrap();
        let common_head = first.snapshot().unwrap();
        assert_eq!(common_head, second.snapshot().unwrap());
        assert_eq!(common_head.version(), 1);
        assert_eq!(common_head.state(), confirm[0]);
        assert_eq!(confirm[0], timeout[0]);
        // Both capabilities exist before the winner writes. Each passed the same
        // Authority from the same prestate, with its own supplied logical time.
        let confirming = publication(&authority, &confirm);
        let expiring = publication(&authority, &timeout);
        let (winner, loser, loser_command) = if confirm_wins {
            (confirming, expiring, &timeout[1])
        } else {
            (expiring, confirming, &confirm[1])
        };
        first.commit_at(1, Hash32::new([2; 32]), winner).unwrap();
        let before_loser = first.snapshot().unwrap();
        // Seeing another connection's write makes the loser audit and persist its
        // checkpoint marker. Let that happen first, so the bytes below isolate
        // the refused write itself. The loser still holds the stale version 1.
        assert_eq!(second.snapshot().unwrap(), before_loser);
        let database_bytes = std::fs::read(&file).unwrap();
        assert!(matches!(
            second.commit_at(1, Hash32::new([3; 32]), loser),
            Err(Error::Concurrent)
        ));
        assert_eq!(std::fs::read(&file).unwrap(), database_bytes);
        assert_eq!(first.snapshot().unwrap(), before_loser);
        assert_eq!(second.snapshot().unwrap(), before_loser);
        // Each snapshot above audits after the other connection's marker write.
        let database_bytes = std::fs::read(&file).unwrap();
        let expected = if confirm_wins {
            [1, 1, 2, 2, 0, 0]
        } else {
            [2, 0, 2, 3, 4, 1]
        };
        assert_eq!(
            before_loser.state(),
            application::state(&definition, &expected).unwrap()
        );
        assert_eq!(before_loser.version(), 2);
        assert_eq!(before_loser.pending(), u64::from(confirm_wins));
        // Re-evaluating the loser's old command after reload is a business
        // stale-version rejection. It cannot reacquire a committing capability.
        let a::PublicationOutcome::Reject(rejected) = authority.publish(c::Raw {
            state: before_loser.state(),
            command: loser_command,
            context: &timeout[2],
        }) else {
            panic!("reloaded loser must reject")
        };
        assert_eq!(
            rejected.outcome().result().as_ref().unwrap().reason(),
            Some(205)
        );
        assert_eq!(std::fs::read(&file).unwrap(), database_bytes);
        first.audit().unwrap();
        second.audit().unwrap();
    }
    println!(
        "retained equipment-booking CAS cases: {}",
        directory.display()
    );
}
