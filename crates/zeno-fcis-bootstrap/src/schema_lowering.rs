//! Checked lowering of authored record and sum shapes into the closed schema.

use std::collections::BTreeMap;
use std::fmt;

use zeno_fcis_schema::{
    FieldDef, FieldId, Schema, SchemaError, SchemaLimits, SumVariantDef, TypeDef, TypeId, TypeKind,
    VariantId,
};
use zeno_fcis_spec::{ProjectSpec, StableId, TypeKind as AuthoredKind};

/// A source declaration cannot be represented by the requested exact schema.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaLoweringError {
    /// The selected root is absent or is not declared as state.
    WrongRoot(StableId),
    /// An authored leaf has no explicit shape and bounds.
    MissingLeafBinding(StableId),
    /// A leaf binding does not correspond to a declaration.
    UnusedLeafBinding(StableId),
    /// A leaf binding was supplied more than once.
    DuplicateLeafBinding(StableId),
    /// A binding attempts to replace an authored record or sum.
    AuthoredShapeOverride(StableId),
    /// One declaration has both fields and variants.
    AmbiguousShape(StableId),
    /// A Boolean or signed-integer declaration has incompatible structure.
    IncompatiblePrimitive(StableId),
    /// A leaf binding supplies a compound shape instead of a scalar.
    UnsupportedLeafShape(StableId),
    /// A stable field or variant ID exceeds the target schema's u16 width.
    IdOutOfRange(StableId),
    /// The existing schema validator rejected the exact lowered definition.
    Schema(SchemaError),
}

impl From<SchemaError> for SchemaLoweringError {
    fn from(error: SchemaError) -> Self {
        Self::Schema(error)
    }
}

impl fmt::Display for SchemaLoweringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongRoot(id) => {
                write!(formatter, "type {} is not a declared state root", id.get())
            }
            Self::MissingLeafBinding(id) => write!(
                formatter,
                "type {} needs an explicit leaf binding",
                id.get()
            ),
            Self::UnusedLeafBinding(id) => {
                write!(formatter, "leaf binding {} has no declaration", id.get())
            }
            Self::DuplicateLeafBinding(id) => {
                write!(formatter, "duplicate leaf binding {}", id.get())
            }
            Self::AuthoredShapeOverride(id) => write!(
                formatter,
                "leaf binding {} overrides authored structure",
                id.get()
            ),
            Self::AmbiguousShape(id) => {
                write!(formatter, "type {} has both fields and variants", id.get())
            }
            Self::IncompatiblePrimitive(id) => write!(
                formatter,
                "type {} changes its authored primitive kind",
                id.get()
            ),
            Self::UnsupportedLeafShape(id) => {
                write!(formatter, "binding {} is not a scalar leaf", id.get())
            }
            Self::IdOutOfRange(id) => {
                write!(formatter, "stable ID {} exceeds schema u16 width", id.get())
            }
            Self::Schema(error) => write!(formatter, "lowered schema rejected: {error}"),
        }
    }
}

impl std::error::Error for SchemaLoweringError {}

/// Lowers every authored type into a closed schema without choosing leaf semantics.
///
/// Fields and variants preserve their authored IDs, names, and referenced types.
/// Every type without fields or variants needs exactly one scalar leaf binding.
/// `bool` requires `Bool`; `int` requires a bounded `I128`. Other roles permit an
/// explicitly selected scalar. Compound shapes must be declared in the source.
/// The schema profile is the exact project name, and incompatible names or IDs
/// are rejected without normalization. All existing schema validation applies.
///
/// This function lowers shape only. Components, laws, catalog meaning, machine
/// implementations, and authorization require their separate checked bindings.
pub fn lower_schema(
    project: &ProjectSpec,
    root: StableId,
    version: u16,
    leaf_bindings: Vec<(StableId, TypeKind)>,
    limits: SchemaLimits,
) -> Result<Schema, SchemaLoweringError> {
    let max_types = usize::try_from(limits.max_types).unwrap_or(usize::MAX);
    if project.types().len() > max_types || leaf_bindings.len() > max_types {
        return Err(SchemaError::LimitExceeded.into());
    }
    if !project
        .types()
        .iter()
        .any(|ty| ty.id() == root && ty.kind() == AuthoredKind::State)
    {
        return Err(SchemaLoweringError::WrongRoot(root));
    }
    let mut leaves = BTreeMap::new();
    for (id, kind) in leaf_bindings {
        if leaves.insert(id, kind).is_some() {
            return Err(SchemaLoweringError::DuplicateLeafBinding(id));
        }
    }
    // Group once: lowering is not a full scan of all fields for every type.
    let mut fields = BTreeMap::<_, Vec<_>>::new();
    for field in project.fields() {
        let row = fields.entry(field.owner()).or_default();
        if row.len() >= usize::try_from(limits.max_fields).unwrap_or(usize::MAX) {
            return Err(SchemaError::LimitExceeded.into());
        }
        let raw = u16::try_from(field.id().get())
            .map_err(|_| SchemaLoweringError::IdOutOfRange(field.id()))?;
        row.push(FieldDef::try_new(
            FieldId::new(raw),
            field.name().as_str(),
            TypeId::new(field.field_type().get()),
        )?);
    }
    let mut variants = BTreeMap::<_, Vec<_>>::new();
    for variant in project.variants() {
        let row = variants.entry(variant.owner()).or_default();
        if row.len() >= usize::try_from(limits.max_variants).unwrap_or(usize::MAX) {
            return Err(SchemaError::LimitExceeded.into());
        }
        let raw = u16::try_from(variant.id().get())
            .map_err(|_| SchemaLoweringError::IdOutOfRange(variant.id()))?;
        row.push(SumVariantDef::try_new(
            VariantId::new(raw),
            variant.name().as_str(),
            variant.payload_type().map(|id| TypeId::new(id.get())),
        )?);
    }
    let mut types = Vec::with_capacity(project.types().len());
    for ty in project.types() {
        let id = ty.id();
        let record = fields.remove(&id);
        let sum = variants.remove(&id);
        let leaf = leaves.remove(&id);
        let kind = match (record, sum, leaf) {
            (Some(_), Some(_), _) => return Err(SchemaLoweringError::AmbiguousShape(id)),
            (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
                return Err(SchemaLoweringError::AuthoredShapeOverride(id));
            }
            (Some(fields), None, None) => TypeKind::Record {
                fields: fields.into_boxed_slice(),
            },
            (None, Some(variants), None) => TypeKind::Sum {
                variants: variants.into_boxed_slice(),
            },
            (None, None, None) => return Err(SchemaLoweringError::MissingLeafBinding(id)),
            (None, None, Some(kind)) => match kind {
                TypeKind::Unit
                | TypeKind::Bool
                | TypeKind::U128 { .. }
                | TypeKind::I128 { .. }
                | TypeKind::Text { .. }
                | TypeKind::Bytes { .. } => kind,
                _ => return Err(SchemaLoweringError::UnsupportedLeafShape(id)),
            },
        };
        if matches!(ty.kind(), AuthoredKind::Bool) && !matches!(kind, TypeKind::Bool)
            || matches!(ty.kind(), AuthoredKind::Int) && !matches!(kind, TypeKind::I128 { .. })
        {
            return Err(SchemaLoweringError::IncompatiblePrimitive(id));
        }
        types.push(TypeDef::try_new(
            TypeId::new(id.get()),
            ty.name().as_str(),
            kind,
            limits,
        )?);
    }
    if let Some((id, _)) = leaves.first_key_value() {
        return Err(SchemaLoweringError::UnusedLeafBinding(*id));
    }
    Ok(Schema::try_new(
        project.name().as_str(),
        version,
        TypeId::new(root.get()),
        types,
        limits,
    )?)
}
