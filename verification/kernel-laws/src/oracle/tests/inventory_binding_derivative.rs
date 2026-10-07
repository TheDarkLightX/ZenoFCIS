//! Private reproducible source-bound metadata derivative. No publication authority.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crate::oracle::templates::inventory_reservation::{self as stock, profile};
use profile::{digest,id,name};
use zeno_fcis_catalog::{CatalogLimits,CatalogManifest,ChannelDefinition,OperationSemantics,ProjectCatalog,ReasonDefinition,ReasonDisposition};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_project::{DomainPrefix,ProfileBindings,ProjectProfile,RegistryEntry,RegistryKind};
use zeno_fcis_schema::TypeId;
use zeno_fcis_codec::Hash32;
#[allow(dead_code,unused_imports)]
#[path="../templates/inventory-reservation/generated/bindings.rs"]
mod original_bindings;
const ORIGINAL:&str=include_str!("../templates/inventory-reservation/generated/bindings.rs");
const DERIVATIVE:&str=include_str!("../templates/inventory-reservation/generated/bindings-current.rs");
fn current_catalog()->ProjectCatalog {
    let project=profile::project();
    let schema=stock::generated::Stock::zfcis_schema().expect("same original schema provider");
    assert_eq!(schema.canonical_bytes().unwrap().as_slice(),include_bytes!("../../../../../crates/zeno-fcis-cli/templates/inventory-reservation/v2/schema.zcve"));
    let reasons = project
        .reasons()
        .iter()
        .map(|reason| {
            let disposition = match reason.id().get() {
                200..=203 => ReasonDisposition::Reject,
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
                // Shipped units leave this state through the decision itself;
                // the request tells the warehouse to send them.
                OperationSemantics::non_value(digest(
                    "example/inventory-reservation/channel",
                    b"Shipment request; units leave the state in the same decision",
                ))
                .expect("non-value channel"),
                profile::program_hash(),
            )
            .expect("reviewed shipment channel")
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
            digest("example/inventory-reservation/root", label.as_bytes()),
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
        name("warehouse"),
        id(project.project_id().get()),
        1,
        id(100),
        id(101),
        id(102),
        DomainPrefix::try_new("example/inventory-reservation").expect("domain"),
        ProfileBindings {
            schema_hash: schema
                .schema_hash::<RustCryptoSha256>()
                .expect("schema hash"),
            precedence_hash: catalog_manifest.precedence_hash(),
            algorithm_hash: profile::program_hash(),
            codec_hash: digest(
                "example/inventory-reservation/codec",
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
    ProjectCatalog::try_new::<RustCryptoSha256>(
        project_profile,
        schema,
        catalog_manifest,
        CatalogLimits::default(),
    )
    .expect("checked catalog")

}
fn literal(hash:Hash32)->String {format!("Hash32::new({:?})",hash.as_bytes())}
fn replace_last(line:&str,hash:Hash32)->String {
    let start=line.rfind("Hash32::new([").unwrap();let end=start+line[start..].find("])").unwrap()+2;
    format!("{}{}{}",&line[..start],literal(hash),&line[end..])
}
fn erased(source:&str)->String {
    let mut rest=source;let mut out=String::new();
    while let Some(start)=rest.find("Hash32::new(["){let end=start+rest[start..].find("])").unwrap()+2;
        out.push_str(&rest[..start]);out.push_str("Hash32::new(<hash>)");rest=&rest[end..];}
    out.push_str(rest);out
}
fn render(c:&ProjectCatalog)->(String,Vec<String>) {
    let mut out=String::new();let mut changes=Vec::new();
    for (index,line) in ORIGINAL.lines().enumerate(){
        let hash=if line.starts_with("pub const CATALOG_HASH:"){Some(c.commitment::<RustCryptoSha256>().unwrap())}
        else if line.starts_with("pub const PROFILE_HASH:"){Some(c.profile_hash())}
        else if line.contains("ReasonDefinition::try_new(")||line.contains("ChannelDefinition::try_new("){Some(profile::program_hash())}
        else if line.trim_start().starts_with("algorithm_hash:"){Some(c.profile().bindings().algorithm_hash)}
        else if line.trim_start().starts_with("precedence_hash:"){Some(c.profile().bindings().precedence_hash)}
        else if line.trim_start().starts_with("channel_registry_hash:"){Some(c.profile().bindings().channel_registry_hash)}
        else if line.trim_start().starts_with("policy_hash:"){Some(c.profile().bindings().policy_hash)}
        else {c.profile().entries().iter().filter(|e|matches!(e.kind(),RegistryKind::Reason|RegistryKind::Channel|RegistryKind::Claim))
            .find(|e|line.contains(&format!("RegistryKind::{:?}, SemanticId::try_new({})?",e.kind(),e.id().get())))
            .map(|e|e.definition_hash())};
        let next=match hash{Some(h)=>replace_last(line,h),None=>line.to_string()};
        if line!=next {changes.push(format!("line{}: {} => {}",index+1,line.trim(),next.trim()));}
        out.push_str(&next);out.push('\n');
    }
    assert_eq!(changes.len(),21);assert_eq!(erased(ORIGINAL),erased(&out));(out,changes)
}
fn compare_original(c:&ProjectCatalog){
    let original=original_bindings::GeneratedProject::try_new::<RustCryptoSha256>().unwrap();let old=original.catalog();
    assert_eq!(old.schema().canonical_bytes().unwrap(),c.schema().canonical_bytes().unwrap());assert_eq!(old.limits(),c.limits());
    let a=old.profile();let b=c.profile();
    assert_eq!((a.project(),a.subsystem(),a.profile_id(),a.version(),a.state_type(),a.command_type(),a.context_type(),a.domain_prefix()),
        (b.project(),b.subsystem(),b.profile_id(),b.version(),b.state_type(),b.command_type(),b.context_type(),b.domain_prefix()));
    let x=a.bindings();let y=b.bindings();assert_eq!((x.schema_hash,x.codec_hash,x.effect_registry_hash),(y.schema_hash,y.codec_hash,y.effect_registry_hash));
    assert_eq!(a.entries().len(),b.entries().len());for (x,y) in a.entries().iter().zip(b.entries()){
        assert_eq!((x.kind(),x.id(),x.name()),(y.kind(),y.id(),y.name()));
        if !matches!(x.kind(),RegistryKind::Reason|RegistryKind::Channel|RegistryKind::Claim){assert_eq!(x,y);}
    }
    assert_eq!(old.manifest().effects(),c.manifest().effects());assert_eq!(old.manifest().reasons().len(),c.manifest().reasons().len());
    for (x,y) in old.manifest().reasons().iter().zip(c.manifest().reasons()){
        assert_eq!((x.id(),x.name(),x.disposition(),x.precedence()),(y.id(),y.name(),y.disposition(),y.precedence()));assert_eq!(y.predicate_hash(),profile::program_hash());
    }
    assert_eq!(old.manifest().channels().len(),c.manifest().channels().len());
    for (x,y) in old.manifest().channels().iter().zip(c.manifest().channels()){
        assert_eq!((x.id(),x.name(),x.destination_type(),x.payload_type(),x.semantics()),(y.id(),y.name(),y.destination_type(),y.payload_type(),y.semantics()));assert_eq!(y.delivery_policy_hash(),profile::program_hash());
    }
}
#[test]
fn current_private_inventory_binding_has_only_derived_hash_changes(){
    let catalog=current_catalog();compare_original(&catalog);let (source,changes)=render(&catalog);
    if let Some(path)=std::env::var_os("ZENO_PRIVATE_BINDING_DERIVATIVE_OUTPUT"){
        let path=std::path::PathBuf::from(path);std::fs::write(path.join("bindings-current.rs"),source.as_bytes()).unwrap();
        std::fs::write(path.join("changes.txt"),changes.join("\n")+"\n").unwrap();
        std::fs::write(path.join("catalog.zcve"),catalog.canonical_bytes().unwrap()).unwrap();
        std::fs::write(path.join("profile.zcve"),catalog.profile().canonical_bytes().unwrap()).unwrap();
        std::fs::write(path.join("schema.zcve"),catalog.schema().canonical_bytes().unwrap()).unwrap();
        std::fs::write(path.join("provenance.txt"),format!("source:{}\nprogram:{}\nchecker:{}\npolicy:{}\nprofile:{}\ncatalog:{}\n",profile::source_hash(),profile::program_hash(),profile::checker_hash(),profile::manifest().commitment::<RustCryptoSha256>().unwrap(),catalog.profile_hash(),catalog.commitment::<RustCryptoSha256>().unwrap())).unwrap();
    }
    assert_eq!(source,DERIVATIVE,"reproduce all exact current private metadata");
    let active=stock::bindings::GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    assert_eq!(active.catalog(),&catalog,"same canonical full catalog and all constructor/registry/profile equalities");
}
