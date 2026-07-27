//! Deterministic renderers and manifest construction.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use zeno_fcis_codec::{CanonicalEncode as _, Domain, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::{Schema, TypeKind};

use crate::adapters;
use crate::model::FORMATTER_ID;
use crate::python;
use crate::root_envelope;
use crate::vectors;
use crate::{CodegenError, GeneratedBundle, GeneratedFile, GenerationSpec};

/// Stable generator semantics identifier.
pub const GENERATOR_ID: &str = "zeno-fcis-codegen/1";

/// Generates typed Rust/Python adapters, patch paths, codec vectors, canonical
/// schema bytes, and a content-addressed manifest.
pub fn generate(schema: &Schema, spec: &GenerationSpec) -> Result<GeneratedBundle, CodegenError> {
    let schema_bytes = schema
        .canonical_bytes()
        .map_err(|_| CodegenError::SchemaEncoding)?;
    let schema_hash = schema
        .schema_hash::<RustCryptoSha256>()
        .map_err(|_| CodegenError::SchemaEncoding)?;
    let constants = collect_constants(schema)?;
    let cases = vectors::build(schema)?;

    let vector_set_hash = vectors::set_hash("zeno-fcis/vector-set", &cases, |bytes| {
        hash_bytes("zeno-fcis/vector-set", bytes)
    })?;

    let rust = render_rust(schema, spec, schema_hash, &schema_bytes, &constants, &cases)?;
    let python_module = python::render_adapter_module(schema, spec, &cases)?;
    let python_codec = python::render_codec_module();

    let mut files = vec![
        GeneratedFile::new(
            format!("python/{}.py", spec.python_module()),
            python_module.into_bytes(),
        ),
        GeneratedFile::new("python/zcve.py".to_owned(), python_codec.into_bytes()),
        GeneratedFile::new(format!("rust/{}.rs", spec.rust_module()), rust.into_bytes()),
        GeneratedFile::new("schema.zcve".to_owned(), schema_bytes),
    ];
    files.sort_by(|left, right| left.path().cmp(right.path()));

    let manifest = render_manifest(schema_hash, vector_set_hash, &files, &cases)?;
    let manifest_hash = hash_bytes("zeno-fcis/generation-manifest", manifest.as_bytes())?;
    files.push(GeneratedFile::new(
        "MANIFEST.zfcis".to_owned(),
        manifest.into_bytes(),
    ));
    files.sort_by(|left, right| left.path().cmp(right.path()));

    Ok(GeneratedBundle::new(
        GENERATOR_ID,
        FORMATTER_ID,
        schema_hash,
        manifest_hash,
        vector_set_hash,
        files,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Constant {
    name: String,
    value: u32,
    type_suffix: &'static str,
}

fn collect_constants(schema: &Schema) -> Result<Vec<Constant>, CodegenError> {
    let mut seen = BTreeSet::new();
    let mut constants = Vec::new();
    for type_definition in schema.types() {
        let type_name = constant_fragment(type_definition.name().as_str());
        push_constant(
            &mut seen,
            &mut constants,
            format!("TYPE_{type_name}"),
            type_definition.id().get(),
            "u32",
        )?;
        match type_definition.kind() {
            TypeKind::Record { fields } => {
                for field in fields {
                    push_constant(
                        &mut seen,
                        &mut constants,
                        format!(
                            "FIELD_{}_{}",
                            type_name,
                            constant_fragment(field.name().as_str())
                        ),
                        u32::from(field.id().get()),
                        "u16",
                    )?;
                }
            }
            TypeKind::Enum { variants } => {
                for variant in variants {
                    push_constant(
                        &mut seen,
                        &mut constants,
                        format!(
                            "VARIANT_{}_{}",
                            type_name,
                            constant_fragment(variant.name().as_str())
                        ),
                        u32::from(variant.id().get()),
                        "u16",
                    )?;
                }
            }
            TypeKind::Sum { variants } => {
                for variant in variants {
                    push_constant(
                        &mut seen,
                        &mut constants,
                        format!(
                            "VARIANT_{}_{}",
                            type_name,
                            constant_fragment(variant.name().as_str())
                        ),
                        u32::from(variant.id().get()),
                        "u16",
                    )?;
                }
            }
            _ => {}
        }
    }
    constants.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(constants)
}

fn push_constant(
    seen: &mut BTreeSet<String>,
    constants: &mut Vec<Constant>,
    name: String,
    value: u32,
    type_suffix: &'static str,
) -> Result<(), CodegenError> {
    if !seen.insert(name.clone()) {
        return Err(CodegenError::ConstantCollision);
    }
    constants.push(Constant {
        name,
        value,
        type_suffix,
    });
    Ok(())
}

fn constant_fragment(value: &str) -> String {
    value.to_ascii_uppercase()
}

fn render_rust(
    schema: &Schema,
    spec: &GenerationSpec,
    schema_hash: Hash32,
    schema_bytes: &[u8],
    constants: &[Constant],
    cases: &[vectors::VectorCase],
) -> Result<String, CodegenError> {
    let adapters = adapters::render(schema)?;
    let root_envelope_block = root_envelope::render(schema, schema_hash)?;
    let mut output = String::new();
    output.push_str("// @generated by zeno-fcis-codegen/1; do not edit.\n\n");
    output.push_str("extern crate alloc;\n\n");
    output.push_str("use alloc::vec;\n");
    output.push_str("use alloc::vec::Vec;\n");
    output.push_str("use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, DecodeError, DecodeLimits, EncodeError, Hash32, decode_value};\n");
    output.push_str("use zeno_fcis_patch::{PathSegment, ValuePath};\n");
    output.push_str(&root_envelope::render_imports(schema));
    output.push_str("use zeno_fcis_value::{Field, MapEntry, Value, ValueError};\n\n");
    let _ = writeln!(output, "pub const GENERATOR_ID: &str = \"{GENERATOR_ID}\";");
    let _ = writeln!(output, "pub const FORMATTER_ID: &str = \"{FORMATTER_ID}\";");
    writeln!(
        output,
        "pub const RUST_MODULE: &str = \"{}\";",
        spec.rust_module()
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    let _ = writeln!(
        output,
        "pub const PROFILE_NAME: &str = \"{}\";",
        schema.profile().as_str()
    );
    let _ = writeln!(
        output,
        "pub const PROFILE_VERSION: u16 = {};",
        schema.version()
    );
    writeln!(
        output,
        "pub const ROOT_TYPE_ID: u32 = {};",
        schema.root_type().get()
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    writeln!(
        output,
        "pub const SCHEMA_HASH_HEX: &str = \"{schema_hash}\";"
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    writeln!(
        output,
        "pub const SCHEMA_BYTES_HEX: &str = \"{}\";\n",
        hex(schema_bytes)
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    for constant in constants {
        let _ = writeln!(
            output,
            "pub const {}: {} = {};",
            constant.name, constant.type_suffix, constant.value
        );
    }
    output.push('\n');
    render_adapter_error(&mut output);
    output.push_str(&adapters);
    output.push_str(&root_envelope_block);
    output.push_str(&vectors::render_rust(cases));
    Ok(output)
}

fn render_adapter_error(output: &mut String) {
    output.push_str("#[derive(Clone, Debug, Eq, PartialEq)]\n");
    output.push_str("pub enum AdapterError {\n");
    output.push_str("    TypeMismatch, IntegerRange, Length, NonAsciiText,\n");
    output.push_str("    RecordShape, UnknownVariant, UnexpectedPayload, MissingPayload,\n");
    output.push_str("    Encode(EncodeError), Value(ValueError), Schema(SchemaError),\n");
    output.push_str("    SchemaEnvelope(SchemaEnvelopeError), SchemaHashMismatch,\n");
    output.push_str("}\n");
    output.push_str("impl From<EncodeError> for AdapterError {\n");
    output.push_str("    fn from(error: EncodeError) -> Self { Self::Encode(error) }\n");
    output.push_str("}\n");
    output.push_str("impl From<ValueError> for AdapterError {\n");
    output.push_str("    fn from(error: ValueError) -> Self { Self::Value(error) }\n");
    output.push_str("}\n");
    output.push_str("impl From<SchemaError> for AdapterError {\n");
    output.push_str("    fn from(error: SchemaError) -> Self { Self::Schema(error) }\n");
    output.push_str("}\n");
    output.push_str("impl From<SchemaEnvelopeError> for AdapterError {\n");
    output.push_str(
        "    fn from(error: SchemaEnvelopeError) -> Self { Self::SchemaEnvelope(error) }\n",
    );
    output.push_str("}\n\n");
}

fn render_manifest(
    schema_hash: Hash32,
    vector_set_hash: Hash32,
    files: &[GeneratedFile],
    cases: &[vectors::VectorCase],
) -> Result<String, CodegenError> {
    let mut output = String::new();
    let _ = writeln!(output, "generator={GENERATOR_ID}");
    let _ = writeln!(output, "formatter={FORMATTER_ID}");
    let _ = writeln!(output, "schema_hash={schema_hash}");
    let _ = writeln!(output, "vector_set_hash={vector_set_hash}");
    for file in files {
        let file_hash = hash_bytes("zeno-fcis/generated-file", file.bytes())?;
        let length = u64::try_from(file.bytes().len()).map_err(|_| CodegenError::LengthOverflow)?;
        let _ = writeln!(
            output,
            "file={}\tlength={}\tsha256={}",
            file.path(),
            length,
            file_hash
        );
    }
    for case in cases {
        let vector_hash = hash_bytes("zeno-fcis/vector", &case.bytes)?;
        let length = u64::try_from(case.bytes.len()).map_err(|_| CodegenError::LengthOverflow)?;
        let _ = writeln!(
            output,
            "vector={}\tkind={}\tlength={}\tsha256={}",
            case.name,
            case.kind.label(),
            length,
            vector_hash
        );
    }
    Ok(output)
}

fn hash_bytes(domain_name: &'static str, bytes: &[u8]) -> Result<Hash32, CodegenError> {
    let domain = Domain::new(domain_name, 1).map_err(|_| CodegenError::SchemaEncoding)?;
    commitment::<RustCryptoSha256>(domain, bytes).map_err(|_| CodegenError::SchemaEncoding)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        output.push(char::from(DIGITS[usize::from(byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    output
}
