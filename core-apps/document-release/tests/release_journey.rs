//! Copied explicitly by tools/check_core_app.py into a fresh generated app.
//! This tests the shell; business decisions and laws stay declarative.
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

const EDIT: i128 = 160;
const SUBMIT: i128 = 161;
const APPROVE: i128 = 162;
const RELEASE: i128 = 163;

#[test]
fn review_locks_release_once_and_lost_ack_is_durable() {
    let parent = std::env::var_os("ZENO_FCIS_COMPOSITION_TEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&parent).unwrap();
    let directory = parent.join(format!("release-journey-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let db = directory.join("store.sqlite");
    let received = directory.join("received.jsonl");
    application::init(&db).unwrap();
    assert_eq!(application::current(&db).unwrap().state, [0, 0, 0, 0, 0, 0]);

    // The fourteen persistent steps of the brief's journey table:
    // (action, expected_version, new_content), (principal, authorized),
    // reason, post state, whether the release intent is emitted.
    let steps = [
        ([EDIT, 0, 1], [0, 0], Some(200), [0, 0, 0, 0, 0, 0], false),
        ([EDIT, 0, 1], [0, 1], None, [1, 1, 0, 0, 0, 0], false),
        ([EDIT, 0, 2], [0, 1], Some(205), [1, 1, 0, 0, 0, 0], false),
        ([EDIT, 1, 2], [0, 1], None, [2, 2, 0, 0, 0, 0], false),
        ([SUBMIT, 2, 0], [0, 1], None, [2, 2, 1, 0, 0, 0], false),
        ([EDIT, 2, 1], [0, 1], Some(202), [2, 2, 1, 0, 0, 0], false),
        ([APPROVE, 0, 0], [0, 1], None, [2, 2, 1, 1, 0, 0], false),
        ([APPROVE, 0, 0], [0, 1], Some(201), [2, 2, 1, 1, 0, 0], false),
        ([RELEASE, 2, 0], [0, 1], Some(203), [2, 2, 1, 1, 0, 0], false),
        ([APPROVE, 0, 0], [1, 1], None, [2, 2, 1, 1, 1, 0], false),
        ([RELEASE, 1, 0], [0, 1], Some(205), [2, 2, 1, 1, 1, 0], false),
        ([RELEASE, 2, 0], [0, 1], None, [2, 2, 2, 1, 1, 0], true),
        ([RELEASE, 2, 0], [0, 1], None, [2, 2, 2, 1, 1, 0], false),
        ([EDIT, 2, 0], [0, 1], Some(202), [2, 2, 2, 1, 1, 0], false),
    ];
    let mut release_id = None;
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
            // The release carries the reviewed, locked content and version.
            assert_eq!(result.outbox, [(300, vec![(140, 2), (141, 2)])]);
            let outgoing = application::pending(&db).unwrap().next.unwrap();
            assert_eq!((outgoing.channel, outgoing.ordinal), (300, 0));
            assert_eq!(
                (outgoing.destination_type, outgoing.payload_type),
                (103, 104)
            );
            assert_eq!(
                outgoing.destination,
                Value::text_ascii("publisher".to_owned())
                    .unwrap()
                    .canonical_bytes()
                    .unwrap()
            );
            let payload = Value::record_canonical(vec![
                Field::new(140, Value::signed(2)),
                Field::new(141, Value::signed(2)),
            ])
            .unwrap();
            assert_eq!(outgoing.payload, payload.canonical_bytes().unwrap());
            release_id = Some(outgoing.id);
        } else {
            assert!(result.outbox.is_empty());
        }
        let waiting = application::pending(&db).unwrap();
        assert_eq!(waiting.head.pending, u64::from(release_id.is_some()));
        assert_eq!(waiting.next.map(|outgoing| outgoing.id), release_id);
    }

    let release_id = release_id.expect("the journey emits one release");
    // The durable receiver deduplicates the retry after an acknowledgment loss.
    let mut receiver = LoseAcknowledgmentOnce {
        receiver: FileDestination::new(&received),
        lose_once: true,
    };
    assert!(application::deliver_to(&db, &mut receiver).is_err());
    assert_eq!(
        application::pending(&db).unwrap().next.unwrap().id,
        release_id
    );
    assert_eq!(
        std::fs::read_to_string(&received).unwrap().lines().count(),
        1
    );
    let delivered = application::deliver_to(&db, &mut receiver).unwrap();
    assert_eq!(delivered.deliveries, [release_id.clone()]);
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
    assert_eq!(
        application::current(&db).unwrap().state,
        [2, 2, 2, 1, 1, 0]
    );
    application::audit(&db).unwrap();
    println!("retained document-release journey: {}", directory.display());
}
