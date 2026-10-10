//! Historical observations cannot authorize new state or external effects.

use super::*;
use zeno_fcis_transition::ExpectedInvocationBindings;

/// Immutable receipt and provenance of one previously committed operation.
///
/// This is an observation, not a commit or delivery capability. No raw
/// authentication material, bundle or current acknowledgement state is exposed.
///
/// ```compile_fail,E0308
/// use zeno_fcis_authority::CatalogTransitionProgram;
/// use zeno_fcis_crypto::RustCryptoSha256;
/// use zeno_fcis_laws::ProjectLawEngine;
/// use zeno_fcis_shell_sqlite::{SqliteShell, StoredCommit};
/// fn publish<P: CatalogTransitionProgram<RustCryptoSha256>, L: ProjectLawEngine, I>(
///     shell: &mut SqliteShell<P, L, I>, historical: StoredCommit,
/// ) {
///     shell.commit(historical);
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredCommit {
    state_version: u64,
    authorization_id: Hash32,
    invocation_id: Hash32,
    replay_id: Hash32,
    candidate_id: CandidateId,
    receipt_bytes: Vec<u8>,
}

impl StoredCommit {
    /// Returns the version at which this operation committed.
    #[must_use]
    pub const fn state_version(&self) -> u64 {
        self.state_version
    }

    /// Returns the original authorization identity.
    #[must_use]
    pub const fn authorization_id(&self) -> Hash32 {
        self.authorization_id
    }

    /// Returns the original pre-state-bound invocation identity.
    #[must_use]
    pub const fn invocation_id(&self) -> Hash32 {
        self.invocation_id
    }

    /// Returns the original replay identity.
    #[must_use]
    pub const fn replay_id(&self) -> Hash32 {
        self.replay_id
    }

    /// Returns the original candidate identity.
    #[must_use]
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Returns the exact canonical receipt, including decision kind and reason.
    #[must_use]
    pub fn receipt_bytes(&self) -> &[u8] {
        &self.receipt_bytes
    }
}

/// One validated local read snapshot, including the scope of an absent result.
///
/// The version/root identify this handle's observed history, not a globally
/// latest checkpoint. A restored consistent older database can report absence;
/// rollback resistance and read authorization require application protocols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoricalLookup {
    policy_id: Hash32,
    genesis_id: GenesisId,
    observed_version: u64,
    observed_root: Hash32,
    committed: Option<StoredCommit>,
}

impl HistoricalLookup {
    /// Returns the policy under which this local history was validated.
    #[must_use]
    pub const fn policy_id(&self) -> Hash32 {
        self.policy_id
    }

    /// Returns the validated genesis identity.
    #[must_use]
    pub const fn genesis_id(&self) -> GenesisId {
        self.genesis_id
    }

    /// Returns the observed local head version for either presence or absence.
    #[must_use]
    pub const fn observed_version(&self) -> u64 {
        self.observed_version
    }

    /// Returns the observed local semantic root.
    #[must_use]
    pub const fn observed_root(&self) -> Hash32 {
        self.observed_root
    }

    /// Returns the exact historical receipt, or absence in this read snapshot.
    #[must_use]
    pub const fn committed(&self) -> Option<&StoredCommit> {
        self.committed.as_ref()
    }
}

impl<P, L, I> SqliteShell<P, L, I>
where
    P: CatalogTransitionProgram<RustCryptoSha256>,
    L: ProjectLawEngine,
{
    /// Looks up an exact historical request without executing its transition.
    ///
    /// Supply bindings derived by the authority from the original request and
    /// admitted context, plus the genesis hash saved at initial submission. The
    /// genesis commits to the exact policy; a changed lineage is an error even
    /// on a miss. Do not replace the original scope with the current deployment.
    /// A matching replay ID with different command or complete
    /// context returns `ReplayConflict` without stored content. Rejections have
    /// no committed record. Evidence expiry is not renewed or checked against a
    /// wall clock. Callers must separately authorize access to the history.
    ///
    /// All persisted checks share one SQLite read transaction. Another writer
    /// may make this handle stale; then reopen/revalidation is required. This
    /// method scans the full validated history and adds no table or index.
    pub fn lookup_committed(
        &mut self,
        authority: &CatalogCommitAuthority<RustCryptoSha256, P, L, I>,
        expected_genesis: Hash32,
        replay_id: Hash32,
        expected: ExpectedInvocationBindings,
    ) -> Result<HistoricalLookup, SqliteShellError> {
        if authority.policy().policy_id() != self.policy_id {
            return Err(SqliteShellError::PolicyMismatch);
        }
        if replay_id == Hash32::ZERO {
            return Err(SqliteShellError::InvalidReplayId);
        }
        // The exclusive shell borrow serializes API calls. The shared-borrow
        // transaction API lets snapshot() reuse exactly the existing validators;
        // the private connection cannot have an externally owned transaction.
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(SqliteShellError::Sqlite)?;
        check_existing_schema(&transaction)?;
        let snapshot = self.snapshot()?;
        let (genesis_id, _) = validate_persisted_genesis(&transaction, authority)?;
        if genesis_id != self.genesis_id {
            return Err(SqliteShellError::CorruptGenesis);
        }
        if expected_genesis != genesis_id.hash() {
            return Err(SqliteShellError::LineageMismatch);
        }
        let mut matching = self
            .validated_history
            .values()
            .filter(|record| record.replay_id == replay_id);
        let record = matching.next();
        if matching.next().is_some() {
            return Err(SqliteShellError::CorruptHistory);
        }
        let committed = if let Some(record) = record {
            let bindings = record.bundle.body().bindings();
            if bindings.command_hash != expected.command_hash()
                || bindings.context_hash != expected.context_hash()
            {
                return Err(SqliteShellError::ReplayConflict);
            }
            Some(StoredCommit {
                state_version: record.state_version,
                authorization_id: record.authorization_id,
                invocation_id: record.invocation_id,
                replay_id: record.replay_id,
                candidate_id: record.bundle.candidate_id(),
                receipt_bytes: record.receipt_bytes.clone(),
            })
        } else {
            None
        };
        let result = HistoricalLookup {
            policy_id: self.policy_id,
            genesis_id,
            observed_version: snapshot.version(),
            observed_root: snapshot.root(),
            committed,
        };
        transaction.commit().map_err(SqliteShellError::Sqlite)?;
        Ok(result)
    }
}
