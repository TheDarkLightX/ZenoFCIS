//! Pure reference semantics for atomic expected-root commit and outbox replay.
//!
//! This crate models what a concrete database/network shell must refine. It
//! does not perform I/O. Every operation returns a new immutable shell state or
//! an error, so partial publication is unrepresentable in the reference model.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_patch::{PatchError, hash_value};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_receipt::{CandidateId, CommitBundle, Receipt, SealError};
use zeno_fcis_value::Value;

/// Replay-key binding stored by the atomic shell.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReplayRecord {
    replay_id: Hash32,
    candidate_id: CandidateId,
}

impl ReplayRecord {
    /// Returns the replay identity.
    #[must_use]
    pub const fn replay_id(self) -> Hash32 {
        self.replay_id
    }

    /// Returns the bound candidate.
    #[must_use]
    pub const fn candidate_id(self) -> CandidateId {
        self.candidate_id
    }
}

/// One committed external-delivery record, including exact delivery data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxRecord {
    candidate_id: CandidateId,
    ordinal: u32,
    delivery_id: Hash32,
    entry_hash: Hash32,
    entry: OutboxEntry,
    acknowledged: bool,
}

impl OutboxRecord {
    /// Returns the candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the entry ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Returns the idempotent delivery identity.
    #[must_use]
    pub const fn delivery_id(&self) -> Hash32 {
        self.delivery_id
    }

    /// Returns the exact outbox-entry commitment.
    #[must_use]
    pub const fn entry_hash(&self) -> Hash32 {
        self.entry_hash
    }

    /// Returns the exact committed delivery obligation.
    #[must_use]
    pub const fn entry(&self) -> &OutboxEntry {
        &self.entry
    }

    /// Returns whether destination acknowledgement was recorded.
    #[must_use]
    pub const fn acknowledged(&self) -> bool {
        self.acknowledged
    }
}

/// Immutable authoritative shell state.
///
/// The semantic state, root, replay record, receipt, complete bundle, and
/// outbox records change together in the reference transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellState {
    state: Value,
    root: Hash32,
    replay: Box<[ReplayRecord]>,
    receipts: Box<[(CandidateId, Receipt)]>,
    bundles: Box<[(CandidateId, CommitBundle)]>,
    outbox: Box<[OutboxRecord]>,
}

impl ShellState {
    /// Creates an empty reference shell and derives its authoritative root.
    pub fn new<H: CommitmentHasher>(
        state: Value,
        state_domain: Domain<'_>,
    ) -> Result<Self, ShellError> {
        let root = hash_value::<H>(state_domain, &state).map_err(ShellError::State)?;
        Ok(Self {
            state,
            root,
            replay: Vec::new().into_boxed_slice(),
            receipts: Vec::new().into_boxed_slice(),
            bundles: Vec::new().into_boxed_slice(),
            outbox: Vec::new().into_boxed_slice(),
        })
    }

    /// Returns the committed semantic state.
    #[must_use]
    pub const fn state(&self) -> &Value {
        &self.state
    }

    /// Returns the current authoritative root.
    #[must_use]
    pub const fn root(&self) -> Hash32 {
        self.root
    }

    /// Returns canonical replay records.
    #[must_use]
    pub fn replay_records(&self) -> &[ReplayRecord] {
        &self.replay
    }

    /// Returns canonical receipts.
    #[must_use]
    pub fn receipts(&self) -> &[(CandidateId, Receipt)] {
        &self.receipts
    }

    /// Returns canonical complete bundles.
    #[must_use]
    pub fn bundles(&self) -> &[(CandidateId, CommitBundle)] {
        &self.bundles
    }

    /// Returns canonical outbox records.
    #[must_use]
    pub fn outbox_records(&self) -> &[OutboxRecord] {
        &self.outbox
    }

    /// Returns the first unacknowledged outbox item in candidate/ordinal order.
    #[must_use]
    pub fn next_pending(&self) -> Option<&OutboxRecord> {
        self.outbox.iter().find(|record| !record.acknowledged)
    }
}

/// Result of atomic commit admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitResult {
    state: ShellState,
    status: CommitStatus,
}

impl CommitResult {
    /// Returns the successor reference state.
    #[must_use]
    pub const fn state(&self) -> &ShellState {
        &self.state
    }

    /// Returns whether this invocation committed or replayed idempotently.
    #[must_use]
    pub const fn status(&self) -> CommitStatus {
        self.status
    }

    /// Consumes the result.
    #[must_use]
    pub fn into_state(self) -> ShellState {
        self.state
    }
}

/// Atomic commit outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitStatus {
    /// The complete bundle was newly published.
    Committed,
    /// The replay identity was already bound to this exact complete bundle.
    IdempotentReplay,
}

/// Applies one structurally valid bundle in the executable reference model.
///
/// This function does not establish catalog, invocation, provider, interpreter,
/// deployment, invariant, or conservation authority. Production commit ports
/// must accept a nominal authorization value from a higher dependency ring.
pub fn apply_reference_bundle<H: CommitmentHasher>(
    shell: &ShellState,
    state_domain: Domain<'_>,
    replay_id: Hash32,
    bundle: &CommitBundle,
) -> Result<CommitResult, ShellError> {
    if let Ok(index) = shell
        .replay
        .binary_search_by_key(&replay_id, |record| record.replay_id)
    {
        let existing = shell.replay[index];
        if existing.candidate_id != bundle.candidate_id() {
            return Err(ShellError::ReplayConflict {
                replay_id,
                existing: existing.candidate_id,
                attempted: bundle.candidate_id(),
            });
        }
        let bundle_index = shell
            .bundles
            .binary_search_by_key(&existing.candidate_id, |entry| entry.0)
            .map_err(|_| ShellError::CorruptReferenceState)?;
        if shell.bundles[bundle_index].1 != *bundle {
            return Err(ShellError::IdempotentBundleMismatch);
        }
        return Ok(CommitResult {
            state: shell.clone(),
            status: CommitStatus::IdempotentReplay,
        });
    }

    if shell.root != bundle.body().pre_root() {
        return Err(ShellError::RootConflict {
            expected: bundle.body().pre_root(),
            actual: shell.root,
        });
    }

    let applied = bundle
        .validate_and_apply::<H>(&shell.state, state_domain)
        .map_err(ShellError::Bundle)?;

    let bundle_index = match shell
        .bundles
        .binary_search_by_key(&bundle.candidate_id(), |entry| entry.0)
    {
        Ok(_) => return Err(ShellError::CandidateAlreadyCommitted),
        Err(index) => index,
    };
    let receipt_index = match shell
        .receipts
        .binary_search_by_key(&bundle.candidate_id(), |entry| entry.0)
    {
        Ok(_) => return Err(ShellError::CandidateAlreadyCommitted),
        Err(index) => index,
    };
    let replay_index = match shell
        .replay
        .binary_search_by_key(&replay_id, |record| record.replay_id)
    {
        Ok(_) => return Err(ShellError::CorruptReferenceState),
        Err(index) => index,
    };

    let mut replay = shell.replay.to_vec();
    replay.insert(
        replay_index,
        ReplayRecord {
            replay_id,
            candidate_id: bundle.candidate_id(),
        },
    );

    let mut receipts = shell.receipts.to_vec();
    receipts.insert(
        receipt_index,
        (bundle.candidate_id(), bundle.receipt().clone()),
    );

    let mut bundles = shell.bundles.to_vec();
    bundles.insert(bundle_index, (bundle.candidate_id(), bundle.clone()));

    let mut outbox = shell.outbox.to_vec();
    for entry in bundle.outbox_plan().entries() {
        let delivery_id = entry
            .delivery_id::<H>(bundle.candidate_id().hash())
            .map_err(ShellError::Encode)?;
        let entry_hash = hash_outbox_entry::<H>(entry)?;
        let record = OutboxRecord {
            candidate_id: bundle.candidate_id(),
            ordinal: entry.ordinal(),
            delivery_id,
            entry_hash,
            entry: entry.clone(),
            acknowledged: false,
        };
        let key = (record.candidate_id, record.ordinal);
        match outbox.binary_search_by_key(&key, |item| (item.candidate_id, item.ordinal)) {
            Ok(_) => return Err(ShellError::DuplicateOutboxRecord),
            Err(index) => outbox.insert(index, record),
        }
    }

    let (state, post_root) = applied.into_parts();
    Ok(CommitResult {
        state: ShellState {
            state,
            root: post_root,
            replay: replay.into_boxed_slice(),
            receipts: receipts.into_boxed_slice(),
            bundles: bundles.into_boxed_slice(),
            outbox: outbox.into_boxed_slice(),
        },
        status: CommitStatus::Committed,
    })
}

/// Records an idempotent destination acknowledgement.
pub fn acknowledge(
    shell: &ShellState,
    delivery_id: Hash32,
    observed_entry_hash: Hash32,
) -> Result<ShellState, ShellError> {
    let Some(index) = shell
        .outbox
        .iter()
        .position(|record| record.delivery_id == delivery_id)
    else {
        return Err(ShellError::UnknownDelivery(delivery_id));
    };
    let current = &shell.outbox[index];
    if current.entry_hash != observed_entry_hash {
        return Err(ShellError::AcknowledgementMismatch {
            delivery_id,
            expected: current.entry_hash,
            observed: observed_entry_hash,
        });
    }
    if current.acknowledged {
        return Ok(shell.clone());
    }
    let mut outbox = shell.outbox.to_vec();
    outbox[index].acknowledged = true;
    Ok(ShellState {
        state: shell.state.clone(),
        root: shell.root,
        replay: shell.replay.clone(),
        receipts: shell.receipts.clone(),
        bundles: shell.bundles.clone(),
        outbox: outbox.into_boxed_slice(),
    })
}

fn hash_outbox_entry<H: CommitmentHasher>(entry: &OutboxEntry) -> Result<Hash32, ShellError> {
    let domain = Domain::new("zeno-fcis/outbox-entry", 1).map_err(ShellError::Encode)?;
    let bytes = entry.canonical_bytes().map_err(ShellError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(ShellError::Encode)
}

/// Reference-shell validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellError {
    /// Initial semantic-state commitment failed.
    State(PatchError),
    /// Bundle relationships failed validation.
    Bundle(SealError),
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// The authoritative root changed since the candidate was planned.
    RootConflict {
        /// Candidate's expected root.
        expected: Hash32,
        /// Current shell root.
        actual: Hash32,
    },
    /// A replay identity was already bound to another candidate.
    ReplayConflict {
        /// Replay identity.
        replay_id: Hash32,
        /// Existing candidate.
        existing: CandidateId,
        /// Attempted candidate.
        attempted: CandidateId,
    },
    /// Candidate identity is already present under another replay path.
    CandidateAlreadyCommitted,
    /// Same replay/candidate identity was supplied with different bundle bytes.
    IdempotentBundleMismatch,
    /// Duplicate candidate/ordinal outbox record.
    DuplicateOutboxRecord,
    /// The reference state violated its own canonical index invariants.
    CorruptReferenceState,
    /// Delivery identity is unknown.
    UnknownDelivery(Hash32),
    /// Destination acknowledged different content under a delivery identity.
    AcknowledgementMismatch {
        /// Delivery identity.
        delivery_id: Hash32,
        /// Expected entry commitment.
        expected: Hash32,
        /// Observed entry commitment.
        observed: Hash32,
    },
}

impl fmt::Display for ShellError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::State(error) => error.fmt(formatter),
            Self::Bundle(error) => error.fmt(formatter),
            Self::Encode(error) => error.fmt(formatter),
            Self::RootConflict { expected, actual } => {
                write!(
                    formatter,
                    "root conflict: expected {expected}, actual {actual}"
                )
            }
            Self::ReplayConflict {
                replay_id,
                existing,
                attempted,
            } => write!(
                formatter,
                "replay {replay_id} is bound to {existing}, not {attempted}"
            ),
            Self::CandidateAlreadyCommitted => formatter.write_str("candidate already committed"),
            Self::IdempotentBundleMismatch => {
                formatter.write_str("idempotent replay supplied different bundle content")
            }
            Self::DuplicateOutboxRecord => formatter.write_str("duplicate outbox record"),
            Self::CorruptReferenceState => {
                formatter.write_str("reference shell indexes are inconsistent")
            }
            Self::UnknownDelivery(delivery_id) => {
                write!(formatter, "unknown delivery {delivery_id}")
            }
            Self::AcknowledgementMismatch {
                delivery_id,
                expected,
                observed,
            } => write!(
                formatter,
                "acknowledgement {delivery_id} expected {expected}, observed {observed}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use zeno_fcis_core::DecisionKind;
    use zeno_fcis_patch::{CanonicalPatch, PatchOp, PathSegment, ValuePath, hash_value};
    use zeno_fcis_plan::{CommitPlan, OutboxEntry, OutboxPlan};
    use zeno_fcis_receipt::{CandidateBindings, CandidateBuilder};
    use zeno_fcis_value::Field;

    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/fold/v1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % 32;
                output[slot] = output[slot].wrapping_add(byte);
            }
            Hash32::new(output)
        }
    }

    fn domain() -> Domain<'static> {
        Domain::new("test/state", 1).unwrap_or_else(|error| panic!("domain: {error}"))
    }

    fn bundle(state: &Value) -> CommitBundle {
        let root = hash_value::<TestHasher>(domain(), state).unwrap_or(Hash32::ZERO);
        let patch = CanonicalPatch::try_new(
            1,
            root,
            vec![PatchOp::Insert {
                path: ValuePath::new(vec![PathSegment::Field(1)]),
                map_key: None,
                value: Value::U128(1),
            }],
        )
        .unwrap_or_else(|error| panic!("patch: {error}"));
        let outbox =
            OutboxPlan::try_new(vec![OutboxEntry::new(0, 1, Value::Unit, Value::Bool(true))])
                .unwrap_or_else(|error| panic!("outbox: {error}"));
        CandidateBuilder::seal::<TestHasher>(
            state,
            domain(),
            DecisionKind::Accept,
            None,
            CandidateBindings {
                profile_hash: Hash32::new([1; 32]),
                command_hash: Hash32::new([2; 32]),
                context_hash: Hash32::new([3; 32]),
                precedence_hash: Hash32::new([4; 32]),
                algorithm_hash: Hash32::new([5; 32]),
                budget_hash: Hash32::new([6; 32]),
            },
            patch,
            CommitPlan::empty(),
            outbox,
        )
        .unwrap_or_else(|error| panic!("bundle: {error}"))
    }

    #[test]
    fn commit_is_atomic_and_replay_is_idempotent() {
        let state = Value::Record(Vec::<Field>::new().into_boxed_slice());
        let bundle = bundle(&state);
        let shell = ShellState::new::<TestHasher>(state, domain())
            .unwrap_or_else(|error| panic!("shell: {error}"));
        let replay = Hash32::new([9; 32]);
        let first = apply_reference_bundle::<TestHasher>(&shell, domain(), replay, &bundle)
            .unwrap_or_else(|error| panic!("commit: {error}"));
        assert_eq!(first.status(), CommitStatus::Committed);
        assert_eq!(
            first.state().state(),
            bundle
                .patch()
                .apply::<TestHasher>(shell.state(), domain())
                .unwrap_or_else(|error| panic!("patch: {error}"))
                .state()
        );
        let second = apply_reference_bundle::<TestHasher>(first.state(), domain(), replay, &bundle)
            .unwrap_or_else(|error| panic!("replay: {error}"));
        assert_eq!(second.status(), CommitStatus::IdempotentReplay);
        assert_eq!(first.state(), second.state());
        assert_eq!(first.state().bundles().len(), 1);
        assert_eq!(
            first.state().outbox_records()[0].entry(),
            bundle
                .outbox_plan()
                .entries()
                .first()
                .unwrap_or_else(|| panic!("entry"))
        );
    }

    #[test]
    fn wrong_expected_root_publishes_nothing() {
        let intended_state = Value::Record(Vec::<Field>::new().into_boxed_slice());
        let bundle = bundle(&intended_state);
        let shell = ShellState::new::<TestHasher>(Value::U128(99), domain())
            .unwrap_or_else(|error| panic!("shell: {error}"));
        let result =
            apply_reference_bundle::<TestHasher>(&shell, domain(), Hash32::new([8; 32]), &bundle);
        assert!(matches!(result, Err(ShellError::RootConflict { .. })));
        assert_eq!(shell.outbox_records().len(), 0);
        assert_eq!(shell.receipts().len(), 0);
        assert_eq!(shell.bundles().len(), 0);
    }

    #[test]
    fn acknowledgement_binds_exact_entry_content() {
        let state = Value::Record(Vec::<Field>::new().into_boxed_slice());
        let bundle = bundle(&state);
        let shell = ShellState::new::<TestHasher>(state, domain())
            .unwrap_or_else(|error| panic!("shell: {error}"));
        let committed =
            apply_reference_bundle::<TestHasher>(&shell, domain(), Hash32::new([7; 32]), &bundle)
                .unwrap_or_else(|error| panic!("commit: {error}"));
        let pending = committed
            .state()
            .next_pending()
            .unwrap_or_else(|| panic!("missing pending record"));
        assert!(acknowledge(committed.state(), pending.delivery_id(), Hash32::ZERO).is_err());
        let acknowledged = acknowledge(
            committed.state(),
            pending.delivery_id(),
            pending.entry_hash(),
        );
        assert!(acknowledged.is_ok());
    }
}
