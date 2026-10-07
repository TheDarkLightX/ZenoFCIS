//! Library-owned V2 authority and genuine publication capabilities.
//!
//! Application-supplied native transition callbacks, raw reviewed pre-state
//! authoring views, usage claims and legacy authorization are retired. Complete
//! original reference algorithms remain in the private nonpublished oracle;
//! their preservation is not a checked full-domain replacement claim.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub use zeno_fcis_synthesis::finite::v2_authority::{
    Authority, Evaluation, Publication, PublicationOutcome, Refusal, WireDelivery, bind,
    policy_bytes,
};
