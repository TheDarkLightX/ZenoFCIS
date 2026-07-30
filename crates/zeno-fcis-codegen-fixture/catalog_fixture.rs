//! Exact reviewed catalog shared by build-time generation and runtime tests.

use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition, HashRequirement,
    OperationSemantics, ProjectCatalog, ReasonDefinition, ReasonDisposition, ValueFlow,
    ValueFlowKind,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::{Schema, TypeId};

pub(crate) fn fixture_catalog(schema: Schema) -> ProjectCatalog {
    let reasons = vec![
        ReasonDefinition::try_new(
            id(10),
            name("denied"),
            ReasonDisposition::Reject,
            0,
            hash(10),
        )
        .unwrap_or_else(|error| panic!("fixture reason rejected: {error}")),
    ];
    let effects = vec![
        EffectDefinition::try_new(
            id(20),
            name("write"),
            TypeId::new(1),
            HashRequirement::Present,
            HashRequirement::Absent,
            OperationSemantics::non_value(hash(120))
                .unwrap_or_else(|error| panic!("fixture semantics rejected: {error}")),
            hash(20),
        )
        .unwrap_or_else(|error| panic!("fixture effect rejected: {error}")),
    ];
    let channels = vec![
        ChannelDefinition::try_new(
            id(30),
            name("notify"),
            TypeId::new(3),
            TypeId::new(9),
            OperationSemantics::value(
                vec![
                    ValueFlow::standard(ValueFlowKind::ExternalValueDelivery, hash(121))
                        .unwrap_or_else(|error| panic!("fixture flow rejected: {error}")),
                ],
                hash(130),
            )
            .unwrap_or_else(|error| panic!("fixture semantics rejected: {error}")),
            hash(30),
        )
        .unwrap_or_else(|error| panic!("fixture channel rejected: {error}")),
    ];
    let manifest = CatalogManifest::try_new::<RustCryptoSha256>(reasons, effects, channels)
        .unwrap_or_else(|error| panic!("fixture manifest rejected: {error}"));
    let mut entries = vec![
        root_entry(RegistryKind::StateType, 12, "state", 1),
        root_entry(RegistryKind::CommandType, 9, "command", 2),
        root_entry(RegistryKind::ContextType, 12, "context", 3),
    ];
    entries.extend_from_slice(manifest.registry_entries());
    let profile = ProjectProfile::try_new(
        name("codegen-fixture"),
        name("core"),
        id(100),
        1,
        id(12),
        id(9),
        id(12),
        DomainPrefix::try_new("codegen-fixture/core")
            .unwrap_or_else(|error| panic!("fixture domain rejected: {error}")),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("fixture schema hash failed: {error}")),
            precedence_hash: manifest.precedence_hash(),
            algorithm_hash: hash(40),
            codec_hash: hash(41),
            effect_registry_hash: manifest.effect_registry_hash(),
            channel_registry_hash: manifest.channel_registry_hash(),
            policy_hash: hash(42),
        },
        entries,
    )
    .unwrap_or_else(|error| panic!("fixture profile rejected: {error}"));
    ProjectCatalog::try_new::<RustCryptoSha256>(profile, schema, manifest, CatalogLimits::default())
        .unwrap_or_else(|error| panic!("fixture catalog rejected: {error}"))
}

fn root_entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
    RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
        .unwrap_or_else(|error| panic!("fixture registry entry rejected: {error}"))
}

fn name(value: &str) -> StableName {
    StableName::try_new(value).unwrap_or_else(|error| panic!("fixture name rejected: {error}"))
}

fn id(value: u32) -> SemanticId {
    SemanticId::try_new(value).unwrap_or_else(|error| panic!("fixture id rejected: {error}"))
}

fn hash(byte: u8) -> Hash32 {
    Hash32::new([byte; 32])
}
