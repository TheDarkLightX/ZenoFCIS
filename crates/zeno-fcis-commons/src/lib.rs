//! Rust implementation of the Python Zeno Commons library and store.
#![forbid(unsafe_code)]

mod canonical;
mod ops;

/// Command-line implementation used by the `zeno-fcis commons` command.
pub mod cli;
/// Stdio MCP implementation used by the `zeno-fcis commons mcp` command.
pub mod mcp;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub use canonical::canonical_json;
pub use ops::normalize_resource;

type CheckpointRow = (i64, i64, String, String, String, String, i64, i64, i64);

/// Store format identifier shared with the Python implementation.
pub const FORMAT: &str = "zeno-fcis/commons/1";
/// SQLite filename used by both implementations.
pub const DB_NAME: &str = "commons.sqlite3";
/// Maximum accepted text length, measured in Unicode scalar values.
pub const MAX_TEXT: usize = 65_536;
/// Maximum number of record tags.
pub const MAX_TAGS: usize = 32;
/// Default on-disk size budget for the live store.
pub const BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// Default upper bound for live events before archive is advised.
pub const MAX_LIVE_EVENTS: i64 = 50_000;
/// Default number of newest events retained by archive.
pub const KEEP_EVENTS: i64 = 20_000;
/// Format identifier stored in compressed archive segment headers.
pub const ARCHIVE_FORMAT: &str = "zeno-fcis/commons-archive/1";
/// Exact refusal returned when archive I/O needs the reference Python tool.
pub const ARCHIVE_REFUSAL: &str = "archives need the reference tool: python3 -I integrations/commons/cli.py (this build has no xz decoder)";
/// Record types accepted by both implementations.
pub const RECORD_TYPES: &[&str] = &[
    "hypothesis",
    "claim",
    "evidence",
    "refutation",
    "counterexample",
    "result",
    "gap",
    "lesson",
    "decision",
    "question",
    "answer",
    "risk",
    "note",
];
/// Record statuses accepted by both implementations.
pub const RECORD_STATUSES: &[&str] = &[
    "open",
    "testing",
    "supported",
    "refuted",
    "superseded",
    "stale",
    "answered",
    "resolved",
    "accepted",
    "rejected",
    "planned",
];
/// Agent lifecycle states accepted by both implementations.
pub const AGENT_STATES: &[&str] = &["working", "blocked", "idle", "done"];
/// Record-link relations accepted by both implementations.
pub const LINK_RELATIONS: &[&str] = &[
    "supports",
    "refutes",
    "answers",
    "supersedes",
    "cites",
    "duplicates",
    "blocks",
    "relates",
];
/// Record types included in negative-memory advice.
pub const ADVISE_TYPES: &[&str] = &[
    "lesson",
    "refutation",
    "counterexample",
    "gap",
    "risk",
    "decision",
];
/// Exact schema text used to initialize the shared store.
pub const SCHEMA: &str = "\nCREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS events(\n  seq INTEGER PRIMARY KEY AUTOINCREMENT, at REAL NOT NULL, actor TEXT NOT NULL, kind TEXT NOT NULL,\n  subject TEXT NOT NULL, body TEXT NOT NULL, prev TEXT NOT NULL, hash TEXT NOT NULL);\nCREATE TABLE IF NOT EXISTS leases(\n  resource TEXT PRIMARY KEY, owner TEXT NOT NULL, note TEXT NOT NULL, taken_seq INTEGER NOT NULL,\n  heartbeat_at REAL NOT NULL, expires_at REAL NOT NULL);\nCREATE TABLE IF NOT EXISTS agents(\n  name TEXT PRIMARY KEY, state TEXT NOT NULL, step TEXT NOT NULL, question TEXT NOT NULL,\n  updated_at REAL NOT NULL, read_through INTEGER NOT NULL DEFAULT 0);\nCREATE TABLE IF NOT EXISTS messages(\n  seq INTEGER PRIMARY KEY, sender TEXT NOT NULL, recipient TEXT NOT NULL, body TEXT NOT NULL, at REAL NOT NULL);\nCREATE TABLE IF NOT EXISTS records(\n  id TEXT PRIMARY KEY, type TEXT NOT NULL, status TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL,\n  tags TEXT NOT NULL, refs TEXT NOT NULL, author TEXT NOT NULL, created_seq INTEGER NOT NULL,\n  updated_seq INTEGER NOT NULL);\nCREATE TABLE IF NOT EXISTS identities(\n  id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, role TEXT NOT NULL,\n  registered_by TEXT NOT NULL, registered_seq INTEGER NOT NULL, registered_at REAL NOT NULL);\nCREATE TABLE IF NOT EXISTS links(\n  src TEXT NOT NULL, rel TEXT NOT NULL, dst TEXT NOT NULL, seq INTEGER NOT NULL, PRIMARY KEY(src, rel, dst));\nCREATE TABLE IF NOT EXISTS checkpoints(\n  last INTEGER PRIMARY KEY, first INTEGER NOT NULL, prev TEXT NOT NULL, head TEXT NOT NULL, segment TEXT NOT NULL,\n  sha256 TEXT NOT NULL, bytes INTEGER NOT NULL, events INTEGER NOT NULL, archived_seq INTEGER NOT NULL);\n";

/// A caller-visible refusal with the same message as the Python library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommonsError(String);

impl CommonsError {
    /// Create a refusal carrying `message`.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Return the refusal message.
    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommonsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CommonsError {}

impl From<rusqlite::Error> for CommonsError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<std::io::Error> for CommonsError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

/// One connection to a shared Commons store.
pub struct Commons {
    directory: PathBuf,
    actor: String,
    pub(crate) connection: Connection,
    clock: Arc<dyn Fn() -> f64 + Send + Sync>,
}

/// Whether an event projection commits or intentionally rolls back an idempotent request.
pub(crate) enum ApplyResult {
    Commit(Value),
    Rollback(Value),
}

impl Commons {
    /// Open or create the store directory for `actor`, using the system clock.
    pub fn open(directory: impl AsRef<Path>, actor: &str) -> Result<Self, CommonsError> {
        Self::open_with_clock(directory, actor, Arc::new(now))
    }

    /// Open or create a store with a supplied clock, primarily for deterministic callers and tests.
    pub fn open_with_clock(
        directory: impl AsRef<Path>,
        actor: &str,
        clock: Arc<dyn Fn() -> f64 + Send + Sync>,
    ) -> Result<Self, CommonsError> {
        let actor = validate_name(actor, "actor")?;
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;
        refuse_store_links(&directory)?;
        let real_directory = fs::canonicalize(&directory)?;
        let database_path = real_directory.join(DB_NAME);
        let connection = Connection::open(&database_path)?;
        connection.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
        connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA cell_size_check=ON;")?;
        let opened: String = connection.query_row("PRAGMA database_list", [], |row| row.get(2))?;
        if !opened.is_empty() && Path::new(&opened) != database_path {
            return Err(CommonsError::new(
                "the opened database is not the store's own file; refusing",
            ));
        }
        let foreign = {
            let mut statement = connection.prepare(
                "SELECT type, name FROM sqlite_master WHERE name NOT LIKE 'sqlite_autoindex_%'",
            )?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .filter(|(kind, name)| {
                    kind != "table"
                        || ![
                            "meta",
                            "events",
                            "leases",
                            "agents",
                            "messages",
                            "records",
                            "identities",
                            "links",
                            "checkpoints",
                            "sqlite_sequence",
                        ]
                        .contains(&name.as_str())
                })
                .collect::<Vec<_>>()
        };
        if !foreign.is_empty() {
            let listed = foreign
                .iter()
                .take(5)
                .map(|(kind, name)| format!("{kind} {name}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CommonsError::new(format!(
                "the store holds objects that are not Commons tables: {listed}"
            )));
        }
        connection.busy_timeout(Duration::from_millis(30_000))?;
        connection.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        connection.execute_batch("PRAGMA busy_timeout=30000; PRAGMA synchronous=NORMAL;")?;
        connection.execute_batch(SCHEMA)?;
        connection.execute(
            "INSERT OR IGNORE INTO meta(key, value) VALUES('format', ?1)",
            [FORMAT],
        )?;
        let found: String =
            connection.query_row("SELECT value FROM meta WHERE key='format'", [], |row| {
                row.get(0)
            })?;
        if found != FORMAT {
            return Err(CommonsError::new(format!(
                "store format {found} is not {FORMAT}"
            )));
        }
        Ok(Self {
            directory,
            actor,
            connection,
            clock,
        })
    }

    /// Return the actor bound to this connection.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// Return the directory holding this store.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Append one hash-chained event and update its projections in one immediate transaction.
    pub(crate) fn write_event<F>(
        &mut self,
        kind: &str,
        subject: &str,
        body: &Value,
        apply: F,
    ) -> Result<Value, CommonsError>
    where
        F: FnOnce(&rusqlite::Transaction<'_>, i64, f64) -> Result<ApplyResult, CommonsError>,
    {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let at = (self.clock)();
        let previous: Option<String> = tx
            .query_row(
                "SELECT hash FROM events ORDER BY seq DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let previous = previous.unwrap_or_else(|| "0".repeat(64));
        let digest = event_hash(&previous, at, &self.actor, kind, subject, body)?;
        let body_text = canonical_json(body)?;
        tx.execute(
            "INSERT INTO events(at, actor, kind, subject, body, prev, hash) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![at, self.actor, kind, subject, body_text, previous, digest],
        )?;
        let seq = tx.last_insert_rowid();
        match apply(&tx, seq, at)? {
            ApplyResult::Commit(result) => {
                tx.commit()?;
                Ok(result)
            }
            ApplyResult::Rollback(result) => {
                tx.rollback()?;
                Ok(result)
            }
        }
    }

    /// Recompute all event hashes and return the first broken sequence, if any.
    pub fn verify(&self) -> Result<Value, CommonsError> {
        self.verify_with(false, &[])
    }

    /// Recompute the live chain and optionally check archive segments and external anchors.
    pub fn verify_with(&self, archives: bool, anchors: &[Value]) -> Result<Value, CommonsError> {
        if archives {
            return Err(CommonsError::new(ARCHIVE_REFUSAL));
        }
        let checkpoints = self.checkpoints()?;
        let (mut previous, first) = if let Some(checkpoint) = checkpoints.last() {
            (checkpoint.3.clone(), checkpoint.1 + 1)
        } else {
            ("0".repeat(64), 1)
        };
        let start = first;
        let mut checked = 0_i64;
        let mut statement = self.connection.prepare(
            "SELECT seq, at, actor, kind, subject, body, prev, hash FROM events ORDER BY seq",
        )?;
        let mut rows = statement.query([])?;
        let mut seen = std::collections::BTreeMap::new();
        while let Some(row) = rows.next()? {
            let seq: i64 = row.get(0)?;
            let at: f64 = row.get(1)?;
            let actor: String = row.get(2)?;
            let kind: String = row.get(3)?;
            let subject: String = row.get(4)?;
            let body_text: String = row.get(5)?;
            let body: Value = serde_json::from_str(&body_text)
                .map_err(|error| CommonsError::new(error.to_string()))?;
            let stored_previous: String = row.get(6)?;
            let stored_hash: String = row.get(7)?;
            if seq != first + checked {
                return Ok(json!({
                    "ok": false,
                    "broken_at": seq,
                    "checked": checked,
                    "reason": format!("expected event {}", first + checked)
                }));
            }
            let expected = event_hash(&previous, at, &actor, &kind, &subject, &body)?;
            if stored_previous != previous || stored_hash != expected {
                return Ok(json!({
                    "ok": false,
                    "broken_at": seq,
                    "checked": checked,
                    "reason": "hash chain mismatch"
                }));
            }
            if kind == "store.archived" && !archived_event_matches(&self.connection, seq, &body)? {
                return Ok(json!({
                    "ok": false,
                    "broken_at": seq,
                    "checked": checked,
                    "reason": "an archive record disagrees with its checkpoint"
                }));
            }
            if anchors
                .iter()
                .any(|anchor| anchor["seq"].as_i64() == Some(seq))
            {
                seen.insert(seq, stored_hash.clone());
            }
            previous = stored_hash;
            checked += 1;
        }
        let top = first + checked - 1;
        let mut anchor_map = std::collections::BTreeMap::new();
        for anchor in anchors {
            anchor_map.insert(
                anchor["seq"]
                    .as_i64()
                    .ok_or_else(|| CommonsError::new("anchor seq must be an integer"))?,
                anchor["hash"]
                    .as_str()
                    .ok_or_else(|| CommonsError::new("anchor hash must be text"))?
                    .to_string(),
            );
        }
        for (seq, digest) in &anchor_map {
            if *seq > top {
                return Ok(json!({
                    "ok": false,
                    "broken_at": seq,
                    "checked": checked,
                    "reason": format!("anchor at event {seq} is past the newest event {top}: events were removed")
                }));
            }
            if *seq >= start && seen.get(seq) != Some(digest) {
                return Ok(json!({
                    "ok": false,
                    "broken_at": seq,
                    "checked": checked,
                    "reason": format!("anchor at event {seq} does not match: the history was rewritten")
                }));
            }
        }
        let anchors_checked = anchor_map.keys().filter(|seq| **seq >= start).count();
        Ok(json!({
            "ok": true,
            "checked": checked,
            "head": previous,
            "from_seq": start,
            "archives": 0,
            "anchors_checked": anchors_checked,
            "anchors_before_range": anchor_map.len() - anchors_checked
        }))
    }

    /// Return the newest event number and hash for an external anchor.
    pub fn head(&self) -> Result<Value, CommonsError> {
        let row = self
            .connection
            .query_row(
                "SELECT seq, hash FROM events ORDER BY seq DESC LIMIT 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(match row {
            Some((seq, hash)) => json!({"seq": seq, "hash": hash}),
            None => json!({"seq": 0, "hash": "0".repeat(64)}),
        })
    }

    fn checkpoints(&self) -> Result<Vec<CheckpointRow>, CommonsError> {
        let mut statement = self.connection.prepare(
            "SELECT first,last,prev,head,segment,sha256,bytes,events,archived_seq FROM checkpoints ORDER BY last",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CommonsError::from)
    }

    /// Return hash-chain events after `after`, filtered by a SQL LIKE kind prefix.
    pub fn events(
        &self,
        after: i64,
        kind_prefix: &str,
        limit: i64,
    ) -> Result<Vec<Value>, CommonsError> {
        let mut statement = self.connection.prepare(
            "SELECT seq, at, actor, kind, subject, body FROM events WHERE seq > ?1 AND kind LIKE ?2 ORDER BY seq LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![after, format!("{kind_prefix}%"), limit.clamp(1, 10_000)],
            |row| {
                let body_text: String = row.get(5)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    body_text,
                ))
            },
        )?;
        rows.map(|row| {
            let (seq, at, actor, kind, subject, body) = row?;
            let body: Value = serde_json::from_str(&body)
                .map_err(|error| CommonsError::new(error.to_string()))?;
            Ok(json!({"seq": seq, "at": iso(at), "actor": actor, "kind": kind, "subject": subject, "body": body}))
        })
        .collect()
    }

    /// Report a factory event and return its sequence number.
    pub fn emit(
        &mut self,
        part: &str,
        event: &str,
        subject: &str,
        body: Value,
    ) -> Result<i64, CommonsError> {
        let kind = format!(
            "factory.{}.{}",
            validate_name(part, "part")?,
            validate_name(event, "event")?
        );
        let body = if is_falsey(&body) { json!({}) } else { body };
        if canonical_json(&body)?.chars().count() > MAX_TEXT {
            return Err(CommonsError::new("event body too large"));
        }
        let subject: String = validate_text(subject, "subject", false)?
            .chars()
            .take(512)
            .collect();
        let output = self.write_event(&kind, &subject, &body, |_, seq, _| {
            Ok(ApplyResult::Commit(json!(seq)))
        })?;
        output
            .as_i64()
            .ok_or_else(|| CommonsError::new("event sequence is out of range"))
    }
}

fn archived_event_matches(
    connection: &Connection,
    seq: i64,
    body: &Value,
) -> Result<bool, CommonsError> {
    let Some(last) = body["last"].as_i64() else {
        return Ok(false);
    };
    let checkpoint = connection
        .query_row(
            "SELECT first,last,prev,head,segment,sha256,bytes,events,archived_seq \
             FROM checkpoints WHERE last=?1",
            [last],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            },
        )
        .optional()?;
    let Some((first, last, prev, head, segment, sha256, bytes, events, archived_seq)) = checkpoint
    else {
        return Ok(false);
    };
    Ok(body["first"].as_i64() == Some(first)
        && body["last"].as_i64() == Some(last)
        && body["prev"].as_str() == Some(prev.as_str())
        && body["head"].as_str() == Some(head.as_str())
        && body["segment"].as_str() == Some(segment.as_str())
        && body["sha256"].as_str() == Some(sha256.as_str())
        && body["bytes"].as_i64() == Some(bytes)
        && body["events"].as_i64() == Some(events)
        && archived_seq == seq)
}

fn refuse_store_links(directory: &Path) -> Result<(), CommonsError> {
    for name in [
        DB_NAME,
        "commons.sqlite3-wal",
        "commons.sqlite3-shm",
        "commons.sqlite3-journal",
        "archive",
    ] {
        if fs::symlink_metadata(directory.join(name))
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(CommonsError::new(format!(
                "{name} in the store is a symbolic link; refusing to open it"
            )));
        }
    }
    Ok(())
}

fn event_hash(
    previous: &str,
    at: f64,
    actor: &str,
    kind: &str,
    subject: &str,
    body: &Value,
) -> Result<String, CommonsError> {
    let payload = json!([at, actor, kind, subject, body]);
    let encoded = canonical_json(&payload)?;
    let mut hasher = Sha256::new();
    hasher.update(previous.as_bytes());
    hasher.update(encoded.as_bytes());
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    output
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

pub(crate) fn validate_name(value: &str, what: &str) -> Result<String, CommonsError> {
    let chars: Vec<char> = value.chars().collect();
    let newline_suffix = chars.last() == Some(&'\n');
    let core = if newline_suffix {
        &chars[..chars.len().saturating_sub(1)]
    } else {
        &chars
    };
    let valid = !core.is_empty()
        && core.len() <= 128
        && core[0].is_ascii_alphanumeric()
        && core[1..]
            .iter()
            .all(|character| character.is_ascii_alphanumeric() || "._:@/-".contains(*character));
    if valid {
        Ok(value.to_string())
    } else {
        Err(CommonsError::new(format!(
            "{what} must be 1-128 characters of letters, digits and ._:@/- and start with a letter or digit"
        )))
    }
}

pub(crate) fn validate_text(
    value: &str,
    what: &str,
    allow_empty: bool,
) -> Result<String, CommonsError> {
    if (!allow_empty && value.trim().is_empty()) || value.chars().count() > MAX_TEXT {
        return Err(CommonsError::new(format!(
            "{what} must be non-empty text of at most {MAX_TEXT} characters"
        )));
    }
    Ok(value.to_string())
}

pub(crate) fn is_falsey(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(value) => !value,
        Value::Number(value) => value.as_f64() == Some(0.0),
        Value::String(value) => value.is_empty(),
        Value::Array(value) => value.is_empty(),
        Value::Object(value) => value.is_empty(),
    }
}

pub(crate) fn iso(at: f64) -> String {
    let seconds = at.floor() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_epoch + 719_468;
    let era = shifted_days.div_euclid(146_097);
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn good<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
        match result {
            Ok(value) => value,
            Err(error) => panic!("unexpected error: {error:?}"),
        }
    }

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("zeno-commons-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        good(fs::create_dir_all(&path));
        path
    }

    #[test]
    fn canonical_json_matches_python_strings_and_sorts_nested_keys() {
        let value: Value = good(serde_json::from_str(
            r#"{"z":1e-7,"é":[true,null,"a/b"],"a":{"z":1.0,"a":-0.0},"big":1e20}"#,
        ));
        assert_eq!(
            good(canonical_json(&value)),
            r#"{"a":{"a":-0.0,"z":1.0},"big":1e+20,"z":1e-07,"é":[true,null,"a/b"]}"#
        );
    }

    #[test]
    fn canonical_json_matches_python_float_exponent_boundaries() {
        let value: Value = good(serde_json::from_str(
            "[1e-4,1e-5,1e-6,1e-7,1e15,1e16,1e20,1.2345678901234567,2.2250738585072014e-308,-0.0,1.0]",
        ));
        assert_eq!(
            good(canonical_json(&value)),
            "[0.0001,1e-05,1e-06,1e-07,1000000000000000.0,1e+16,1e+20,1.2345678901234567,2.2250738585072014e-308,-0.0,1.0]"
        );
    }

    #[test]
    fn store_uses_python_schema_format_and_hash_chain() {
        let path = temp_dir();
        let clock: Arc<dyn Fn() -> f64 + Send + Sync> = Arc::new(|| 1_800_000_000.25);
        let mut store = good(Commons::open_with_clock(&path, "test", clock));
        let seq = good(store.emit("test", "tick", "café", json!({"n": 1, "text": "雪"})));
        assert_eq!(seq, 1);
        let connection = good(Connection::open(path.join(DB_NAME)));
        let head: String = good(
            connection
                .query_row("SELECT hash FROM events WHERE seq=1", [], |row| row.get(0))
                .map_err(CommonsError::from),
        );
        assert_eq!(
            good(store.verify()),
            json!({
                "ok": true,
                "checked": 1,
                "head": head,
                "from_seq": 1,
                "archives": 0,
                "anchors_checked": 0,
                "anchors_before_range": 0
            })
        );
        let format: String = good(
            connection
                .query_row("SELECT value FROM meta WHERE key='format'", [], |row| {
                    row.get(0)
                })
                .map_err(CommonsError::from),
        );
        assert_eq!(format, FORMAT);
        let journal: String = good(
            connection
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(CommonsError::from),
        );
        assert_eq!(journal, "wal");
        drop(store);
        let _ = fs::remove_dir_all(path);
    }
}
