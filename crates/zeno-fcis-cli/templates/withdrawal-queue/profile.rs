//! Reviewed example bindings shared by generation and the runtime law checker.
//! These hashes identify local source and policy, not independently certified builds.
//!
//! Every function here builds a value from reviewed constants and the exact
//! source. Each returns an error instead of panicking, so the application
//! reports a broken binding rather than aborting; `build.rs` turns such an
//! error into a build failure.

use zeno_fcis_codec::{CanonicalEncode, Domain, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawEvidenceRequirement, LawFamilyPolicy,
    LawKind, LawManifest,
};
use zeno_fcis_project::{SemanticId, StableName};
use zeno_fcis_spec::{ProjectLimits, ProjectSpec, SourceLimits, elaborate_project, parse_project};

/// The committed-failure law. This application never commits a failure, so
/// the law checker refuses any committed failure outright. It has no formula,
/// so it is registered here instead of as an always-false law in
/// `project.zeno`.
pub const NO_COMMITTED_FAILURES: u32 = 508;
/// The rejection law. The framework checks it for every rejection: a
/// rejection commits no state, effect, or outbox entry. It has no formula, so
/// it is registered here instead of as an always-true law in `project.zeno`.
pub const REJECT_PUBLISHES_NOTHING: u32 = 509;

/// A reviewed constant or the authored source failed the library's admission.
/// Every fallible function in this file returns it, naming what was refused.
#[derive(Debug)]
pub struct BindingError(pub &'static str);

impl core::fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "reviewed binding failed: {}", self.0)
    }
}

/// A domain-separated commitment to `bytes` under the reviewed `label`.
///
/// # Errors
///
/// A label the codec refuses, or a payload beyond its length bound.
pub fn digest(label: &str, bytes: &[u8]) -> Result<Hash32, BindingError> {
    let domain = Domain::new(label, 1).map_err(|_| BindingError("domain label"))?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(|_| BindingError("commitment"))
}

/// # Errors
///
/// Zero, which is not a semantic ID.
pub fn id(raw: u32) -> Result<SemanticId, BindingError> {
    SemanticId::try_new(raw).map_err(|_| BindingError("semantic ID"))
}

/// # Errors
///
/// A label that is not a stable name.
pub fn name(label: &str) -> Result<StableName, BindingError> {
    StableName::try_new(label).map_err(|_| BindingError("stable name"))
}

/// The authored project, parsed and elaborated from `project.zeno`.
///
/// # Errors
///
/// A source that does not parse or elaborate, which `zeno-fcis check` reports
/// in full.
pub fn project() -> Result<ProjectSpec, BindingError> {
    let parsed = parse_project(include_str!("project.zeno"), SourceLimits::default())
        .map_err(|_| BindingError("authored project does not parse"))?;
    elaborate_project(parsed, ProjectLimits::default())
        .map_err(|_| BindingError("authored project does not elaborate"))
}

/// The exact source, including the checked controller: the synthesis problem
/// and its selected step, the contract, the contract's pin, the accepted
/// strategy, the model that wrote them, and the Rust tables.
///
/// # Errors
///
/// A commitment the codec refuses.
pub fn source_hash() -> Result<Hash32, BindingError> {
    let source = digest(
        "example/withdrawal-queue/source",
        concat!(
            include_str!("project.zeno"),
            "\0",
            include_str!("profile.rs"),
            "\0",
            include_str!("build.rs"),
            "\0",
            include_str!("Cargo.toml"),
            "\0",
            include_str!("src/lib.rs"),
            "\0",
            include_str!("src/program.rs"),
            "\0",
            include_str!("src/laws.rs"),
            "\0",
            include_str!("src/delivery.rs"),
            "\0",
            include_str!("src/controller.rs"),
            "\0",
            include_str!("synthesis.json"),
            "\0",
            include_str!("synthesized/problem.json"),
            "\0",
            include_str!("synthesized/manifest.json"),
            "\0",
            include_str!("synthesized/vectors.json"),
            "\0",
            include_str!("synthesized/transition.rs"),
            "\0",
            include_str!("controller/contract.json"),
            "\0",
            include_str!("controller/contract.sha256"),
            "\0",
            include_str!("controller/strategy.json"),
            "\0",
            include_str!("controller/model.py")
        )
        .as_bytes(),
    )?;
    let program = digest(
        "example/withdrawal-queue/closed-ir",
        include_bytes!("synthesized/program.zcve"),
    )?;
    digest(
        "example/withdrawal-queue/source-complete",
        &[source.as_bytes().as_slice(), program.as_bytes().as_slice()].concat(),
    )
}

/// The decision's identity: the source hash under the program domain.
///
/// # Errors
///
/// A commitment the codec refuses.
pub fn program_hash() -> Result<Hash32, BindingError> {
    digest(
        "example/withdrawal-queue/program",
        source_hash()?.as_bytes(),
    )
}

/// The law checker's identity: the source hash under the checker domain.
///
/// # Errors
///
/// A commitment the codec refuses.
pub fn checker_hash() -> Result<Hash32, BindingError> {
    digest(
        "example/withdrawal-queue/checker",
        source_hash()?.as_bytes(),
    )
}

/// The runtime-only law manifest: every law family's policy, each authored
/// law bound to its kind and scope, and the two laws without formulas.
///
/// # Errors
///
/// A law without a reviewed binding, or a definition the law library
/// refuses.
pub fn manifest() -> Result<LawManifest, BindingError> {
    let project = project()?;
    let checker = checker_hash()?;
    let required = [
        LawKind::StateInvariant,
        LawKind::AssetConservation,
        LawKind::DebitCreditEffectEquality,
        LawKind::AuthoritySubjectRecipient,
        LawKind::RejectNoAuthority,
        LawKind::CommittedFailureEffects,
    ];
    let mut families = Vec::new();
    for kind in LawKind::ALL {
        families.push(if required.contains(&kind) {
            LawFamilyPolicy::required(kind)
        } else {
            LawFamilyPolicy::not_applicable(
                kind,
                digest(
                    "example/withdrawal-queue/not-applicable",
                    b"No minting, burning, or fees: deposits are the only inflow and payouts the only outflow",
                )?,
            )
            .map_err(|_| BindingError("non-value family"))?
        });
    }
    let mut definitions = Vec::new();
    for law in project.laws() {
        // This application never commits a failure: the law checker refuses
        // every one (law 508). So each action law is enforced on every
        // committing decision, which lets claims 600 and 601 assume it on
        // every commit.
        let (kind, scope, genesis) = match law.id().get() {
            500 => (
                LawKind::StateInvariant,
                DecisionScope::Committing,
                GenesisApplicability::Required,
            ),
            501 => (
                LawKind::AssetConservation,
                DecisionScope::Committing,
                no_genesis()?,
            ),
            502 => (
                LawKind::AuthoritySubjectRecipient,
                DecisionScope::Committing,
                no_genesis()?,
            ),
            503 => (
                LawKind::DebitCreditEffectEquality,
                DecisionScope::Committing,
                no_genesis()?,
            ),
            _ => return Err(BindingError("a new law needs a reviewed runtime binding")),
        };
        let mut claim = project
            .canonical_bytes()
            .map_err(|_| BindingError("canonical authored AST"))?;
        claim.extend_from_slice(&law.id().get().to_be_bytes());
        definitions.push(
            LawDefinition::try_new(
                id(law.id().get())?,
                name(law.name().as_str())?,
                kind,
                scope,
                genesis,
                digest("example/withdrawal-queue/law", &claim)?,
                checker,
                LawEvidenceRequirement::RuntimeOnly,
            )
            .map_err(|_| BindingError("runtime law definition"))?,
        );
    }
    definitions.push(
        LawDefinition::try_new(
            id(NO_COMMITTED_FAILURES)?,
            name("no_committed_failures")?,
            LawKind::CommittedFailureEffects,
            DecisionScope::CommittedFailure,
            no_genesis()?,
            digest(
                "example/withdrawal-queue/framework-law",
                b"This application never commits a failure",
            )?,
            checker,
            LawEvidenceRequirement::RuntimeOnly,
        )
        .map_err(|_| BindingError("no-committed-failure law"))?,
    );
    definitions.push(
        LawDefinition::try_new(
            id(REJECT_PUBLISHES_NOTHING)?,
            name("reject_publishes_nothing")?,
            LawKind::RejectNoAuthority,
            DecisionScope::Reject,
            no_genesis()?,
            digest(
                "example/withdrawal-queue/framework-law",
                b"A rejection commits no state, effect, or outbox entry",
            )?,
            checker,
            LawEvidenceRequirement::RuntimeOnly,
        )
        .map_err(|_| BindingError("framework rejection law"))?,
    );
    LawManifest::try_new(families, definitions)
        .map_err(|_| BindingError("complete reviewed law manifest"))
}

fn no_genesis() -> Result<GenesisApplicability, BindingError> {
    Ok(GenesisApplicability::NotApplicable {
        rationale_hash: digest(
            "example/withdrawal-queue/no-genesis",
            b"This law relates an invocation to its decision",
        )?,
    })
}
