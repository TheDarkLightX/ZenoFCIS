//! Read retained schema v9/v10 records without replaying or migrating them.
//!
//! These are stored assertions, including their original identities and sealed
//! bytes. Schema recognition is the only validation here. Use the original
//! engine and lineage to validate replay; this report cannot construct a
//! checked snapshot, checkpoint, publication or writable store handle.
use super::{Error, check_schema, check_schema_v9};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, types::ValueRef};
use std::path::Path;

/// Caller-selected bounds for a read-only report, outside execution metering.
#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    /// Maximum total rows across every history table.
    pub rows: u64,
    /// Maximum total scalar payload bytes (integers count as eight bytes).
    pub bytes: u64,
}

/// One exact stored cell. No scalar width or Authority is inferred from it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoredValue {
    /// SQLite sequence, identifier or status value, retaining its original width.
    Integer(i64),
    /// Unchanged canonical bytes, identity, seal, certificate or hash.
    Bytes(Vec<u8>),
}

/// One named table's complete retained rows, in its original key order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredTable {
    /// Schema table name.
    pub name: &'static str,
    /// Exact column order corresponding to each row's cells.
    pub columns: Vec<String>,
    /// Owned copies of the stored cells.
    pub rows: Vec<Vec<StoredValue>>,
}

/// A coherent read transaction's stored assertions. This is not an audit receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredHistory {
    /// Retained schema version; reading never upgrades it.
    pub schema_version: i64,
    /// Genesis, state, commits, deliveries, checkpoints and (in v10) upgrades.
    pub tables: Vec<StoredTable>,
}

/// Reads an existing store with SQLite's read-only flag and a deferred transaction.
/// No database is created, no connection write settings are applied, and no
/// identity, publication, row or schema is rewritten. Bounds count returned
/// payloads; SQLite allocation and physical memory remain outside those bounds.
pub fn read(path: impl AsRef<Path>, limits: ReadLimits) -> Result<StoredHistory, Error> {
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let schema_version = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match schema_version {
        9 => check_schema_v9(&transaction)?,
        10 => check_schema(&transaction)?,
        other => return Err(Error::Schema(other)),
    }
    let mut tables = Vec::new();
    let mut rows_left = limits.rows;
    let mut bytes_left = limits.bytes;
    // All SQL identifiers and ordering clauses are fixed library data.
    for (name, query) in [
        ("v2_genesis", "SELECT * FROM v2_genesis ORDER BY singleton"),
        ("v2_state", "SELECT * FROM v2_state ORDER BY singleton"),
        ("v2_commits", "SELECT * FROM v2_commits ORDER BY sequence"),
        (
            "v2_deliveries",
            "SELECT * FROM v2_deliveries ORDER BY sequence,lane,ordinal",
        ),
        (
            "v2_checkpoints",
            "SELECT * FROM v2_checkpoints ORDER BY sequence",
        ),
        ("v2_upgrades", "SELECT * FROM v2_upgrades ORDER BY ordinal"),
    ] {
        if name == "v2_upgrades" && schema_version == 9 {
            continue;
        }
        let mut statement = transaction.prepare(query)?;
        let columns = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let count = statement.column_count();
        let mut cursor = statement.query([])?;
        let mut rows = Vec::new();
        while let Some(row) = cursor.next()? {
            rows_left = rows_left.checked_sub(1).ok_or(Error::Range)?;
            let mut cells = Vec::new();
            for index in 0..count {
                let cell = match row.get_ref(index)? {
                    ValueRef::Integer(value) => {
                        bytes_left = bytes_left.checked_sub(8).ok_or(Error::Range)?;
                        StoredValue::Integer(value)
                    }
                    ValueRef::Blob(bytes) => {
                        let len = u64::try_from(bytes.len()).map_err(|_| Error::Range)?;
                        bytes_left = bytes_left.checked_sub(len).ok_or(Error::Range)?;
                        StoredValue::Bytes(bytes.to_vec())
                    }
                    _ => return Err(Error::History),
                };
                cells.push(cell);
            }
            rows.push(cells);
        }
        tables.push(StoredTable {
            name,
            columns,
            rows,
        });
    }
    transaction.commit()?;
    Ok(StoredHistory {
        schema_version,
        tables,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn path() -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "scalar-history-{}-{}.sqlite",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
    fn ok<T, E: std::fmt::Debug>(value: Result<T, E>) -> T {
        value.unwrap_or_else(|error| panic!("history fixture: {error:?}"))
    }
    const LIMITS: ReadLimits = ReadLimits {
        rows: 100,
        bytes: 65536,
    };

    #[test]
    fn historical_identities_and_rows_read_without_mutating_or_migrating() {
        for version in [9, 10] {
            let path = path();
            let connection = ok(Connection::open(&path));
            ok(connection.execute_batch(&if version == 9 {
                super::super::schema_v9()
            } else {
                super::super::schema_v10()
            }));
            // Stored assertions deliberately need no current Authority. The
            // reader must preserve them, and must not claim to authenticate them.
            ok(connection.execute(
                "INSERT INTO v2_genesis VALUES(1,?1,?2,?3,?4,?5)",
                rusqlite::params![
                    b"historical-i64-identity".as_slice(),
                    b"original-state".as_slice(),
                    b"original-seal".as_slice(),
                    [7u8; 32].as_slice(),
                    [8u8; 32].as_slice()
                ],
            ));
            drop(connection);
            let before = ok(std::fs::read(&path));
            let mut report = ok(read(&path, LIMITS));
            assert_eq!(report.schema_version, version);
            assert_eq!(report.tables.len(), if version == 9 { 5 } else { 6 });
            assert_eq!(
                report.tables[0].rows[0][1],
                StoredValue::Bytes(b"historical-i64-identity".to_vec())
            );
            // Mutating a report cannot mutate retained state or later reads.
            report.tables[0].rows[0][1] = StoredValue::Bytes(vec![]);
            assert_ne!(
                ok(read(&path, LIMITS)).tables[0].rows[0][1],
                report.tables[0].rows[0][1]
            );
            assert!(matches!(
                read(&path, ReadLimits { rows: 0, ..LIMITS }),
                Err(Error::Range)
            ));
            assert!(matches!(
                read(&path, ReadLimits { bytes: 0, ..LIMITS }),
                Err(Error::Range)
            ));
            assert_eq!(ok(std::fs::read(&path)), before);
            ok(std::fs::remove_file(path));
        }
    }

    #[test]
    fn missing_store_is_not_created_and_unknown_schema_is_not_migrated() {
        let path = path();
        assert!(read(&path, LIMITS).is_err());
        assert!(!path.exists());
        drop(ok(Connection::open(&path)));
        let before = ok(std::fs::read(&path));
        assert!(matches!(read(&path, LIMITS), Err(Error::Schema(0))));
        assert_eq!(ok(std::fs::read(&path)), before);
        ok(std::fs::remove_file(path));
    }
}
