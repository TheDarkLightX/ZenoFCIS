//! Pure internal complete policy serializer for checked catalog equality.
use super::super::composition::{Descriptor, Framing};
use super::{
    bound,
    canonical::{self, Part},
    descriptor,
};
#[cfg(verus_keep_ghost)]
use super::{policy_spec as model, spec as encoding};
use alloc::vec::Vec;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// Serialize declared policy for reviewed build artifacts. This pure function
/// performs no admission and grants no authority or guarantee of intended meaning.
#[cfg_attr(verus_keep_ghost,verus_spec(result=>ensures (match result{Some(v)=>Some(v@),None=>None::<Seq<u8>>})==super::policy_value(d,original_schema@,framing,channel_roots@),))]
pub fn policy_bytes(
    d: &Descriptor<'_>,
    original_schema: &[u8],
    framing: &Framing,
    channel_roots: &[(u32, u32, u32)],
) -> Option<Vec<u8>> {
    #[cfg(verus_keep_ghost)]
    proof! {reveal(super::policy_value);}
    let framed = bound::framing_bytes(framing)?;
    let mut parts = alloc::vec![
        Part::Word(0x5a504f32),
        Part::Word(1),
        Part::Bytes(original_schema),
        Part::Bytes(&framed),
        Part::Word(channel_roots.len() as u128)
    ];
    let mut i = 0usize;
    #[cfg_attr(verus_keep_ghost,verus_spec(invariant i<=channel_roots.len(),encoding::tokens(parts@)==
        seq![encoding::Token::Word(0x5a504f32),encoding::Token::Word(1),encoding::Token::Bytes(original_schema@),encoding::Token::Bytes(framed@),encoding::Token::Word(channel_roots.len() as u128)]+model::links_prefix(channel_roots@,i as nat),
        decreases channel_roots.len()-i,))]
    while i < channel_roots.len() {
        #[cfg(verus_keep_ghost)]
        proof_decl! {let ghost previous=parts@;}
        let v = channel_roots[i];
        parts.push(Part::Word(v.0 as u128));
        parts.push(Part::Word(v.1 as u128));
        parts.push(Part::Word(v.2 as u128));
        #[cfg(verus_keep_ghost)]
        proof! {
            let added=seq![Part::Word(v.0 as u128),Part::Word(v.1 as u128),Part::Word(v.2 as u128)];
            assert(parts@==previous+added);
            encoding::tokens_append(previous,added);
            assert(encoding::tokens(added)=~=seq![encoding::Token::Word(v.0 as u128),encoding::Token::Word(v.1 as u128),encoding::Token::Word(v.2 as u128)]);
        }
        i += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof_decl! {let ghost before=parts@;}
    descriptor::append_descriptor(&mut parts, d);
    #[cfg(verus_keep_ghost)]
    proof! {
        encoding::tokens_append(before,super::descriptor_spec::fields(d));
        assert(encoding::tokens(parts@)=~=seq![encoding::Token::Word(0x5a504f32),encoding::Token::Word(1),encoding::Token::Bytes(original_schema@),encoding::Token::Bytes(framed@)]
            +model::links(channel_roots@)+encoding::tokens(super::descriptor_spec::fields(d)));
        encoding::token_correspondence(parts@);
        assert(encoding::encode(parts@)==model::policy(d,original_schema@,framing,channel_roots@));
    }
    canonical::encode(&parts)
}
