//! The tables a check runs, each folded into one SHA-256 the result keeps.
//!
//! A table is a sequence of records in enumeration order. It starts from the
//! SHA-256 of its tag; each record replaces the running digest with the
//! SHA-256 of the running digest followed by the record's bytes. A record is
//! the row's values, each an 8-byte big-endian two's-complement integer,
//! followed by what the check observed on that row. The digest therefore
//! binds every row, its order and every observation, in constant memory.

use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;

/// Which check a table records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableKind {
    /// Every local input: I, A and the Authority's outcome.
    Local,
    /// Every state of the state domain: I.
    StateFact,
    /// Every interface row: the provider facts and the consumer assumption.
    Interface,
    /// Every row of a group's check that its children's assumptions follow
    /// from the inner guarantees and its outer assumption.
    GroupAssumption,
    /// Every row of a group's check that its outer guarantee follows from
    /// its children's guarantees.
    GroupGuarantee,
}

impl TableKind {
    const fn tag(self) -> &'static [u8] {
        match self {
            Self::Local => b"ZFCIS-CHECKED-COMPOSITION-TABLE\0local\0",
            Self::StateFact => b"ZFCIS-CHECKED-COMPOSITION-TABLE\0state-fact\0",
            Self::Interface => b"ZFCIS-CHECKED-COMPOSITION-TABLE\0interface\0",
            Self::GroupAssumption => b"ZFCIS-CHECKED-COMPOSITION-TABLE\0group-assumption\0",
            Self::GroupGuarantee => b"ZFCIS-CHECKED-COMPOSITION-TABLE\0group-guarantee\0",
        }
    }
}

/// A table being recorded.
#[derive(Debug)]
pub(crate) struct Table {
    digest: [u8; 32],
    record: Vec<u8>,
    open: bool,
}

impl Table {
    pub(crate) fn new(kind: TableKind) -> Self {
        Self {
            digest: *RustCryptoSha256::hash(kind.tag()).as_bytes(),
            record: Vec::new(),
            open: false,
        }
    }

    /// Folds in the previous record, starts the next one with a row's values
    /// and returns its bytes for the check to extend.
    pub(crate) fn row(&mut self, values: &[i64]) -> &mut Vec<u8> {
        self.fold();
        for value in values {
            self.record.extend_from_slice(&value.to_be_bytes());
        }
        self.open = true;
        &mut self.record
    }

    /// The bytes of the current record.
    pub(crate) fn bytes(&mut self) -> &mut Vec<u8> {
        &mut self.record
    }

    fn fold(&mut self) {
        if self.open {
            self.digest = *RustCryptoSha256::hash_parts(&[&self.digest, &self.record]).as_bytes();
            self.record.clear();
            self.open = false;
        }
    }

    /// The table's digest after its last record.
    pub(crate) fn digest(mut self) -> [u8; 32] {
        self.fold();
        self.digest
    }
}
