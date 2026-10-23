//! Internal local-service SQLite shell. Only genuine ServicePublication commits.
//! This mutable handle owns storage and interruption controls, never decisions.
use std::path::Path;
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_contract::checked_composition::service::{
    ServiceAuthority, ServicePublication, ServiceRequest, ServiceState, ServiceUsage,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_shell_sqlite::sqlite::{Connection, OptionalExtension, TransactionBehavior, params};

const HEAD: &str = "CREATE TABLE service_head (id INTEGER PRIMARY KEY CHECK(id=0), identity BLOB NOT NULL, version INTEGER NOT NULL CHECK(version>=0), root BLOB NOT NULL, state BLOB NOT NULL, receipt BLOB NOT NULL, chain BLOB NOT NULL)";
const JOURNAL: &str = "CREATE TABLE service_journal (version INTEGER PRIMARY KEY CHECK(version>=0), principal TEXT NOT NULL, replay_key TEXT NOT NULL, request BLOB NOT NULL, receipt BLOB NOT NULL, state BLOB NOT NULL, chain BLOB NOT NULL, UNIQUE(principal,replay_key))";
const MAX_ROWS: u64 = 65536;
const MAX_JOURNAL_BYTES: i64 = 64 * 1024 * 1024;

// Commit-fence head fields: version, root, state, chain and service identity.
type HeadRow = (i64, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
// Stored journal: version, principal, replay key, request, receipt, state and chain.
type JournalRow = (u64, String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>);
#[derive(Debug)]
pub(crate) struct Error {
    pub(crate) reason: String,
    pub(crate) usage: Option<ServiceUsage>,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}
impl std::error::Error for Error {}
impl From<zeno_fcis_shell_sqlite::sqlite::Error> for Error {
    fn from(e: zeno_fcis_shell_sqlite::sqlite::Error) -> Self {
        failure(format!("sqlite:{e}"))
    }
}
fn failure(reason: impl Into<String>) -> Error {
    Error {
        reason: reason.into(),
        usage: None,
    }
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    *RustCryptoSha256::hash(bytes).as_bytes()
}
fn chain(previous: &[u8], receipt: &[u8]) -> [u8; 32] {
    let mut bytes = b"ZFCIS-SERVICE-CHAIN-1\0".to_vec();
    bytes.extend(previous);
    bytes.extend((receipt.len() as u64).to_be_bytes());
    bytes.extend(receipt);
    hash(&bytes)
}
#[derive(Debug)]
pub(crate) struct Snapshot {
    pub(crate) version: u64,
    pub(crate) state: ServiceState,
    pub(crate) root: [u8; 32],
}
#[derive(Debug)]
pub(crate) struct Committed {
    pub(crate) version: u64,
    pub(crate) receipt: Vec<u8>,
    pub(crate) recovered: bool,
    /// Actual current invocation, including when another attempt already committed.
    pub(crate) attempt_usage: Option<ServiceUsage>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    BeforeTransaction,
    AfterJournal,
    AfterHead,
    BeforeCommit,
    AfterCommit,
}
#[derive(Debug)]
pub(crate) struct Store {
    db: Connection,
    identity: [u8; 32],
}
impl Store {
    pub(crate) fn open(path: &Path, authority: &ServiceAuthority) -> Result<Self, Error> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let transaction = db.unchecked_transaction()?;
        for schema in [HEAD, JOURNAL] {
            transaction.execute_batch(&schema.replacen(
                "CREATE TABLE ",
                "CREATE TABLE IF NOT EXISTS ",
                1,
            ))?;
        }
        transaction.commit()?;
        let mut store = Self {
            db,
            identity: *authority.identity(),
        };
        store.schema()?;
        let count: i64 = store
            .db
            .query_row("SELECT COUNT(*) FROM service_head", [], |r| r.get(0))?;
        if count == 0 {
            let journal: i64 =
                store
                    .db
                    .query_row("SELECT COUNT(*) FROM service_journal", [], |r| r.get(0))?;
            if journal != 0 {
                return Err(failure("head-missing"));
            }
            let genesis = authority.genesis().map_err(|e| failure(e.reason))?;
            let receipt = genesis.receipt();
            let root = genesis.post_root();
            let state = genesis.post().bytes();
            let checksum = chain(&[0; 32], &receipt);
            let tx = store
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute(
                "INSERT INTO service_journal VALUES(0,'','',?1,?2,?3,?4)",
                params![Vec::<u8>::new(), &receipt, &state, &checksum[..]],
            )?;
            tx.execute(
                "INSERT INTO service_head VALUES(0,?1,0,?2,?3,?4,?5)",
                params![
                    &authority.identity()[..],
                    &root[..],
                    &state,
                    &receipt,
                    &checksum[..]
                ],
            )?;
            tx.commit()?;
        }
        store.audit(authority)?;
        Ok(store)
    }
    fn schema(&self) -> Result<(), Error> {
        let mut stmt=self.db.prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
        let values = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if values
            != vec![
                ("service_head".into(), HEAD.into()),
                ("service_journal".into(), JOURNAL.into()),
            ]
        {
            return Err(failure("store-schema"));
        }
        let extras:i64=self.db.query_row("SELECT COUNT(*) FROM sqlite_schema WHERE type IN ('trigger','view') OR (type='index' AND name NOT LIKE 'sqlite_autoindex_%')",[],|r|r.get(0))?;
        if extras != 0 {
            return Err(failure("store-extra-schema"));
        }
        Ok(())
    }
    pub(crate) fn snapshot(&self, authority: &ServiceAuthority) -> Result<Snapshot, Error> {
        let (identity,version,root,bytes):(Vec<u8>,i64,Vec<u8>,Vec<u8>)=self.db.query_row("SELECT identity,version,root,state FROM service_head WHERE id=0 AND length(state)<=65536 AND length(receipt)<=1048576",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
        let state = authority.decode_state(&bytes).map_err(failure)?;
        let computed = authority.root(&state).map_err(failure)?;
        if identity != authority.identity() || root != computed || version < 0 {
            return Err(failure("head-binding"));
        }
        Ok(Snapshot {
            version: version as u64,
            state,
            root: computed,
        })
    }
    pub(crate) fn recover(&self, request: &ServiceRequest) -> Result<Option<Committed>, Error> {
        let row:Option<(i64,Vec<u8>,Vec<u8>)>=self.db.query_row("SELECT version,request,receipt FROM service_journal WHERE principal=?1 AND replay_key=?2 AND length(receipt)<=1048576 AND length(request)<=65536",params![request.principal(),request.key()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        match row {
            Some((version, bytes, receipt)) => {
                if bytes != request.bytes() {
                    return Err(failure("replay-key-collision"));
                }
                Ok(Some(Committed {
                    version: version as u64,
                    receipt,
                    recovered: true,
                    attempt_usage: None,
                }))
            }
            None => Ok(None),
        }
    }
    pub(crate) fn execute(
        &mut self,
        authority: &ServiceAuthority,
        request: &ServiceRequest,
    ) -> Result<Committed, Error> {
        if &self.identity != authority.identity() {
            return Err(failure("service-identity"));
        }
        if let Some(original) = self.recover(request)? {
            return Ok(original);
        }
        let pre = self.snapshot(authority)?;
        let publication = authority
            .decide(&pre.state, request, pre.version)
            .map_err(|e| Error {
                reason: e.reason,
                usage: Some(e.usage),
            })?;
        self.commit(authority, publication)
    }
    pub(crate) fn commit(
        &mut self,
        authority: &ServiceAuthority,
        publication: ServicePublication,
    ) -> Result<Committed, Error> {
        self.commit_at(authority, publication, None, false)
    }
    // Deterministic crash controls are shell-only and never grant publication authority.
    pub(crate) fn commit_at(
        &mut self,
        authority: &ServiceAuthority,
        publication: ServicePublication,
        fault: Option<Fault>,
        abort: bool,
    ) -> Result<Committed, Error> {
        let usage = publication.usage();
        let result = (|| -> Result<Committed, Error> {
            let request = publication
                .request()
                .ok_or_else(|| failure("not-step-publication"))?;
            if publication.identity() != authority.identity()
                || &self.identity != authority.identity()
            {
                return Err(failure("service-identity"));
            }
            if let Some(original) = self.recover(request)? {
                return Ok(original);
            }
            let interrupt = |point| -> Result<(), Error> {
                if fault == Some(point) {
                    if abort {
                        std::process::abort();
                    }
                    return Err(failure(format!("injected:{point:?}")));
                }
                Ok(())
            };
            interrupt(Fault::BeforeTransaction)?;
            let receipt = publication.receipt();
            let post = publication.post().bytes();
            let next = publication
                .version()
                .checked_add(1)
                .ok_or_else(|| failure("version-overflow"))?;
            if next > MAX_ROWS {
                return Err(failure("journal-budget"));
            }
            let tx = self
                .db
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current: Option<HeadRow> = tx
                .query_row(
                    "SELECT version,root,state,chain,identity FROM service_head WHERE id=0",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let (version, root, state, previous, identity) =
                current.ok_or_else(|| failure("head-missing"))?;
            if version < 0
                || version as u64 != publication.version()
                || root != publication.pre_root()
                || state != publication.pre().bytes()
                || identity != authority.identity()
            {
                return Err(failure("compare-and-swap"));
            }
            let retained:i64=tx.query_row("SELECT COALESCE(SUM(length(request)+length(receipt)+length(state)+length(chain)),0) FROM service_journal",[],|r|r.get(0))?;
            let added = i64::try_from(request.bytes().len() + receipt.len() + post.len() + 32)
                .map_err(|_| failure("journal-byte-budget"))?;
            if retained
                .checked_add(added)
                .is_none_or(|n| n > MAX_JOURNAL_BYTES)
            {
                return Err(failure("journal-byte-budget"));
            }
            let checksum = chain(&previous, &receipt);
            tx.execute(
                "INSERT INTO service_journal VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![
                    next as i64,
                    request.principal(),
                    request.key(),
                    request.bytes(),
                    &receipt,
                    &post,
                    &checksum[..]
                ],
            )?;
            interrupt(Fault::AfterJournal)?;
            let changed=tx.execute("UPDATE service_head SET version=?1,root=?2,state=?3,receipt=?4,chain=?5 WHERE id=0 AND version=?6 AND root=?7",params![next as i64,&publication.post_root()[..],&post,&receipt,&checksum[..],publication.version() as i64,&publication.pre_root()[..]])?;
            if changed != 1 {
                return Err(failure("compare-and-swap"));
            }
            interrupt(Fault::AfterHead)?;
            interrupt(Fault::BeforeCommit)?;
            tx.commit()?;
            interrupt(Fault::AfterCommit)?;
            Ok(Committed {
                version: next,
                receipt,
                recovered: false,
                attempt_usage: None,
            })
        })();
        result
            .map(|mut committed| {
                committed.attempt_usage = Some(usage);
                committed
            })
            .map_err(|mut error| {
                error.usage = Some(usage);
                error
            })
    }
    fn rows(&self) -> Result<Vec<JournalRow>, Error> {
        let count: i64 = self
            .db
            .query_row("SELECT COUNT(*) FROM service_journal", [], |r| r.get(0))?;
        let oversized:i64=self.db.query_row("SELECT COUNT(*) FROM service_journal WHERE length(request)>65536 OR length(receipt)>1048576 OR length(state)>65536 OR length(chain)!=32",[],|r|r.get(0))?;
        let total:i64=self.db.query_row("SELECT COALESCE(SUM(length(request)+length(receipt)+length(state)+length(chain)),0) FROM service_journal",[],|r|r.get(0))?;
        if count <= 0 || count as u64 > MAX_ROWS + 1 || oversized != 0 || total > MAX_JOURNAL_BYTES
        {
            return Err(failure("journal-shape-or-budget"));
        }
        let mut stmt=self.db.prepare("SELECT version,principal,replay_key,request,receipt,state,chain FROM service_journal ORDER BY version")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
    pub(crate) fn audit(&self, authority: &ServiceAuthority) -> Result<(), Error> {
        self.schema()?;
        let genesis = authority.genesis().map_err(|e| failure(e.reason))?;
        let mut state = genesis.post().clone();
        let mut previous = vec![0; 32];
        let mut last_receipt = Vec::new();
        let mut version = 0;
        let mut keys = std::collections::BTreeSet::new();
        for (at, (number, principal, key, request, receipt, post, stored_chain)) in
            self.rows()?.into_iter().enumerate()
        {
            if number != at as u64 {
                return Err(failure("journal-sequence"));
            }
            let expected = if at == 0 {
                if !principal.is_empty() || !key.is_empty() || !request.is_empty() {
                    return Err(failure("genesis-journal"));
                }
                authority.genesis().map_err(|e| failure(e.reason))?
            } else {
                let request = authority.decode_request(&request).map_err(failure)?;
                if request.principal() != principal
                    || request.key() != key
                    || !keys.insert((principal, key))
                {
                    return Err(failure("journal-replay-binding"));
                }
                authority
                    .decide(&state, &request, version)
                    .map_err(|e| failure(format!("journal-decision:{}", e.reason)))?
            };
            // Exact comparison detects any missing, substituted or reordered core root.
            if !expected.matches_receipt(&receipt)
                || receipt != expected.receipt()
                || post != expected.post().bytes()
            {
                return Err(failure("receipt-replay"));
            }
            let checksum = chain(&previous, &receipt);
            if stored_chain != checksum {
                return Err(failure("receipt-chain"));
            }
            previous = stored_chain;
            last_receipt = receipt;
            state = expected.post().clone();
            version = number;
        }
        let head = self.snapshot(authority)?;
        let (receipt, checksum): (Vec<u8>, Vec<u8>) = self.db.query_row(
            "SELECT receipt,chain FROM service_head WHERE id=0",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if head.version != version
            || head.state != state
            || receipt != last_receipt
            || checksum != previous
        {
            return Err(failure("head-journal-mismatch"));
        }
        Ok(())
    }
    pub(crate) fn replay_into(
        &self,
        authority: &ServiceAuthority,
        target: &mut Store,
    ) -> Result<u64, Error> {
        self.audit(authority)?;
        if target.snapshot(authority)?.version != 0 {
            return Err(failure("replay-target-not-fresh"));
        }
        let mut count = 0;
        for (version, _, _, request, receipt, _, _) in self.rows()?.into_iter().skip(1) {
            let request = authority.decode_request(&request).map_err(failure)?;
            let actual = target.execute(authority, &request)?;
            if actual.version != version || actual.receipt != receipt {
                return Err(failure("journal-replay-difference"));
            }
            count += 1;
        }
        target.audit(authority)?;
        Ok(count)
    }
}
