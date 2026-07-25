//! Typed Rust domain adapter rendering.
//!
//! Emits ordinary Rust structs, enums, newtypes, strict `to_value` and
//! `try_from_value` conversions, and typed patch-path constructors for
//! admitted record fields. Stable numeric identifiers drive every emitted
//! reference; source declaration order and names never define protocol meaning.

use std::fmt::Write as _;

use zeno_fcis_schema::{FieldDef, Schema, TypeDef, TypeId, TypeKind};

use crate::CodegenError;

/// Renders the complete typed Rust adapter block (types, conversions, paths).
pub(crate) fn render(schema: &Schema) -> Result<String, CodegenError> {
    validate_names(schema)?;
    let mut output = String::new();
    for definition in schema.types() {
        render_type(&mut output, schema, definition)?;
    }
    Ok(output)
}

fn validate_names(schema: &Schema) -> Result<(), CodegenError> {
    for definition in schema.types() {
        rust_name(definition.name().as_str())?;
        match definition.kind() {
            TypeKind::Record { fields } => {
                for field in fields.iter() {
                    rust_name(field.name().as_str())?;
                }
            }
            TypeKind::Enum { variants } => {
                for variant in variants.iter() {
                    rust_name(variant.name().as_str())?;
                }
            }
            TypeKind::Sum { variants } => {
                for variant in variants.iter() {
                    rust_name(variant.name().as_str())?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn render_type(
    output: &mut String,
    schema: &Schema,
    definition: &TypeDef,
) -> Result<(), CodegenError> {
    let name = rust_name(definition.name().as_str())?;
    match definition.kind() {
        TypeKind::Unit => render_unit(output, &name),
        TypeKind::Bool => render_bool(output, &name),
        TypeKind::U128 { min, max } => render_u128(output, &name, *min, *max),
        TypeKind::I128 { min, max } => render_i128(output, &name, *min, *max),
        TypeKind::Bytes { min_len, max_len } => render_bytes(output, &name, *min_len, *max_len),
        TypeKind::Text { min_len, max_len } => render_text(output, &name, *min_len, *max_len),
        TypeKind::Enum { variants } => render_enum(output, schema, definition, &name, variants),
        TypeKind::Tuple { items } => render_tuple(output, schema, definition, &name, items),
        TypeKind::Record { fields } => render_record(output, schema, definition, &name, fields)?,
        TypeKind::Sum { variants } => render_sum(output, schema, definition, &name, variants),
        TypeKind::Vector {
            element,
            min_len,
            max_len,
        } => render_vector(output, schema, &name, element, *min_len, *max_len),
        TypeKind::Map {
            key,
            value,
            min_len,
            max_len,
        } => render_map(output, schema, &name, key, value, *min_len, *max_len),
    }
    Ok(())
}

fn render_unit(output: &mut String, name: &str) {
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name};");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{ Ok(Value::Unit) }}"
    );
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        match value {{ Value::Unit => Ok(Self), _ => Err(AdapterError::TypeMismatch) }}"
    );
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_bool(output: &mut String, name: &str) {
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub bool);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{ Ok(Value::Bool(self.0)) }}"
    );
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        match value {{ Value::Bool(v) => Ok(Self(v)), _ => Err(AdapterError::TypeMismatch) }}"
    );
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_u128(output: &mut String, name: &str, min: u128, max: u128) {
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub u128);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN: u128 = {min};");
    let _ = writeln!(output, "    pub const MAX: u128 = {max};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{ Ok(Value::U128(self.0)) }}"
    );
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(
        output,
        "            Value::U128(v) if v >= Self::MIN && v <= Self::MAX => Ok(Self(v)),"
    );
    let _ = writeln!(
        output,
        "            Value::U128(_) => Err(AdapterError::IntegerRange),"
    );
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_i128(output: &mut String, name: &str, min: i128, max: i128) {
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub i128);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN: i128 = {min};");
    let _ = writeln!(output, "    pub const MAX: i128 = {max};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{ Ok(Value::I128(self.0)) }}"
    );
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(
        output,
        "            Value::I128(v) if v >= Self::MIN && v <= Self::MAX => Ok(Self(v)),"
    );
    let _ = writeln!(
        output,
        "            Value::I128(_) => Err(AdapterError::IntegerRange),"
    );
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_bytes(output: &mut String, name: &str, min_len: u32, max_len: u32) {
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub Box<[u8]>);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN_LEN: u32 = {min_len};");
    let _ = writeln!(output, "    pub const MAX_LEN: u32 = {max_len};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{ Ok(Value::Bytes(self.0.clone())) }}"
    );
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Bytes(b) => {{");
    let _ = writeln!(
        output,
        "                let len = u32::try_from(b.len()).map_err(|_| AdapterError::Length)?;"
    );
    let _ = writeln!(
        output,
        "                if len < Self::MIN_LEN || len > Self::MAX_LEN {{ return Err(AdapterError::Length); }}"
    );
    let _ = writeln!(output, "                Ok(Self(b))");
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_text(output: &mut String, name: &str, min_len: u32, max_len: u32) {
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub Box<str>);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN_LEN: u32 = {min_len};");
    let _ = writeln!(output, "    pub const MAX_LEN: u32 = {max_len};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        if !self.0.is_ascii() {{ return Err(AdapterError::NonAsciiText); }}"
    );
    let _ = writeln!(output, "        Ok(Value::Text(self.0.clone()))");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Text(t) => {{");
    let _ = writeln!(
        output,
        "                if !t.is_ascii() {{ return Err(AdapterError::NonAsciiText); }}"
    );
    let _ = writeln!(
        output,
        "                let len = u32::try_from(t.len()).map_err(|_| AdapterError::Length)?;"
    );
    let _ = writeln!(
        output,
        "                if len < Self::MIN_LEN || len > Self::MAX_LEN {{ return Err(AdapterError::Length); }}"
    );
    let _ = writeln!(output, "                Ok(Self(t))");
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_enum(
    output: &mut String,
    _schema: &Schema,
    definition: &TypeDef,
    name: &str,
    variants: &[zeno_fcis_schema::EnumVariantDef],
) {
    let type_const = type_const_name(name);
    let _ = writeln!(output, "#[derive(Clone, Copy, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum {name} {{");
    for variant in variants {
        let vname = variant.name().as_str();
        let _ = writeln!(output, "    {vname},");
    }
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(output, "        Ok(match self {{");
    for variant in variants {
        let vname = variant.name().as_str();
        let vconst = variant_const_name(name, variant.name().as_str());
        let _ = writeln!(
            output,
            "            Self::{vname} => Value::Enum {{ type_id: {type_const}, variant: {vconst} }},"
        );
    }
    let _ = writeln!(output, "        }})");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(
        output,
        "            Value::Enum {{ type_id, variant }} if type_id == {type_const} => match variant {{"
    );
    for variant in variants {
        let vname = variant.name().as_str();
        let vconst = variant_const_name(name, variant.name().as_str());
        let _ = writeln!(output, "                {vconst} => Ok(Self::{vname}),");
    }
    let _ = writeln!(
        output,
        "                _ => Err(AdapterError::UnknownVariant),"
    );
    let _ = writeln!(output, "            }},");
    let _ = writeln!(
        output,
        "            Value::Enum {{ .. }} => Err(AdapterError::TypeMismatch),"
    );
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
    let _ = definition;
}

fn render_tuple(
    output: &mut String,
    schema: &Schema,
    _definition: &TypeDef,
    name: &str,
    items: &[TypeId],
) {
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name} {{");
    for (index, item) in items.iter().enumerate() {
        let item_name = resolve_type_name(schema, *item);
        let _ = writeln!(output, "    pub field_{index}: {item_name},");
    }
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        let mut items: Vec<Value> = Vec::with_capacity({count});",
        count = items.len()
    );
    for (index, _item) in items.iter().enumerate() {
        let _ = writeln!(
            output,
            "        items.push(self.field_{index}.to_value()?);"
        );
    }
    let _ = writeln!(output, "        Ok(Value::tuple(items))");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Tuple(items) => {{");
    let _ = writeln!(
        output,
        "                if items.len() != {count} {{ return Err(AdapterError::RecordShape); }}",
        count = items.len()
    );
    let _ = writeln!(
        output,
        "                let mut iter = Vec::from(items).into_iter();"
    );
    for (index, item) in items.iter().enumerate() {
        let item_name = resolve_type_name(schema, *item);
        let _ = writeln!(
            output,
            "                let field_{index} = {item_name}::try_from_value(iter.next().ok_or(AdapterError::RecordShape)?)?;"
        );
    }
    let _ = writeln!(output, "                Ok(Self {{");
    for (index, _item) in items.iter().enumerate() {
        let _ = writeln!(output, "                    field_{index},");
    }
    let _ = writeln!(output, "                }})");
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_record(
    output: &mut String,
    schema: &Schema,
    definition: &TypeDef,
    name: &str,
    fields: &[FieldDef],
) -> Result<(), CodegenError> {
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name} {{");
    for field in fields {
        let fname = rust_name(field.name().as_str())?;
        let ftype = resolve_type_name(schema, field.type_id());
        let _ = writeln!(output, "    pub {fname}: {ftype},");
    }
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        let mut fields: Vec<Field> = Vec::with_capacity({count});",
        count = fields.len()
    );
    for field in fields {
        let fname = rust_name(field.name().as_str())?;
        let fconst = field_const_name(name, field.name().as_str());
        let _ = writeln!(
            output,
            "        fields.push(Field::new({fconst} as u16, self.{fname}.to_value()?));"
        );
    }
    let _ = writeln!(output, "        Ok(Value::record_canonical(fields)?)");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Record(fields) => {{");
    let _ = writeln!(
        output,
        "                if fields.len() != {count} {{ return Err(AdapterError::RecordShape); }}",
        count = fields.len()
    );
    let _ = writeln!(
        output,
        "                let mut iter = fields.into_vec().into_iter();"
    );
    for field in fields {
        let fname = rust_name(field.name().as_str())?;
        let ftype = resolve_type_name(schema, field.type_id());
        let fconst = field_const_name(name, field.name().as_str());
        let _ = writeln!(
            output,
            "                let next = iter.next().ok_or(AdapterError::RecordShape)?;"
        );
        let _ = writeln!(
            output,
            "                if next.id() != {fconst} as u16 {{ return Err(AdapterError::RecordShape); }}"
        );
        let _ = writeln!(
            output,
            "                let {fname} = {ftype}::try_from_value(next.into_parts().1)?;"
        );
    }
    let _ = writeln!(output, "                Ok(Self {{");
    for field in fields {
        let fname = rust_name(field.name().as_str())?;
        let _ = writeln!(output, "                    {fname},");
    }
    let _ = writeln!(output, "                }})");
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    render_record_paths(output, name, fields)?;
    let _ = writeln!(output, "}}\n");
    let _ = definition;
    Ok(())
}

fn render_record_paths(
    output: &mut String,
    name: &str,
    fields: &[FieldDef],
) -> Result<(), CodegenError> {
    for field in fields {
        let fname = rust_name(field.name().as_str())?;
        let fconst = field_const_name(name, field.name().as_str());
        let _ = writeln!(
            output,
            "    pub fn {fname}_path() -> ValuePath {{ ValuePath::new(vec![PathSegment::Field({fconst} as u16)]) }}"
        );
    }
    Ok(())
}

fn render_sum(
    output: &mut String,
    schema: &Schema,
    _definition: &TypeDef,
    name: &str,
    variants: &[zeno_fcis_schema::SumVariantDef],
) {
    let type_const = type_const_name(name);
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub enum {name} {{");
    for variant in variants {
        let vname = variant.name().as_str();
        match variant.payload() {
            None => {
                let _ = writeln!(output, "    {vname},");
            }
            Some(payload) => {
                let payload_name = resolve_type_name(schema, payload);
                let _ = writeln!(output, "    {vname}({payload_name}),");
            }
        }
    }
    let _ = writeln!(output, "}}");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(output, "        Ok(match self {{");
    for variant in variants {
        let vname = variant.name().as_str();
        let vconst = variant_const_name(name, variant.name().as_str());
        match variant.payload() {
            None => {
                let _ = writeln!(
                    output,
                    "            Self::{vname} => Value::Sum {{ type_id: {type_const}, variant: {vconst}, payload: None }},"
                );
            }
            Some(_) => {
                let _ = writeln!(
                    output,
                    "            Self::{vname}(p) => Value::Sum {{ type_id: {type_const}, variant: {vconst}, payload: Some(Box::new(p.to_value()?)) }},"
                );
            }
        }
    }
    let _ = writeln!(output, "        }})");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(
        output,
        "            Value::Sum {{ type_id, variant, payload }} if type_id == {type_const} => match variant {{"
    );
    for variant in variants {
        let vname = variant.name().as_str();
        let vconst = variant_const_name(name, variant.name().as_str());
        match variant.payload() {
            None => {
                let _ = writeln!(
                    output,
                    "                {vconst} => match payload {{ None => Ok(Self::{vname}), Some(_) => Err(AdapterError::UnexpectedPayload) }},"
                );
            }
            Some(payload) => {
                let payload_name = resolve_type_name(schema, payload);
                let _ = writeln!(
                    output,
                    "                {vconst} => match payload {{ None => Err(AdapterError::MissingPayload), Some(p) => Ok(Self::{vname}({payload_name}::try_from_value(*p)?)) }},"
                );
            }
        }
    }
    let _ = writeln!(
        output,
        "                _ => Err(AdapterError::UnknownVariant),"
    );
    let _ = writeln!(output, "            }},");
    let _ = writeln!(
        output,
        "            Value::Sum {{ .. }} => Err(AdapterError::TypeMismatch),"
    );
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_vector(
    output: &mut String,
    schema: &Schema,
    name: &str,
    element: &TypeId,
    min_len: u32,
    max_len: u32,
) {
    let element_name = resolve_type_name(schema, *element);
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub Box<[{element_name}]>);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN_LEN: u32 = {min_len};");
    let _ = writeln!(output, "    pub const MAX_LEN: u32 = {max_len};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        let items: Result<Vec<Value>, _> = self.0.iter().map(|x| x.to_value()).collect();"
    );
    let _ = writeln!(output, "        Ok(Value::vector(items?))");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Vector(items) => {{");
    let _ = writeln!(
        output,
        "                let len = u32::try_from(items.len()).map_err(|_| AdapterError::Length)?;"
    );
    let _ = writeln!(
        output,
        "                if len < Self::MIN_LEN || len > Self::MAX_LEN {{ return Err(AdapterError::Length); }}"
    );
    let _ = writeln!(
        output,
        "                let mapped: Result<Vec<{element_name}>, _> = Vec::from(items).into_iter().map({element_name}::try_from_value).collect();"
    );
    let _ = writeln!(
        output,
        "                Ok(Self(mapped?.into_boxed_slice()))"
    );
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn render_map(
    output: &mut String,
    schema: &Schema,
    name: &str,
    key: &TypeId,
    value: &TypeId,
    min_len: u32,
    max_len: u32,
) {
    let key_name = resolve_type_name(schema, *key);
    let value_name = resolve_type_name(schema, *value);
    let entry_name = format!("{name}Entry");
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(
        output,
        "pub struct {entry_name} {{ pub key: {key_name}, pub value: {value_name} }}"
    );
    let _ = writeln!(output, "#[derive(Clone, Debug, Eq, PartialEq)]");
    let _ = writeln!(output, "pub struct {name}(pub Box<[{entry_name}]>);");
    let _ = writeln!(output, "impl {name} {{");
    let _ = writeln!(output, "    pub const MIN_LEN: u32 = {min_len};");
    let _ = writeln!(output, "    pub const MAX_LEN: u32 = {max_len};");
    let _ = writeln!(
        output,
        "    pub fn to_value(&self) -> Result<Value, AdapterError> {{"
    );
    let _ = writeln!(
        output,
        "        let mut entries: Vec<MapEntry> = Vec::with_capacity(self.0.len());"
    );
    let _ = writeln!(output, "        for entry in self.0.iter() {{");
    let _ = writeln!(output, "            let key = entry.key.to_value()?;");
    let _ = writeln!(output, "            let value = entry.value.to_value()?;");
    let _ = writeln!(
        output,
        "            let encoded_key = key.canonical_bytes()?;"
    );
    let _ = writeln!(
        output,
        "            entries.push(MapEntry::new(encoded_key, key, value));"
    );
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "        Ok(Value::normalize_map(entries)?)");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(
        output,
        "    pub fn try_from_value(value: Value) -> Result<Self, AdapterError> {{"
    );
    let _ = writeln!(output, "        match value {{");
    let _ = writeln!(output, "            Value::Map(entries) => {{");
    let _ = writeln!(
        output,
        "                let len = u32::try_from(entries.len()).map_err(|_| AdapterError::Length)?;"
    );
    let _ = writeln!(
        output,
        "                if len < Self::MIN_LEN || len > Self::MAX_LEN {{ return Err(AdapterError::Length); }}"
    );
    let _ = writeln!(
        output,
        "                let mapped: Result<Vec<{entry_name}>, AdapterError> = Vec::from(entries).into_iter().map(|e| {{"
    );
    let _ = writeln!(
        output,
        "            let (_encoded_key, key, value) = e.into_parts();"
    );
    let _ = writeln!(
        output,
        "            Ok({entry_name} {{ key: {key_name}::try_from_value(key)?, value: {value_name}::try_from_value(value)? }})"
    );
    let _ = writeln!(output, "        }}).collect();");
    let _ = writeln!(
        output,
        "                Ok(Self(mapped?.into_boxed_slice()))"
    );
    let _ = writeln!(output, "            }}");
    let _ = writeln!(output, "            _ => Err(AdapterError::TypeMismatch),");
    let _ = writeln!(output, "        }}");
    let _ = writeln!(output, "    }}");
    let _ = writeln!(output, "}}\n");
}

fn resolve_type_name(schema: &Schema, id: TypeId) -> String {
    match schema.type_by_id(id) {
        Some(definition) => definition.name().as_str().to_owned(),
        None => "Unresolved".to_owned(),
    }
}

fn type_const_name(name: &str) -> String {
    format!("TYPE_{}", name.to_ascii_uppercase())
}

fn field_const_name(type_name: &str, field_name: &str) -> String {
    format!(
        "FIELD_{}_{}",
        type_name.to_ascii_uppercase(),
        field_name.to_ascii_uppercase()
    )
}

fn variant_const_name(type_name: &str, variant_name: &str) -> String {
    format!(
        "VARIANT_{}_{}",
        type_name.to_ascii_uppercase(),
        variant_name.to_ascii_uppercase()
    )
}

fn rust_name(value: &str) -> Result<String, CodegenError> {
    if is_rust_reserved(value) {
        return Err(CodegenError::InvalidIdentifier);
    }
    Ok(value.to_owned())
}

fn is_rust_reserved(value: &str) -> bool {
    const RESERVED: &[&str] = &[
        "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn",
        "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
        "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe",
        "use", "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try",
        "union",
    ];
    RESERVED.contains(&value)
}
