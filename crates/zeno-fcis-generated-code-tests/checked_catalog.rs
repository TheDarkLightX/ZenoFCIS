//! A separate supported generated workflow; it does not replace the compound fixture.

use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, ChannelDefinition, OperationSemantics, ProjectCatalog,
    ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::Hash32;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
    StableName,
};
use zeno_fcis_schema::{FieldDef, FieldId, Schema, SchemaLimits, TypeDef, TypeId, TypeKind};

fn id(value: u32) -> SemanticId {
    SemanticId::try_new(value).unwrap_or_else(|error| panic!("checked fixture id: {error}"))
}
fn name(value: &str) -> StableName {
    StableName::try_new(value).unwrap_or_else(|error| panic!("checked fixture name: {error}"))
}
fn hash(value: u8) -> Hash32 {
    Hash32::new([value; 32])
}
fn field(id: u16, name: &str, kind: u32) -> FieldDef {
    FieldDef::try_new(FieldId::new(id), name, TypeId::new(kind))
        .unwrap_or_else(|error| panic!("checked fixture field: {error}"))
}
fn ty(id: u32, name: &str, kind: TypeKind) -> TypeDef {
    TypeDef::try_new(TypeId::new(id), name, kind, SchemaLimits::default())
        .unwrap_or_else(|error| panic!("checked fixture type: {error}"))
}

pub(crate) fn checked_schema() -> Schema {
    Schema::try_new(
        "AdditionalCheckedWorkflow",
        1,
        TypeId::new(10),
        vec![
            ty(1, "Ready", TypeKind::Bool),
            ty(2, "Code", TypeKind::I128 { min: 0, max: 2 }),
            ty(
                3,
                "Destination",
                TypeKind::Text {
                    min_len: 1,
                    max_len: 16,
                },
            ),
            ty(
                4,
                "CheckedPayload",
                TypeKind::Record {
                    fields: vec![field(1, "code", 2)].into_boxed_slice(),
                },
            ),
            ty(
                10,
                "CheckedState",
                TypeKind::Record {
                    fields: vec![field(1, "ready", 1)].into_boxed_slice(),
                },
            ),
            ty(
                11,
                "CheckedContext",
                TypeKind::Record {
                    fields: vec![field(1, "approved", 1)].into_boxed_slice(),
                },
            ),
        ],
        SchemaLimits::default(),
    )
    .unwrap_or_else(|error| panic!("checked fixture schema: {error}"))
}

pub(crate) fn checked_catalog(schema: Schema) -> ProjectCatalog {
    let manifest = CatalogManifest::try_new::<RustCryptoSha256>(
        vec![
            ReasonDefinition::try_new(
                id(10),
                name("denied"),
                ReasonDisposition::Reject,
                0,
                hash(10),
            )
            .unwrap_or_else(|error| panic!("checked fixture reason: {error}")),
            ReasonDefinition::try_new(
                id(11),
                name("committing-failure"),
                ReasonDisposition::CommittedFailure,
                1,
                hash(11),
            )
            .unwrap_or_else(|error| panic!("checked fixture reason: {error}")),
        ],
        vec![],
        vec![
            ChannelDefinition::try_new(
                id(30),
                name("notification"),
                TypeId::new(3),
                TypeId::new(4),
                OperationSemantics::non_value(hash(130))
                    .unwrap_or_else(|error| panic!("checked fixture semantics: {error}")),
                hash(30),
            )
            .unwrap_or_else(|error| panic!("checked fixture channel: {error}")),
        ],
    )
    .unwrap_or_else(|error| panic!("checked fixture manifest: {error}"));
    let mut entries = vec![
        RegistryEntry::try_new(RegistryKind::StateType, id(10), name("state"), hash(1)),
        RegistryEntry::try_new(RegistryKind::CommandType, id(2), name("command"), hash(2)),
        RegistryEntry::try_new(RegistryKind::ContextType, id(11), name("context"), hash(3)),
    ]
    .into_iter()
    .map(|entry| entry.unwrap_or_else(|error| panic!("checked fixture entry: {error}")))
    .collect::<Vec<_>>();
    entries.extend_from_slice(manifest.registry_entries());
    let profile = ProjectProfile::try_new(
        name("additional-checked-fixture"),
        name("core"),
        id(101),
        1,
        id(10),
        id(2),
        id(11),
        DomainPrefix::try_new("additional-checked-fixture/core")
            .unwrap_or_else(|error| panic!("checked fixture domain: {error}")),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .unwrap_or_else(|error| panic!("checked fixture schema hash: {error}")),
            precedence_hash: manifest.precedence_hash(),
            algorithm_hash: hash(140),
            codec_hash: hash(141),
            effect_registry_hash: manifest.effect_registry_hash(),
            channel_registry_hash: manifest.channel_registry_hash(),
            policy_hash: hash(142),
        },
        entries,
    )
    .unwrap_or_else(|error| panic!("checked fixture profile: {error}"));
    ProjectCatalog::try_new::<RustCryptoSha256>(profile, schema, manifest, CatalogLimits::default())
        .unwrap_or_else(|error| panic!("checked fixture catalog: {error}"))
}
