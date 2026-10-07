//! Declarative project-law metadata and exact economic requirements.
//!
//! Definitions, family requirements, scopes and canonical manifest bytes remain
//! shared metadata. These values construct no native law engine or V2 authority.
//!
//! Original native invocation inputs, evidence callbacks, evaluation algorithms
//! and assertions are retained only in the nonpublished private kernel-law
//! oracle. The checked V2 interpreter owns production law and genesis judgment.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_catalog::{CatalogError, ProjectCatalog, ValueFlowKind};
use zeno_fcis_codec::{CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::DecisionKind;
use zeno_fcis_project::{ProfileError, RegistryEntry, RegistryKind, SemanticId, StableName};
use zeno_fcis_spec::{LawScope, ProjectSpec};

/// Canonical project-law manifest format version.
pub const LAW_MANIFEST_FORMAT_VERSION: u16 = 2;
/// Canonical verified law-set format version.
pub const LAW_SET_FORMAT_VERSION: u16 = 2;
/// Canonical per-invocation evaluation format version.
pub const LAW_EVALUATION_FORMAT_VERSION: u16 = 2;
/// Canonical genesis-law evaluation format version.
pub const GENESIS_LAW_EVALUATION_FORMAT_VERSION: u16 = 2;
/// Hard maximum number of definitions, evidence items, or observations.
pub const MAX_PROJECT_LAWS: usize = 4_096;

/// Closed relational-law families understood by the authorization boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LawKind {
    /// Invariants over every committing successor state.
    StateInvariant = 0,
    /// Aggregate asset conservation.
    AssetConservation = 1,
    /// Minting and burning authority and supply relations.
    MintBurnAuthorization = 2,
    /// Equality between semantic debits/credits and committed external obligations.
    DebitCreditEffectEquality = 3,
    /// Fee, scale, dust, and rounding-remainder relations.
    FeeAndRounding = 4,
    /// Authority, subject, asset, and recipient relationships.
    AuthoritySubjectRecipient = 5,
    /// Ordinary rejection carries no successor or authority-bearing plan.
    RejectNoAuthority = 6,
    /// Committed failure changes only the explicitly admitted failure surface.
    CommittedFailureEffects = 7,
}

impl LawKind {
    /// Every law family in stable protocol order.
    pub const ALL: [Self; 8] = [
        Self::StateInvariant,
        Self::AssetConservation,
        Self::MintBurnAuthorization,
        Self::DebitCreditEffectEquality,
        Self::FeeAndRounding,
        Self::AuthoritySubjectRecipient,
        Self::RejectNoAuthority,
        Self::CommittedFailureEffects,
    ];

    const fn mandatory(self) -> bool {
        matches!(
            self,
            Self::StateInvariant | Self::RejectNoAuthority | Self::CommittedFailureEffects
        )
    }
}

impl LawKind {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Whether one complete law family is required by this project.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawFamilyDisposition {
    /// At least one definition in this family is required.
    Required,
    /// The family is inapplicable under an exact reviewed rationale.
    NotApplicable {
        /// Nonzero commitment to the reviewed rationale.
        rationale_hash: Hash32,
    },
}

impl LawFamilyDisposition {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Required => output.push(0),
            Self::NotApplicable { rationale_hash } => {
                output.push(1);
                output.extend_from_slice(rationale_hash.as_bytes());
            }
        }
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Complete policy for one closed law family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawFamilyPolicy {
    kind: LawKind,
    disposition: LawFamilyDisposition,
}

impl LawFamilyPolicy {
    /// Requires at least one law in `kind`.
    #[must_use]
    pub const fn required(kind: LawKind) -> Self {
        Self {
            kind,
            disposition: LawFamilyDisposition::Required,
        }
    }

    /// Marks a non-mandatory family inapplicable under a nonzero rationale.
    pub fn not_applicable(kind: LawKind, rationale_hash: Hash32) -> Result<Self, LawError> {
        if kind.mandatory() {
            return Err(LawError::MandatoryFamilyNotApplicable(kind));
        }
        if rationale_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Rationale));
        }
        Ok(Self {
            kind,
            disposition: LawFamilyDisposition::NotApplicable { rationale_hash },
        })
    }

    /// Returns the family.
    #[must_use]
    pub const fn kind(self) -> LawKind {
        self.kind
    }

    /// Returns its required-or-inapplicable policy.
    #[must_use]
    pub const fn disposition(self) -> LawFamilyDisposition {
        self.disposition
    }
}

impl LawFamilyPolicy {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        self.disposition.encode_to(output)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Decisions for which one law definition applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
#[non_exhaustive]
pub enum DecisionScope {
    /// Accept, Reject, and CommittedFailure.
    Always = 0,
    /// Accept only.
    Accept = 1,
    /// Reject only.
    Reject = 2,
    /// CommittedFailure only.
    CommittedFailure = 3,
    /// Accept and CommittedFailure.
    Committing = 4,
}

impl DecisionScope {
    const fn applies(self, decision: DecisionKind) -> bool {
        match self {
            Self::Always => true,
            Self::Accept => matches!(decision, DecisionKind::Accept),
            Self::Reject => matches!(decision, DecisionKind::Reject),
            Self::CommittedFailure => matches!(decision, DecisionKind::CommittedFailure),
            Self::Committing => !matches!(decision, DecisionKind::Reject),
        }
    }

    const fn covers_committing(self) -> bool {
        self.applies(DecisionKind::Accept) && self.applies(DecisionKind::CommittedFailure)
    }

    /// Returns true when a law with this scope is checked on every decision of
    /// `decision`'s kind.
    #[must_use]
    pub const fn covers(self, decision: DecisionKind) -> bool {
        self.applies(decision)
    }
}

/// The committing decisions an inductive claim assumes a law on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepCase {
    /// Accepts and committed failures alike.
    EveryCommit,
    /// Accepted decisions.
    Accepts,
    /// Committed failures.
    CommittedFailures,
}

impl StepCase {
    const fn enforced_by(self, scope: DecisionScope) -> bool {
        match self {
            Self::EveryCommit => scope.covers_committing(),
            Self::Accepts => scope.applies(DecisionKind::Accept),
            Self::CommittedFailures => scope.applies(DecisionKind::CommittedFailure),
        }
    }
}

/// Why an assumed law cannot support an inductive claim's step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssumptionGap {
    /// The manifest defines no law with this identifier.
    Missing {
        /// The assumed law.
        law: SemanticId,
        /// The decisions it was assumed on.
        case: StepCase,
    },
    /// The manifest defines the law, but does not check it on every decision
    /// the claim assumes it on.
    NotEnforced {
        /// The assumed law.
        law: SemanticId,
        /// The decisions it was assumed on.
        case: StepCase,
        /// The decisions the manifest checks it on.
        scope: DecisionScope,
    },
}

/// How a law manifest differs from the scopes its project declares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ScopeMismatch {
    /// The project declares a scope for a law the manifest does not define.
    Missing {
        /// The law.
        law: SemanticId,
    },
    /// The manifest enforces the law on other decisions than the project
    /// declares.
    Scope {
        /// The law.
        law: SemanticId,
        /// The decisions the project declares.
        declared: DecisionScope,
        /// The decisions the manifest checks the law on.
        enforced: DecisionScope,
    },
    /// The manifest applies the law at genesis where the project declares it
    /// does not, or the reverse.
    Genesis {
        /// The law.
        law: SemanticId,
        /// Whether the project declares `, genesis` for the law.
        declared: bool,
    },
}

/// The manifest scope a project's declared law scope stands for.
const fn declared_scope(scope: LawScope) -> DecisionScope {
    match scope {
        LawScope::Always => DecisionScope::Always,
        LawScope::Accept => DecisionScope::Accept,
        LawScope::Reject => DecisionScope::Reject,
        LawScope::CommittedFailure => DecisionScope::CommittedFailure,
        LawScope::Committing => DecisionScope::Committing,
    }
}

impl DecisionScope {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Whether one project law participates in the separately authorized genesis ceremony.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenesisApplicability {
    /// The law must be evaluated for the exact initial state and policy.
    Required,
    /// The law is inapplicable to genesis under a reviewed nonzero rationale.
    NotApplicable {
        /// Commitment to the reviewed inapplicability rationale.
        rationale_hash: Hash32,
    },
}

impl GenesisApplicability {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Required => output.push(0),
            Self::NotApplicable { rationale_hash } => {
                output.push(1);
                output.extend_from_slice(rationale_hash.as_bytes());
            }
        }
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Tool-neutral evidence coverage required by one law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawEvidenceRequirement {
    /// The reviewed deterministic runtime checker evaluates every invocation.
    RuntimeOnly,
    /// A complete finite domain is retained under an exact domain identity.
    ExhaustiveFinite {
        /// Exact finite-domain definition commitment.
        domain_hash: Hash32,
        /// Exact number of domain members.
        cardinality: u64,
    },
    /// A retained proof establishes the exact theorem claim.
    ProofAssisted {
        /// Exact theorem statement commitment.
        theorem_claim: Hash32,
    },
}

impl LawEvidenceRequirement {
    fn validate(self, claim_hash: Hash32) -> Result<(), LawError> {
        match self {
            Self::RuntimeOnly => Ok(()),
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => {
                if domain_hash == Hash32::ZERO {
                    return Err(LawError::ZeroBinding(LawField::Domain));
                }
                if cardinality == 0 {
                    return Err(LawError::ZeroCardinality);
                }
                Ok(())
            }
            Self::ProofAssisted { theorem_claim } => {
                if theorem_claim == Hash32::ZERO {
                    return Err(LawError::ZeroBinding(LawField::Theorem));
                }
                if theorem_claim != claim_hash {
                    return Err(LawError::TheoremClaimMismatch);
                }
                Ok(())
            }
        }
    }

    /// Returns whether this declaration requires retained independent evidence.
    #[must_use]
    pub const fn requires_retained_evidence(self) -> bool {
        !matches!(self, Self::RuntimeOnly)
    }
}

impl LawEvidenceRequirement {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::RuntimeOnly => output.push(0),
            Self::ExhaustiveFinite {
                domain_hash,
                cardinality,
            } => {
                output.push(1);
                output.extend_from_slice(domain_hash.as_bytes());
                output.extend_from_slice(&cardinality.to_be_bytes());
            }
            Self::ProofAssisted { theorem_claim } => {
                output.push(2);
                output.extend_from_slice(theorem_claim.as_bytes());
            }
        }
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// One stable project law committed by the profile claim registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawDefinition {
    id: SemanticId,
    name: StableName,
    kind: LawKind,
    scope: DecisionScope,
    genesis: GenesisApplicability,
    claim_hash: Hash32,
    checker_profile_hash: Hash32,
    evidence: LawEvidenceRequirement,
}

impl LawDefinition {
    /// Constructs one bounded tool-neutral law definition.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        id: SemanticId,
        name: StableName,
        kind: LawKind,
        scope: DecisionScope,
        genesis: GenesisApplicability,
        claim_hash: Hash32,
        checker_profile_hash: Hash32,
        evidence: LawEvidenceRequirement,
    ) -> Result<Self, LawError> {
        if claim_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::Claim));
        }
        if checker_profile_hash == Hash32::ZERO {
            return Err(LawError::ZeroBinding(LawField::CheckerProfile));
        }
        evidence.validate(claim_hash)?;
        validate_scope(kind, scope)?;
        validate_genesis_applicability(kind, genesis)?;
        Ok(Self {
            id,
            name,
            kind,
            scope,
            genesis,
            claim_hash,
            checker_profile_hash,
            evidence,
        })
    }

    /// Returns the stable law identifier.
    #[must_use]
    pub const fn id(&self) -> SemanticId {
        self.id
    }

    /// Returns the stable law name.
    #[must_use]
    pub const fn name(&self) -> &StableName {
        &self.name
    }

    /// Returns the law family.
    #[must_use]
    pub const fn kind(&self) -> LawKind {
        self.kind
    }

    /// Returns the decision scope.
    #[must_use]
    pub const fn scope(&self) -> DecisionScope {
        self.scope
    }

    /// Returns whether this law must be checked for genesis.
    #[must_use]
    pub const fn genesis_applicability(&self) -> GenesisApplicability {
        self.genesis
    }

    /// Returns the exact claim commitment.
    #[must_use]
    pub const fn claim_hash(&self) -> Hash32 {
        self.claim_hash
    }

    /// Returns the reviewed checker semantics commitment.
    #[must_use]
    pub const fn checker_profile_hash(&self) -> Hash32 {
        self.checker_profile_hash
    }

    /// Returns the required coverage class.
    #[must_use]
    pub const fn evidence_requirement(&self) -> LawEvidenceRequirement {
        self.evidence
    }

    fn registry_entry<H: CommitmentHasher>(&self) -> Result<RegistryEntry, LawError> {
        RegistryEntry::try_new(
            RegistryKind::Claim,
            self.id,
            self.name.clone(),
            hash_canonical::<H>(
                zeno_fcis_codec::domains::LAW_DEFINITION,
                (self).canonical_bytes(),
            )?,
        )
        .map_err(LawError::Profile)
    }
}

impl LawDefinition {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.name.encode_to(output)?;
        self.kind.encode_to(output)?;
        self.scope.encode_to(output)?;
        self.genesis.encode_to(output)?;
        output.extend_from_slice(self.claim_hash.as_bytes());
        output.extend_from_slice(self.checker_profile_hash.as_bytes());
        self.evidence.encode_to(output)
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Complete required-or-inapplicable policy and all stable project laws.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LawManifest {
    families: Box<[LawFamilyPolicy]>,
    definitions: Box<[LawDefinition]>,
}

impl LawManifest {
    /// Validates completeness, mandatory families, identifiers, names, and scopes.
    pub fn try_new(
        mut families: Vec<LawFamilyPolicy>,
        mut definitions: Vec<LawDefinition>,
    ) -> Result<Self, LawError> {
        if definitions.len() > MAX_PROJECT_LAWS {
            return Err(LawError::ResourceLimit);
        }
        families.sort_by_key(|policy| policy.kind);
        let actual = families
            .iter()
            .map(|policy| policy.kind)
            .collect::<Vec<_>>();
        if actual != LawKind::ALL {
            return Err(LawError::IncompleteFamilyPolicy);
        }
        for family in &families {
            if family.kind.mandatory()
                && matches!(
                    family.disposition,
                    LawFamilyDisposition::NotApplicable { .. }
                )
            {
                return Err(LawError::MandatoryFamilyNotApplicable(family.kind));
            }
            if let LawFamilyDisposition::NotApplicable { rationale_hash } = family.disposition
                && rationale_hash == Hash32::ZERO
            {
                return Err(LawError::ZeroBinding(LawField::Rationale));
            }
        }
        definitions.sort_by_key(LawDefinition::id);
        if definitions.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(LawError::DuplicateLawId);
        }
        let mut names = BTreeSet::new();
        for definition in &definitions {
            if !names.insert(definition.name.clone()) {
                return Err(LawError::DuplicateLawName);
            }
        }
        for family in &families {
            let count = definitions
                .iter()
                .filter(|definition| definition.kind == family.kind)
                .count();
            match family.disposition {
                LawFamilyDisposition::Required if count == 0 => {
                    return Err(LawError::MissingRequiredFamily(family.kind));
                }
                LawFamilyDisposition::NotApplicable { .. } if count != 0 => {
                    return Err(LawError::DefinitionForInapplicableFamily(family.kind));
                }
                _ => {}
            }
        }
        Ok(Self {
            families: families.into_boxed_slice(),
            definitions: definitions.into_boxed_slice(),
        })
    }

    /// Returns family policies in stable `LawKind` order.
    #[must_use]
    pub const fn families(&self) -> &[LawFamilyPolicy] {
        &self.families
    }

    /// Returns definitions in stable ID order.
    #[must_use]
    pub const fn definitions(&self) -> &[LawDefinition] {
        &self.definitions
    }

    /// Checks that this manifest enforces each law an inductive claim assumes
    /// on the decisions the claim assumes it on.
    ///
    /// A proof of an inductive claim's step says something about the
    /// application only when this passes, together with the base case on the
    /// exact genesis state. The groups mirror the claim's `assume`, `accept`,
    /// and `failure` lists, mapped to this manifest's identifiers.
    ///
    /// # Errors
    ///
    /// Returns every gap found, in group order.
    pub fn check_step_assumptions(
        &self,
        every_commit: &[SemanticId],
        accepts: &[SemanticId],
        committed_failures: &[SemanticId],
    ) -> Result<(), Vec<AssumptionGap>> {
        let mut gaps = Vec::new();
        for (case, group) in [
            (StepCase::EveryCommit, every_commit),
            (StepCase::Accepts, accepts),
            (StepCase::CommittedFailures, committed_failures),
        ] {
            for law in group {
                match self
                    .definitions
                    .iter()
                    .find(|definition| definition.id() == *law)
                {
                    None => gaps.push(AssumptionGap::Missing { law: *law, case }),
                    Some(definition) if !case.enforced_by(definition.scope()) => {
                        gaps.push(AssumptionGap::NotEnforced {
                            law: *law,
                            case,
                            scope: definition.scope(),
                        });
                    }
                    Some(_) => {}
                }
            }
        }
        if gaps.is_empty() { Ok(()) } else { Err(gaps) }
    }

    /// Checks that this manifest enforces each law exactly on the decisions
    /// `project.zeno` declares for it (`law ID name on SCOPE`), and at genesis
    /// exactly when the declaration says `, genesis`. A law without a declared
    /// scope is not compared.
    ///
    /// An application that runs this check before building its authority
    /// knows the declared scopes, which `check` and `prove` read, are the
    /// scopes its authority enforces.
    ///
    /// # Errors
    ///
    /// Returns every mismatch found, in law order.
    pub fn check_declared_scopes(&self, project: &ProjectSpec) -> Result<(), Vec<ScopeMismatch>> {
        let mut mismatches = Vec::new();
        for law in project.laws() {
            let Some(applicability) = law.applicability() else {
                continue;
            };
            // A stable ID is nonzero, so it is always a semantic ID.
            let Ok(id) = SemanticId::try_new(law.id().get()) else {
                continue;
            };
            let Some(definition) = self
                .definitions
                .iter()
                .find(|definition| definition.id() == id)
            else {
                mismatches.push(ScopeMismatch::Missing { law: id });
                continue;
            };
            let declared = declared_scope(applicability.scope());
            if definition.scope() != declared {
                mismatches.push(ScopeMismatch::Scope {
                    law: id,
                    declared,
                    enforced: definition.scope(),
                });
            }
            let at_genesis = matches!(
                definition.genesis_applicability(),
                GenesisApplicability::Required
            );
            if applicability.genesis() != at_genesis {
                mismatches.push(ScopeMismatch::Genesis {
                    law: id,
                    declared: applicability.genesis(),
                });
            }
        }
        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(mismatches)
        }
    }

    /// Computes the exact policy commitment bound by `ProjectProfile`.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, LawError> {
        hash_canonical::<H>(
            zeno_fcis_codec::domains::LAW_MANIFEST,
            (self).canonical_bytes(),
        )
    }

    /// Derives the complete stable claim registry for this manifest.
    pub fn registry_entries<H: CommitmentHasher>(&self) -> Result<Vec<RegistryEntry>, LawError> {
        self.definitions
            .iter()
            .map(LawDefinition::registry_entry::<H>)
            .collect()
    }
}

impl LawManifest {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-LAW-MANIFEST\0");
        output.extend_from_slice(&LAW_MANIFEST_FORMAT_VERSION.to_be_bytes());
        put_u32_length(output, self.families.len())?;
        for family in &self.families {
            family.encode_to(output)?;
        }
        put_u32_length(output, self.definitions.len())?;
        for definition in &self.definitions {
            put_blob(output, &definition.canonical_bytes()?)?;
        }
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Deterministic law-set resource bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LawLimits {
    /// Maximum manifest definitions.
    pub max_definitions: u32,
    /// Maximum retained formal evidence envelopes.
    pub max_evidence: u32,
    /// Maximum retained artifact bytes across the complete law set.
    pub max_artifact_bytes: u64,
    /// Maximum per-invocation observations.
    pub max_observations: u32,
}

impl Default for LawLimits {
    fn default() -> Self {
        Self {
            max_definitions: 4_096,
            max_evidence: 4_096,
            max_artifact_bytes: 64 * 1024 * 1024,
            max_observations: 4_096,
        }
    }
}

impl LawLimits {
    /// Checks the complete deterministic limit envelope without constructing authority.
    pub fn validate(self) -> Result<(), LawError> {
        let hard = u32::try_from(MAX_PROJECT_LAWS).map_err(|_| LawError::ResourceLimit)?;
        if self.max_definitions == 0
            || self.max_evidence == 0
            || self.max_observations == 0
            || self.max_artifact_bytes == 0
            || self.max_definitions > hard
            || self.max_evidence > hard
            || self.max_observations > hard
        {
            return Err(LawError::InvalidLimits);
        }
        Ok(())
    }
}

impl LawLimits {
    /// Appends this protocol type's exact canonical encoding.
    pub fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.max_definitions.to_be_bytes());
        output.extend_from_slice(&self.max_evidence.to_be_bytes());
        output.extend_from_slice(&self.max_artifact_bytes.to_be_bytes());
        output.extend_from_slice(&self.max_observations.to_be_bytes());
        Ok(())
    }

    /// Returns this protocol type's exact canonical bytes.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut output = Vec::new();
        self.encode_to(&mut output)?;
        Ok(output)
    }
}

/// Closed checker result for one applicable law.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
#[non_exhaustive]
pub enum LawStatus {
    /// The law holds for this exact input.
    Satisfied = 0,
    /// A retained counterexample violates the law.
    Violated = 1,
    /// The checker cannot decide within the admitted semantics or bounds.
    Indeterminate = 2,
}

/// Closed failures from one deterministic project law checker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LawEngineFailure {
    /// The input is outside the checker's reviewed model.
    Unsupported,
    /// Logical fuel or another deterministic bound was exhausted.
    Incomplete,
    /// The checker emitted malformed or contradictory output.
    InvalidOutput,
}

/// Checks all economic law families required by the catalog's exact operations.
/// This metadata check constructs no law engine or V2 authorization.
pub fn validate_catalog_law_requirements(
    catalog: &ProjectCatalog,
    manifest: &LawManifest,
) -> Result<(), LawError> {
    let mut required = BTreeSet::new();
    let mut custom_claims = BTreeSet::new();
    let semantics = catalog
        .manifest()
        .effects()
        .iter()
        .map(|definition| definition.semantics())
        .chain(
            catalog
                .manifest()
                .channels()
                .iter()
                .map(|definition| definition.semantics()),
        );
    for operation in semantics {
        if !operation.is_value_moving() {
            continue;
        }
        for flow in operation.flows().iter().copied() {
            required.insert(LawKind::AssetConservation);
            required.insert(LawKind::AuthoritySubjectRecipient);
            match flow.kind() {
                ValueFlowKind::Transfer
                | ValueFlowKind::EscrowLock
                | ValueFlowKind::EscrowRelease
                | ValueFlowKind::ExternalValueDelivery => {
                    required.insert(LawKind::DebitCreditEffectEquality);
                }
                ValueFlowKind::Mint | ValueFlowKind::Burn => {
                    required.insert(LawKind::MintBurnAuthorization);
                }
                ValueFlowKind::FeeCharge => {
                    required.insert(LawKind::DebitCreditEffectEquality);
                    required.insert(LawKind::FeeAndRounding);
                }
                ValueFlowKind::Settlement | ValueFlowKind::Custom => {
                    required.insert(LawKind::MintBurnAuthorization);
                    required.insert(LawKind::DebitCreditEffectEquality);
                    required.insert(LawKind::FeeAndRounding);
                }
            }
            if let Some(claim) = flow.custom_claim() {
                custom_claims.insert(claim);
            }
        }
    }
    for kind in required {
        let policy = manifest
            .families
            .iter()
            .find(|policy| policy.kind == kind)
            .ok_or(LawError::IncompleteFamilyPolicy)?;
        if !matches!(policy.disposition, LawFamilyDisposition::Required) {
            return Err(LawError::CatalogRequiredFamilyNotApplicable(kind));
        }
        let mut definitions = manifest
            .definitions
            .iter()
            .filter(|definition| definition.kind == kind);
        let covers_accept = definitions
            .clone()
            .any(|definition| definition.scope.applies(DecisionKind::Accept));
        let covers_committed_failure =
            definitions.any(|definition| definition.scope.applies(DecisionKind::CommittedFailure));
        if !covers_accept || !covers_committed_failure {
            return Err(LawError::MissingCommittingCoverage(kind));
        }
    }
    for (claim_id, claim_hash) in custom_claims {
        let definition = manifest
            .definitions
            .binary_search_by_key(&claim_id, LawDefinition::id)
            .ok()
            .map(|index| &manifest.definitions[index])
            .ok_or(LawError::MissingCustomValueLaw(claim_id))?;
        if definition.claim_hash != claim_hash {
            return Err(LawError::CustomValueLawClaimMismatch(claim_id));
        }
        if !definition.scope.covers_committing() {
            return Err(LawError::MissingCustomValueLawCoverage(claim_id));
        }
        if !definition.evidence.requires_retained_evidence() {
            return Err(LawError::CustomValueLawRequiresEvidence(claim_id));
        }
    }
    Ok(())
}

fn validate_scope(kind: LawKind, scope: DecisionScope) -> Result<(), LawError> {
    match (kind, scope) {
        (LawKind::StateInvariant, DecisionScope::Committing)
        | (LawKind::RejectNoAuthority, DecisionScope::Reject)
        | (LawKind::CommittedFailureEffects, DecisionScope::CommittedFailure) => Ok(()),
        (
            LawKind::StateInvariant | LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            _,
        ) => Err(LawError::InvalidMandatoryScope(kind)),
        _ => Ok(()),
    }
}

fn validate_genesis_applicability(
    kind: LawKind,
    genesis: GenesisApplicability,
) -> Result<(), LawError> {
    match (kind, genesis) {
        (LawKind::StateInvariant, GenesisApplicability::Required)
        | (
            LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            GenesisApplicability::NotApplicable { .. },
        ) => {}
        (
            LawKind::StateInvariant | LawKind::RejectNoAuthority | LawKind::CommittedFailureEffects,
            _,
        ) => return Err(LawError::InvalidGenesisApplicability(kind)),
        _ => {}
    }
    if let GenesisApplicability::NotApplicable { rationale_hash } = genesis
        && rationale_hash == Hash32::ZERO
    {
        return Err(LawError::ZeroBinding(LawField::Rationale));
    }
    Ok(())
}

fn hash_canonical<H: CommitmentHasher>(
    domain: Domain<'static>,
    value: Result<Vec<u8>, EncodeError>,
) -> Result<Hash32, LawError> {
    let bytes = value.map_err(LawError::Encode)?;
    commitment::<H>(domain, &bytes).map_err(LawError::Encode)
}

fn put_u32_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    put_u32_length(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Exact field rejected for a zero authority binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LawField {
    /// Family inapplicability rationale.
    Rationale,
    /// Declarative law claim.
    Claim,
    /// Reviewed checker semantics.
    CheckerProfile,
    /// Exhaustive finite domain.
    Domain,
    /// Proof theorem.
    Theorem,
    /// Project catalog.
    Catalog,
    /// Complete authorization policy.
    Policy,
    /// Reviewed genesis-policy binding.
    Genesis,
    /// Exact invocation.
    Invocation,
    /// Law observation or counterexample.
    Observation,
    /// Runtime law-engine build.
    EngineBuild,
    /// Independent evidence-verifier build.
    EvidenceVerifier,
    /// Independent verification claim.
    VerificationClaim,
}

/// Fail-closed law manifest, evidence, or evaluation error.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum LawError {
    /// A required content binding was zero.
    ZeroBinding(LawField),
    /// A mandatory family was marked inapplicable.
    MandatoryFamilyNotApplicable(LawKind),
    /// The family policy does not contain each closed family exactly once.
    IncompleteFamilyPolicy,
    /// A required family has no definition.
    MissingRequiredFamily(LawKind),
    /// An inapplicable family contains a hidden definition.
    DefinitionForInapplicableFamily(LawKind),
    /// The exact catalog carries value but marks a derived economic family inapplicable.
    CatalogRequiredFamilyNotApplicable(LawKind),
    /// A catalog-required economic family does not cover both committing decisions.
    MissingCommittingCoverage(LawKind),
    /// A custom value flow names no law in the complete manifest.
    MissingCustomValueLaw(SemanticId),
    /// A custom value flow's claim differs from the registered law claim.
    CustomValueLawClaimMismatch(SemanticId),
    /// A custom value-flow law does not cover both committing decisions.
    MissingCustomValueLawCoverage(SemanticId),
    /// A custom value-flow law lacks independently retained evidence.
    CustomValueLawRequiresEvidence(SemanticId),
    /// Two definitions share one stable identifier.
    DuplicateLawId,
    /// Two definitions share one stable name.
    DuplicateLawName,
    /// A mandatory family uses a scope that weakens its meaning.
    InvalidMandatoryScope(LawKind),
    /// A mandatory framework law has invalid genesis applicability.
    InvalidGenesisApplicability(LawKind),
    /// A proof-assisted theorem does not equal the law claim.
    TheoremClaimMismatch,
    /// Exhaustive coverage declared zero members.
    ZeroCardinality,
    /// Resource limits are zero or exceed hard bounds.
    InvalidLimits,
    /// A deterministic hard limit was exceeded.
    ResourceLimit,
    /// The manifest does not equal the profile policy commitment.
    PolicyBindingMismatch,
    /// Generated claim entries do not equal the complete profile claim registry.
    ClaimRegistryMismatch,
    /// Retained evidence does not equal the exact required law set.
    EvidenceSetMismatch,
    /// Two retained envelopes target one law.
    DuplicateLawEvidence,
    /// Evidence source, claim, or query identity differs.
    EvidenceBindingMismatch,
    /// Evidence coverage differs from the law requirement.
    EvidenceCoverageMismatch,
    /// Retained assumptions are not strictly ordered and duplicate-free.
    NonCanonicalAssumptions,
    /// Retained artifact bytes do not match the envelope digest.
    ArtifactDigestMismatch(SemanticId),
    /// The independently mounted checker refuted the exact law subject.
    EvidenceRefuted {
        /// Stable law identifier.
        law_id: SemanticId,
        /// Retained counterexample commitment.
        counterexample_hash: Hash32,
    },
    /// The independently mounted checker could not attest the exact law.
    EvidenceIndeterminate(SemanticId),
    /// An invocation was checked against another catalog.
    CatalogMismatch,
    /// The checker returned one law more than once.
    DuplicateObservation,
    /// Missing or hidden extra observations were returned.
    ObservationSetMismatch,
    /// One exact law was violated or indeterminate.
    LawNotSatisfied {
        /// Stable law identifier.
        law_id: SemanticId,
        /// Checker result.
        status: LawStatus,
        /// Result or counterexample commitment.
        witness_hash: Hash32,
    },
    /// The reviewed runtime checker failed closed.
    Engine(LawEngineFailure),
    /// Canonical encoding failed.
    Encode(EncodeError),
    /// Catalog validation or commitment failed.
    Catalog(CatalogError),
    /// Stable registry construction failed.
    Profile(ProfileError),
}

impl fmt::Display for LawError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "project law validation failed: {self:?}")
    }
}

impl core::error::Error for LawError {}
