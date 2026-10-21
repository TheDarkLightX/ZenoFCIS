use super::{
    model::{Cell, Database, Delivery},
    ok,
};
use rusqlite::{Connection, OptionalExtension, params, types::ValueRef};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zeno_fcis_codec::Hash32;

static NEXT: AtomicU64 = AtomicU64::new(0);
pub(crate) struct Files {
    pub(crate) store: PathBuf,
    pub(crate) receiver: PathBuf,
    _keepers: Vec<Connection>,
}
impl Files {
    // Named, separate SQLite databases survive handle reopens while their
    // exclusively owned keepers live. Real process-abort cases use new().
    pub(crate) fn memory() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let store = PathBuf::from(format!(
            "file:shell-simulation-{}-{id}?mode=memory&cache=shared",
            std::process::id()
        ));
        let receiver = PathBuf::from(format!(
            "file:shell-receiver-{}-{id}?mode=memory&cache=shared",
            std::process::id()
        ));
        let keepers = vec![
            ok(Connection::open(&store)),
            ok(Connection::open(&receiver)),
        ];
        Self {
            store,
            receiver,
            _keepers: keepers,
        }
    }
    pub(crate) fn new() -> Self {
        let base = std::env::temp_dir().join(format!(
            "shell-simulation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self {
            store: base.with_extension("db"),
            receiver: base.with_extension("receiver.db"),
            _keepers: vec![],
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        for path in [&self.store, &self.receiver] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}-journal", path.display()));
        }
    }
}
// A single read transaction covers every v2_* table, including empty tables.
// No store audit or cached Snapshot is used for this projection.
pub(crate) fn snapshot(path: &Path) -> Database {
    snapshot_matching(path, "v2_*")
}
pub(crate) fn snapshot_matching(path: &Path, pattern: &str) -> Database {
    let mut c = ok(Connection::open(path));
    let tx = ok(c.transaction());
    let names = ok(ok(tx.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name GLOB ?1 ORDER BY name",
    ))
    .query_map([pattern], |r| r.get::<_, String>(0)))
    .map(ok)
    .collect::<Vec<_>>();
    let mut tables = BTreeMap::new();
    for name in names {
        assert!(name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'));
        let columns = ok(tx.prepare(&format!("SELECT * FROM {name}"))).column_count();
        let order = (1..=columns)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = ok(tx.prepare(&format!("SELECT * FROM {name} ORDER BY {order}")));
        let rows = ok(statement.query_map([], |r| {
            (0..columns)
                .map(|i| {
                    Ok(match r.get_ref(i)? {
                        ValueRef::Integer(n) => Cell::Int(n),
                        ValueRef::Blob(b) => Cell::Bytes(b.to_vec()),
                        other => panic!("unexpected cell {other:?}"),
                    })
                })
                .collect::<Result<Vec<_>, rusqlite::Error>>()
        }))
        .map(ok)
        .collect();
        tables.insert(name, rows);
    }
    ok(tx.commit());
    Database(tables)
}
pub(crate) fn compare(expected: &Database, actual: &Database) -> Result<(), String> {
    // Checkpoints cache validated prefixes. Synchronization can save one
    // implicitly; that timing is connection-specific, not protocol authority.
    // Validate every actual marker against independently predicted history,
    // require every explicitly requested marker, then project this cache away.
    let mut projected = actual.clone();
    if let (Some(markers), Some(required), Some(commits)) = (
        actual.0.get("v2_checkpoints"),
        expected.0.get("v2_checkpoints"),
        expected.0.get("v2_commits"),
    ) {
        for marker in markers {
            let valid = required.contains(marker)
                || commits
                    .iter()
                    .any(|row| marker == &vec![row[0].clone(), row[3].clone(), row[5].clone()]);
            if !valid {
                return Err(format!(
                    "I1-I5 database mismatch: invalid checkpoint {marker:?}"
                ));
            }
        }
        if required.iter().any(|r| !markers.contains(r)) {
            return Err("I1-I5 database mismatch: missing explicit checkpoint".into());
        }
        projected
            .0
            .insert("v2_checkpoints".into(), required.clone());
    }
    let actual = &projected;
    if expected == actual {
        Ok(())
    } else {
        let difference = expected.0.iter().find_map(|(name, rows)| {
            let other = actual.0.get(name)?;
            if rows.len() != other.len() {
                return Some(format!(
                    "{name}: row counts {} / {}",
                    rows.len(),
                    other.len()
                ));
            }
            rows.iter()
                .zip(other)
                .enumerate()
                .find_map(|(i, (left, right))| {
                    left.iter().zip(right).enumerate().find_map(|(j, (x, y))| {
                        (x != y).then(|| format!("{name} row={i} column={j}: {x:?} / {y:?}"))
                    })
                })
        });
        Err(format!(
            "I1-I5 database mismatch: {difference:?}\nexpected={expected:?}\nactual={actual:?}"
        ))
    }
}
pub(crate) struct Receiver {
    connection: Connection,
}
impl Receiver {
    pub(crate) fn open(path: &Path) -> Self {
        let c = ok(Connection::open(path));
        ok(c.execute_batch("PRAGMA synchronous=EXTRA; CREATE TABLE IF NOT EXISTS dedup (id BLOB PRIMARY KEY, payload BLOB NOT NULL); CREATE TABLE IF NOT EXISTS effects (id BLOB NOT NULL, payload BLOB NOT NULL);"));
        Self { connection: c }
    }
    pub(crate) fn receive(&mut self, d: &Delivery, lose_reply: bool) {
        let tx = ok(self.connection.transaction());
        let previous: Option<Vec<u8>> = ok(tx
            .query_row(
                "SELECT payload FROM dedup WHERE id=?1",
                [d.id.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional());
        if let Some(payload) = previous {
            assert_eq!(payload, d.payload, "receiver ID collision");
        } else {
            ok(tx.execute(
                "INSERT INTO effects VALUES(?1,?2)",
                params![d.id.as_bytes().as_slice(), &d.payload],
            ));
            ok(tx.execute(
                "INSERT INTO dedup VALUES(?1,?2)",
                params![d.id.as_bytes().as_slice(), &d.payload],
            ));
        }
        ok(tx.commit());
        let _ = lose_reply;
    }
    pub(crate) fn unprotected_effect(&mut self, id: Hash32, payload: &[u8]) {
        ok(self.connection.execute(
            "INSERT INTO effects VALUES(?1,?2)",
            params![id.as_bytes().as_slice(), payload],
        ));
    }
    pub(crate) fn effect_count(&self, id: Hash32) -> i64 {
        ok(self.connection.query_row(
            "SELECT count(*) FROM effects WHERE id=?1",
            [id.as_bytes().as_slice()],
            |r| r.get(0),
        ))
    }
    pub(crate) fn state(&self) -> BTreeMap<Hash32, Vec<u8>> {
        let mut statement = ok(self
            .connection
            .prepare("SELECT id,payload FROM dedup ORDER BY id"));
        ok(statement.query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        }))
        .map(ok)
        .map(|(id, payload)| (Hash32::new(ok(id.as_slice().try_into())), payload))
        .collect()
    }
    pub(crate) fn invariants(&self) {
        let bad: i64 = ok(self.connection.query_row(
            "SELECT count(*) FROM (SELECT id FROM effects GROUP BY id HAVING count(*)>1)",
            [],
            |r| r.get(0),
        ));
        assert_eq!(bad, 0, "I6 duplicate receiver effect");
        let mismatches: i64 = ok(self.connection.query_row("SELECT (SELECT count(*) FROM effects e LEFT JOIN dedup d ON e.id=d.id AND e.payload=d.payload WHERE d.id IS NULL)+(SELECT count(*) FROM dedup d LEFT JOIN effects e ON e.id=d.id AND e.payload=d.payload WHERE e.id IS NULL)",[],|r|r.get(0)));
        assert_eq!(mismatches, 0, "I6 effect/dedup atomicity");
    }
}
