//! Runtime validation of owned values against closed schemas.

use zeno_fcis_value::Value;

use crate::{
    EnumVariantDef, Schema, SumVariantDef, TypeId, TypeKind, ValueValidationError, VariantId,
};

/// Deterministic resource limits for one schema/value validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationLimits {
    /// Maximum recursive value depth, with the root at depth zero.
    pub max_depth: u16,
    /// Maximum visited value nodes.
    pub max_nodes: u64,
}

impl Default for ValidationLimits {
    fn default() -> Self {
        Self {
            max_depth: 128,
            max_nodes: 1_000_000,
        }
    }
}

/// Deterministic resources consumed by successful validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    /// Number of visited values.
    pub nodes: u64,
    /// Maximum observed depth.
    pub maximum_depth: u16,
}

struct ValidationState {
    limits: ValidationLimits,
    nodes: u64,
    maximum_depth: u16,
}

impl ValidationState {
    fn enter(&mut self, depth: u16) -> Result<(), ValueValidationError> {
        if depth > self.limits.max_depth {
            return Err(ValueValidationError::BudgetExceeded);
        }
        self.nodes = self
            .nodes
            .checked_add(1)
            .ok_or(ValueValidationError::BudgetExceeded)?;
        if self.nodes > self.limits.max_nodes {
            return Err(ValueValidationError::BudgetExceeded);
        }
        self.maximum_depth = self.maximum_depth.max(depth);
        Ok(())
    }
}

impl Schema {
    /// Validates an owned value against one schema type.
    pub fn validate_value(
        &self,
        type_id: TypeId,
        value: &Value,
        limits: ValidationLimits,
    ) -> Result<ValidationReport, ValueValidationError> {
        let mut state = ValidationState {
            limits,
            nodes: 0,
            maximum_depth: 0,
        };
        validate_node(self, type_id, value, 0, &mut state)?;
        Ok(ValidationReport {
            nodes: state.nodes,
            maximum_depth: state.maximum_depth,
        })
    }

    /// Validates an owned value against the declared root type.
    pub fn validate_root(
        &self,
        value: &Value,
        limits: ValidationLimits,
    ) -> Result<ValidationReport, ValueValidationError> {
        self.validate_value(self.root_type(), value, limits)
    }
}

fn validate_node(
    schema: &Schema,
    type_id: TypeId,
    value: &Value,
    depth: u16,
    state: &mut ValidationState,
) -> Result<(), ValueValidationError> {
    state.enter(depth)?;
    let definition = schema
        .type_by_id(type_id)
        .ok_or(ValueValidationError::UnknownType(type_id))?;
    match (definition.kind(), value) {
        (TypeKind::Unit, Value::Unit) | (TypeKind::Bool, Value::Bool(_)) => Ok(()),
        (TypeKind::U128 { min, max }, Value::U128(integer)) => {
            if min <= integer && integer <= max {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::I128 { min, max }, Value::I128(integer)) => {
            if min <= integer && integer <= max {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::Bytes { min_len, max_len }, Value::Bytes(bytes)) => {
            validate_length(bytes.len(), *min_len, *max_len)
        }
        (TypeKind::Text { min_len, max_len }, Value::Text(text)) => {
            validate_length(text.as_bytes().len(), *min_len, *max_len)
        }
        (TypeKind::Enum { variants }, Value::Enum { type_id, variant }) => {
            if *type_id != definition.id().get() {
                return Err(ValueValidationError::TypeIdentity);
            }
            let variant_id = VariantId::new(*variant);
            find_enum_variant(variants, variant_id)
                .ok_or(ValueValidationError::UnknownVariant(variant_id))?;
            Ok(())
        }
        (TypeKind::Tuple { items: types }, Value::Tuple(items)) => {
            if types.len() != items.len() {
                return Err(ValueValidationError::Length);
            }
            let next = next_depth(depth)?;
            for (child_type, child) in types.iter().zip(items.iter()) {
                validate_node(schema, *child_type, child, next, state)?;
            }
            Ok(())
        }
        (
            TypeKind::Record {
                fields: definitions,
            },
            Value::Record(fields),
        ) => {
            if definitions.len() != fields.len() {
                return Err(ValueValidationError::RecordShape);
            }
            let next = next_depth(depth)?;
            for (definition, field) in definitions.iter().zip(fields.iter()) {
                if definition.id().get() != field.id() {
                    return Err(ValueValidationError::RecordShape);
                }
                validate_node(schema, definition.type_id(), field.value(), next, state)?;
            }
            Ok(())
        }
        (
            TypeKind::Sum { variants },
            Value::Sum {
                type_id,
                variant,
                payload,
            },
        ) => {
            if *type_id != definition.id().get() {
                return Err(ValueValidationError::TypeIdentity);
            }
            let variant_id = VariantId::new(*variant);
            let variant = find_sum_variant(variants, variant_id)
                .ok_or(ValueValidationError::UnknownVariant(variant_id))?;
            match (variant.payload(), payload) {
                (None, None) => Ok(()),
                (None, Some(_)) => Err(ValueValidationError::UnexpectedPayload),
                (Some(_), None) => Err(ValueValidationError::MissingPayload),
                (Some(child_type), Some(child)) => {
                    validate_node(schema, child_type, child, next_depth(depth)?, state)
                }
            }
        }
        (
            TypeKind::Vector {
                element,
                min_len,
                max_len,
            },
            Value::Vector(items),
        ) => {
            validate_length(items.len(), *min_len, *max_len)?;
            let next = next_depth(depth)?;
            for item in items {
                validate_node(schema, *element, item, next, state)?;
            }
            Ok(())
        }
        (
            TypeKind::Map {
                key,
                value: value_type,
                min_len,
                max_len,
            },
            Value::Map(entries),
        ) => {
            validate_length(entries.len(), *min_len, *max_len)?;
            let next = next_depth(depth)?;
            for entry in entries {
                validate_node(schema, *key, entry.key(), next, state)?;
                validate_node(schema, *value_type, entry.value(), next, state)?;
            }
            Ok(())
        }
        _ => Err(ValueValidationError::TypeMismatch),
    }
}

fn validate_length(length: usize, minimum: u32, maximum: u32) -> Result<(), ValueValidationError> {
    let length = u32::try_from(length).map_err(|_| ValueValidationError::Length)?;
    if minimum <= length && length <= maximum {
        Ok(())
    } else {
        Err(ValueValidationError::Length)
    }
}

fn find_enum_variant(variants: &[EnumVariantDef], id: VariantId) -> Option<&EnumVariantDef> {
    variants
        .binary_search_by_key(&id, EnumVariantDef::id)
        .ok()
        .map(|index| &variants[index])
}

fn find_sum_variant(variants: &[SumVariantDef], id: VariantId) -> Option<&SumVariantDef> {
    variants
        .binary_search_by_key(&id, SumVariantDef::id)
        .ok()
        .map(|index| &variants[index])
}

fn next_depth(depth: u16) -> Result<u16, ValueValidationError> {
    depth
        .checked_add(1)
        .ok_or(ValueValidationError::BudgetExceeded)
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;
    use alloc::vec;

    use super::*;
    use crate::{FieldDef, SchemaLimits, TypeDef};

    fn scalar_schema() -> Schema {
        let limits = SchemaLimits::default();
        let amount = TypeDef::try_new(
            TypeId::new(1),
            "Amount",
            TypeKind::U128 { min: 1, max: 100 },
            limits,
        );
        match amount {
            Ok(amount) => {
                match Schema::try_new("ScalarProfile", 1, TypeId::new(1), vec![amount], limits) {
                    Ok(schema) => schema,
                    Err(error) => panic!("schema rejected: {error}"),
                }
            }
            Err(error) => panic!("type rejected: {error}"),
        }
    }

    #[test]
    fn validates_bounded_scalar() {
        let schema = scalar_schema();
        assert!(
            schema
                .validate_root(&Value::U128(42), ValidationLimits::default())
                .is_ok()
        );
        assert_eq!(
            schema.validate_root(&Value::U128(0), ValidationLimits::default()),
            Err(ValueValidationError::IntegerRange)
        );
    }

    #[test]
    fn rejects_recursive_type_graph() {
        let limits = SchemaLimits::default();
        let field = FieldDef::try_new(FieldId::new(1), "child", TypeId::new(1));
        let field = match field {
            Ok(field) => field,
            Err(error) => panic!("field rejected: {error}"),
        };
        let node = TypeDef::try_new(
            TypeId::new(1),
            "Node",
            TypeKind::Record {
                fields: vec![field].into_boxed_slice(),
            },
            limits,
        );
        let node = match node {
            Ok(node) => node,
            Err(error) => panic!("type rejected: {error}"),
        };
        assert_eq!(
            Schema::try_new("Recursive", 1, TypeId::new(1), vec![node], limits),
            Err(SchemaError::RecursiveTypeCycle(TypeId::new(1)))
        );
    }

    #[test]
    fn tuple_budget_is_deterministic() {
        let limits = SchemaLimits::default();
        let scalar = match TypeDef::try_new(TypeId::new(1), "Bit", TypeKind::Bool, limits) {
            Ok(value) => value,
            Err(error) => panic!("type rejected: {error}"),
        };
        let pair = match TypeDef::try_new(
            TypeId::new(2),
            "Pair",
            TypeKind::Tuple {
                items: vec![TypeId::new(1), TypeId::new(1)].into_boxed_slice(),
            },
            limits,
        ) {
            Ok(value) => value,
            Err(error) => panic!("type rejected: {error}"),
        };
        let schema =
            match Schema::try_new("PairProfile", 1, TypeId::new(2), vec![pair, scalar], limits) {
                Ok(value) => value,
                Err(error) => panic!("schema rejected: {error}"),
            };
        let value = Value::Tuple(Box::new([Value::Bool(true), Value::Bool(false)]));
        let report = schema.validate_root(
            &value,
            ValidationLimits {
                max_depth: 2,
                max_nodes: 3,
            },
        );
        assert_eq!(
            report,
            Ok(ValidationReport {
                nodes: 3,
                maximum_depth: 1,
            })
        );
    }
}
