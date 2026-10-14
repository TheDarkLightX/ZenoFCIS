//! Copied explicitly by tools/check_core_app.py into a fresh generated app.
//! This tests the shell; business decisions and laws stay declarative.
#![cfg(feature = "sqlite")]

use application::{Destination, Failure, FileDestination, Outgoing, Sent};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_value::{Field, Value};

struct LoseAcknowledgmentOnce {
    destination: FileDestination,
    lose_once: bool,
}

impl Destination for LoseAcknowledgmentOnce {
    fn send(&mut self, outgoing: &Outgoing) -> Result<Sent, Failure> {
        let sent = self.destination.send(outgoing)?;
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

#[test]
fn quorum_payment_retry_and_lost_ack_are_durable() {
    let parent = std::env::var_os("ZENO_FCIS_COMPOSITION_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = parent.join(format!("expense-journey-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let db = directory.join("store.sqlite");
    let received = directory.join("received.jsonl");
    application::init(&db).unwrap();
    assert_eq!(application::current(&db).unwrap().state, [7, 0, 0, 0, 0]);

    // These are the independently specified eleven persistent journey steps.
    let steps = [
        ([162, 3, 1], [0, 1], Some(202), [7, 0, 0, 0, 0], false),
        ([160, 3, 1], [0, 0], Some(200), [7, 0, 0, 0, 0], false),
        ([160, 3, 1], [0, 1], None, [7, 1, 0, 0, 0], false),
        ([161, 3, 1], [0, 1], None, [7, 1, 1, 0, 0], false),
        ([161, 3, 1], [0, 1], Some(201), [7, 1, 1, 0, 0], false),
        ([162, 3, 1], [0, 1], Some(203), [7, 1, 1, 0, 0], false),
        ([161, 3, 1], [1, 1], None, [7, 1, 1, 1, 0], false),
        ([162, 4, 1], [0, 1], Some(205), [7, 1, 1, 1, 0], false),
        ([162, 3, 1], [0, 1], None, [4, 2, 1, 1, 0], true),
        ([162, 3, 1], [0, 1], None, [4, 2, 1, 1, 0], false),
        ([162, 3, 0], [0, 1], Some(205), [4, 2, 1, 1, 0], false),
    ];
    let mut payment_id = None;
    for (command, context, reason, post, emits) in steps {
        let before = application::current(&db).unwrap();
        let bytes = std::fs::read(&db).unwrap();
        let journal = std::fs::read(application::journal_path(&db)).unwrap();
        let result = application::submit(&db, &command, &context).unwrap();
        assert_eq!(
            result.class,
            if reason.is_some() { "Reject" } else { "Accept" }
        );
        assert_eq!(result.reason, reason);
        assert_eq!(result.post, post);
        assert_eq!(application::current(&db).unwrap().state, post);
        if reason.is_some() {
            assert_eq!(application::current(&db).unwrap(), before);
            assert_eq!(std::fs::read(&db).unwrap(), bytes);
            assert_eq!(
                std::fs::read(application::journal_path(&db)).unwrap(),
                journal
            );
        }
        if emits {
            assert_eq!(result.outbox, [(300, vec![(140, 3), (141, 1)])]);
            let outgoing = application::pending(&db).unwrap().next.unwrap();
            assert_eq!((outgoing.channel, outgoing.ordinal), (300, 0));
            assert_eq!(
                (outgoing.destination_type, outgoing.payload_type),
                (103, 104)
            );
            assert_eq!(
                outgoing.destination,
                Value::text_ascii("bank".to_owned())
                    .unwrap()
                    .canonical_bytes()
                    .unwrap()
            );
            let payload = Value::record_canonical(vec![
                Field::new(140, Value::signed(3)),
                Field::new(141, Value::signed(1)),
            ])
            .unwrap();
            assert_eq!(outgoing.payload, payload.canonical_bytes().unwrap());
            payment_id = Some(outgoing.id);
        } else {
            assert!(result.outbox.is_empty());
        }
        let waiting = application::pending(&db).unwrap();
        assert_eq!(waiting.head.pending, u64::from(payment_id.is_some()));
        assert_eq!(waiting.next.map(|outgoing| outgoing.id), payment_id);
    }

    let payment_id = payment_id.expect("the journey emits one payment");
    // The payment's durable receiver deduplicates the retry after an ack loss.
    let mut receiver = LoseAcknowledgmentOnce {
        destination: FileDestination::new(&received),
        lose_once: true,
    };
    assert!(application::deliver_to(&db, &mut receiver).is_err());
    assert_eq!(
        application::pending(&db).unwrap().next.unwrap().id,
        payment_id
    );
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        1
    );
    let delivered = application::deliver_to(&db, &mut receiver).unwrap();
    assert_eq!(delivered.deliveries, [payment_id.clone()]);
    assert_eq!(delivered.head.pending, 0);
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        1
    );
    assert!(
        application::deliver_to(&db, &mut receiver)
            .unwrap()
            .deliveries
            .is_empty()
    );
    assert_eq!(application::current(&db).unwrap().state, [4, 2, 1, 1, 0]);
    application::audit(&db).unwrap();
    println!("retained expense journey: {}", directory.display());
}
