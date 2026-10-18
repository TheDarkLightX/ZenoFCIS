//! The contract compiler: `project.zeno` declarations and `v2/policy.json`
//! rules become the original schema, the program definition and the policy
//! bytes the library binds, and the review's input domains and evaluation.
//!
//! This module mirrors the command line tool's former `contract` module, so
//! the tool keeps using it under its old paths. It is shared implementation
//! detail with no stability promise; the supported API is the crate root.

pub mod declarations;
pub mod delivery;
pub mod expr;
pub mod graph;
pub mod model;
pub mod policy;
pub mod quantity;
pub mod review;
pub mod rules;
pub mod schema;

pub use crate::ContractError;

use zeno_fcis_codec::{commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;

/// The schema commitment envelopes carry: the domain-separated SHA-256 of
/// the original schema bytes.
pub fn schema_commitment(schema: &[u8]) -> Result<[u8; 32], ContractError> {
    commitment::<RustCryptoSha256>(domains::SCHEMA, schema)
        .map(|hash| *hash.as_bytes())
        .map_err(|_| ContractError::new("v2/schema.zcve", "is too large to commit to"))
}

/// The canonical bytes of an admitted program: what receipts name.
pub fn program_bytes(
    program: &zeno_fcis_synthesis::finite::Program,
) -> Result<Vec<u8>, ContractError> {
    use zeno_fcis_codec::CanonicalEncode;
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .map_err(|error| {
            ContractError::new(
                "v2/policy.json",
                format!("the decision program has no canonical encoding: {error:?}"),
            )
        })
}
