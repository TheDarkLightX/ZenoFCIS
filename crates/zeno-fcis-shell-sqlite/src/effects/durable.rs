//! SQLite stores opaque reservation bytes; policy validation stays in the core.

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::path::Path;
use zeno_fcis_shell::effects::durable::{Error, MAX_BYTES, StateStore};

/// One checkpoint row. Connections are exclusively owned; writes compare bytes.
pub struct SqliteStateStore {
    connection: Connection,
}

impl SqliteStateStore {
    /// Create a NEW lifetime in a new file. Existing paths are never overwritten.
    /// Supply bytes from `durable::initial`; never call this as recovery fallback.
    pub fn create(path: impl AsRef<Path>, initial: &[u8]) -> Result<Self, Error> {
        if initial.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path.as_ref())
            .map_err(|_| Error::Storage)?;
        let mut store = Self::connect(path.as_ref())?;
        let transaction = store.connection.transaction().map_err(|_| Error::Storage)?;
        transaction.execute_batch("CREATE TABLE effect_reservation (id INTEGER PRIMARY KEY CHECK (id = 1), bytes BLOB NOT NULL);").map_err(|_| Error::Storage)?;
        transaction
            .execute("INSERT INTO effect_reservation VALUES (1, ?1)", [initial])
            .map_err(|_| Error::Storage)?;
        transaction.commit().map_err(|_| Error::Storage)?;
        file.sync_all().map_err(|_| Error::Storage)?;
        // Persist the new directory entry as well as SQLite's committed bytes.
        let parent = path
            .as_ref()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| Error::Storage)?;
        Ok(store)
    }

    /// Open existing storage. Missing files/tables/rows are never initialized.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut store = Self::connect(path.as_ref())?;
        store.load()?;
        Ok(store)
    }

    fn connect(path: &Path) -> Result<Self, Error> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|_| Error::Storage)?;
        connection
            .execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")
            .map_err(|_| Error::Storage)?;
        Ok(Self { connection })
    }
}

impl StateStore for SqliteStateStore {
    fn load(&mut self) -> Result<Vec<u8>, Error> {
        // One read snapshot bounds allocation even if another connection writes.
        self.connection.query_row(
            "SELECT bytes FROM effect_reservation WHERE id = 1 AND typeof(bytes) = 'blob' AND length(bytes) <= ?1",
            [MAX_BYTES as i64], |r| r.get(0),
        ).optional().map_err(|_| Error::Storage)?.ok_or(Error::Missing)
    }

    fn commit(&mut self, expected: &[u8], next: &[u8]) -> Result<(), Error> {
        if next.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        let changed = self
            .connection
            .execute(
                "UPDATE effect_reservation SET bytes = ?1 WHERE id = 1 AND bytes = ?2",
                params![next, expected],
            )
            .map_err(|_| Error::Storage)?;
        if changed != 1 {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}
