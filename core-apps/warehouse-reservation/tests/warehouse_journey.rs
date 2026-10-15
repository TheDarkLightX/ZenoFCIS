//! Additional shell evidence, copied explicitly by tools/check_core_app.py.
//! Business decisions and laws remain declarative data, not test callbacks.
#![cfg(feature = "sqlite")]

use application::{Destination, Failure, FileDestination, Outgoing, Sent};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_value::{Field, Value};

struct HoldThenLoseAcknowledgment {
    file: FileDestination,
    lose_once: bool,
}

impl Destination for HoldThenLoseAcknowledgment {
    fn send(&mut self, outgoing: &Outgoing) -> Result<Sent, Failure> {
        let sent = self.file.send(outgoing)?;
        if self.lose_once {
            self.lose_once = false;
            Err(Failure::Refused(
                "simulated acknowledgment loss after durable receipt".to_owned(),
            ))
        } else {
            Ok(sent)
        }
    }
}

#[test]
fn reservation_shipping_retry_and_lost_ack_are_durable() {
    let parent = std::env::var_os("ZENO_FCIS_COMPOSITION_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = parent.join(format!("warehouse-journey-{}", std::process::id()));
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let db = directory.join("store.sqlite");
    let destination = directory.join("received.jsonl");
    application::init(&db).unwrap();
    assert_eq!(application::current(&db).unwrap().state, [4, 0, 0, 0, 0]);

    // Both rejected decisions preserve the actual database and submission log.
    for (command, context, reason) in [([151, 3, 1], [1], 202), ([150, 3, 1], [0], 200)] {
        let before = application::current(&db).unwrap();
        let database = std::fs::read(&db).unwrap();
        let journal = std::fs::read(application::journal_path(&db)).ok();
        let outcome = application::submit(&db, &command, &context).unwrap();
        assert_eq!(outcome.class, "Reject");
        assert_eq!(outcome.reason, Some(reason));
        assert_eq!(application::current(&db).unwrap(), before);
        assert_eq!(std::fs::read(&db).unwrap(), database);
        assert_eq!(std::fs::read(application::journal_path(&db)).ok(), journal);
    }

    let reserve = application::submit(&db, &[150, 3, 1], &[1]).unwrap();
    assert_eq!(reserve.class, "Accept");
    assert_eq!(reserve.post, [1, 3, 1, 3, 1]);
    assert_eq!(application::pending(&db).unwrap().head.pending, 0);
    let retry = application::submit(&db, &[150, 3, 1], &[1]).unwrap();
    assert_eq!(retry.post, reserve.post);
    assert!(retry.outbox.is_empty());
    let before = application::current(&db).unwrap();
    for (command, reason) in [([150, 2, 1], 201), ([151, 3, 0], 202)] {
        let bytes = std::fs::read(&db).unwrap();
        let rejected = application::submit(&db, &command, &[1]).unwrap();
        assert_eq!(rejected.reason, Some(reason));
        assert_eq!(application::current(&db).unwrap(), before);
        assert_eq!(std::fs::read(&db).unwrap(), bytes);
    }
    let ship = application::submit(&db, &[151, 3, 1], &[1]).unwrap();
    assert_eq!(ship.class, "Accept");
    assert_eq!(ship.post, [1, 0, 2, 3, 1]);
    assert_eq!(ship.outbox, [(300, vec![(140, 3), (141, 1)])]);
    let original = application::pending(&db).unwrap().next.unwrap();
    assert_eq!(original.channel, 300);
    assert_eq!(original.ordinal, 0);
    assert_eq!(original.destination_type, 103);
    assert_eq!(original.payload_type, 104);
    assert_eq!(
        original.destination,
        Value::text_ascii("warehouse".to_owned())
            .unwrap()
            .canonical_bytes()
            .unwrap()
    );
    let payload = Value::record_canonical(vec![
        Field::new(140, Value::signed(3)),
        Field::new(141, Value::signed(1)),
    ])
    .unwrap();
    assert_eq!(original.payload, payload.canonical_bytes().unwrap());
    for command in [[151, 3, 1], [150, 3, 1]] {
        let duplicate = application::submit(&db, &command, &[1]).unwrap();
        assert_eq!(duplicate.post, ship.post);
        assert!(duplicate.outbox.is_empty());
        let waiting = application::pending(&db).unwrap();
        assert_eq!(waiting.head.pending, 1);
        assert_eq!(waiting.next.unwrap().id, original.id);
    }

    // The receiver durably holds the intent, then fails before acknowledgment.
    // Retrying uses the same identity and the file receiver deduplicates it.
    let mut receiver = HoldThenLoseAcknowledgment {
        file: FileDestination::new(&destination),
        lose_once: true,
    };
    assert!(application::deliver_to(&db, &mut receiver).is_err());
    assert_eq!(
        application::pending(&db).unwrap().next.unwrap().id,
        original.id
    );
    assert_eq!(
        std::fs::read_to_string(&destination)
            .unwrap()
            .lines()
            .count(),
        1
    );
    let delivered = application::deliver_to(&db, &mut receiver).unwrap();
    assert_eq!(delivered.deliveries, [original.id]);
    assert_eq!(delivered.head.pending, 0);
    assert_eq!(
        std::fs::read_to_string(&destination)
            .unwrap()
            .lines()
            .count(),
        1
    );
    assert!(
        application::deliver_to(&db, &mut receiver)
            .unwrap()
            .deliveries
            .is_empty()
    );
    assert_eq!(application::current(&db).unwrap().state, ship.post);
    application::audit(&db).unwrap();
    println!("retained warehouse journey: {}", directory.display());
}
