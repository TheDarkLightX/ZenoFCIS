//! Independent protocol transitions. No store, authority, or reference-model calls.
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_codec::{CanonicalEncode, Domain, Envelope, Hash32, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_value::{Field, Value};

use super::ok;

#[derive(Clone, Eq, PartialEq)]
pub(crate) enum Cell {
    Int(i64),
    Bytes(Vec<u8>),
}
impl std::fmt::Debug for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Bytes(b) => {
                if b.len() <= 48 {
                    write!(f, "{b:02x?}")
                } else {
                    write!(
                        f,
                        "blob(len={}, sha256={:02x?})",
                        b.len(),
                        super::payload_hash(b).as_bytes()
                    )
                }
            }
        }
    }
}
pub(crate) type Row = Vec<Cell>;
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Database(pub(crate) BTreeMap<String, Vec<Row>>);
impl Database {
    pub(crate) fn canonical_bytes(&self) -> Vec<u8> {
        let mut b = Vec::new();
        for (name, rows) in &self.0 {
            b.extend_from_slice(&(name.len() as u64).to_be_bytes());
            b.extend_from_slice(name.as_bytes());
            b.extend_from_slice(&(rows.len() as u64).to_be_bytes());
            for row in rows {
                b.extend_from_slice(&(row.len() as u64).to_be_bytes());
                for cell in row {
                    match cell {
                        Cell::Int(n) => {
                            b.push(0);
                            b.extend_from_slice(&n.to_be_bytes());
                        }
                        Cell::Bytes(v) => {
                            b.push(1);
                            b.extend_from_slice(&(v.len() as u64).to_be_bytes());
                            b.extend_from_slice(v);
                        }
                    }
                }
            }
        }
        b
    }
}
pub(crate) fn int(n: impl Into<i64>) -> Cell {
    Cell::Int(n.into())
}
pub(crate) fn bytes(b: impl AsRef<[u8]>) -> Cell {
    Cell::Bytes(b.as_ref().to_vec())
}
pub(crate) fn hash(domain: Domain<'static>, b: &[u8]) -> Hash32 {
    ok(commitment::<RustCryptoSha256>(domain, b))
}
pub(crate) fn framed(domain: Domain<'static>, parts: &[&[u8]]) -> Hash32 {
    let mut b = Vec::new();
    for part in parts {
        b.extend_from_slice(&(part.len() as u64).to_be_bytes());
        b.extend_from_slice(part);
    }
    hash(domain, &b)
}
pub(crate) fn envelope(root: u32, schema: Hash32, value: Value) -> Vec<u8> {
    ok(Envelope::new(root, schema, value).canonical_bytes())
}
pub(crate) fn state(schema: Hash32, counts: [i128; 2]) -> Vec<u8> {
    envelope(100, schema, record(110, counts))
}
fn record(first: u16, counts: [i128; 2]) -> Value {
    ok(Value::record_canonical(vec![
        Field::new(first, Value::signed(counts[0])),
        Field::new(first + 1, Value::signed(counts[1])),
    ]))
}
// The storage format removes the identity field from the nested subject.
// This encoder is intentionally independent of the shell's compact module.
pub(crate) fn compact(identity: &[u8], subject: &[u8]) -> Vec<u8> {
    let inner_len = u128::from_be_bytes(ok(subject[52..68].try_into())) as usize;
    let inner = &subject[68..68 + inner_len];
    assert_eq!(&inner[68..68 + identity.len()], identity);
    let mut b = subject[..52].to_vec();
    b.extend_from_slice(&((inner_len - 17 - identity.len()) as u128).to_be_bytes());
    b.extend_from_slice(&inner[..51]);
    b.extend_from_slice(&inner[68 + identity.len()..]);
    b.extend_from_slice(&subject[68 + inner_len..]);
    b
}
#[derive(Clone, Debug)]
pub(crate) struct Genesis {
    pub(crate) schema: Hash32,
    pub(crate) identity: Vec<u8>,
    pub(crate) subject: Vec<u8>,
    pub(crate) interpreter: Hash32,
    pub(crate) entries: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Input {
    pub(crate) key: Hash32,
    pub(crate) before: [i128; 2],
    pub(crate) variant: u16,
    pub(crate) command: Vec<u8>,
    pub(crate) context: Vec<u8>,
    pub(crate) subject: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Refusal {
    Domain,
    Crash(u8),
    Busy,
    Concurrent,
    PreState,
    Replay,
    Unknown,
    AlreadyAcknowledged,
    PayloadMismatch,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Committed(u64, Hash32),
    Replay(u64, Hash32),
    Refused(Refusal),
    Observed,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Delivery {
    pub(crate) sequence: u64,
    pub(crate) ordinal: u32,
    pub(crate) id: Hash32,
    pub(crate) payload: Vec<u8>,
    pub(crate) row: Row,
    pub(crate) acknowledged: bool,
}
#[derive(Clone, Debug)]
struct Commit {
    input: Input,
    after: [i128; 2],
    chain: Hash32,
    row: Row,
    deliveries: Vec<Delivery>,
}
#[derive(Clone, Debug)]
pub(crate) struct Model {
    pub(crate) genesis: Genesis,
    pub(crate) counts: [i128; 2],
    pub(crate) root: Hash32,
    pub(crate) chain: Hash32,
    history: Vec<Commit>,
    checkpoints: BTreeMap<u64, (Hash32, Hash32)>,
    pub(crate) deliveries: Vec<Delivery>,
    transaction: Option<Commit>,
    pub(crate) receiver: BTreeMap<Hash32, Vec<u8>>,
}
impl Model {
    pub(crate) fn new(genesis: Genesis) -> Self {
        let root = hash(domains::V2_STATE, &state(genesis.schema, [0, 0]));
        let chain = framed(
            domains::V2_GENESIS,
            &[
                &genesis.identity,
                &genesis.subject,
                root.as_bytes(),
                genesis.interpreter.as_bytes(),
            ],
        );
        Self {
            genesis,
            counts: [0, 0],
            root,
            chain,
            history: Vec::new(),
            checkpoints: BTreeMap::from([(0, (root, chain))]),
            deliveries: Vec::new(),
            transaction: None,
            receiver: BTreeMap::new(),
        }
    }
    pub(crate) fn audit(&mut self) {
        self.checkpoints
            .insert(self.sequence(), (self.root, self.chain));
    }
    pub(crate) fn sequence(&self) -> u64 {
        self.history.len() as u64
    }
    pub(crate) fn old_input(&self, choice: u64) -> Option<Input> {
        if self.history.is_empty() {
            None
        } else {
            Some(
                self.history[choice as usize % self.history.len()]
                    .input
                    .clone(),
            )
        }
    }
    fn prepare(&self, input: Input) -> Commit {
        let sequence = self.sequence() + 1;
        let mut after = input.before;
        after[usize::from(input.variant == 121)] += 1;
        assert!(after.into_iter().all(|n| n <= 3));
        let post = state(self.genesis.schema, after);
        let root = hash(domains::V2_STATE, &post);
        let mut cert = b"ZFCISV2-CERT\0".to_vec();
        cert.extend_from_slice(&sequence.to_be_bytes());
        for h in [
            input.key,
            self.root,
            root,
            self.chain,
            hash(domains::V2_PUBLICATION, &input.subject),
        ] {
            cert.extend_from_slice(h.as_bytes());
        }
        let chain = hash(domains::V2_CHAIN, &cert);
        let candidate = framed(domains::V2_PUBLICATION, &[&input.subject, &cert]);
        let destination = ok(ok(Value::text_ascii("local-observer".into())).canonical_bytes());
        let payload = ok(record(112, after).canonical_bytes());
        let marker = ok(Value::unsigned(0).canonical_bytes());
        let deliveries = (0..self.genesis.entries)
            .map(|ordinal| {
                let mut entry = ordinal.to_be_bytes().to_vec();
                entry.extend_from_slice(&300_u32.to_be_bytes());
                for b in [&destination, &payload] {
                    entry.extend_from_slice(&(b.len() as u32).to_be_bytes());
                    entry.extend_from_slice(b);
                }
                let mut preimage = candidate.as_bytes().to_vec();
                preimage.extend_from_slice(&entry);
                let id = hash(domains::DELIVERY, &preimage);
                let entry_hash = hash(domains::OUTBOX_ENTRY, &entry);
                let row = vec![
                    int(sequence as i64),
                    int(1),
                    int(ordinal),
                    int(300),
                    int(103),
                    int(104),
                    bytes(&destination),
                    bytes(&payload),
                    bytes(&marker),
                    bytes(id.as_bytes()),
                    bytes(entry_hash.as_bytes()),
                    int(0),
                ];
                Delivery {
                    sequence,
                    ordinal,
                    id,
                    payload: payload.clone(),
                    row,
                    acknowledged: false,
                }
            })
            .collect();
        let row = vec![
            int(sequence as i64),
            bytes(input.key.as_bytes()),
            bytes(self.root.as_bytes()),
            bytes(root.as_bytes()),
            bytes(self.chain.as_bytes()),
            bytes(chain.as_bytes()),
            bytes(&input.command),
            bytes(&input.context),
            bytes(post),
            bytes(compact(&self.genesis.identity, &input.subject)),
            bytes(cert),
        ];
        Commit {
            input,
            after,
            chain,
            row,
            deliveries,
        }
    }
    pub(crate) fn commit(&mut self, input: Input, fault: u8, expected: Option<u64>) -> Outcome {
        use Refusal::*;
        // Business rejection occurs before a committing capability exists.
        if input.before[usize::from(input.variant == 121)] == 3 {
            return Outcome::Refused(Domain);
        }
        if fault == 1 {
            return Outcome::Refused(Crash(1));
        }
        if fault == 8 {
            return Outcome::Refused(Busy);
        }
        if expected.is_some_and(|e| e != self.sequence()) {
            return Outcome::Refused(Concurrent);
        }
        if let Some(previous) = self.history.iter().find(|c| c.input.key == input.key) {
            return if previous.input == input {
                Outcome::Replay(previous.deliveries[0].sequence, previous.chain)
            } else {
                Outcome::Refused(Replay)
            };
        }
        if input.before != self.counts {
            return Outcome::Refused(PreState);
        }
        self.transaction = Some(self.prepare(input));
        if (2..=6).contains(&fault) {
            self.transaction = None;
            return Outcome::Refused(Crash(fault));
        }
        let c = self
            .transaction
            .take()
            .unwrap_or_else(|| panic!("missing model transaction"));
        self.counts = c.after;
        self.root = hash(domains::V2_STATE, &state(self.genesis.schema, c.after));
        self.chain = c.chain;
        self.deliveries.extend(c.deliveries.clone());
        self.history.push(c);
        if fault == 7 {
            Outcome::Refused(Crash(7))
        } else {
            Outcome::Committed(self.sequence(), self.chain)
        }
    }
    pub(crate) fn acknowledge(&mut self, id: Hash32, payload_hash: Hash32) -> Outcome {
        let Some(d) = self.deliveries.iter_mut().find(|d| d.id == id) else {
            return Outcome::Refused(Refusal::Unknown);
        };
        if d.acknowledged {
            return Outcome::Refused(Refusal::AlreadyAcknowledged);
        }
        if super::payload_hash(&d.payload) != payload_hash {
            return Outcome::Refused(Refusal::PayloadMismatch);
        }
        d.acknowledged = true;
        d.row[11] = int(1);
        Outcome::Observed
    }
    pub(crate) fn database(&self) -> Database {
        let g = &self.genesis;
        let initial = state(g.schema, [0, 0]);
        let (_, chain) = self.checkpoints[&0];
        Database(BTreeMap::from([
            (
                "v2_genesis".into(),
                vec![vec![
                    int(1),
                    bytes(&g.identity),
                    bytes(initial),
                    bytes(compact(&g.identity, &g.subject)),
                    bytes(g.interpreter.as_bytes()),
                    bytes(chain.as_bytes()),
                ]],
            ),
            (
                "v2_state".into(),
                vec![vec![
                    int(1),
                    int(self.sequence() as i64),
                    bytes(state(g.schema, self.counts)),
                    bytes(self.root.as_bytes()),
                    bytes(self.chain.as_bytes()),
                ]],
            ),
            (
                "v2_commits".into(),
                self.history.iter().map(|c| c.row.clone()).collect(),
            ),
            (
                "v2_deliveries".into(),
                self.deliveries.iter().map(|d| d.row.clone()).collect(),
            ),
            (
                "v2_checkpoints".into(),
                self.checkpoints
                    .iter()
                    .map(|(seq, (root, chain))| {
                        vec![
                            int(*seq as i64),
                            bytes(root.as_bytes()),
                            bytes(chain.as_bytes()),
                        ]
                    })
                    .collect(),
            ),
            ("v2_upgrades".into(), vec![]),
        ]))
    }
    pub(crate) fn invariants(&self) {
        let mut folded = [0, 0];
        let mut keys = BTreeSet::new();
        for c in &self.history {
            assert_eq!(c.input.before, folded, "I1");
            folded[usize::from(c.input.variant == 121)] += 1;
            assert_eq!(c.after, folded, "I1");
            assert!(keys.insert(c.input.key), "I2");
            assert_eq!(c.deliveries.len(), self.genesis.entries as usize, "I3");
        }
        assert_eq!(self.counts, folded, "I1");
        assert!(self.transaction.is_none(), "I5");
        assert_eq!(
            self.deliveries.len(),
            self.history.len() * self.genesis.entries as usize,
            "I3/I5"
        );
    }
}
