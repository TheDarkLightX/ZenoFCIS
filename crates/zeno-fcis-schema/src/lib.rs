//! Closed, acyclic, canonically encoded protocol schemas for ZenoFCIS.
//!
//! A schema fixes stable type, field, and variant identifiers before runtime
//! values are admitted. It is immutable, bounded, content-addressable, and
//! independent of Rust layout or source declaration order.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod encoding;
mod envelope;
mod error;
mod ids;
mod model;
mod validate;

pub use envelope::{SchemaAdmittedEnvelope, SchemaAdmittedTypeEnvelope, SchemaEnvelopeError};
pub use error::{SchemaError, ValueValidationError};
pub use ids::{FieldId, SchemaName, TypeId, VariantId};
pub use model::{
    EnumVariantDef, FieldDef, Schema, SchemaLimits, SchemaMetrics, SumVariantDef, TypeDef, TypeKind,
};
pub use validate::{ValidationLimits, ValidationReport};

#[cfg(test)]
mod tests {
    use alloc::vec;

    use zeno_fcis_codec::{CanonicalEncode, Hash32};

    use super::*;

    struct TestHash;

    impl zeno_fcis_codec::CommitmentHasher for TestHash {
        const ALGORITHM_ID: &'static str = "test/noncryptographic";

        fn hash(bytes: &[u8]) -> Hash32 {
            let mut output = [0_u8; 32];
            for (index, byte) in bytes.iter().copied().enumerate() {
                let slot = index % output.len();
                output[slot] = output[slot]
                    .wrapping_add(byte)
                    .rotate_left((slot % 7) as u32);
            }
            Hash32::new(output)
        }
    }

    fn type_def(id: u32, name: &str) -> TypeDef {
        match TypeDef::try_new(
            TypeId::new(id),
            name,
            TypeKind::U128 { min: 0, max: 10 },
            SchemaLimits::default(),
        ) {
            Ok(value) => value,
            Err(error) => panic!("type rejected: {error}"),
        }
    }

    #[test]
    fn input_order_does_not_change_schema_bytes() {
        let limits = SchemaLimits::default();
        let left = Schema::try_new(
            "OrderIndependent",
            1,
            TypeId::new(1),
            vec![type_def(2, "Second"), type_def(1, "First")],
            limits,
        );
        let right = Schema::try_new(
            "OrderIndependent",
            1,
            TypeId::new(1),
            vec![type_def(1, "First"), type_def(2, "Second")],
            limits,
        );
        let (left, right) = match (left, right) {
            (Ok(left), Ok(right)) => (left, right),
            (left, right) => panic!("schemas rejected: {left:?} {right:?}"),
        };
        assert_eq!(left.canonical_bytes(), right.canonical_bytes());
        assert_eq!(
            left.schema_hash::<TestHash>(),
            right.schema_hash::<TestHash>()
        );
    }

    #[test]
    fn schema_hash_changes_with_version() {
        let limits = SchemaLimits::default();
        let first = Schema::try_new(
            "Versioned",
            1,
            TypeId::new(1),
            vec![type_def(1, "Value")],
            limits,
        );
        let second = Schema::try_new(
            "Versioned",
            2,
            TypeId::new(1),
            vec![type_def(1, "Value")],
            limits,
        );
        let (first, second) = match (first, second) {
            (Ok(first), Ok(second)) => (first, second),
            (first, second) => panic!("schemas rejected: {first:?} {second:?}"),
        };
        assert_ne!(
            first.schema_hash::<TestHash>(),
            second.schema_hash::<TestHash>()
        );
    }
}
