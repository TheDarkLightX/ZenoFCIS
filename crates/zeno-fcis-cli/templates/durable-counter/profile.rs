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
    digest(
        "example/durable-counter/source",
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
            include_str!("src/delivery.rs")
        )
        .as_bytes(),
    )
}

pub fn program_hash() -> Hash32 {
    digest("example/durable-counter/program", source_hash().as_bytes())
}

pub fn checker_hash() -> Hash32 {
    digest("example/durable-counter/checker", source_hash().as_bytes())
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
                        "example/durable-counter/non-value",
                        b"No assets, fees, debits, credits or value delivery",
                    ),
                )
                .expect("non-value family")
            }
        })
        .collect();
    let definitions = project
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
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                502 => (
                    LawKind::CommittedFailureEffects,
                    DecisionScope::CommittedFailure,
                    no_genesis(),
                ),
                503 => (
                    LawKind::RejectNoAuthority,
                    DecisionScope::Reject,
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
                digest("example/durable-counter/law", &claim),
                checker_hash(),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .expect("runtime law definition")
        })
        .collect();
    LawManifest::try_new(families, definitions).expect("complete reviewed law manifest")
}

fn no_genesis() -> GenesisApplicability {
    GenesisApplicability::NotApplicable {
        rationale_hash: digest(
            "example/durable-counter/no-genesis",
            b"This law relates an invocation to its decision",
        ),
    }
}
