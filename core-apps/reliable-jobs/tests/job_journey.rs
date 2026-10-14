//! Shell journeys; application decision and law code remain generated data.
#![cfg(feature = "sqlite")]

use application::{Destination, Failure, FileDestination, Outgoing, Sent};
use zeno_fcis_codec::CanonicalEncode;
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

type Step = ([i128; 2], [i128; 2], Option<u32>, [i128; 5], bool);

fn journey(directory: &std::path::Path, name: &str, steps: &[Step], expected_intents: u64) {
    let db = directory.join(format!("{name}.sqlite"));
    let received = directory.join(format!("{name}-received.jsonl"));
    application::init(&db).unwrap();
    assert_eq!(application::current(&db).unwrap().state, [3, 0, 0, 0, 0]);
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
            assert_eq!(result.outbox, [(300, vec![(140, command[1]), (141, 1)])]);
            if first_id.is_none() {
                let outgoing = application::pending(&db).unwrap().next.unwrap();
                assert_eq!((outgoing.channel, outgoing.ordinal), (300, 0));
                assert_eq!(
                    (outgoing.destination_type, outgoing.payload_type),
                    (103, 104)
                );
                assert_eq!(
                    outgoing.destination,
                    Value::text_ascii("worker".to_owned())
                        .unwrap()
                        .canonical_bytes()
                        .unwrap()
                );
                let payload = Value::record_canonical(vec![
                    Field::new(140, Value::signed(command[1])),
                    Field::new(141, Value::signed(1)),
                ])
                .unwrap();
                assert_eq!(outgoing.payload, payload.canonical_bytes().unwrap());
                first_id = Some(outgoing.id);
            }
        } else {
            assert!(result.outbox.is_empty());
        }
        let pending = application::pending(&db).unwrap();
        assert_eq!(pending.head.pending, intents);
        assert_eq!(pending.next.map(|outgoing| outgoing.id), first_id);
    }
    assert_eq!(intents, expected_intents);
    let first_id = first_id.expect("each journey emits at least one intent");
    let before_delivery = application::current(&db).unwrap().state;
    let mut receiver = LoseAcknowledgmentOnce {
        receiver: FileDestination::new(&received),
        lose_once: true,
    };
    assert!(application::deliver_to(&db, &mut receiver).is_err());
    assert_eq!(
        application::pending(&db).unwrap().next.unwrap().id,
        first_id
    );
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        1
    );
    let delivered = application::deliver_to(&db, &mut receiver).unwrap();
    assert_eq!(delivered.deliveries.len(), expected_intents as usize);
    assert_eq!(delivered.deliveries[0], first_id);
    assert_eq!(
        delivered
            .deliveries
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        expected_intents as usize
    );
    assert_eq!(delivered.head.pending, 0);
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        expected_intents as usize
    );
    assert!(
        application::deliver_to(&db, &mut receiver)
            .unwrap()
            .deliveries
            .is_empty()
    );
    assert_eq!(application::current(&db).unwrap().state, before_delivery);
    application::audit(&db).unwrap();
}

#[test]
fn retry_budget_deadline_and_lost_ack_are_durable() {
    let parent = std::env::var_os("ZENO_FCIS_COMPOSITION_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let directory = parent.join(format!("job-journey-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    // Ten steps complete on attempt two. Every retry keeps the same pending IDs.
    journey(
        &directory,
        "completion",
        &[
            ([152, 0], [0, 1], Some(202), [3, 0, 0, 0, 0], false),
            ([150, 1], [0, 0], Some(200), [3, 0, 0, 0, 0], false),
            ([150, 1], [0, 1], None, [2, 1, 0, 0, 0], true),
            ([150, 1], [0, 1], None, [2, 1, 0, 0, 0], false),
            ([151, 1], [0, 1], None, [2, 2, 0, 0, 0], false),
            ([150, 2], [3, 1], Some(203), [2, 2, 0, 0, 0], false),
            ([150, 2], [4, 1], None, [1, 1, 4, 1, 4], true),
            ([152, 1], [4, 1], Some(205), [1, 1, 4, 1, 4], false),
            ([152, 2], [4, 1], None, [1, 3, 4, 1, 4], false),
            ([152, 2], [4, 1], None, [1, 3, 4, 1, 4], false),
        ],
        2,
    );
    // Seven independent steps exercise the last budget slot and nonrefund.
    // The repeated genesis/Start/Fail rows are intentionally executed again.
    journey(
        &directory,
        "exhaustion",
        &[
            ([150, 1], [0, 1], None, [2, 1, 0, 0, 0], true),
            ([151, 1], [0, 1], None, [2, 2, 0, 0, 0], false),
            ([150, 2], [4, 1], None, [1, 1, 4, 1, 4], true),
            ([151, 2], [4, 1], None, [1, 2, 4, 1, 4], false),
            ([150, 3], [4, 1], None, [0, 1, 4, 1, 4], true),
            ([151, 3], [4, 1], None, [0, 2, 4, 1, 4], false),
            ([150, 3], [4, 1], Some(201), [0, 2, 4, 1, 4], false),
        ],
        3,
    );
    println!("retained reliable-job journeys: {}", directory.display());
}
