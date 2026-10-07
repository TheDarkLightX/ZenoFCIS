//! Concrete library memory-delivery interpreter and its source identity.
//!
//! The identity binds this exact source and the sealed SHA-256 provider. It is
//! source provenance under the compiler/platform assumptions, not binary attestation.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use zeno_fcis_codec::{CommitmentHasher, EncodeError, Hash32, RustCryptoSha256, commitment};
use zeno_fcis_plan::OutboxEntry;

/// Idempotent destination boundary: the contract a delivery adapter refines.
///
/// It lives here, in the pure reference model; `zeno-fcis-shell-sqlite`
/// re-exports it under its previous path.
pub trait IdempotentDestination {
    /// Destination-specific failure type.
    type Error: fmt::Display;

    /// Delivers once by identity and returns the observed exact entry hash.
    fn deliver(
        &mut self,
        delivery_id: Hash32,
        entry_hash: Hash32,
        entry: &OutboxEntry,
    ) -> Result<Hash32, Self::Error>;
}

/// Deterministic destination stub that rejects identity/content collisions.
///
/// It performs no I/O and needs only `alloc`, so it lives here;
/// `zeno-fcis-shell-sqlite` re-exports it under its previous path.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryDestination {
    delivered: BTreeMap<Hash32, Hash32>,
}

impl MemoryDestination {
    /// Library-computed identity of the actual concrete interpreter source.
    /// Callers cannot substitute an implementation or a self-reported identity.
    pub fn interpreter_identity() -> Result<Hash32, EncodeError> {
        let source = include_bytes!("delivery.rs");
        let provider = RustCryptoSha256::ALGORITHM_ID.as_bytes();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            &u64::try_from(provider.len())
                .map_err(|_| EncodeError::LengthOverflow)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(provider);
        bytes.extend_from_slice(
            &u64::try_from(source.len())
                .map_err(|_| EncodeError::LengthOverflow)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(source);
        commitment::<RustCryptoSha256>(zeno_fcis_codec::domains::DELIVERY_INTERPRETER, &bytes)
    }

    /// Returns the exact number of distinct delivered identities.
    #[must_use]
    pub fn delivered_count(&self) -> usize {
        self.delivered.len()
    }
}

/// Memory-destination collision failure, defined here and re-exported by
/// `zeno-fcis-shell-sqlite`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryCollision;

impl fmt::Display for DeliveryCollision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("delivery identity already binds different entry content")
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DeliveryCollision {}

impl IdempotentDestination for MemoryDestination {
    type Error = DeliveryCollision;

    fn deliver(
        &mut self,
        delivery_id: Hash32,
        entry_hash: Hash32,
        _: &OutboxEntry,
    ) -> Result<Hash32, Self::Error> {
        match self.delivered.get(&delivery_id) {
            Some(existing) if *existing != entry_hash => Err(DeliveryCollision),
            Some(existing) => Ok(*existing),
            None => {
                self.delivered.insert(delivery_id, entry_hash);
                Ok(entry_hash)
            }
        }
    }
}
