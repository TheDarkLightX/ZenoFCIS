//! Deterministic bounded project-bundle assembly and manifest construction.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use zeno_fcis_catalog::ProjectCatalog;
use zeno_fcis_codec::{CanonicalEncode as _, CommitmentHasher, Domain, Hash32, commitment};
use zeno_fcis_codegen::{GeneratedBundle, generate};

use crate::templates::{
    render_architecture, render_ci, render_evidence, render_migration, render_python_runtime,
    render_registry, render_rust_project, render_rust_runtime,
};
use crate::{
    BOOTSTRAP_FORMAT_VERSION, BOOTSTRAP_GENERATOR_ID, BootstrapBundle, BootstrapError,
    BootstrapFile, BootstrapLimits, BootstrapSpec,
};

const MANIFEST_PATH: &str = "BOOTSTRAP-MANIFEST.zfcis";
const MAX_PORTABLE_PATH_BYTES: usize = 512;

/// Generates one complete, atomically bounded project starter.
///
/// `H` must be the exact provider identity used to construct `catalog`.
/// Schema code generation retains its own separately bound provider identity.
pub fn generate_project<H: CommitmentHasher>(
    catalog: &ProjectCatalog,
    spec: &BootstrapSpec,
) -> Result<BootstrapBundle, BootstrapError> {
    spec.limits().validate()?;
    let catalog_hash = catalog.commitment::<H>()?;
    let profile_hash = catalog.profile_hash();
    let schema_hash = catalog.schema_hash();
    let schema_bundle = generate(catalog.schema(), spec.generation())?;

    let mut assembly = Assembly::new(spec.limits());
    for file in schema_bundle.files() {
        assembly.push(
            format!("schema-codegen/{}", file.path()),
            file.bytes().to_vec(),
        )?;
    }
    assembly.push(
        "profile.zcve".to_owned(),
        catalog.profile().canonical_bytes()?,
    )?;
    assembly.push("catalog.zcve".to_owned(), catalog.canonical_bytes()?)?;
    assembly.push(
        "registry.tsv".to_owned(),
        render_registry(catalog)?.into_bytes(),
    )?;
    assembly.push(
        "rust/project.rs".to_owned(),
        render_rust_project(catalog, spec, catalog_hash, profile_hash, schema_hash)?.into_bytes(),
    )?;
    assembly.push(
        "rust/runtime.rs".to_owned(),
        render_rust_runtime(spec).into_bytes(),
    )?;
    assembly.push(
        "python/runtime.py".to_owned(),
        render_python_runtime(spec).into_bytes(),
    )?;
    assembly.push(
        "migration.stub.zfcis".to_owned(),
        render_migration(catalog).into_bytes(),
    )?;
    assembly.push(
        "evidence.requirements.zfcis".to_owned(),
        render_evidence(catalog).into_bytes(),
    )?;
    assembly.push(
        ".github/workflows/generated-project-ci.yml".to_owned(),
        render_ci(spec).into_bytes(),
    )?;
    assembly.push(
        "ARCHITECTURE.md".to_owned(),
        render_architecture(catalog, spec).into_bytes(),
    )?;
    assembly.sort();

    let manifest = render_manifest::<H>(
        catalog_hash,
        profile_hash,
        schema_hash,
        &schema_bundle,
        assembly.files(),
        assembly.total_bytes(),
    )?;
    let manifest_hash = hash_manifest::<H>(manifest.as_bytes())?;
    assembly.push(MANIFEST_PATH.to_owned(), manifest.into_bytes())?;
    assembly.sort();

    Ok(BootstrapBundle::new(
        H::ALGORITHM_ID.to_owned(),
        catalog_hash,
        profile_hash,
        schema_hash,
        schema_bundle.manifest_hash(),
        schema_bundle.vector_set_hash(),
        manifest_hash,
        assembly.total_bytes(),
        assembly.into_files(),
    ))
}

struct Assembly {
    limits: BootstrapLimits,
    paths: BTreeSet<String>,
    total_bytes: u64,
    files: Vec<BootstrapFile>,
}

impl Assembly {
    fn new(limits: BootstrapLimits) -> Self {
        Self {
            limits,
            paths: BTreeSet::new(),
            total_bytes: 0,
            files: Vec::new(),
        }
    }

    fn push(&mut self, path: String, bytes: Vec<u8>) -> Result<(), BootstrapError> {
        if !portable_path(&path) {
            return Err(BootstrapError::InvalidPath(path));
        }
        if !self.paths.insert(path.clone()) {
            return Err(BootstrapError::DuplicatePath(path));
        }
        let actual_files = u32::try_from(self.files.len())
            .map_err(|_| BootstrapError::LengthOverflow)?
            .checked_add(1)
            .ok_or(BootstrapError::LengthOverflow)?;
        if actual_files > self.limits.max_files() {
            return Err(BootstrapError::FileCountExceeded {
                limit: self.limits.max_files(),
                actual: actual_files,
            });
        }
        let actual_bytes =
            u64::try_from(bytes.len()).map_err(|_| BootstrapError::LengthOverflow)?;
        if actual_bytes > self.limits.max_file_bytes() {
            return Err(BootstrapError::FileBytesExceeded {
                path,
                limit: self.limits.max_file_bytes(),
                actual: actual_bytes,
            });
        }
        let total_bytes = self
            .total_bytes
            .checked_add(actual_bytes)
            .ok_or(BootstrapError::LengthOverflow)?;
        if total_bytes > self.limits.max_total_bytes() {
            return Err(BootstrapError::TotalBytesExceeded {
                limit: self.limits.max_total_bytes(),
                actual: total_bytes,
            });
        }
        self.total_bytes = total_bytes;
        self.files.push(BootstrapFile::new(path, bytes));
        Ok(())
    }

    fn sort(&mut self) {
        self.files
            .sort_by(|left, right| left.path().cmp(right.path()));
    }

    fn files(&self) -> &[BootstrapFile] {
        &self.files
    }

    const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    fn into_files(self) -> Vec<BootstrapFile> {
        self.files
    }
}

fn render_manifest<H: CommitmentHasher>(
    catalog_hash: Hash32,
    profile_hash: Hash32,
    catalog_schema_hash: Hash32,
    schema_bundle: &GeneratedBundle,
    files: &[BootstrapFile],
    nonmanifest_bytes: u64,
) -> Result<String, BootstrapError> {
    let mut output = String::new();
    writeln!(
        output,
        "format=zeno-fcis-bootstrap/{BOOTSTRAP_FORMAT_VERSION}"
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(output, "generator={BOOTSTRAP_GENERATOR_ID}").map_err(|_| BootstrapError::Render)?;
    writeln!(output, "hash_algorithm={}", H::ALGORITHM_ID).map_err(|_| BootstrapError::Render)?;
    writeln!(output, "catalog_hash={catalog_hash}").map_err(|_| BootstrapError::Render)?;
    writeln!(output, "profile_hash={profile_hash}").map_err(|_| BootstrapError::Render)?;
    writeln!(output, "catalog_schema_hash={catalog_schema_hash}")
        .map_err(|_| BootstrapError::Render)?;
    writeln!(
        output,
        "schema_codegen_generator={}",
        schema_bundle.generator_id()
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(
        output,
        "schema_codegen_formatter={}",
        schema_bundle.formatter_id()
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(
        output,
        "schema_codegen_schema_hash={}",
        schema_bundle.schema_hash()
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(
        output,
        "schema_codegen_manifest_hash={}",
        schema_bundle.manifest_hash()
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(
        output,
        "schema_codegen_vector_set_hash={}",
        schema_bundle.vector_set_hash()
    )
    .map_err(|_| BootstrapError::Render)?;
    writeln!(output, "nonmanifest_file_count={}", files.len())
        .map_err(|_| BootstrapError::Render)?;
    writeln!(output, "nonmanifest_total_bytes={nonmanifest_bytes}")
        .map_err(|_| BootstrapError::Render)?;
    for file in files {
        let length =
            u64::try_from(file.bytes().len()).map_err(|_| BootstrapError::LengthOverflow)?;
        let hash = hash_file::<H>(file.path(), file.bytes())?;
        writeln!(
            output,
            "file={}\tlength={}\tcommitment={}",
            file.path(),
            length,
            hash
        )
        .map_err(|_| BootstrapError::Render)?;
    }
    Ok(output)
}

fn hash_file<H: CommitmentHasher>(path: &str, bytes: &[u8]) -> Result<Hash32, BootstrapError> {
    let path_len = u32::try_from(path.len()).map_err(|_| BootstrapError::LengthOverflow)?;
    let byte_len = u64::try_from(bytes.len()).map_err(|_| BootstrapError::LengthOverflow)?;
    let mut preimage = Vec::with_capacity(
        4_usize
            .checked_add(path.len())
            .and_then(|length| length.checked_add(8))
            .and_then(|length| length.checked_add(bytes.len()))
            .ok_or(BootstrapError::LengthOverflow)?,
    );
    preimage.extend_from_slice(&path_len.to_be_bytes());
    preimage.extend_from_slice(path.as_bytes());
    preimage.extend_from_slice(&byte_len.to_be_bytes());
    preimage.extend_from_slice(bytes);
    hash_bytes::<H>("zeno-fcis/bootstrap-file", &preimage)
}

fn hash_manifest<H: CommitmentHasher>(bytes: &[u8]) -> Result<Hash32, BootstrapError> {
    hash_bytes::<H>("zeno-fcis/bootstrap-manifest", bytes)
}

fn hash_bytes<H: CommitmentHasher>(
    name: &'static str,
    bytes: &[u8],
) -> Result<Hash32, BootstrapError> {
    let domain = Domain::new(name, BOOTSTRAP_FORMAT_VERSION)?;
    let hash = commitment::<H>(domain, bytes)?;
    if hash == Hash32::ZERO {
        Err(BootstrapError::ZeroDerivedCommitment)
    } else {
        Ok(hash)
    }
}

fn portable_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PORTABLE_PATH_BYTES
        && path.is_ascii()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

#[cfg(test)]
mod tests {
    use zeno_fcis_catalog::{
        CatalogError, CatalogLimits, CatalogManifest, ChannelDefinition, EffectDefinition,
        HashRequirement, OperationSemantics, ReasonDefinition, ReasonDisposition,
    };
    use zeno_fcis_codegen::fixture_schema;
    use zeno_fcis_project::{
        DomainPrefix, ProfileBindings, ProjectProfile, RegistryEntry, RegistryKind, SemanticId,
        StableName,
    };
    use zeno_fcis_schema::TypeId;

    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct TestHasher;

    impl CommitmentHasher for TestHasher {
        const ALGORITHM_ID: &'static str = "test/bootstrap/1";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(byte)
                    .rotate_left((index % 7) as u32);
            }
            if output == [0_u8; 32] {
                output[0] = 1;
            }
            Hash32::new(output)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct WrongHasher;

    impl CommitmentHasher for WrongHasher {
        const ALGORITHM_ID: &'static str = "test/bootstrap/wrong";

        fn hash(bytes: &[u8]) -> Hash32 {
            TestHasher::hash(bytes)
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct ZeroBootstrapHasher;

    impl CommitmentHasher for ZeroBootstrapHasher {
        const ALGORITHM_ID: &'static str = TestHasher::ALGORITHM_ID;

        fn hash(bytes: &[u8]) -> Hash32 {
            if bytes
                .windows(b"zeno-fcis/bootstrap".len())
                .any(|window| window == b"zeno-fcis/bootstrap")
            {
                Hash32::ZERO
            } else {
                TestHasher::hash(bytes)
            }
        }
    }

    fn hash(byte: u8) -> Hash32 {
        Hash32::new([byte; 32])
    }

    fn id(value: u32) -> SemanticId {
        SemanticId::try_new(value).unwrap_or_else(|error| panic!("semantic id: {error}"))
    }

    fn name(value: &str) -> StableName {
        StableName::try_new(value).unwrap_or_else(|error| panic!("stable name: {error}"))
    }

    fn root_entry(kind: RegistryKind, raw_id: u32, label: &str, byte: u8) -> RegistryEntry {
        RegistryEntry::try_new(kind, id(raw_id), name(label), hash(byte))
            .unwrap_or_else(|error| panic!("root entry: {error}"))
    }

    fn catalog(reverse: bool) -> ProjectCatalog {
        catalog_with_context_type(reverse, 12)
    }

    fn catalog_with_context_type(reverse: bool, context_type: u32) -> ProjectCatalog {
        let schema = fixture_schema().unwrap_or_else(|error| panic!("fixture schema: {error}"));
        let mut reasons = vec![
            ReasonDefinition::try_new(
                id(10),
                name("denied"),
                ReasonDisposition::Reject,
                0,
                hash(10),
            )
            .unwrap_or_else(|error| panic!("reason: {error}")),
            ReasonDefinition::try_new(
                id(11),
                name("committed-denial"),
                ReasonDisposition::CommittedFailure,
                1,
                hash(11),
            )
            .unwrap_or_else(|error| panic!("reason: {error}")),
        ];
        let mut effects = vec![
            EffectDefinition::try_new(
                id(20),
                name("write"),
                TypeId::new(1),
                HashRequirement::Present,
                HashRequirement::Absent,
                OperationSemantics::non_value(hash(120))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(20),
            )
            .unwrap_or_else(|error| panic!("effect: {error}")),
            EffectDefinition::try_new(
                id(21),
                name("write-secondary"),
                TypeId::new(2),
                HashRequirement::Any,
                HashRequirement::Any,
                OperationSemantics::non_value(hash(121))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(21),
            )
            .unwrap_or_else(|error| panic!("effect: {error}")),
            EffectDefinition::try_new(
                id(22),
                name("write-exact"),
                TypeId::new(1),
                HashRequirement::exact(hash(22))
                    .unwrap_or_else(|error| panic!("authority requirement: {error}")),
                HashRequirement::exact(hash(23))
                    .unwrap_or_else(|error| panic!("subject requirement: {error}")),
                OperationSemantics::non_value(hash(122))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(24),
            )
            .unwrap_or_else(|error| panic!("effect: {error}")),
        ];
        let mut channels = vec![
            ChannelDefinition::try_new(
                id(30),
                name("notify"),
                TypeId::new(3),
                TypeId::new(9),
                OperationSemantics::non_value(hash(130))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(30),
            )
            .unwrap_or_else(|error| panic!("channel: {error}")),
            ChannelDefinition::try_new(
                id(31),
                name("notify-secondary"),
                TypeId::new(4),
                TypeId::new(5),
                OperationSemantics::non_value(hash(131))
                    .unwrap_or_else(|error| panic!("semantics: {error}")),
                hash(31),
            )
            .unwrap_or_else(|error| panic!("channel: {error}")),
        ];
        if reverse {
            reasons.reverse();
            effects.reverse();
            channels.reverse();
        }
        let manifest = CatalogManifest::try_new::<TestHasher>(reasons, effects, channels)
            .unwrap_or_else(|error| panic!("manifest: {error}"));
        let mut entries = vec![
            root_entry(RegistryKind::StateType, 12, "state", 1),
            root_entry(RegistryKind::CommandType, 9, "command", 2),
            root_entry(RegistryKind::ContextType, context_type, "context", 3),
        ];
        entries.extend_from_slice(manifest.registry_entries());
        let profile = ProjectProfile::try_new(
            name("example"),
            name("core"),
            id(100),
            1,
            id(12),
            id(9),
            id(context_type),
            DomainPrefix::try_new("example/core")
                .unwrap_or_else(|error| panic!("domain prefix: {error}")),
            ProfileBindings {
                schema_hash: schema
                    .schema_hash::<TestHasher>()
                    .unwrap_or_else(|error| panic!("schema hash: {error}")),
                precedence_hash: manifest.precedence_hash(),
                algorithm_hash: hash(40),
                codec_hash: hash(41),
                effect_registry_hash: manifest.effect_registry_hash(),
                channel_registry_hash: manifest.channel_registry_hash(),
                policy_hash: hash(42),
            },
            entries,
        )
        .unwrap_or_else(|error| panic!("profile: {error}"));
        ProjectCatalog::try_new::<TestHasher>(profile, schema, manifest, CatalogLimits::default())
            .unwrap_or_else(|error| panic!("catalog: {error}"))
    }

    fn spec(limits: BootstrapLimits) -> BootstrapSpec {
        BootstrapSpec::try_new("example-core", "example_core", "example_core", limits)
            .unwrap_or_else(|error| panic!("bootstrap spec: {error}"))
    }

    fn bundle() -> BootstrapBundle {
        generate_project::<TestHasher>(&catalog(false), &spec(BootstrapLimits::default()))
            .unwrap_or_else(|error| panic!("bootstrap generation: {error}"))
    }

    #[test]
    fn repeated_generation_is_byte_identical() {
        assert_eq!(bundle(), bundle());
    }

    #[test]
    fn declaration_order_does_not_change_output() {
        let normal =
            generate_project::<TestHasher>(&catalog(false), &spec(BootstrapLimits::default()));
        let reversed =
            generate_project::<TestHasher>(&catalog(true), &spec(BootstrapLimits::default()));
        assert_eq!(normal, reversed);
    }

    #[test]
    fn files_are_strictly_ordered_and_include_manifest() {
        let generated = bundle();
        assert!(
            generated
                .files()
                .windows(2)
                .all(|pair| pair[0].path() < pair[1].path())
        );
        assert!(
            generated
                .files()
                .iter()
                .any(|file| file.path() == MANIFEST_PATH)
        );
    }

    #[test]
    fn manifest_binds_catalog_schema_codegen_and_every_nonmanifest_file() {
        let generated = bundle();
        let manifest = generated
            .files()
            .iter()
            .find(|file| file.path() == MANIFEST_PATH)
            .unwrap_or_else(|| panic!("bootstrap manifest missing"));
        let text = core::str::from_utf8(manifest.bytes())
            .unwrap_or_else(|error| panic!("manifest UTF-8: {error}"));
        assert!(text.contains(&format!("catalog_hash={}", generated.catalog_hash())));
        assert!(text.contains(&format!("profile_hash={}", generated.profile_hash())));
        assert!(text.contains(&format!(
            "schema_codegen_manifest_hash={}",
            generated.schema_manifest_hash()
        )));
        for file in generated
            .files()
            .iter()
            .filter(|file| file.path() != MANIFEST_PATH)
        {
            assert!(text.contains(&format!("file={}\t", file.path())));
        }
    }

    #[test]
    fn generated_helpers_use_exact_ids_and_schema_types() {
        let generated = bundle();
        let project = generated
            .files()
            .iter()
            .find(|file| file.path() == "rust/project.rs")
            .unwrap_or_else(|| panic!("project helpers missing"));
        let text = core::str::from_utf8(project.bytes())
            .unwrap_or_else(|error| panic!("project source UTF-8: {error}"));
        assert!(text.contains("Effect20 = 20"));
        assert!(text.contains("payload: &crate::generated::Amount"));
        assert!(text.contains("Channel30 = 30"));
        assert!(text.contains("destination: &crate::generated::Label"));
        assert!(text.contains("pub struct GeneratedProject {"));
        assert!(text.contains("pub fn admit_root<H: CommitmentHasher>"));
        assert!(text.contains("pub struct GeneratedCommandEnvelope {"));
        assert!(text.contains("pub struct GeneratedContextEnvelope {"));
        assert!(text.contains("pub enum RejectReasonId {"));
        assert!(text.contains("pub enum CommittedFailureReasonId {"));
        assert!(text.contains("pub struct GeneratedTransition<'a, H: CommitmentHasher> {"));
        assert!(text.contains(
            "pub fn read_amount(\n        &mut self,\n    ) -> Result<crate::generated::Amount, GeneratedProjectError>"
        ));
        assert!(text.contains(
            ".read(crate::generated::BalanceState::amount_path())?\n            .clone();"
        ));
        assert!(text.contains(
            "pub fn update_amount(\n        &mut self,\n        value: &crate::generated::Amount,"
        ));
        assert!(text.contains(
            "crate::generated::BalanceState::amount_path(),\n            value.to_value()?,"
        ));
        assert!(text.contains(
            "pub fn emit_effect_20(\n        &mut self,\n        ordinal: u32,\n        authority: zeno_fcis_catalog::NonZeroHash,\n        payload: &crate::generated::Amount,"
        ));
        assert!(text.contains(
            "let effect = effect_20(\n            ordinal,\n            authority.get(),\n            Hash32::ZERO,"
        ));
        assert!(text.contains(
            "pub fn emit_effect_21(\n        &mut self,\n        ordinal: u32,\n        authority: Hash32,\n        subject: Hash32,\n        payload: &crate::generated::Signed,"
        ));
        assert!(text.contains(
            "pub fn emit_effect_22(\n        &mut self,\n        ordinal: u32,\n        payload: &crate::generated::Amount,"
        ));
        assert!(text.contains(
            "pub fn enqueue_channel_30(\n        &mut self,\n        ordinal: u32,\n        destination: &crate::generated::Label,\n        payload: &crate::generated::Event,"
        ));
        assert!(text.contains("reason: RejectReasonId"));
        assert!(text.contains("reason: CommittedFailureReasonId"));
        assert!(text.contains("Result<GeneratedTransition<'a, H>, GeneratedProjectError>"));
        assert!(text.contains("pub fn admit_command<H: CommitmentHasher>"));
        assert!(text.contains("command: &crate::generated::Event"));
        assert!(text.contains("pub fn admit_context<H: CommitmentHasher>"));
        assert!(text.contains("context: &crate::generated::BalanceState"));
        assert!(text.contains(
            "pub fn observe_context_amount(\n        &mut self,\n    ) -> Result<&mut Self, GeneratedProjectError>"
        ));
        assert!(text.contains("CONTEXT_TYPE_ID,\n            vec![PathAtom::Field(1)],"));
        assert!(text.contains("command: &GeneratedCommandEnvelope"));
        assert!(text.contains("context: &GeneratedContextEnvelope"));
        assert!(text.contains("INPUT_COMMITMENT_FORMAT_VERSION: u16 = 1"));
        assert!(text.contains("COMMAND_DOMAIN: &str = \"example/core/command\""));
        assert!(text.contains("CONTEXT_DOMAIN: &str = \"example/core/context\""));
        assert!(text.contains("pre_state: &'a SchemaAdmittedEnvelope"));
        assert!(text.contains("let actual_catalog_hash = self.catalog.commitment::<H>()?"));
        assert!(!text.contains("pre_state: &'a Value"));
        assert!(!text.contains("catalog: &'a ProjectCatalog"));
        assert!(!text.contains("command_hash: Hash32"));
        assert!(!text.contains("context_hash: Hash32"));
        assert!(!text.contains("pub admitted: SchemaAdmittedTypeEnvelope"));
        assert!(!text.contains("pub commitment: Hash32"));
        assert!(!text.contains("pub inner: CataloguedTransitionBuilder"));
        assert!(!text.contains("pub fn emit(&mut self, effect: Effect)"));
        assert!(!text.contains("pub fn enqueue(&mut self, entry: OutboxEntry)"));
        assert!(!text.contains("pub fn read(&mut self, path: ValuePath) -> Result<&'a Value"));
        assert!(!text.contains("pub fn update(&mut self, path: ValuePath, value: Value)"));
        assert!(!text.contains("pub fn insert(&mut self, path: ValuePath"));
        assert!(!text.contains("pub fn delete(&mut self, path: ValuePath"));
        assert!(!text.contains("pub fn observe_context(&mut self, path: AccessPath)"));
        assert!(!text.contains("ValuePath"));
        assert!(!text.contains("zeno_fcis_value::Value"));
        assert!(!text.contains("reason_id: SemanticId"));
        assert!(!text.contains("Result<CataloguedTransitionBuilder<'a, H>"));
    }

    #[test]
    fn non_record_context_generates_typed_root_observation() {
        let generated = generate_project::<TestHasher>(
            &catalog_with_context_type(false, 5),
            &spec(BootstrapLimits::default()),
        )
        .unwrap_or_else(|error| panic!("bootstrap generation: {error}"));
        let project = generated
            .files()
            .iter()
            .find(|file| file.path() == "rust/project.rs")
            .unwrap_or_else(|| panic!("project helpers missing"));
        let text = core::str::from_utf8(project.bytes())
            .unwrap_or_else(|error| panic!("project source UTF-8: {error}"));
        assert!(text.contains(
            "pub fn observe_context_root(&mut self) -> Result<&mut Self, GeneratedProjectError>"
        ));
        assert!(text.contains("AccessPath::try_new(CONTEXT_TYPE_ID, vec![])"));
        assert!(!text.contains("pub fn observe_context(&mut self, path: AccessPath)"));
    }

    #[test]
    fn migration_and_evidence_begin_without_authority() {
        let generated = bundle();
        let migration = generated
            .files()
            .iter()
            .find(|file| file.path() == "migration.stub.zfcis")
            .unwrap_or_else(|| panic!("migration stub missing"));
        let evidence = generated
            .files()
            .iter()
            .find(|file| file.path() == "evidence.requirements.zfcis")
            .unwrap_or_else(|| panic!("evidence requirements missing"));
        let migration = core::str::from_utf8(migration.bytes())
            .unwrap_or_else(|error| panic!("migration UTF-8: {error}"));
        let evidence = core::str::from_utf8(evidence.bytes())
            .unwrap_or_else(|error| panic!("evidence UTF-8: {error}"));
        assert!(migration.contains("status = not-specified"));
        assert!(evidence.contains("promotion_status = denied"));
    }

    #[test]
    fn duplicate_and_nonportable_paths_fail_closed() {
        let mut duplicate = Assembly::new(BootstrapLimits::default());
        duplicate
            .push("safe/file.txt".to_owned(), vec![1])
            .unwrap_or_else(|error| panic!("first file: {error}"));
        assert_eq!(
            duplicate.push("safe/file.txt".to_owned(), vec![2]),
            Err(BootstrapError::DuplicatePath("safe/file.txt".to_owned()))
        );

        let mut invalid = Assembly::new(BootstrapLimits::default());
        assert_eq!(
            invalid.push("../escape".to_owned(), vec![1]),
            Err(BootstrapError::InvalidPath("../escape".to_owned()))
        );
    }

    #[test]
    fn wrong_catalog_provider_fails_closed() {
        let result =
            generate_project::<WrongHasher>(&catalog(false), &spec(BootstrapLimits::default()));
        assert_eq!(
            result,
            Err(BootstrapError::Catalog(CatalogError::HashAlgorithmMismatch))
        );
    }

    #[test]
    fn reserved_zero_bootstrap_commitment_fails_closed() {
        assert_eq!(
            generate_project::<ZeroBootstrapHasher>(
                &catalog(false),
                &spec(BootstrapLimits::default()),
            ),
            Err(BootstrapError::ZeroDerivedCommitment)
        );
    }

    #[test]
    fn exact_file_count_boundary_includes_bootstrap_manifest() {
        let generated = bundle();
        let file_count = u32::try_from(generated.files().len())
            .unwrap_or_else(|error| panic!("file count: {error}"));
        let limits = BootstrapLimits::try_new(
            file_count - 1,
            BootstrapLimits::default().max_file_bytes(),
            BootstrapLimits::default().max_total_bytes(),
        )
        .unwrap_or_else(|error| panic!("limits: {error}"));
        assert!(matches!(
            generate_project::<TestHasher>(&catalog(false), &spec(limits)),
            Err(BootstrapError::FileCountExceeded { .. })
        ));
    }

    #[test]
    fn exact_per_file_boundary_fails_closed() {
        let limits =
            BootstrapLimits::try_new(256, 1, 1).unwrap_or_else(|error| panic!("limits: {error}"));
        assert!(matches!(
            generate_project::<TestHasher>(&catalog(false), &spec(limits)),
            Err(BootstrapError::FileBytesExceeded { .. })
        ));
    }

    #[test]
    fn exact_aggregate_boundary_includes_bootstrap_manifest() {
        let generated = bundle();
        let maximum_file = generated
            .files()
            .iter()
            .map(|file| u64::try_from(file.bytes().len()).unwrap_or(u64::MAX))
            .max()
            .unwrap_or_else(|| panic!("generated bundle empty"));
        let limits =
            BootstrapLimits::try_new(crate::MAX_BOOTSTRAP_FILES, maximum_file, maximum_file)
                .unwrap_or_else(|error| panic!("limits: {error}"));
        assert!(matches!(
            generate_project::<TestHasher>(&catalog(false), &spec(limits)),
            Err(BootstrapError::TotalBytesExceeded { .. })
        ));
    }
}
