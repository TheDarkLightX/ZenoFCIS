//! Exact retained original compound schema through durable Authority publication.
#[path = "../../zeno-fcis-synthesis/tests/fixtures/g3-original-compound/fixture.rs"]
mod fixture;
use fixture::*;
use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::CommitStatus;
use zeno_fcis_shell_sqlite::v2::V2SqliteShell;
use zeno_fcis_synthesis::finite::v2_composition as c;

#[test]
fn original_twelve_types_survive_sqlite_commit_open_audit_and_idempotent_replay() {
    with_authority(|authority| {
        let path = std::env::temp_dir().join(format!(
            "zenofcis-original-compound-{}.db",
            std::process::id()
        ));
        let _file = ok(std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path));
        let initial = state(5, Some(12));
        let mut db = ok(V2SqliteShell::create(
            &path,
            authority,
            commit(authority.publish_genesis(&initial)),
        ));
        let context = wire(5, &[2]);
        let mut pre = initial;
        for (index, event_value) in [None, Some(0), Some(1_000_000)].into_iter().enumerate() {
            let command = wire(9, &event(event_value));
            let raw = c::Raw {
                state: &pre,
                command: &command,
                context: &context,
            };
            let expected = state(6, event_value);
            let publication = commit(authority.publish(raw));
            assert_eq!(publication.poststate(), expected);
            let key = Hash32::new([index as u8 + 1; 32]);
            assert_eq!(
                ok(db.commit(key, publication)).status(),
                CommitStatus::Committed
            );
            assert_eq!(
                ok(db.commit(key, commit(authority.publish(raw)))).status(),
                CommitStatus::IdempotentReplay
            );
            ok(db.audit());
            drop(db);
            db = ok(V2SqliteShell::open(&path, authority));
            assert_eq!(ok(db.snapshot()).state(), expected);
            assert_eq!(ok(db.snapshot()).version(), index as u64 + 1);
            pre = expected;
        }
        drop(db);
        ok(std::fs::remove_file(path));
    });
}
