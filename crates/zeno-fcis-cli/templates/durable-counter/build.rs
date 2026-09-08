use std::{env, fs, path::PathBuf};

mod profile;

use profile::{digest, id, name};
use zeno_fcis_bootstrap::{BootstrapLimits, BootstrapSpec, generate_project, lower_schema};
use zeno_fcis_catalog::{
    CatalogLimits, CatalogManifest, ChannelDefinition, OperationSemantics, ProjectCatalog,
    ReasonDefinition, ReasonDisposition,
};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::{
    DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind,
};
use zeno_fcis_schema::{SchemaLimits, TypeId, TypeKind};
use zeno_fcis_spec::StableId;

fn main() {
    let project = profile::project();
    let stable = |n| StableId::new(n).expect("static ID");
    let schema = lower_schema(
        &project,
        stable(100),
        1,
        vec![
            (stable(102), TypeKind::Bool),
            (
                stable(103),
                TypeKind::Text {
                    min_len: 1,
                    max_len: 32,
                },
            ),
            (stable(105), TypeKind::I128 { min: 0, max: 3 }),
        ],
        SchemaLimits::default(),
    )
    .expect("lower exact authored shapes and reviewed scalar bounds");
    assert!(
        project.effects().is_empty(),
        "new effects require a reviewed program and checker"
    );
    let reasons = project
        .reasons()
        .iter()
        .map(|reason| {
            let disposition = match reason.id().get() {
                200 | 201 => ReasonDisposition::Reject,
                202 => ReasonDisposition::CommittedFailure,
                _ => panic!("new reason needs a reviewed disposition"),
            };
            ReasonDefinition::try_new(
                id(reason.id().get()),
                name(reason.name().as_str()),
                disposition,
                reason.precedence(),
                profile::program_hash(),
            )
            .expect("reviewed reason")
        })
        .collect();
    let channels = project
        .channels()
        .iter()
        .map(|channel| {
            assert_eq!(
                channel.id().get(),
                300,
                "new channel requires reviewed semantics"
            );
            ChannelDefinition::try_new(
                id(channel.id().get()),
                name(channel.name().as_str()),
                TypeId::new(channel.destination_type().get()),
                TypeId::new(channel.payload_type().get()),
                OperationSemantics::non_value(digest(
                    "example/durable-counter/channel",
                    b"State notification; no value transfer",
                ))
                .expect("non-value channel"),
                profile::program_hash(),
            )
            .expect("reviewed notification channel")
        })
        .collect();
    let catalog_manifest = CatalogManifest::try_new::<RustCryptoSha256>(reasons, vec![], channels)
        .expect("catalog manifest");
    let mut entries = [
        (RegistryKind::StateType, 100, "state"),
        (RegistryKind::CommandType, 101, "command"),
        (RegistryKind::ContextType, 102, "context"),
    ]
    .into_iter()
    .map(|(kind, raw, label)| {
        RegistryEntry::try_new(
            kind,
            id(raw),
            name(label),
            digest("example/durable-counter/root", label.as_bytes()),
        )
        .expect("root entry")
    })
    .collect::<Vec<_>>();
    entries.extend_from_slice(catalog_manifest.registry_entries());
    let laws = profile::manifest();
    entries.extend(
        laws.registry_entries::<RustCryptoSha256>()
            .expect("law entries"),
    );
    let project_profile = ProjectProfile::try_new(
        name(project.name().as_str()),
        name("core"),
        id(project.project_id().get()),
        1,
        id(100),
        id(101),
        id(102),
        DomainPrefix::try_new("example/durable-counter").expect("domain"),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .expect("schema hash"),
            precedence_hash: catalog_manifest.precedence_hash(),
            algorithm_hash: profile::program_hash(),
            codec_hash: digest(
                "example/durable-counter/codec",
                b"ZCVE canonical value encoding v1",
            ),
            effect_registry_hash: catalog_manifest.effect_registry_hash(),
            channel_registry_hash: catalog_manifest.channel_registry_hash(),
            policy_hash: laws
                .commitment::<RustCryptoSha256>()
                .expect("law manifest hash"),
        },
        entries,
    )
    .expect("reviewed project profile");
    let catalog = ProjectCatalog::try_new::<RustCryptoSha256>(
        project_profile,
        schema,
        catalog_manifest,
        CatalogLimits::default(),
    )
    .expect("checked catalog");
    let spec = BootstrapSpec::try_new(
        "durable-counter",
        "counter",
        "counter",
        BootstrapLimits::default(),
    )
    .expect("bootstrap spec");
    let bundle =
        generate_project::<RustCryptoSha256>(&catalog, &spec).expect("generate typed project");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    for file in bundle.files() {
        let path = out.join(file.path());
        fs::create_dir_all(path.parent().expect("generated parent"))
            .expect("create generated directory");
        fs::write(path, file.bytes()).expect("retain generated artifact");
    }
    fs::write(
        out.join("project.ast"),
        project.canonical_bytes().expect("AST bytes"),
    )
    .expect("retain AST");
    fs::write(
        out.join("source.sha256"),
        profile::source_hash().to_string(),
    )
    .expect("retain source identity");
    for path in [
        "project.zeno",
        "profile.rs",
        "build.rs",
        "Cargo.toml",
        "src/lib.rs",
        "src/program.rs",
        "src/laws.rs",
        "src/delivery.rs",
        "synthesis.json",
        "synthesized/problem.json",
        "synthesized/manifest.json",
        "synthesized/program.zcve",
        "synthesized/vectors.json",
        "synthesized/transition.rs",
    ] {
        println!("cargo::rerun-if-changed={path}");
    }
}
