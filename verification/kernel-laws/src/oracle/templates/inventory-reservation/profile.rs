//! Reviewed example bindings shared by generation and the runtime law checker.
//! These hashes identify local source and policy, not independently certified builds.

use crate::oracle::authority::finite_decision::{
    ConditionalOutbox, DecisionBranch, FiniteDecisionContract, InputBinding, InputScalar,
    InputSource, StateAssignment,
};
use zeno_fcis_codec::{Domain, Hash32, commitment};
use crate::oracle::core::{BudgetLimits, Resource};
use zeno_fcis_crypto::RustCryptoSha256;
use crate::oracle::laws::{
    DecisionScope, GenesisApplicability, LawDefinition, LawEvidenceRequirement, LawFamilyPolicy,
    LawKind, LawManifest,
};
use zeno_fcis_project::{SemanticId, StableName};
use zeno_fcis_spec::{ProjectLimits, ProjectSpec, SourceLimits, elaborate_project, parse_project};
use zeno_fcis_synthesis::finite::Domain as FiniteDomain;
use zeno_fcis_synthesis::finite_runtime::evaluator_hash;
use zeno_fcis_value::Value;

/// The committed-failure law. This application never commits a failure, so
/// the law checker refuses any committed failure outright. It has no formula,
/// so it is registered here instead of as an always-false law in
/// `project.zeno`.
pub const NO_COMMITTED_FAILURES: u32 = 508;
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
    let parsed = parse_project(include_str!("original/project.zeno"), SourceLimits::default())
        .expect("parse authored project");
    elaborate_project(parsed, ProjectLimits::default()).expect("check authored project")
}

pub fn source_hash() -> Hash32 {
    let source = digest(
        "example/inventory-reservation/source",
        concat!(
            include_str!("original/project.zeno"),
            "\0",
            include_str!("original/profile.rs"),
            "\0",
            include_str!("original/build.rs"),
            "\0",
            include_str!("original/Cargo.toml"),
            "\0",
            include_str!("original/src/lib.rs"),
            "\0",
            include_str!("original/src/program.rs"),
            "\0",
            include_str!("original/src/laws.rs"),
            "\0",
            include_str!("original/src/delivery.rs"),
            "\0",
            include_str!("original/synthesis.json"),
            "\0",
            include_str!("original/synthesized/problem.json"),
            "\0",
            include_str!("original/synthesized/manifest.json"),
            "\0",
            include_str!("original/synthesized/vectors.json"),
            "\0",
            include_str!("original/synthesized/transition.rs")
        )
        .as_bytes(),
    );
    let program = digest(
        "example/inventory-reservation/closed-ir",
        include_bytes!("original/synthesized/program.zcve"),
    );
    let evaluator = evaluator_hash().expect("static finite evaluator source");
    digest(
        "example/inventory-reservation/source-complete",
        &[
            source.as_bytes().as_slice(),
            program.as_bytes().as_slice(),
            evaluator.as_bytes().as_slice(),
        ]
        .concat(),
    )
}

pub fn program_hash() -> Hash32 {
    decision_contract()
        .identity::<RustCryptoSha256>()
        .expect("closed decision identity")
}

pub fn decision_contract() -> FiniteDecisionContract {
    let inputs = vec![
        FiniteDomain::Int { min: 0, max: 5 },
        FiniteDomain::Int { min: 0, max: 5 },
        FiniteDomain::Int { min: 0, max: 3 },
        FiniteDomain::Int { min: 1, max: 3 },
        FiniteDomain::Bool,
    ];
    let outputs = vec![
        FiniteDomain::Int { min: 0, max: 4 },
        FiniteDomain::Int { min: 0, max: 5 },
        FiniteDomain::Int { min: 0, max: 5 },
        FiniteDomain::Bool,
    ];
    FiniteDecisionContract::try_new(
        source_hash(),
        include_bytes!("original/synthesized/program.zcve"),
        inputs,
        outputs,
        vec![
            InputBinding {
                source: InputSource::State,
                field: 110,
                scalar: InputScalar::I128,
            },
            InputBinding {
                source: InputSource::State,
                field: 111,
                scalar: InputScalar::I128,
            },
            InputBinding {
                source: InputSource::Command,
                field: 120,
                scalar: InputScalar::SumVariants {
                    type_id: 107,
                    variants: vec![(150, 0), (151, 1), (152, 2), (153, 3)],
                },
            },
            InputBinding {
                source: InputSource::Command,
                field: 121,
                scalar: InputScalar::I128,
            },
            InputBinding {
                source: InputSource::Context,
                field: 130,
                scalar: InputScalar::Bool,
            },
        ],
        vec![
            DecisionBranch {
                code: 0,
                rejection: Some(id(201)),
            },
            DecisionBranch {
                code: 1,
                rejection: Some(id(202)),
            },
            DecisionBranch {
                code: 2,
                rejection: Some(id(203)),
            },
            DecisionBranch {
                code: 3,
                rejection: None,
            },
            DecisionBranch {
                code: 4,
                rejection: Some(id(200)),
            },
        ],
        vec![
            StateAssignment {
                field: 110,
                output: 1,
            },
            StateAssignment {
                field: 111,
                output: 2,
            },
        ],
        vec![ConditionalOutbox {
            ordinal: 0,
            channel: id(300),
            when_output: 3,
            destination: Value::text_ascii("warehouse".into()).expect("ASCII destination literal"),
            payload_fields: vec![(140, 3)],
        }],
        BudgetLimits::zero()
            .with_limit(Resource::Read, 7)
            .with_limit(Resource::Write, 2)
            .with_limit(Resource::Candidate, 1)
            .with_limit(Resource::Effect, 1),
        256,
    )
    .expect("complete checked inventory decision contract")
}

pub fn checker_hash() -> Hash32 {
    digest(
        "example/inventory-reservation/checker",
        source_hash().as_bytes(),
    )
}

pub fn manifest() -> LawManifest {
    let project = project();
    let required = [
        LawKind::StateInvariant,
        LawKind::AssetConservation,
        LawKind::DebitCreditEffectEquality,
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
                        "example/inventory-reservation/not-applicable",
                        b"No minting, burning, or fees; law 502 checks the operator flag",
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
                    LawKind::AssetConservation,
                    DecisionScope::Accept,
                    no_genesis(),
                ),
                502 => (
                    LawKind::DebitCreditEffectEquality,
                    DecisionScope::Accept,
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
                digest("example/inventory-reservation/law", &claim),
                checker_hash(),
                LawEvidenceRequirement::RuntimeOnly,
            )
            .expect("runtime law definition")
        })
        .collect::<Vec<_>>();
    definitions.push(
        LawDefinition::try_new(
            id(NO_COMMITTED_FAILURES),
            name("no_committed_failures"),
            LawKind::CommittedFailureEffects,
            DecisionScope::CommittedFailure,
            no_genesis(),
            digest(
                "example/inventory-reservation/framework-law",
                b"This application never commits a failure",
            ),
            checker_hash(),
            LawEvidenceRequirement::RuntimeOnly,
        )
        .expect("no-committed-failure law"),
    );
    definitions.push(
        LawDefinition::try_new(
            id(REJECT_PUBLISHES_NOTHING),
            name("reject_publishes_nothing"),
            LawKind::RejectNoAuthority,
            DecisionScope::Reject,
            no_genesis(),
            digest(
                "example/inventory-reservation/framework-law",
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
            "example/inventory-reservation/no-genesis",
            b"This law relates an invocation to its decision",
        ),
    }
}
