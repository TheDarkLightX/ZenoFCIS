//! Closed schema model and construction-time normalization.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::{FieldId, SchemaError, SchemaName, TypeId, VariantId};

/// Hard bounds for one closed protocol schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaLimits {
    /// Maximum type definitions.
    pub max_types: u32,
    /// Maximum fields in one record.
    pub max_fields: u32,
    /// Maximum variants in one enum or sum.
    pub max_variants: u32,
    /// Maximum items in one fixed tuple.
    pub max_tuple_items: u32,
}

impl Default for SchemaLimits {
    fn default() -> Self {
        Self {
            max_types: 4_096,
            max_fields: 1_024,
            max_variants: 1_024,
            max_tuple_items: 1_024,
        }
    }
}

/// One stable record field definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldDef {
    id: FieldId,
    name: SchemaName,
    type_id: TypeId,
}

impl FieldDef {
    /// Constructs a field definition.
    pub fn try_new(
        id: FieldId,
        name: impl Into<Box<str>>,
        type_id: TypeId,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id,
            name: SchemaName::try_new(name)?,
            type_id,
        })
    }

    /// Returns the stable field identifier.
    #[must_use]
    pub const fn id(&self) -> FieldId {
        self.id
    }

    /// Returns the source-facing field name.
    #[must_use]
    pub const fn name(&self) -> &SchemaName {
        &self.name
    }

    /// Returns the field value type.
    #[must_use]
    pub const fn type_id(&self) -> TypeId {
        self.type_id
    }
}

/// One payload-free enum variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnumVariantDef {
    id: VariantId,
    name: SchemaName,
}

impl EnumVariantDef {
    /// Constructs an enum variant.
    pub fn try_new(id: VariantId, name: impl Into<Box<str>>) -> Result<Self, SchemaError> {
        Ok(Self {
            id,
            name: SchemaName::try_new(name)?,
        })
    }

    /// Returns the stable variant identifier.
    #[must_use]
    pub const fn id(&self) -> VariantId {
        self.id
    }

    /// Returns the source-facing variant name.
    #[must_use]
    pub const fn name(&self) -> &SchemaName {
        &self.name
    }
}

/// One sum variant with an optional payload type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SumVariantDef {
    id: VariantId,
    name: SchemaName,
    payload: Option<TypeId>,
}

impl SumVariantDef {
    /// Constructs a sum variant.
    pub fn try_new(
        id: VariantId,
        name: impl Into<Box<str>>,
        payload: Option<TypeId>,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id,
            name: SchemaName::try_new(name)?,
            payload,
        })
    }

    /// Returns the stable variant identifier.
    #[must_use]
    pub const fn id(&self) -> VariantId {
        self.id
    }

    /// Returns the source-facing variant name.
    #[must_use]
    pub const fn name(&self) -> &SchemaName {
        &self.name
    }

    /// Returns the payload type, or `None` for a unit variant.
    #[must_use]
    pub const fn payload(&self) -> Option<TypeId> {
        self.payload
    }
}

/// The complete nonrecursive type grammar admitted by ZenoFCIS schema v1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    /// Unit value.
    Unit,
    /// Boolean value.
    Bool,
    /// Inclusive unsigned 128-bit interval.
    U128 {
        /// Minimum admitted value.
        min: u128,
        /// Maximum admitted value.
        max: u128,
    },
    /// Inclusive signed 128-bit interval.
    I128 {
        /// Minimum admitted value.
        min: i128,
        /// Maximum admitted value.
        max: i128,
    },
    /// Bounded byte string.
    Bytes {
        /// Minimum bytes.
        min_len: u32,
        /// Maximum bytes.
        max_len: u32,
    },
    /// Bounded ASCII text.
    Text {
        /// Minimum bytes.
        min_len: u32,
        /// Maximum bytes.
        max_len: u32,
    },
    /// Payload-free enum.
    Enum {
        /// Canonically ordered variants.
        variants: Box<[EnumVariantDef]>,
    },
    /// Fixed heterogeneous product.
    Tuple {
        /// Item types in semantic order.
        items: Box<[TypeId]>,
    },
    /// Product with stable numeric field identifiers.
    Record {
        /// Canonically ordered fields.
        fields: Box<[FieldDef]>,
    },
    /// Tagged sum of unit and payload variants.
    Sum {
        /// Canonically ordered variants.
        variants: Box<[SumVariantDef]>,
    },
    /// Bounded homogeneous sequence.
    Vector {
        /// Element type.
        element: TypeId,
        /// Minimum element count.
        min_len: u32,
        /// Maximum element count.
        max_len: u32,
    },
    /// Bounded canonical map.
    Map {
        /// Key type.
        key: TypeId,
        /// Value type.
        value: TypeId,
        /// Minimum entry count.
        min_len: u32,
        /// Maximum entry count.
        max_len: u32,
    },
}

impl TypeKind {
    pub(crate) fn normalize(self, limits: SchemaLimits) -> Result<Self, SchemaError> {
        match self {
            Self::U128 { min, max } if min > max => Err(SchemaError::InvalidRange),
            Self::I128 { min, max } if min > max => Err(SchemaError::InvalidRange),
            Self::Bytes { min_len, max_len } | Self::Text { min_len, max_len }
                if min_len > max_len =>
            {
                Err(SchemaError::InvalidRange)
            }
            Self::Vector {
                min_len, max_len, ..
            }
            | Self::Map {
                min_len, max_len, ..
            } if min_len > max_len => Err(SchemaError::InvalidRange),
            Self::Enum { variants } => {
                let variants = normalize_enum_variants(variants.into_vec(), limits)?;
                Ok(Self::Enum {
                    variants: variants.into_boxed_slice(),
                })
            }
            Self::Sum { variants } => {
                let variants = normalize_sum_variants(variants.into_vec(), limits)?;
                Ok(Self::Sum {
                    variants: variants.into_boxed_slice(),
                })
            }
            Self::Record { fields } => {
                let fields = normalize_fields(fields.into_vec(), limits)?;
                Ok(Self::Record {
                    fields: fields.into_boxed_slice(),
                })
            }
            Self::Tuple { items }
                if items.len() > usize::try_from(limits.max_tuple_items).unwrap_or(usize::MAX) =>
            {
                Err(SchemaError::LimitExceeded)
            }
            other => Ok(other),
        }
    }

    pub(crate) fn references(&self) -> Vec<TypeId> {
        match self {
            Self::Unit
            | Self::Bool
            | Self::U128 { .. }
            | Self::I128 { .. }
            | Self::Bytes { .. }
            | Self::Text { .. }
            | Self::Enum { .. } => Vec::new(),
            Self::Tuple { items } => items.to_vec(),
            Self::Record { fields } => fields.iter().map(FieldDef::type_id).collect(),
            Self::Sum { variants } => variants.iter().filter_map(SumVariantDef::payload).collect(),
            Self::Vector { element, .. } => vec![*element],
            Self::Map { key, value, .. } => vec![*key, *value],
        }
    }
}

/// One named type definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeDef {
    id: TypeId,
    name: SchemaName,
    kind: TypeKind,
}

impl TypeDef {
    /// Constructs and locally normalizes a type definition.
    pub fn try_new(
        id: TypeId,
        name: impl Into<Box<str>>,
        kind: TypeKind,
        limits: SchemaLimits,
    ) -> Result<Self, SchemaError> {
        Ok(Self {
            id,
            name: SchemaName::try_new(name)?,
            kind: kind.normalize(limits)?,
        })
    }

    /// Returns the stable type identifier.
    #[must_use]
    pub const fn id(&self) -> TypeId {
        self.id
    }

    /// Returns the source-facing type name.
    #[must_use]
    pub const fn name(&self) -> &SchemaName {
        &self.name
    }

    /// Returns the type grammar node.
    #[must_use]
    pub const fn kind(&self) -> &TypeKind {
        &self.kind
    }
}

/// Metrics fixed by a validated schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchemaMetrics {
    /// Number of type definitions.
    pub types: u32,
    /// Total record fields.
    pub fields: u32,
    /// Total enum and sum variants.
    pub variants: u32,
}

/// A closed, acyclic, canonically ordered protocol schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schema {
    profile: SchemaName,
    version: u16,
    root_type: TypeId,
    types: Box<[TypeDef]>,
    metrics: SchemaMetrics,
}

impl Schema {
    /// Constructs, normalizes, closes, and cycle-checks a schema.
    pub fn try_new(
        profile: impl Into<Box<str>>,
        version: u16,
        root_type: TypeId,
        mut types: Vec<TypeDef>,
        limits: SchemaLimits,
    ) -> Result<Self, SchemaError> {
        if types.is_empty() {
            return Err(SchemaError::EmptySchema);
        }
        if types.len() > usize::try_from(limits.max_types).unwrap_or(usize::MAX) {
            return Err(SchemaError::LimitExceeded);
        }
        types.sort_by_key(TypeDef::id);
        for pair in types.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(SchemaError::DuplicateTypeId(pair[0].id));
            }
        }
        for (index, item) in types.iter().enumerate() {
            if types[..index].iter().any(|prior| prior.name == item.name) {
                return Err(SchemaError::DuplicateTypeName);
            }
        }
        let metrics = compute_metrics(&types)?;
        let schema = Self {
            profile: SchemaName::try_new(profile)?,
            version,
            root_type,
            types: types.into_boxed_slice(),
            metrics,
        };
        if schema.type_by_id(root_type).is_none() {
            return Err(SchemaError::UnknownRootType(root_type));
        }
        schema.validate_closed_and_acyclic()?;
        Ok(schema)
    }

    /// Returns the profile identifier.
    #[must_use]
    pub const fn profile(&self) -> &SchemaName {
        &self.profile
    }

    /// Returns the schema version.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// Returns the declared root type.
    #[must_use]
    pub const fn root_type(&self) -> TypeId {
        self.root_type
    }

    /// Returns canonical type definitions.
    #[must_use]
    pub const fn types(&self) -> &[TypeDef] {
        &self.types
    }

    /// Returns construction-time metrics.
    #[must_use]
    pub const fn metrics(&self) -> SchemaMetrics {
        self.metrics
    }

    /// Resolves a type identifier.
    #[must_use]
    pub fn type_by_id(&self, id: TypeId) -> Option<&TypeDef> {
        self.types
            .binary_search_by_key(&id, TypeDef::id)
            .ok()
            .map(|index| &self.types[index])
    }

    fn validate_closed_and_acyclic(&self) -> Result<(), SchemaError> {
        let mut states = vec![0_u8; self.types.len()];
        for index in 0..self.types.len() {
            self.visit_type(index, &mut states)?;
        }
        Ok(())
    }

    fn visit_type(&self, index: usize, states: &mut [u8]) -> Result<(), SchemaError> {
        match states[index] {
            1 => return Err(SchemaError::RecursiveTypeCycle(self.types[index].id)),
            2 => return Ok(()),
            _ => {}
        }
        states[index] = 1;
        for reference in self.types[index].kind.references() {
            let child = self
                .types
                .binary_search_by_key(&reference, TypeDef::id)
                .map_err(|_| SchemaError::UnknownTypeReference(reference))?;
            self.visit_type(child, states)?;
        }
        states[index] = 2;
        Ok(())
    }
}

fn normalize_fields(
    mut fields: Vec<FieldDef>,
    limits: SchemaLimits,
) -> Result<Vec<FieldDef>, SchemaError> {
    if fields.len() > usize::try_from(limits.max_fields).unwrap_or(usize::MAX) {
        return Err(SchemaError::LimitExceeded);
    }
    fields.sort_by_key(FieldDef::id);
    for pair in fields.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(SchemaError::DuplicateFieldId(pair[0].id));
        }
    }
    for (index, item) in fields.iter().enumerate() {
        if fields[..index].iter().any(|prior| prior.name == item.name) {
            return Err(SchemaError::DuplicateFieldName);
        }
    }
    Ok(fields)
}

fn normalize_enum_variants(
    mut variants: Vec<EnumVariantDef>,
    limits: SchemaLimits,
) -> Result<Vec<EnumVariantDef>, SchemaError> {
    if variants.len() > usize::try_from(limits.max_variants).unwrap_or(usize::MAX) {
        return Err(SchemaError::LimitExceeded);
    }
    variants.sort_by_key(EnumVariantDef::id);
    for pair in variants.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(SchemaError::DuplicateVariantId(pair[0].id));
        }
    }
    for (index, item) in variants.iter().enumerate() {
        if variants[..index]
            .iter()
            .any(|prior| prior.name == item.name)
        {
            return Err(SchemaError::DuplicateVariantName);
        }
    }
    Ok(variants)
}

fn normalize_sum_variants(
    mut variants: Vec<SumVariantDef>,
    limits: SchemaLimits,
) -> Result<Vec<SumVariantDef>, SchemaError> {
    if variants.len() > usize::try_from(limits.max_variants).unwrap_or(usize::MAX) {
        return Err(SchemaError::LimitExceeded);
    }
    variants.sort_by_key(SumVariantDef::id);
    for pair in variants.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(SchemaError::DuplicateVariantId(pair[0].id));
        }
    }
    for (index, item) in variants.iter().enumerate() {
        if variants[..index]
            .iter()
            .any(|prior| prior.name == item.name)
        {
            return Err(SchemaError::DuplicateVariantName);
        }
    }
    Ok(variants)
}

fn compute_metrics(types: &[TypeDef]) -> Result<SchemaMetrics, SchemaError> {
    let mut fields = 0_usize;
    let mut variants = 0_usize;
    for item in types {
        match item.kind() {
            TypeKind::Record { fields: items } => fields = fields.saturating_add(items.len()),
            TypeKind::Enum { variants: items } => {
                variants = variants.saturating_add(items.len());
            }
            TypeKind::Sum { variants: items } => {
                variants = variants.saturating_add(items.len());
            }
            _ => {}
        }
    }
    Ok(SchemaMetrics {
        types: u32::try_from(types.len()).map_err(|_| SchemaError::LimitExceeded)?,
        fields: u32::try_from(fields).map_err(|_| SchemaError::LimitExceeded)?,
        variants: u32::try_from(variants).map_err(|_| SchemaError::LimitExceeded)?,
    })
}
