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

pub fn digest(label: &str, bytes: &[u8]) -> Hash32 {
    commitment::<RustCryptoSha256>(Domain::new(label, 1).expect("static domain"), bytes)
        .expect("bounded source commitment")
}

pub fn id(raw: u32) -> SemanticId {
    SemanticId::try_new(raw).expect("reviewed nonzero ID")
}

pub fn name(label: &str) -> StableName {
    StableName::try_new(label).expect("reviewed stable name")
}

pub fn project() -> ProjectSpec {
    let parsed = parse_project(include_str!("project.zeno"), SourceLimits::default())
        .expect("parse authored project");
    elaborate_project(parsed, ProjectLimits::default()).expect("check authored project")
}

pub fn source_hash() -> Hash32 {
    let source = digest(
        "example/compliance-gateway/source",
        concat!(
            include_str!("project.zeno"),
            "\0",
            include_str!("rules.txt"),
            "\0",
            include_str!("profile.rs"),
            "\0",
            include_str!("build.rs"),
            "\0",
            include_str!("Cargo.toml"),
            "\0",
            include_str!("src/lib.rs"),
            "\0",
            include_str!("src/rules.rs"),
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
        "example/compliance-gateway/closed-ir",
        include_bytes!("synthesized/program.zcve"),
    );
    digest(
        "example/compliance-gateway/source-complete",
        &[source.as_bytes().as_slice(), program.as_bytes().as_slice()].concat(),
    )
}

pub fn program_hash() -> Hash32 {
    digest(
        "example/compliance-gateway/program",
        source_hash().as_bytes(),
    )
}

pub fn checker_hash() -> Hash32 {
    digest(
        "example/compliance-gateway/checker",
        source_hash().as_bytes(),
    )
}

pub fn manifest() -> LawManifest {
    let project = project();
    let required = [
        LawKind::StateInvariant,
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
                        "example/compliance-gateway/non-value",
                        b"No assets, fees, debits, credits or value delivery",
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
                501 | 502 => (
                    LawKind::AuthoritySubjectRecipient,
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                503 => (
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
                digest("example/compliance-gateway/law", &claim),
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
                "example/compliance-gateway/framework-law",
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
            "example/compliance-gateway/no-genesis",
            b"This law relates an invocation to its decision",
        ),
    }
}
