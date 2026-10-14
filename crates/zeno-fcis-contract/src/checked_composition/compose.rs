//! A checked composition as `zeno-fcis-compose` claims.
//!
//! [`require_checked`] runs the whole check on the sources itself and
//! accepts a specification only when it is exactly the one the manifest
//! denotes, which compose judged, and the composition's claim is
//! unconditional. It returns a [`CheckedAssumeGuarantee`], the checked
//! class of that specification's guarantees and assumption discharges. A
//! composition whose root group's outer assumption leaves a condition is
//! refused: its claims hold only under that condition, which compose's
//! claims do not state. No function here takes an `EvidenceVerifier` or a
//! `CompositionReport`: whatever a generic verifier accepts stays attested,
//! and only the checker's own run produces the checked class.

use zeno_fcis_compose::{CompositionClaim, CompositionSpec, ContractError as ComposeError};
use zeno_fcis_crypto::RustCryptoSha256;

use super::{ClaimKind, CompositionSources, Condition, Refusal, check, hex};

/// The guarantees and assumption discharges of one exact specification,
/// each established by a checked composition whose claim is unconditional:
/// a leaf's guarantee by its local and StateFact tables, a discharge by the
/// consumer's interface table or, under groups, by the group checks; a
/// group's outer guarantee by its guarantee table, and its delivery of its
/// outer assumption by the checks of its parent. Fields are private and
/// only [`require_checked`] builds one.
///
/// ```compile_fail,E0451
/// use zeno_fcis_contract::checked_composition::CheckedAssumeGuarantee;
/// let forged = CheckedAssumeGuarantee { claims: Vec::new() };
/// ```
///
/// A compose report, whatever its verifier accepted, does not convert:
///
/// ```compile_fail,E0277
/// use zeno_fcis_compose::CompositionReport;
/// use zeno_fcis_contract::checked_composition::CheckedAssumeGuarantee;
/// fn promote(report: CompositionReport) -> CheckedAssumeGuarantee {
///     report.into()
/// }
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedAssumeGuarantee {
    spec_hash: [u8; 32],
    manifest_commitment: [u8; 32],
    checker_source_digest: [u8; 32],
    claims: Vec<CompositionClaim>,
}

impl CheckedAssumeGuarantee {
    /// The kind of the composition's claim; always unconditional, since
    /// [`require_checked`] refuses any other.
    #[must_use]
    pub const fn claim_kind(&self) -> ClaimKind {
        ClaimKind::Unconditional
    }

    /// The claim's conditions; always empty, since [`require_checked`]
    /// refuses a composition with any.
    #[must_use]
    pub const fn conditions(&self) -> &[Condition] {
        &[]
    }

    /// The commitment of the specification these claims belong to.
    #[must_use]
    pub const fn spec_hash(&self) -> [u8; 32] {
        self.spec_hash
    }

    /// The commitment of the checked manifest.
    #[must_use]
    pub const fn manifest_commitment(&self) -> [u8; 32] {
        self.manifest_commitment
    }

    /// The source-bound identity of the checker that established them.
    #[must_use]
    pub const fn checker_source_digest(&self) -> [u8; 32] {
        self.checker_source_digest
    }

    /// Every guarantee and assumption discharge, as compose states them.
    #[must_use]
    pub fn claims(&self) -> &[CompositionClaim] {
        &self.claims
    }

    /// Whether the checker established this exact claim.
    #[must_use]
    pub fn covers(&self, claim: &CompositionClaim) -> bool {
        self.claims.contains(claim)
    }
}

/// Why a specification was not accepted as checked.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ComposeRefusal {
    /// The check refused the sources.
    Check(Refusal),
    /// Compose cannot commit the supplied specification.
    Spec(ComposeError),
    /// The composition's claim is not unconditional: its root group's outer
    /// assumption leaves conditions, which compose's claims do not state.
    Conditional {
        /// The claim's kind.
        kind: ClaimKind,
        /// How many conditions the root leaves.
        conditions: usize,
    },
    /// The supplied specification is not the one the manifest denotes.
    Different {
        /// The commitment of the specification the manifest denotes.
        expected: [u8; 32],
        /// The commitment of the supplied specification.
        actual: [u8; 32],
    },
}

impl std::fmt::Display for ComposeRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Check(refusal) => write!(formatter, "the check refused: {refusal}"),
            Self::Spec(error) => {
                write!(
                    formatter,
                    "compose cannot commit the specification: {error}"
                )
            }
            Self::Conditional { kind, conditions } => write!(
                formatter,
                "the claim is {} with {conditions} conditions; compose's claims would drop them",
                kind.name()
            ),
            Self::Different { expected, actual } => write!(
                formatter,
                "the specification {} is not the checked one {}",
                hex(actual),
                hex(expected)
            ),
        }
    }
}

impl std::error::Error for ComposeRefusal {}

/// Checks the composition from its sources and returns the checked class
/// of `spec`'s claims when `spec` is exactly the specification its manifest
/// denotes and the composition's claim is unconditional.
///
/// # Errors
/// The check's refusal, a claim with conditions, or a specification compose
/// cannot commit or that differs from the checked one.
pub fn require_checked(
    spec: &CompositionSpec,
    sources: CompositionSources<'_>,
) -> Result<CheckedAssumeGuarantee, ComposeRefusal> {
    let checked = check(sources).map_err(ComposeRefusal::Check)?;
    if checked.claim_kind() != ClaimKind::Unconditional {
        return Err(ComposeRefusal::Conditional {
            kind: checked.claim_kind(),
            conditions: checked.conditions().len(),
        });
    }
    let actual = *spec
        .commitment::<RustCryptoSha256>()
        .map_err(ComposeRefusal::Spec)?
        .as_bytes();
    if actual != checked.spec_hash || *spec != checked.spec {
        return Err(ComposeRefusal::Different {
            expected: checked.spec_hash,
            actual,
        });
    }
    Ok(CheckedAssumeGuarantee {
        spec_hash: checked.spec_hash,
        manifest_commitment: checked.manifest_commitment,
        checker_source_digest: checked.identity.digest(),
        claims: checked.claims,
    })
}
