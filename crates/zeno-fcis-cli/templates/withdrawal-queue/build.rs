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

/// The most units the vault holds; `project.zeno` and `src/program.rs` state
/// the same bound.
const MAX_BALANCE: i128 = 4;
/// The largest deposit or withdrawal.
const MAX_AMOUNT: i128 = 2;
/// Ticks that stay paused after an honored alarm's own tick; `project.zeno`
/// and `controller/model.py` state the same number.
const PAUSE_TICKS: i128 = 2;

// A build script reports a broken reviewed binding by failing the build, so
// the `expect` calls here are its error handling.
fn main() {
    let project = profile::project().expect("authored project");
    let stable = |n| StableId::new(n).expect("static ID");
    let schema = lower_schema(
        &project,
        stable(100),
        1,
        vec![
            (
                stable(103),
                TypeKind::Text {
                    min_len: 1,
                    max_len: 32,
                },
            ),
            (
                stable(105),
                TypeKind::I128 {
                    min: 0,
                    max: MAX_BALANCE,
                },
            ),
            (
                stable(106),
                TypeKind::I128 {
                    min: 1,
                    max: MAX_AMOUNT,
                },
            ),
            (
                stable(110),
                TypeKind::I128 {
                    min: 0,
                    max: PAUSE_TICKS,
                },
            ),
            (stable(111), TypeKind::Bool),
        ],
        SchemaLimits::default(),
    )
    .expect("lower exact authored shapes and reviewed scalar bounds");
    assert!(
        project.effects().is_empty(),
        "new effects require a reviewed program and checker"
    );
    let program_hash = profile::program_hash().expect("program identity");
    let reasons = project
        .reasons()
        .iter()
        .map(|reason| {
            let disposition = match reason.id().get() {
                200..=203 => ReasonDisposition::Reject,
                _ => panic!("new reason needs a reviewed disposition"),
            };
            ReasonDefinition::try_new(
                id(reason.id().get()).expect("reason ID"),
                name(reason.name().as_str()).expect("reason name"),
                disposition,
                reason.precedence(),
                program_hash,
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
                id(channel.id().get()).expect("channel ID"),
                name(channel.name().as_str()).expect("channel name"),
                TypeId::new(channel.destination_type().get()),
                TypeId::new(channel.payload_type().get()),
                // The paid amount leaves the balance in the decision itself;
                // the request tells the settlement rail to pay it out.
                OperationSemantics::non_value(
                    digest(
                        "example/withdrawal-queue/channel",
                        b"Payout request; the amount leaves the balance in the same decision",
                    )
                    .expect("channel rationale"),
                )
                .expect("non-value channel"),
                program_hash,
            )
            .expect("reviewed payout channel")
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
            id(raw).expect("root ID"),
            name(label).expect("root name"),
            digest("example/withdrawal-queue/root", label.as_bytes()).expect("root digest"),
        )
        .expect("root entry")
    })
    .collect::<Vec<_>>();
    entries.extend_from_slice(catalog_manifest.registry_entries());
    let laws = profile::manifest().expect("law manifest");
    entries.extend(
        laws.registry_entries::<RustCryptoSha256>()
            .expect("law entries"),
    );
    let project_profile = ProjectProfile::try_new(
        name(project.name().as_str()).expect("project name"),
        name("vault").expect("namespace"),
        id(project.project_id().get()).expect("project ID"),
        1,
        id(100).expect("state ID"),
        id(101).expect("command ID"),
        id(102).expect("context ID"),
        DomainPrefix::try_new("example/withdrawal-queue").expect("domain"),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .expect("schema hash"),
            precedence_hash: catalog_manifest.precedence_hash(),
            algorithm_hash: program_hash,
            codec_hash: digest(
                "example/withdrawal-queue/codec",
                b"ZCVE canonical value encoding v1",
            )
            .expect("codec digest"),
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
        "withdrawal-queue",
        "vault",
        "vault",
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
        profile::source_hash().expect("source identity").to_string(),
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
        "src/controller.rs",
        "synthesis.json",
        "synthesized/problem.json",
        "synthesized/manifest.json",
        "synthesized/program.zcve",
        "synthesized/vectors.json",
        "synthesized/transition.rs",
        "controller/contract.json",
        "controller/contract.sha256",
        "controller/strategy.json",
        "controller/model.py",
    ] {
        println!("cargo::rerun-if-changed={path}");
    }
}
