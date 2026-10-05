//! Versioned exact identity and persisted-subject framing. No authorization is issued.
use super::canonical::{Part, encode};
#[cfg(verus_keep_ghost)]
use super::spec as model;
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

pub use super::super::composition::Kind;

/// Exact contract plus exact evaluator bytes, with separate unambiguous fields.
/// The mandatory caller derives both from its immutable library-owned binding.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result{Some(v)=>Some(v@),None=>None::<Seq<u8>>})==model::identity(descriptor@,evaluator@),
))]
pub fn identity_bytes(descriptor: &[u8], evaluator: &[u8]) -> Option<Vec<u8>> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal_with_fuel(model::length,10);reveal_with_fuel(model::prefix,10);}
    let parts = alloc::vec![
        Part::Word(0x5a494432),
        Part::Word(1),
        Part::Bytes(descriptor),
        Part::Bytes(evaluator)
    ];
    encode(&parts)
}

/// Full persisted subject. An expected subject is untrusted input to comparison,
/// never a completed-core token or a replacement for recomputation.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    (match result{Some(v)=>Some(v@),None=>None::<Seq<u8>>})==model::subject(kind,identity@,state@,command@,context@,artifact@),
))]
pub fn subject_bytes(
    kind: Kind,
    identity: &[u8],
    state: &[u8],
    command: &[u8],
    context: &[u8],
    artifact: &[u8],
) -> Option<Vec<u8>> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal_with_fuel(model::length,10);reveal_with_fuel(model::prefix,10);reveal_with_fuel(model::blobs,6);}
    let parts = alloc::vec![
        Part::Word(0x5a525032),
        Part::Word(1),
        Part::Word(match kind {
            Kind::Genesis => 0,
            Kind::Transition => 1,
        }),
        Part::Bytes(identity),
        Part::Bytes(state),
        Part::Bytes(command),
        Part::Bytes(context),
        Part::Bytes(artifact)
    ];
    encode(&parts)
}
