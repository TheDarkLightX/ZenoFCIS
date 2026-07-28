//! Catalog-aware pure transition construction and same-candidate sealing.
//!
//! The builder owns only fresh execution-local buffers. It derives profile,
//! precedence, algorithm, footprint, catalog, and resource bindings before it
//! delegates complete candidate construction to `zeno-fcis-receipt`.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use zeno_fcis_catalog::{CatalogError, CatalogMetrics, ProjectCatalog, ReasonDisposition};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_compose::{AccessPath, ContractError, Footprint, PathAtom, PathSet};
use zeno_fcis_core::{Accepted, BudgetUsed, Decision, DecisionKind, Failed, Rejected, Resource};
use zeno_fcis_patch::{
    AppliedPatch, CanonicalPatch, PatchError, PatchOp, PathSegment, ValuePath,
    hash_precondition_value, hash_value, value_at,
};
use zeno_fcis_plan::{CommitPlan, Effect, OutboxEntry, OutboxPlan, PlanError};
use zeno_fcis_project::{SemanticId, StableName};
use zeno_fcis_receipt::{
    CandidateBindings, CandidateBuilder, CommitBundle, RejectReceipt, SealError,
};
use zeno_fcis_schema::{ValidationLimits, ValueValidationError};
use zeno_fcis_value::Value;

/// Canonical transition-builder artifact format version.
pub const TRANSITION_FORMAT_VERSION: u16 = 1;
/// Hard maximum staged patch operations.
pub const MAX_TRANSITION_PATCH_OPERATIONS: u32 = 4_096;
/// Hard maximum observed paths in any one footprint set.
pub const MAX_TRANSITION_OBSERVED_PATHS: u32 = 4_096;
/// Hard maximum applicable reasons retained for one decision.
pub const MAX_TRANSITION_REASONS: u32 = 4_096;
/// Hard maximum encoded bytes in one map-key path atom.
pub const MAX_TRANSITION_MAP_KEY_BYTES: u32 = 1_048_576;
/// Hard maximum recursive state-validation depth.
pub const MAX_TRANSITION_STATE_DEPTH: u16 = 1_024;
/// Hard maximum state nodes admitted by one validation pass.
pub const MAX_TRANSITION_STATE_NODES: u64 = 10_000_000;

/// Execution-local construction bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionLimits {
    max_patch_operations: u32,
    max_observed_paths: u32,
    max_applicable_reasons: u32,
    max_map_key_bytes: u32,
    max_state_depth: u16,
    max_state_nodes: u64,
}

impl TransitionLimits {
    /// Creates a transition envelope under the hard library maxima.
    pub fn try_new(
        max_patch_operations: u32,
        max_observed_paths: u32,
        max_applicable_reasons: u32,
        max_map_key_bytes: u32,
        max_state_depth: u16,
        max_state_nodes: u64,
    ) -> Result<Self, TransitionError> {
        let limits = Self {
            max_patch_operations,
            max_observed_paths,
            max_applicable_reasons,
            max_map_key_bytes,
            max_state_depth,
            max_state_nodes,
        };
        limits.validate()?;
        Ok(limits)
    }

    fn validate(self) -> Result<(), TransitionError> {
        if self.max_patch_operations > MAX_TRANSITION_PATCH_OPERATIONS
            || self.max_observed_paths > MAX_TRANSITION_OBSERVED_PATHS
            || self.max_applicable_reasons > MAX_TRANSITION_REASONS
            || self.max_map_key_bytes > MAX_TRANSITION_MAP_KEY_BYTES
            || self.max_state_depth == 0
            || self.max_state_depth > MAX_TRANSITION_STATE_DEPTH
            || self.max_state_nodes == 0
            || self.max_state_nodes > MAX_TRANSITION_STATE_NODES
        {
            Err(TransitionError::InvalidLimits)
        } else {
            Ok(())
        }
    }

    /// Returns the patch-operation bound.
    #[must_use]
    pub const fn max_patch_operations(self) -> u32 {
        self.max_patch_operations
    }

    /// Returns the per-footprint-set observed-path bound.
    #[must_use]
    pub const fn max_observed_paths(self) -> u32 {
        self.max_observed_paths
    }

    /// Returns the applicable-reason bound.
    #[must_use]
    pub const fn max_applicable_reasons(self) -> u32 {
        self.max_applicable_reasons
    }

    /// Returns the per-map-key encoded-byte bound.
    #[must_use]
    pub const fn max_map_key_bytes(self) -> u32 {
        self.max_map_key_bytes
    }

    /// Returns the recursive state-validation depth bound.
    #[must_use]
    pub const fn max_state_depth(self) -> u16 {
        self.max_state_depth
    }

    /// Returns the state-validation node bound.
    #[must_use]
    pub const fn max_state_nodes(self) -> u64 {
        self.max_state_nodes
    }

    const fn state_validation_limits(self) -> ValidationLimits {
        ValidationLimits {
            max_depth: self.max_state_depth,
            max_nodes: self.max_state_nodes,
        }
    }
}

impl Default for TransitionLimits {
    fn default() -> Self {
        Self {
            max_patch_operations: MAX_TRANSITION_PATCH_OPERATIONS,
            max_observed_paths: MAX_TRANSITION_OBSERVED_PATHS,
            max_applicable_reasons: MAX_TRANSITION_REASONS,
            max_map_key_bytes: 65_536,
            max_state_depth: 128,
            max_state_nodes: 1_000_000,
        }
    }
}

impl CanonicalEncode for TransitionLimits {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.max_patch_operations.to_be_bytes());
        output.extend_from_slice(&self.max_observed_paths.to_be_bytes());
        output.extend_from_slice(&self.max_applicable_reasons.to_be_bytes());
        output.extend_from_slice(&self.max_map_key_bytes.to_be_bytes());
        output.extend_from_slice(&self.max_state_depth.to_be_bytes());
        output.extend_from_slice(&self.max_state_nodes.to_be_bytes());
        Ok(())
    }
}

/// Exact logical and structural resources bound into one decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionResourceReport {
    catalog_hash: Hash32,
    limits: TransitionLimits,
    budget_used: BudgetUsed,
    catalog_metrics: CatalogMetrics,
    footprint_hash: Hash32,
}

impl TransitionResourceReport {
    fn new(
        catalog_hash: Hash32,
        limits: TransitionLimits,
        budget_used: BudgetUsed,
        catalog_metrics: CatalogMetrics,
        footprint_hash: Hash32,
    ) -> Self {
        Self {
            catalog_hash,
            limits,
            budget_used,
            catalog_metrics,
            footprint_hash,
        }
    }

    /// Returns the complete catalog commitment, including catalog limits.
    #[must_use]
    pub const fn catalog_hash(&self) -> Hash32 {
        self.catalog_hash
    }

    /// Returns the builder envelope.
    #[must_use]
    pub const fn limits(&self) -> TransitionLimits {
        self.limits
    }

    /// Returns caller-reported pure-transition budget usage.
    #[must_use]
    pub const fn budget_used(&self) -> BudgetUsed {
        self.budget_used
    }

    /// Returns exact catalog validation metrics.
    #[must_use]
    pub const fn catalog_metrics(&self) -> CatalogMetrics {
        self.catalog_metrics
    }

    /// Returns the observed-footprint commitment.
    #[must_use]
    pub const fn footprint_hash(&self) -> Hash32 {
        self.footprint_hash
    }

    /// Computes the candidate budget binding.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, TransitionError> {
        hash_canonical::<H>("zeno-fcis/transition-resources", self)
    }
}

impl CanonicalEncode for TransitionResourceReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-TRANSITION-RESOURCES\0");
        output.extend_from_slice(&TRANSITION_FORMAT_VERSION.to_be_bytes());
        output.extend_from_slice(self.catalog_hash.as_bytes());
        put_blob(output, &self.limits.canonical_bytes()?)?;
        encode_budget_used(self.budget_used, output);
        put_blob(output, &self.catalog_metrics.canonical_bytes()?)?;
        output.extend_from_slice(self.footprint_hash.as_bytes());
        Ok(())
    }
}

/// Externally expected command and authenticated-context commitments.
///
/// This value must be derived from the invocation admitted by the caller-facing
/// boundary. Transition validation never derives either expected field from the
/// artifact being validated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpectedInvocationBindings {
    command_hash: Hash32,
    context_hash: Hash32,
}

impl ExpectedInvocationBindings {
    /// Creates nonzero expected invocation commitments.
    pub fn try_new(command_hash: Hash32, context_hash: Hash32) -> Result<Self, TransitionError> {
        if command_hash == Hash32::ZERO {
            return Err(TransitionError::ZeroCommandHash);
        }
        if context_hash == Hash32::ZERO {
            return Err(TransitionError::ZeroContextHash);
        }
        Ok(Self {
            command_hash,
            context_hash,
        })
    }

    /// Returns the externally expected command commitment.
    #[must_use]
    pub const fn command_hash(self) -> Hash32 {
        self.command_hash
    }

    /// Returns the externally expected authenticated-context commitment.
    #[must_use]
    pub const fn context_hash(self) -> Hash32 {
        self.context_hash
    }
}

impl CanonicalEncode for ExpectedInvocationBindings {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(self.command_hash.as_bytes());
        output.extend_from_slice(self.context_hash.as_bytes());
        Ok(())
    }
}

/// Accepted or committed-failure artifacts from one catalogued execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionArtifacts {
    reason_id: Option<SemanticId>,
    bundle: CommitBundle,
    footprint: Footprint,
    catalog_metrics: CatalogMetrics,
    resources: TransitionResourceReport,
}

impl TransitionArtifacts {
    /// Returns the committed-failure reason identifier, if any.
    #[must_use]
    pub const fn reason_id(&self) -> Option<SemanticId> {
        self.reason_id
    }

    /// Returns the complete candidate bundle.
    #[must_use]
    pub const fn bundle(&self) -> &CommitBundle {
        &self.bundle
    }

    /// Returns the execution-observed footprint.
    #[must_use]
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
    }

    /// Returns exact catalog metrics.
    #[must_use]
    pub const fn catalog_metrics(&self) -> CatalogMetrics {
        self.catalog_metrics
    }

    /// Returns the resource report bound by the candidate.
    #[must_use]
    pub const fn resources(&self) -> &TransitionResourceReport {
        &self.resources
    }

    /// Revalidates the catalog, resource, reason, and candidate relationships.
    pub fn validate<H: CommitmentHasher>(
        &self,
        catalog: &ProjectCatalog,
        expected_invocation: ExpectedInvocationBindings,
        pre_state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<(), TransitionError> {
        self.validate_and_apply::<H>(catalog, expected_invocation, pre_state, state_domain)
            .map(|_| ())
    }

    /// Revalidates every relationship and returns the exact pure successor.
    pub fn validate_and_apply<H: CommitmentHasher>(
        &self,
        catalog: &ProjectCatalog,
        expected_invocation: ExpectedInvocationBindings,
        pre_state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<AppliedPatch, TransitionError> {
        let metrics =
            catalog.validate_plans(self.bundle.commit_plan(), self.bundle.outbox_plan())?;
        if metrics != self.catalog_metrics || self.resources.catalog_metrics != metrics {
            return Err(TransitionError::ArtifactMismatch(
                ArtifactField::CatalogMetrics,
            ));
        }
        validate_resource_report::<H>(catalog, &self.footprint, &self.resources)?;
        let actual = self.bundle.body().bindings();
        let expected = candidate_bindings::<H>(
            catalog,
            expected_invocation.command_hash,
            expected_invocation.context_hash,
            &self.resources,
        )?;
        if actual != expected {
            return Err(TransitionError::ArtifactMismatch(
                ArtifactField::CandidateBindings,
            ));
        }
        if self.bundle.patch().state_type() != catalog.profile().state_type().get() {
            return Err(TransitionError::ArtifactMismatch(ArtifactField::StateType));
        }
        match (self.bundle.body().decision_kind(), self.reason_id) {
            (DecisionKind::Accept, None) => {
                if self.bundle.body().reason_code().is_some() {
                    return Err(TransitionError::ArtifactMismatch(ArtifactField::Reason));
                }
            }
            (DecisionKind::CommittedFailure, Some(reason_id)) => {
                let reason =
                    catalog.validate_reason(reason_id.get(), DecisionKind::CommittedFailure)?;
                let actual_reason = self.bundle.body().reason_code().map(|value| value.as_str());
                if actual_reason != Some(reason.name().as_str()) {
                    return Err(TransitionError::ArtifactMismatch(ArtifactField::Reason));
                }
            }
            _ => return Err(TransitionError::ArtifactMismatch(ArtifactField::Decision)),
        }
        catalog
            .schema()
            .validate_root(pre_state, self.resources.limits.state_validation_limits())?;
        let applied = self
            .bundle
            .validate_and_apply::<H>(pre_state, state_domain)?;
        catalog.schema().validate_root(
            applied.state(),
            self.resources.limits.state_validation_limits(),
        )?;
        Ok(applied)
    }
}

/// Unchanged-state rejection evidence with no candidate or authoritative plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionReject {
    reason_id: SemanticId,
    receipt: RejectReceipt,
    footprint: Footprint,
    resources: TransitionResourceReport,
}

impl TransitionReject {
    /// Returns the selected rejection identifier.
    #[must_use]
    pub const fn reason_id(&self) -> SemanticId {
        self.reason_id
    }

    /// Returns the unchanged-state rejection receipt.
    #[must_use]
    pub const fn receipt(&self) -> &RejectReceipt {
        &self.receipt
    }

    /// Returns the execution-observed footprint.
    #[must_use]
    pub const fn footprint(&self) -> &Footprint {
        &self.footprint
    }

    /// Returns the resource report bound by the rejection receipt.
    #[must_use]
    pub const fn resources(&self) -> &TransitionResourceReport {
        &self.resources
    }

    /// Revalidates the unchanged-state receipt and all resource bindings.
    pub fn validate<H: CommitmentHasher>(
        &self,
        catalog: &ProjectCatalog,
        expected_invocation: ExpectedInvocationBindings,
        pre_state: &Value,
        state_domain: Domain<'_>,
    ) -> Result<(), TransitionError> {
        let reason = catalog.validate_reason(self.reason_id.get(), DecisionKind::Reject)?;
        if self.receipt.reason_code().as_str() != reason.name().as_str() {
            return Err(TransitionError::ArtifactMismatch(ArtifactField::Reason));
        }
        if self.resources.catalog_metrics != CatalogMetrics::default() {
            return Err(TransitionError::ArtifactMismatch(
                ArtifactField::CatalogMetrics,
            ));
        }
        validate_resource_report::<H>(catalog, &self.footprint, &self.resources)?;
        let actual = self.receipt.bindings();
        let expected = candidate_bindings::<H>(
            catalog,
            expected_invocation.command_hash,
            expected_invocation.context_hash,
            &self.resources,
        )?;
        if actual != expected {
            return Err(TransitionError::ArtifactMismatch(
                ArtifactField::CandidateBindings,
            ));
        }
        let pre_root = hash_value::<H>(state_domain, pre_state)?;
        if pre_root != self.receipt.pre_root() {
            return Err(TransitionError::ArtifactMismatch(ArtifactField::PreRoot));
        }
        catalog
            .schema()
            .validate_root(pre_state, self.resources.limits.state_validation_limits())?;
        Ok(())
    }
}

/// Three-way output of one catalogued builder execution.
pub type TransitionDecision = Decision<TransitionArtifacts, TransitionReject, SemanticId>;

/// Validates a complete three-way decision and the outer committed-failure reason.
pub fn validate_transition_decision<H: CommitmentHasher>(
    decision: &TransitionDecision,
    catalog: &ProjectCatalog,
    expected_invocation: ExpectedInvocationBindings,
    pre_state: &Value,
    state_domain: Domain<'_>,
) -> Result<(), TransitionError> {
    match decision {
        Decision::Accept(accepted) => {
            if accepted.candidate().reason_id().is_some() {
                return Err(TransitionError::ArtifactMismatch(ArtifactField::Reason));
            }
            accepted.candidate().validate::<H>(
                catalog,
                expected_invocation,
                pre_state,
                state_domain,
            )
        }
        Decision::Reject(rejected) => {
            rejected
                .reason()
                .validate::<H>(catalog, expected_invocation, pre_state, state_domain)
        }
        Decision::CommittedFailure(failed) => {
            if failed.candidate().reason_id() != Some(*failed.reason()) {
                return Err(TransitionError::ArtifactMismatch(ArtifactField::Reason));
            }
            failed
                .candidate()
                .validate::<H>(catalog, expected_invocation, pre_state, state_domain)
        }
    }
}

/// Fresh execution-local builder for one catalogued transition.
pub struct CataloguedTransitionBuilder<'a, H: CommitmentHasher> {
    catalog: &'a ProjectCatalog,
    pre_state: &'a Value,
    state_domain: Domain<'a>,
    command_hash: Hash32,
    context_hash: Hash32,
    budget_used: BudgetUsed,
    limits: TransitionLimits,
    catalog_hash: Hash32,
    pre_root: Hash32,
    patch_operations: Vec<PatchOp>,
    effects: Vec<Effect>,
    outbox_entries: Vec<OutboxEntry>,
    reads: Vec<AccessPath>,
    writes: Vec<AccessPath>,
    contexts: Vec<AccessPath>,
    effect_paths: Vec<AccessPath>,
    applicable_reasons: Vec<SemanticId>,
    marker: PhantomData<H>,
}

impl<'a, H: CommitmentHasher> CataloguedTransitionBuilder<'a, H> {
    /// Creates a fresh builder and derives the exact pre-root and catalog identity.
    pub fn try_new(
        catalog: &'a ProjectCatalog,
        pre_state: &'a Value,
        state_domain: Domain<'a>,
        command_hash: Hash32,
        context_hash: Hash32,
        budget_used: BudgetUsed,
        limits: TransitionLimits,
    ) -> Result<Self, TransitionError> {
        limits.validate()?;
        if command_hash == Hash32::ZERO {
            return Err(TransitionError::ZeroCommandHash);
        }
        if context_hash == Hash32::ZERO {
            return Err(TransitionError::ZeroContextHash);
        }
        let catalog_hash = catalog.commitment::<H>()?;
        catalog
            .schema()
            .validate_root(pre_state, limits.state_validation_limits())?;
        let pre_root = hash_value::<H>(state_domain, pre_state)?;
        Ok(Self {
            catalog,
            pre_state,
            state_domain,
            command_hash,
            context_hash,
            budget_used,
            limits,
            catalog_hash,
            pre_root,
            patch_operations: Vec::new(),
            effects: Vec::new(),
            outbox_entries: Vec::new(),
            reads: Vec::new(),
            writes: Vec::new(),
            contexts: Vec::new(),
            effect_paths: Vec::new(),
            applicable_reasons: Vec::new(),
            marker: PhantomData,
        })
    }

    /// Observes one immutable pre-state path and records its read footprint.
    pub fn read(&mut self, path: ValuePath) -> Result<&'a Value, TransitionError> {
        ensure_capacity(
            self.reads.len(),
            self.limits.max_observed_paths,
            LimitKind::Reads,
        )?;
        let access = canonical_access_path::<H>(
            self.catalog.profile().state_type().get(),
            &path,
            self.limits.max_map_key_bytes,
        )?;
        let observed = value_at(self.pre_state, &path)?;
        self.reads.push(access);
        Ok(observed)
    }

    /// Stages one preconditioned update and records its read/write footprint.
    pub fn update(&mut self, path: ValuePath, value: Value) -> Result<&mut Self, TransitionError> {
        self.ensure_patch_and_state_path_capacity()?;
        let access = canonical_access_path::<H>(
            self.catalog.profile().state_type().get(),
            &path,
            self.limits.max_map_key_bytes,
        )?;
        let old = value_at(self.pre_state, &path)?;
        let expected_old_hash = hash_precondition_value::<H>(old)?;
        self.patch_operations.push(PatchOp::Update {
            path,
            expected_old_hash,
            value,
        });
        self.reads.push(access.clone());
        self.writes.push(access);
        Ok(self)
    }

    /// Stages an absent record-field or map-entry insertion.
    pub fn insert(
        &mut self,
        path: ValuePath,
        map_key: Option<Value>,
        value: Value,
    ) -> Result<&mut Self, TransitionError> {
        self.ensure_patch_and_state_path_capacity()?;
        let access = canonical_access_path::<H>(
            self.catalog.profile().state_type().get(),
            &path,
            self.limits.max_map_key_bytes,
        )?;
        self.patch_operations.push(PatchOp::Insert {
            path,
            map_key,
            value,
        });
        self.reads.push(access.clone());
        self.writes.push(access);
        Ok(self)
    }

    /// Stages one preconditioned deletion and records its read/write footprint.
    pub fn delete(&mut self, path: ValuePath) -> Result<&mut Self, TransitionError> {
        self.ensure_patch_and_state_path_capacity()?;
        let access = canonical_access_path::<H>(
            self.catalog.profile().state_type().get(),
            &path,
            self.limits.max_map_key_bytes,
        )?;
        let old = value_at(self.pre_state, &path)?;
        let expected_old_hash = hash_precondition_value::<H>(old)?;
        self.patch_operations.push(PatchOp::Delete {
            path,
            expected_old_hash,
        });
        self.reads.push(access.clone());
        self.writes.push(access);
        Ok(self)
    }

    /// Records one exact context observation.
    pub fn observe_context(&mut self, path: AccessPath) -> Result<&mut Self, TransitionError> {
        if path.namespace() != self.catalog.profile().context_type().get() {
            return Err(TransitionError::ContextNamespaceMismatch {
                expected: self.catalog.profile().context_type().get(),
                actual: path.namespace(),
            });
        }
        if path
            .atoms()
            .iter()
            .any(|atom| matches!(atom, PathAtom::AnyDescendant))
        {
            return Err(TransitionError::ObservedWildcard);
        }
        ensure_capacity(
            self.contexts.len(),
            self.limits.max_observed_paths,
            LimitKind::Contexts,
        )?;
        self.contexts.push(path);
        Ok(self)
    }

    /// Stages one authoritative effect and records its operation footprint.
    pub fn emit(&mut self, effect: Effect) -> Result<&mut Self, TransitionError> {
        ensure_capacity(
            self.effects.len(),
            self.catalog.limits().max_effects(),
            LimitKind::Effects,
        )?;
        ensure_capacity(
            self.effect_paths.len(),
            self.limits.max_observed_paths,
            LimitKind::EffectPaths,
        )?;
        let path = AccessPath::try_new(effect.operation(), Vec::new())?;
        self.effects.push(effect);
        self.effect_paths.push(path);
        Ok(self)
    }

    /// Stages one external-delivery obligation.
    pub fn enqueue(&mut self, entry: OutboxEntry) -> Result<&mut Self, TransitionError> {
        ensure_capacity(
            self.outbox_entries.len(),
            self.catalog.limits().max_outbox_entries(),
            LimitKind::OutboxEntries,
        )?;
        self.outbox_entries.push(entry);
        Ok(self)
    }

    /// Records a catalogued ordinary rejection when `condition` is false.
    pub fn require(
        &mut self,
        condition: bool,
        reason_id: SemanticId,
    ) -> Result<&mut Self, TransitionError> {
        self.catalog
            .validate_reason(reason_id.get(), DecisionKind::Reject)?;
        if !condition {
            self.push_reason(reason_id)?;
        }
        Ok(self)
    }

    /// Records a catalogued committed failure when `condition` is true.
    pub fn fail_if(
        &mut self,
        condition: bool,
        reason_id: SemanticId,
    ) -> Result<&mut Self, TransitionError> {
        self.catalog
            .validate_reason(reason_id.get(), DecisionKind::CommittedFailure)?;
        if condition {
            self.push_reason(reason_id)?;
        }
        Ok(self)
    }

    /// Canonicalizes, validates, resource-binds, and seals one three-way decision.
    pub fn seal(self) -> Result<TransitionDecision, TransitionError> {
        let selected = self.selected_reason()?;
        match selected {
            Some(reason) if reason.disposition == ReasonDisposition::Reject => {
                let footprint = self.rejection_footprint()?;
                self.seal_reject(footprint, reason)
            }
            reason => {
                let footprint = self.observed_footprint()?;
                self.seal_candidate(footprint, reason)
            }
        }
    }

    fn ensure_patch_and_state_path_capacity(&self) -> Result<(), TransitionError> {
        ensure_capacity(
            self.patch_operations.len(),
            self.limits.max_patch_operations,
            LimitKind::PatchOperations,
        )?;
        ensure_capacity(
            self.reads.len(),
            self.limits.max_observed_paths,
            LimitKind::Reads,
        )?;
        ensure_capacity(
            self.writes.len(),
            self.limits.max_observed_paths,
            LimitKind::Writes,
        )
    }

    fn push_reason(&mut self, reason_id: SemanticId) -> Result<(), TransitionError> {
        if self.applicable_reasons.contains(&reason_id) {
            return Ok(());
        }
        ensure_capacity(
            self.applicable_reasons.len(),
            self.limits.max_applicable_reasons,
            LimitKind::ApplicableReasons,
        )?;
        self.applicable_reasons.push(reason_id);
        Ok(())
    }

    fn observed_footprint(&self) -> Result<Footprint, TransitionError> {
        Ok(Footprint::new(
            normalize_paths(self.reads.clone())?,
            normalize_paths(self.writes.clone())?,
            normalize_paths(self.contexts.clone())?,
            normalize_paths(self.effect_paths.clone())?,
        ))
    }

    fn rejection_footprint(&self) -> Result<Footprint, TransitionError> {
        Ok(Footprint::new(
            normalize_paths(self.reads.clone())?,
            PathSet::empty(),
            normalize_paths(self.contexts.clone())?,
            PathSet::empty(),
        ))
    }

    fn selected_reason(&self) -> Result<Option<SelectedReason>, TransitionError> {
        let mut selected: Option<SelectedReason> = None;
        for id in &self.applicable_reasons {
            let reason = self
                .catalog
                .manifest()
                .reason(*id)
                .ok_or(CatalogError::UnknownReason(id.get()))?;
            let candidate = SelectedReason {
                id: *id,
                name: reason.name().clone(),
                disposition: reason.disposition(),
                precedence: reason.precedence(),
            };
            if selected.as_ref().is_none_or(|current| {
                (candidate.precedence, candidate.id) < (current.precedence, current.id)
            }) {
                selected = Some(candidate);
            }
        }
        Ok(selected)
    }

    fn seal_reject(
        self,
        footprint: Footprint,
        reason: SelectedReason,
    ) -> Result<TransitionDecision, TransitionError> {
        let resources = resource_report::<H>(
            self.catalog_hash,
            self.limits,
            self.budget_used,
            &footprint,
            CatalogMetrics::default(),
        )?;
        let bindings = candidate_bindings::<H>(
            self.catalog,
            self.command_hash,
            self.context_hash,
            &resources,
        )?;
        let receipt = RejectReceipt::new(bindings, self.pre_root, reason.name.as_str())?;
        let reject = TransitionReject {
            reason_id: reason.id,
            receipt,
            footprint,
            resources,
        };
        let expected_invocation =
            ExpectedInvocationBindings::try_new(self.command_hash, self.context_hash)?;
        reject.validate::<H>(
            self.catalog,
            expected_invocation,
            self.pre_state,
            self.state_domain,
        )?;
        Ok(Decision::Reject(Rejected::new(reject)))
    }

    fn seal_candidate(
        self,
        footprint: Footprint,
        reason: Option<SelectedReason>,
    ) -> Result<TransitionDecision, TransitionError> {
        let catalog_hash = self.catalog_hash;
        let limits = self.limits;
        let budget_used = self.budget_used;
        let commit_plan = CommitPlan::try_new(self.effects)?;
        let outbox_plan = OutboxPlan::try_new(self.outbox_entries)?;
        let catalog_metrics = self.catalog.validate_plans(&commit_plan, &outbox_plan)?;
        let patch = CanonicalPatch::try_new(
            self.catalog.profile().state_type().get(),
            self.pre_root,
            self.patch_operations,
        )?;
        let resources = resource_report::<H>(
            catalog_hash,
            limits,
            budget_used,
            &footprint,
            catalog_metrics,
        )?;
        let bindings = candidate_bindings::<H>(
            self.catalog,
            self.command_hash,
            self.context_hash,
            &resources,
        )?;
        let decision_kind = if reason.is_some() {
            DecisionKind::CommittedFailure
        } else {
            DecisionKind::Accept
        };
        let reason_name = reason.as_ref().map(|value| value.name.as_str());
        let bundle = CandidateBuilder::seal::<H>(
            self.pre_state,
            self.state_domain,
            decision_kind,
            reason_name,
            bindings,
            patch,
            commit_plan,
            outbox_plan,
        )?;
        let reason_id = reason.as_ref().map(|value| value.id);
        let artifacts = TransitionArtifacts {
            reason_id,
            bundle,
            footprint,
            catalog_metrics,
            resources,
        };
        let expected_invocation =
            ExpectedInvocationBindings::try_new(self.command_hash, self.context_hash)?;
        artifacts.validate::<H>(
            self.catalog,
            expected_invocation,
            self.pre_state,
            self.state_domain,
        )?;
        match reason_id {
            None => Ok(Decision::Accept(Accepted::new(artifacts))),
            Some(id) => Ok(Decision::CommittedFailure(Failed::new(artifacts, id))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SelectedReason {
    id: SemanticId,
    name: StableName,
    disposition: ReasonDisposition,
    precedence: u32,
}

fn resource_report<H: CommitmentHasher>(
    catalog_hash: Hash32,
    limits: TransitionLimits,
    budget_used: BudgetUsed,
    footprint: &Footprint,
    metrics: CatalogMetrics,
) -> Result<TransitionResourceReport, TransitionError> {
    let footprint_hash = hash_canonical::<H>("zeno-fcis/observed-footprint", footprint)?;
    Ok(TransitionResourceReport::new(
        catalog_hash,
        limits,
        budget_used,
        metrics,
        footprint_hash,
    ))
}

fn candidate_bindings<H: CommitmentHasher>(
    catalog: &ProjectCatalog,
    command_hash: Hash32,
    context_hash: Hash32,
    resources: &TransitionResourceReport,
) -> Result<CandidateBindings, TransitionError> {
    if command_hash == Hash32::ZERO {
        return Err(TransitionError::ZeroCommandHash);
    }
    if context_hash == Hash32::ZERO {
        return Err(TransitionError::ZeroContextHash);
    }
    let profile = catalog.profile();
    let profile_bindings = profile.bindings();
    Ok(CandidateBindings {
        profile_hash: catalog.profile_hash(),
        command_hash,
        context_hash,
        precedence_hash: catalog.manifest().precedence_hash(),
        algorithm_hash: profile_bindings.algorithm_hash,
        budget_hash: resources.commitment::<H>()?,
    })
}

fn validate_resource_report<H: CommitmentHasher>(
    catalog: &ProjectCatalog,
    footprint: &Footprint,
    resources: &TransitionResourceReport,
) -> Result<(), TransitionError> {
    resources.limits.validate()?;
    if resources.catalog_hash != catalog.commitment::<H>()? {
        return Err(TransitionError::ArtifactMismatch(ArtifactField::Catalog));
    }
    let footprint_hash = hash_canonical::<H>("zeno-fcis/observed-footprint", footprint)?;
    if resources.footprint_hash != footprint_hash {
        return Err(TransitionError::ArtifactMismatch(ArtifactField::Footprint));
    }
    Ok(())
}

/// Converts one concrete value path into its canonical hierarchical footprint path.
///
/// Map-key segments are replaced by the protocol-defined commitment of their
/// already-canonical encoded key bytes. Wildcards cannot be introduced through
/// this conversion.
pub fn canonical_access_path<H: CommitmentHasher>(
    namespace: u32,
    path: &ValuePath,
    max_map_key_bytes: u32,
) -> Result<AccessPath, TransitionError> {
    let mut atoms = Vec::with_capacity(path.segments().len());
    for segment in path.segments() {
        atoms.push(match segment {
            PathSegment::Field(id) => PathAtom::Field(*id),
            PathSegment::TupleIndex(index) => PathAtom::TupleIndex(*index),
            PathSegment::VectorIndex(index) => PathAtom::VectorIndex(*index),
            PathSegment::SumPayload => PathAtom::SumPayload,
            PathSegment::MapKey(encoded_key) => {
                let actual = u32::try_from(encoded_key.len()).map_err(|_| {
                    TransitionError::MapKeyBytesExceeded {
                        limit: max_map_key_bytes,
                        actual: u32::MAX,
                    }
                })?;
                if actual > max_map_key_bytes {
                    return Err(TransitionError::MapKeyBytesExceeded {
                        limit: max_map_key_bytes,
                        actual,
                    });
                }
                let domain = Domain::new("zeno-fcis/access-map-key", TRANSITION_FORMAT_VERSION)?;
                PathAtom::MapKey(commitment::<H>(domain, encoded_key)?)
            }
        });
    }
    AccessPath::try_new(namespace, atoms).map_err(TransitionError::Contract)
}

fn normalize_paths(mut paths: Vec<AccessPath>) -> Result<PathSet, TransitionError> {
    paths.sort();
    paths.dedup();
    PathSet::try_new(paths).map_err(TransitionError::Contract)
}

fn ensure_capacity(current: usize, limit: u32, kind: LimitKind) -> Result<(), TransitionError> {
    let current = u32::try_from(current).map_err(|_| TransitionError::LimitExceeded {
        kind,
        limit,
        attempted: u32::MAX,
    })?;
    let attempted = current
        .checked_add(1)
        .ok_or(TransitionError::LimitExceeded {
            kind,
            limit,
            attempted: u32::MAX,
        })?;
    if attempted > limit {
        Err(TransitionError::LimitExceeded {
            kind,
            limit,
            attempted,
        })
    } else {
        Ok(())
    }
}

fn hash_canonical<H: CommitmentHasher>(
    domain_name: &'static str,
    value: &impl CanonicalEncode,
) -> Result<Hash32, TransitionError> {
    let bytes = value.canonical_bytes()?;
    let domain = Domain::new(domain_name, TRANSITION_FORMAT_VERSION)?;
    commitment::<H>(domain, &bytes).map_err(TransitionError::Encode)
}

fn encode_budget_used(budget: BudgetUsed, output: &mut Vec<u8>) {
    for resource in [
        Resource::Read,
        Resource::Write,
        Resource::Candidate,
        Resource::Effect,
        Resource::Byte,
        Resource::WitnessByte,
        Resource::Depth,
    ] {
        output.extend_from_slice(&budget.used(resource).to_be_bytes());
    }
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

/// Builder-local bounded resource category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    /// Patch proposal count.
    PatchOperations,
    /// Observed state reads.
    Reads,
    /// Proposed state writes.
    Writes,
    /// Observed context paths.
    Contexts,
    /// Authoritative effects.
    Effects,
    /// Observed effect paths.
    EffectPaths,
    /// External-delivery entries.
    OutboxEntries,
    /// Applicable reason count.
    ApplicableReasons,
}

/// Relationship revalidated in a built decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactField {
    /// Complete catalog identity.
    Catalog,
    /// Observed footprint commitment.
    Footprint,
    /// Catalog validation metrics.
    CatalogMetrics,
    /// Candidate shared bindings.
    CandidateBindings,
    /// Patch state type.
    StateType,
    /// Decision class.
    Decision,
    /// Reason identity or readable code.
    Reason,
    /// Rejection pre-root.
    PreRoot,
}

/// Transition construction or output-validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransitionError {
    /// The builder envelope exceeds a hard maximum.
    InvalidLimits,
    /// A command commitment used the zero sentinel.
    ZeroCommandHash,
    /// An authenticated-context commitment used the zero sentinel.
    ZeroContextHash,
    /// One execution-local collection exceeded its bound.
    LimitExceeded {
        /// Bounded category.
        kind: LimitKind,
        /// Configured limit.
        limit: u32,
        /// Attempted count.
        attempted: u32,
    },
    /// One map-key path atom exceeded its encoded-byte bound.
    MapKeyBytesExceeded {
        /// Configured limit.
        limit: u32,
        /// Observed bytes.
        actual: u32,
    },
    /// An observed context path used the wrong profile namespace.
    ContextNamespaceMismatch {
        /// Profile context namespace.
        expected: u32,
        /// Supplied namespace.
        actual: u32,
    },
    /// An execution-observed path used a descendant wildcard.
    ObservedWildcard,
    /// A built output failed an exact relationship check.
    ArtifactMismatch(ArtifactField),
    /// Catalog construction or admission failed.
    Catalog(CatalogError),
    /// Canonical encoding or commitment construction failed.
    Encode(EncodeError),
    /// Composition-path construction failed.
    Contract(ContractError),
    /// Patch construction or application failed.
    Patch(PatchError),
    /// Commit/outbox plan construction failed.
    Plan(PlanError),
    /// Candidate or rejection-receipt sealing failed.
    Seal(SealError),
    /// The supplied pre-state or committing successor violated the catalog schema.
    Schema(ValueValidationError),
}

impl From<CatalogError> for TransitionError {
    fn from(error: CatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<EncodeError> for TransitionError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl From<ContractError> for TransitionError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<PatchError> for TransitionError {
    fn from(error: PatchError) -> Self {
        Self::Patch(error)
    }
}

impl From<PlanError> for TransitionError {
    fn from(error: PlanError) -> Self {
        Self::Plan(error)
    }
}

impl From<SealError> for TransitionError {
    fn from(error: SealError) -> Self {
        Self::Seal(error)
    }
}

impl From<ValueValidationError> for TransitionError {
    fn from(error: ValueValidationError) -> Self {
        Self::Schema(error)
    }
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLimits => formatter.write_str("invalid transition limits"),
            Self::ZeroCommandHash => formatter.write_str("command commitment is zero"),
            Self::ZeroContextHash => formatter.write_str("context commitment is zero"),
            Self::LimitExceeded {
                kind,
                limit,
                attempted,
            } => write!(
                formatter,
                "{kind:?} count {attempted} exceeds transition limit {limit}"
            ),
            Self::MapKeyBytesExceeded { limit, actual } => write!(
                formatter,
                "map-key path bytes {actual} exceed transition limit {limit}"
            ),
            Self::ContextNamespaceMismatch { expected, actual } => write!(
                formatter,
                "context namespace {actual} differs from profile namespace {expected}"
            ),
            Self::ObservedWildcard => {
                formatter.write_str("observed context path may not contain a wildcard")
            }
            Self::ArtifactMismatch(field) => {
                write!(formatter, "transition artifact mismatch: {field:?}")
            }
            Self::Catalog(error) => write!(formatter, "catalog failed: {error}"),
            Self::Encode(error) => write!(formatter, "transition encoding failed: {error}"),
            Self::Contract(error) => write!(formatter, "footprint failed: {error}"),
            Self::Patch(error) => write!(formatter, "patch failed: {error}"),
            Self::Plan(error) => write!(formatter, "plan failed: {error}"),
            Self::Seal(error) => write!(formatter, "candidate sealing failed: {error}"),
            Self::Schema(error) => write!(formatter, "state schema admission failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TransitionError {}
