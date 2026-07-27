//! Inspectable Rust schema reconstruction and root-envelope helpers.

use std::fmt::Write as _;

use zeno_fcis_codec::Hash32;
use zeno_fcis_schema::{Schema, TypeKind};

use crate::CodegenError;

pub(crate) fn render_imports(schema: &Schema) -> String {
    let has_record = schema
        .types()
        .iter()
        .any(|definition| matches!(definition.kind(), TypeKind::Record { .. }));
    let has_enum = schema
        .types()
        .iter()
        .any(|definition| matches!(definition.kind(), TypeKind::Enum { .. }));
    let has_sum = schema
        .types()
        .iter()
        .any(|definition| matches!(definition.kind(), TypeKind::Sum { .. }));

    let mut names = vec![
        "Schema",
        "SchemaAdmittedEnvelope",
        "SchemaEnvelopeError",
        "SchemaError",
        "SchemaLimits",
        "TypeDef",
        "TypeId",
        "TypeKind",
        "ValidationLimits",
    ];
    if has_record {
        names.extend_from_slice(&["FieldDef", "FieldId"]);
    }
    if has_enum {
        names.push("EnumVariantDef");
    }
    if has_sum {
        names.push("SumVariantDef");
    }
    if has_enum || has_sum {
        names.push("VariantId");
    }
    names.sort_unstable();
    format!("use zeno_fcis_schema::{{{}}};\n", names.join(", "))
}

pub(crate) fn render(schema: &Schema, schema_hash: Hash32) -> Result<String, CodegenError> {
    let root = schema
        .type_by_id(schema.root_type())
        .ok_or(CodegenError::SchemaEncoding)?;
    let reconstruction_limits = reconstruction_limits(schema)?;
    let mut output = String::new();

    writeln!(output, "impl {} {{", root.name().as_str())
        .map_err(|_| CodegenError::LengthOverflow)?;
    output
        .push_str("    /// Reconstructs the exact closed schema used to generate this adapter.\n");
    output.push_str("    pub fn zfcis_schema() -> Result<Schema, SchemaError> {\n");
    writeln!(
        output,
        "        let limits = SchemaLimits {{ max_types: {}, max_fields: {}, max_variants: {}, max_tuple_items: {} }};",
        reconstruction_limits.max_types,
        reconstruction_limits.max_fields,
        reconstruction_limits.max_variants,
        reconstruction_limits.max_tuple_items,
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    writeln!(output, "        let mut types: Vec<TypeDef> = vec![];")
        .map_err(|_| CodegenError::LengthOverflow)?;
    for definition in schema.types() {
        let kind = render_kind(definition.kind())?;
        writeln!(
            output,
            "        types.push(TypeDef::try_new(TypeId::new({}), {:?}, {kind}, limits)?);",
            definition.id().get(),
            definition.name().as_str(),
        )
        .map_err(|_| CodegenError::LengthOverflow)?;
    }
    writeln!(
        output,
        "        Schema::try_new(PROFILE_NAME, PROFILE_VERSION, TypeId::new(ROOT_TYPE_ID), types, limits)"
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    output.push_str("    }\n\n");
    output.push_str("    /// Converts this typed root into a schema-bound canonical envelope.\n");
    output.push_str("    pub fn to_root_envelope<H: CommitmentHasher>(\n");
    output.push_str("        &self,\n");
    output.push_str("        limits: ValidationLimits,\n");
    output.push_str("    ) -> Result<SchemaAdmittedEnvelope, AdapterError> {\n");
    output.push_str("        let value = self.to_value()?;\n");
    output.push_str("        let schema = Self::zfcis_schema()?;\n");
    output.push_str(
        "        let envelope = SchemaAdmittedEnvelope::try_new::<H>(&schema, value, limits)?;\n",
    );
    writeln!(
        output,
        "        if envelope.schema_hash() != {} {{",
        render_hash(schema_hash)
    )
    .map_err(|_| CodegenError::LengthOverflow)?;
    output.push_str("            return Err(AdapterError::SchemaHashMismatch);\n");
    output.push_str("        }\n");
    output.push_str("        Ok(envelope)\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");

    Ok(output)
}

#[derive(Clone, Copy)]
struct ReconstructionLimits {
    max_types: u32,
    max_fields: u32,
    max_variants: u32,
    max_tuple_items: u32,
}

fn reconstruction_limits(schema: &Schema) -> Result<ReconstructionLimits, CodegenError> {
    let max_types =
        u32::try_from(schema.types().len()).map_err(|_| CodegenError::LengthOverflow)?;
    let mut max_fields = 0_u32;
    let mut max_variants = 0_u32;
    let mut max_tuple_items = 0_u32;
    for definition in schema.types() {
        match definition.kind() {
            TypeKind::Record { fields } => {
                max_fields = max_fields
                    .max(u32::try_from(fields.len()).map_err(|_| CodegenError::LengthOverflow)?);
            }
            TypeKind::Enum { variants } => {
                max_variants = max_variants
                    .max(u32::try_from(variants.len()).map_err(|_| CodegenError::LengthOverflow)?);
            }
            TypeKind::Sum { variants } => {
                max_variants = max_variants
                    .max(u32::try_from(variants.len()).map_err(|_| CodegenError::LengthOverflow)?);
            }
            TypeKind::Tuple { items } => {
                max_tuple_items = max_tuple_items
                    .max(u32::try_from(items.len()).map_err(|_| CodegenError::LengthOverflow)?);
            }
            _ => {}
        }
    }
    Ok(ReconstructionLimits {
        max_types,
        max_fields,
        max_variants,
        max_tuple_items,
    })
}

fn render_kind(kind: &TypeKind) -> Result<String, CodegenError> {
    let mut output = String::new();
    match kind {
        TypeKind::Unit => output.push_str("TypeKind::Unit"),
        TypeKind::Bool => output.push_str("TypeKind::Bool"),
        TypeKind::U128 { min, max } => {
            write!(
                output,
                "TypeKind::U128 {{ min: {min}_u128, max: {max}_u128 }}"
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
        TypeKind::I128 { min, max } => {
            write!(
                output,
                "TypeKind::I128 {{ min: {}, max: {} }}",
                render_i128(*min),
                render_i128(*max),
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
        TypeKind::Bytes { min_len, max_len } => {
            write!(
                output,
                "TypeKind::Bytes {{ min_len: {min_len}, max_len: {max_len} }}"
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
        TypeKind::Text { min_len, max_len } => {
            write!(
                output,
                "TypeKind::Text {{ min_len: {min_len}, max_len: {max_len} }}"
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
        TypeKind::Enum { variants } => {
            output.push_str("TypeKind::Enum { variants: vec![");
            for variant in variants {
                write!(
                    output,
                    "EnumVariantDef::try_new(VariantId::new({}), {:?})?,",
                    variant.id().get(),
                    variant.name().as_str(),
                )
                .map_err(|_| CodegenError::LengthOverflow)?;
            }
            output.push_str("].into_boxed_slice() }");
        }
        TypeKind::Tuple { items } => {
            output.push_str("TypeKind::Tuple { items: vec![");
            for item in items {
                write!(output, "TypeId::new({}),", item.get())
                    .map_err(|_| CodegenError::LengthOverflow)?;
            }
            output.push_str("].into_boxed_slice() }");
        }
        TypeKind::Record { fields } => {
            output.push_str("TypeKind::Record { fields: vec![");
            for field in fields {
                write!(
                    output,
                    "FieldDef::try_new(FieldId::new({}), {:?}, TypeId::new({}))?,",
                    field.id().get(),
                    field.name().as_str(),
                    field.type_id().get(),
                )
                .map_err(|_| CodegenError::LengthOverflow)?;
            }
            output.push_str("].into_boxed_slice() }");
        }
        TypeKind::Sum { variants } => {
            output.push_str("TypeKind::Sum { variants: vec![");
            for variant in variants {
                let payload = match variant.payload() {
                    Some(payload) => format!("Some(TypeId::new({}))", payload.get()),
                    None => "None".to_owned(),
                };
                write!(
                    output,
                    "SumVariantDef::try_new(VariantId::new({}), {:?}, {payload})?,",
                    variant.id().get(),
                    variant.name().as_str(),
                )
                .map_err(|_| CodegenError::LengthOverflow)?;
            }
            output.push_str("].into_boxed_slice() }");
        }
        TypeKind::Vector {
            element,
            min_len,
            max_len,
        } => {
            write!(
                output,
                "TypeKind::Vector {{ element: TypeId::new({}), min_len: {min_len}, max_len: {max_len} }}",
                element.get(),
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
        TypeKind::Map {
            key,
            value,
            min_len,
            max_len,
        } => {
            write!(
                output,
                "TypeKind::Map {{ key: TypeId::new({}), value: TypeId::new({}), min_len: {min_len}, max_len: {max_len} }}",
                key.get(),
                value.get(),
            )
            .map_err(|_| CodegenError::LengthOverflow)?;
        }
    }
    Ok(output)
}

fn render_i128(value: i128) -> String {
    if value == i128::MIN {
        "i128::MIN".to_owned()
    } else if value == i128::MAX {
        "i128::MAX".to_owned()
    } else {
        format!("{value}_i128")
    }
}

fn render_hash(hash: Hash32) -> String {
    let mut output = String::from("Hash32::new([");
    for (index, byte) in hash.as_bytes().iter().copied().enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        let _ = write!(output, "0x{byte:02x}");
    }
    output.push_str("])");
    output
}
