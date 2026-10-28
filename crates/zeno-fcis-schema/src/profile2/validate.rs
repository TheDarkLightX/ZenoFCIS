//! Runtime validation of owned values against closed schemas.

use zeno_fcis_value::profile2::{Value, ValueRef};

use crate::{
    EnumVariantDef, SumVariantDef, TypeId, ValidationLimits, ValidationReport,
    ValueValidationError, VariantId,
};

use super::{Schema, TypeKind};

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
    match (definition.kind(), value.view()) {
        (TypeKind::Unit, ValueRef::Unit) | (TypeKind::Bool, ValueRef::Bool(_)) => Ok(()),
        (TypeKind::U128 { min, max }, ValueRef::U128(integer)) => {
            if *min <= integer && integer <= *max {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::I128 { min, max }, ValueRef::I128(integer)) => {
            if *min <= integer && integer <= *max {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::U256 { min, max }, ValueRef::U256(integer)) => {
            if !(*min).cmp(integer).is_gt() && !integer.cmp(*max).is_gt() {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::I256 { min, max }, ValueRef::I256(integer)) => {
            if !(*min).cmp(integer).is_gt() && !integer.cmp(*max).is_gt() {
                Ok(())
            } else {
                Err(ValueValidationError::IntegerRange)
            }
        }
        (TypeKind::Bytes { min_len, max_len }, ValueRef::Bytes(bytes)) => {
            validate_length(bytes.len(), *min_len, *max_len)
        }
        (TypeKind::Text { min_len, max_len }, ValueRef::Text(text)) => {
            validate_length(text.len(), *min_len, *max_len)
        }
        (TypeKind::Enum { variants }, ValueRef::Enum { type_id, variant }) => {
            if type_id != definition.id().get() {
                return Err(ValueValidationError::TypeIdentity);
            }
            let variant_id = VariantId::new(variant);
            find_enum_variant(variants, variant_id)
                .ok_or(ValueValidationError::UnknownVariant(variant_id))?;
            Ok(())
        }
        (TypeKind::Tuple { items: types }, ValueRef::Tuple(items)) => {
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
            ValueRef::Record(fields),
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
            ValueRef::Sum {
                type_id,
                variant,
                payload,
            },
        ) => {
            if type_id != definition.id().get() {
                return Err(ValueValidationError::TypeIdentity);
            }
            let variant_id = VariantId::new(variant);
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
            ValueRef::Vector(items),
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
            ValueRef::Map(entries),
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
