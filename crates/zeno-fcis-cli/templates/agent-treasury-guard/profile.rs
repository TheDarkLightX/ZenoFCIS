//! Reviewed example bindings shared by generation and the runtime law checker.
//! These hashes identify local source and policy, not independently certified builds.

use zeno_fcis_codec::{CanonicalEncode, Domain, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawEvidenceRequirement, LawFamilyPolicy,
    LawKind, LawManifest,
};
use zeno_fcis_project::{SemanticId, StableName};
use zeno_fcis_spec::{ProjectLimits, ProjectSpec, SourceLimits, elaborate_project, parse_project};

/// The rejection law. The framework checks it for every rejection: a
/// rejection commits no state, effect, or outbox entry. It has no formula, so
/// it is registered here instead of as an always-true law in `project.zeno`.
pub const REJECT_PUBLISHES_NOTHING: u32 = 509;

/// A domain-separated commitment over reviewed bytes.
///
/// # Panics
///
/// On a label that is not a valid domain, which the constants here never
/// are.
#[must_use]
pub fn digest(label: &str, bytes: &[u8]) -> Hash32 {
    commitment::<RustCryptoSha256>(Domain::new(label, 1).expect("static domain"), bytes)
        .expect("bounded source commitment")
}

/// A reviewed nonzero semantic ID.
///
/// # Panics
///
/// On zero, which the IDs in `project.zeno` never are.
#[must_use]
pub fn id(raw: u32) -> SemanticId {
    SemanticId::try_new(raw).expect("reviewed nonzero ID")
}

/// A reviewed stable name.
///
/// # Panics
///
/// On a name the registry cannot hold, which the names here never are.
#[must_use]
pub fn name(label: &str) -> StableName {
    StableName::try_new(label).expect("reviewed stable name")
}

/// The authored project, parsed and checked.
///
/// # Panics
///
/// If `project.zeno` does not elaborate; `zeno-fcis check` reports why.
#[must_use]
pub fn project() -> ProjectSpec {
    let parsed = parse_project(include_str!("project.zeno"), SourceLimits::default())
        .expect("parse authored project");
    elaborate_project(parsed, ProjectLimits::default()).expect("check authored project")
}

/// The identity of this template's reviewed source, including the synthesis.
#[must_use]
pub fn source_hash() -> Hash32 {
    let source = digest(
        "example/agent-treasury-guard/source",
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
            include_str!("synthesis.json"),
            "\0",
            include_str!("synthesized/problem.json"),
            "\0",
            include_str!("synthesized/manifest.json"),
            "\0",
            include_str!("synthesized/vectors.json"),
            "\0",
            include_str!("synthesized/transition.rs")
        )
        .as_bytes(),
    );
    let program = digest(
        "example/agent-treasury-guard/closed-ir",
        include_bytes!("synthesized/program.zcve"),
    );
    digest(
        "example/agent-treasury-guard/source-complete",
        &[source.as_bytes().as_slice(), program.as_bytes().as_slice()].concat(),
    )
}

#[must_use]
pub fn program_hash() -> Hash32 {
    digest(
        "example/agent-treasury-guard/program",
        source_hash().as_bytes(),
    )
}

#[must_use]
pub fn checker_hash() -> Hash32 {
    digest(
        "example/agent-treasury-guard/checker",
        source_hash().as_bytes(),
    )
}

/// The runtime-only law manifest: every law's family, scope, and genesis
/// applicability. One arm per law keeps the binding reviewable, so the
/// function is long. The scopes are what claims 600 and 601 assume: 501 and
/// 502 on every committing decision, 503 to 507 on accepts, and 508 on
/// committed failures; `tests/claims.rs` checks the claims against them.
///
/// # Panics
///
/// If a law in `project.zeno` has no binding here, or the manifest is
/// incomplete; both are reviewed constants.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn manifest() -> LawManifest {
    let project = project();
    // The swap channel is classified non-value, because the sold amount leaves
    // this state in the same decision. The economic families are required all
    // the same: the balances here are the treasury's own.
    let required = [
        LawKind::StateInvariant,
        LawKind::AssetConservation,
        LawKind::DebitCreditEffectEquality,
        LawKind::FeeAndRounding,
        LawKind::AuthoritySubjectRecipient,
        LawKind::RejectNoAuthority,
        LawKind::CommittedFailureEffects,
    ];
    let families = LawKind::ALL
        .into_iter()
        .map(|kind| {
            if required.contains(&kind) {
                LawFamilyPolicy::required(kind)
            } else {
                LawFamilyPolicy::not_applicable(
                    kind,
                    digest(
                        "example/agent-treasury-guard/not-applicable",
                        b"No supply is minted or burned; the treasury only exchanges assets it holds",
                    ),
                )
                .expect("non-value family")
            }
        })
        .collect();
    let mut definitions = project
        .laws()
        .iter()
        .map(|law| {
            let (kind, scope, genesis) = match law.id().get() {
                500 => (
                    LawKind::StateInvariant,
                    DecisionScope::Committing,
                    GenesisApplicability::Required,
                ),
                501 => (
                    LawKind::AuthoritySubjectRecipient,
                    DecisionScope::Committing,
                    no_genesis(),
                ),
                502 => (
                    LawKind::AssetConservation,
                    DecisionScope::Committing,
                    no_genesis(),
                ),
                503 => (
                    LawKind::AuthoritySubjectRecipient,
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                504 | 505 => (
                    LawKind::AssetConservation,
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                506 => (
                    LawKind::DebitCreditEffectEquality,
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                507 => (LawKind::FeeAndRounding, DecisionScope::Accept, no_genesis()),
                508 => (
                    LawKind::CommittedFailureEffects,
                    DecisionScope::CommittedFailure,
                    no_genesis(),
                ),
                _ => panic!("a new law needs a reviewed runtime binding"),
            };
            let mut claim = project.canonical_bytes().expect("canonical authored AST");
            claim.extend_from_slice(&law.id().get().to_be_bytes());
            LawDefinition::try_new(
                id(law.id().get()),
                name(law.name().as_str()),
                kind,
                scope,
                genesis,
                digest("example/agent-treasury-guard/law", &claim),
                checker_hash(),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .expect("runtime law definition")
        })
        .collect::<Vec<_>>();
    definitions.push(
        LawDefinition::try_new(
            id(REJECT_PUBLISHES_NOTHING),
            name("reject_publishes_nothing"),
            LawKind::RejectNoAuthority,
            DecisionScope::Reject,
            no_genesis(),
            digest(
                "example/agent-treasury-guard/framework-law",
                b"A rejection commits no state, effect, or outbox entry",
            ),
            checker_hash(),
            LawEvidenceRequirement::RuntimeOnly,
        )
        .expect("framework rejection law"),
    );
    LawManifest::try_new(families, definitions).expect("complete reviewed law manifest")
}

fn no_genesis() -> GenesisApplicability {
    GenesisApplicability::NotApplicable {
        rationale_hash: digest(
            "example/agent-treasury-guard/no-genesis",
            b"This law relates an invocation to its decision",
        ),
    }
}
