//! Noncircular complete reviewed policy, computed only from actual bound fields.
use super::super::composition::{Descriptor, Framing};
use super::{
    bound_spec, descriptor_spec,
    spec::{self, Token},
};
use vstd::prelude::*;
verus! {
pub open spec fn links_prefix(values:Seq<(u32,u32,u32)>,n:nat)->Seq<Token>
    recommends n<=values.len(),decreases n,
{if n==0{Seq::empty()}else{let v=values[n-1];links_prefix(values,(n-1) as nat)+seq![Token::Word(v.0 as u128),Token::Word(v.1 as u128),Token::Word(v.2 as u128)]}}
pub open spec fn links(values:Seq<(u32,u32,u32)>)->Seq<Token>{seq![Token::Word(values.len() as u128)]+links_prefix(values,values.len())}
pub open spec fn policy(d:&Descriptor,schema:Seq<u8>,framing:&Framing,channels:Seq<(u32,u32,u32)>)->Option<Seq<u8>>{
    spec::token_encode(seq![Token::Word(0x5a504f32),Token::Word(1),Token::Bytes(schema),Token::Bytes(bound_spec::framing_bytes(framing))]+
        links(channels)+spec::tokens(descriptor_spec::fields(d)))
}
}
