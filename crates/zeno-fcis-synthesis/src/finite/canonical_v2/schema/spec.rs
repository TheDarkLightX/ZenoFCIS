//! Exact schema wire grammar; no project callback supplies correspondence.
use super::super::spec::{read_signed, read_unsigned};
use super::{Definition, Description, Field, Kind, SumVariant, Variant};
use vstd::prelude::*;

verus! {
pub open spec fn word(bytes: Seq<u8>, offset: usize, width: usize, expected: u128) -> Option<usize> {
    match read_unsigned(bytes,offset,width) {
        Some((actual,end)) => if actual == expected { Some(end) } else { None },
        None => None,
    }
}
pub open spec fn signed(bytes: Seq<u8>, offset: usize, expected: i128) -> Option<usize> {
    match read_signed(bytes,offset) {
        Some((actual,end)) => if actual == expected { Some(end) } else { None },
        None => None,
    }
}
pub open spec fn blob(bytes: Seq<u8>, offset: usize, expected: Seq<u8>) -> Option<usize> {
    if offset <= bytes.len() && expected.len() <= bytes.len() - offset
        && (forall|i:int| 0 <= i < expected.len() ==> bytes[offset as int+i] == expected[i]) {
        Some((offset + expected.len()) as usize)
    } else { None }
}
pub open spec fn name(bytes: Seq<u8>, offset: usize, expected: Seq<u8>) -> Option<usize> {
    match word(bytes,offset,2,expected.len() as u128) {
        Some(next) => blob(bytes,next,expected), None => None,
    }
}
pub open spec fn variant(bytes: Seq<u8>, offset: usize, expected: Variant, sum: bool) -> Option<usize> {
    match word(bytes,offset,2,expected.id as u128) {
        Some(next) => match name(bytes,next,expected.name@) {
            Some(end) => if sum { word(bytes,end,1,0) } else { Some(end) }, None => None,
        }, None => None,
    }
}
pub open spec fn variant_sequence(bytes: Seq<u8>, offset: usize, expected: Seq<Variant>, count: nat, sum: bool) -> Option<usize>
    recommends count <= expected.len(),
    decreases count,
{
    if count == 0 { Some(offset) }
    else { match variant_sequence(bytes,offset,expected,(count-1) as nat,sum) {
        Some(next) => variant(bytes,next,expected[count as int-1],sum), None => None,
    } }
}
pub proof fn variant_failure(bytes: Seq<u8>, offset: usize, expected: Seq<Variant>, failed:nat,count:nat,sum:bool)
    requires failed <= count <= expected.len(),variant_sequence(bytes,offset,expected,failed,sum) == None,
    ensures variant_sequence(bytes,offset,expected,count,sum) == None,
    decreases count-failed,
{ if count > failed { variant_failure(bytes,offset,expected,failed,(count-1) as nat,sum); } }
pub open spec fn sum_variant(bytes:Seq<u8>,offset:usize,expected:SumVariant)->Option<usize> {
    match word(bytes,offset,2,expected.id as u128) {
        Some(next)=>match name(bytes,next,expected.name@) {
            Some(end)=>match expected.payload {
                None=>word(bytes,end,1,0),
                Some(id)=>match word(bytes,end,1,1){Some(p)=>word(bytes,p,4,id as u128),None=>None},
            },None=>None,
        },None=>None,
    }
}
pub open spec fn sum_variants(bytes:Seq<u8>,offset:usize,expected:Seq<SumVariant>,count:nat)->Option<usize>
    recommends count<=expected.len(),decreases count,
{if count==0{Some(offset)}else{match sum_variants(bytes,offset,expected,(count-1) as nat){
    Some(next)=>sum_variant(bytes,next,expected[count as int-1]),None=>None,
}}}
pub proof fn sum_variants_failure(bytes:Seq<u8>,offset:usize,expected:Seq<SumVariant>,failed:nat,count:nat)
    requires failed<=count<=expected.len(),sum_variants(bytes,offset,expected,failed)==None,
    ensures sum_variants(bytes,offset,expected,count)==None,decreases count-failed,
{if count>failed{sum_variants_failure(bytes,offset,expected,failed,(count-1) as nat);}}
pub open spec fn type_sequence(bytes:Seq<u8>,offset:usize,expected:Seq<u32>,count:nat)->Option<usize>
    recommends count<=expected.len(),decreases count,
{if count==0{Some(offset)}else{match type_sequence(bytes,offset,expected,(count-1) as nat){
    Some(next)=>word(bytes,next,4,expected[count as int-1] as u128),None=>None,
}}}
pub proof fn type_sequence_failure(bytes:Seq<u8>,offset:usize,expected:Seq<u32>,failed:nat,count:nat)
    requires failed<=count<=expected.len(),type_sequence(bytes,offset,expected,failed)==None,
    ensures type_sequence(bytes,offset,expected,count)==None,decreases count-failed,
{if count>failed{type_sequence_failure(bytes,offset,expected,failed,(count-1) as nat);}}
pub open spec fn field(bytes: Seq<u8>, offset: usize, expected: Field) -> Option<usize> {
    match word(bytes,offset,2,expected.id as u128) {
        Some(next) => match name(bytes,next,expected.name@) {
            Some(end) => word(bytes,end,4,expected.type_id as u128), None => None,
        }, None => None,
    }
}
pub open spec fn field_sequence(bytes: Seq<u8>, offset: usize, expected: Seq<Field>, count:nat) -> Option<usize>
    recommends count <= expected.len(),
    decreases count,
{
    if count == 0 { Some(offset) }
    else { match field_sequence(bytes,offset,expected,(count-1) as nat) {
        Some(next) => field(bytes,next,expected[count as int-1]), None => None,
    } }
}
pub proof fn field_failure(bytes: Seq<u8>, offset: usize, expected: Seq<Field>, failed:nat,count:nat)
    requires failed <= count <= expected.len(),field_sequence(bytes,offset,expected,failed) == None,
    ensures field_sequence(bytes,offset,expected,count) == None,
    decreases count-failed,
{ if count > failed { field_failure(bytes,offset,expected,failed,(count-1) as nat); } }
pub open spec fn kind(bytes:Seq<u8>, offset:usize, expected:Kind) -> Option<usize> {
    match expected {
        Kind::Unit => word(bytes,offset,1,0),
        Kind::Bool => word(bytes,offset,1,1),
        Kind::U128 { min,max } => match word(bytes,offset,1,2) {
            Some(a) => match word(bytes,a,16,min) { Some(b) => word(bytes,b,16,max),None=>None },None=>None,
        },
        Kind::I128 { min,max } => match word(bytes,offset,1,3) {
            Some(a) => match signed(bytes,a,min) { Some(b) => signed(bytes,b,max),None=>None },None=>None,
        },
        Kind::Bytes {min,max} | Kind::Text {min,max} => {
            let tag = match expected { Kind::Bytes {..} => 4u128,_=>5u128 };
            match word(bytes,offset,1,tag) {
                Some(a)=>match word(bytes,a,4,min as u128){Some(b)=>word(bytes,b,4,max as u128),None=>None},None=>None,
            }
        },
        Kind::Enum(variants) | Kind::Sum(variants) => {
            let sum = matches!(expected,Kind::Sum(_));
            match word(bytes,offset,1,if sum {9u128}else{6u128}) {
                Some(a)=>match word(bytes,a,4,variants@.len() as u128){
                    Some(b)=>variant_sequence(bytes,b,variants@,variants@.len(),sum),None=>None,
                },None=>None,
            }
        },
        Kind::Record(fields)=>match word(bytes,offset,1,8){
            Some(a)=>match word(bytes,a,4,fields@.len() as u128){
                Some(b)=>field_sequence(bytes,b,fields@,fields@.len()),None=>None,
            },None=>None,
        },
        Kind::Tuple(items)=>match word(bytes,offset,1,7){
            Some(a)=>match word(bytes,a,4,items@.len() as u128){
                Some(b)=>type_sequence(bytes,b,items@,items@.len()),None=>None,
            },None=>None,
        },
        Kind::SumPayload(variants)=>match word(bytes,offset,1,9){
            Some(a)=>match word(bytes,a,4,variants@.len() as u128){
                Some(b)=>sum_variants(bytes,b,variants@,variants@.len()),None=>None,
            },None=>None,
        },
        Kind::Vector{element,min,max}=>match word(bytes,offset,1,10){
            Some(a)=>match word(bytes,a,4,element as u128){
                Some(b)=>match word(bytes,b,4,min as u128){Some(c)=>word(bytes,c,4,max as u128),None=>None},None=>None,
            },None=>None,
        },
        Kind::Map{key,value,min,max}=>match word(bytes,offset,1,11){
            Some(a)=>match word(bytes,a,4,key as u128){Some(b)=>match word(bytes,b,4,value as u128){
                Some(c)=>match word(bytes,c,4,min as u128){Some(d)=>word(bytes,d,4,max as u128),None=>None},None=>None,
            },None=>None},None=>None,
        },
    }
}
pub open spec fn definition(bytes:Seq<u8>,offset:usize,expected:Definition)->Option<usize>{
    match word(bytes,offset,4,expected.id as u128){
        Some(a)=>match name(bytes,a,expected.name@){Some(b)=>kind(bytes,b,expected.kind),None=>None},None=>None,
    }
}
pub open spec fn definitions(bytes:Seq<u8>,offset:usize,expected:Seq<Definition>,count:nat)->Option<usize>
    recommends count <= expected.len(),
    decreases count,
{
    if count==0{Some(offset)}else{match definitions(bytes,offset,expected,(count-1) as nat){
        Some(a)=>definition(bytes,a,expected[count as int-1]),None=>None,
    }}
}
pub proof fn definitions_failure(bytes:Seq<u8>,offset:usize,expected:Seq<Definition>,failed:nat,count:nat)
    requires failed <= count <= expected.len(),definitions(bytes,offset,expected,failed)==None,
    ensures definitions(bytes,offset,expected,count)==None,
    decreases count-failed,
{if count>failed{definitions_failure(bytes,offset,expected,failed,(count-1) as nat);}}
pub open spec fn magic()->Seq<u8>{seq![90u8,70u8,67u8,73u8,83u8,83u8,67u8,72u8,69u8,77u8,65u8,49u8,0u8]}
pub open spec fn schema_end(bytes:Seq<u8>,description:Description)->Option<usize>{
    match blob(bytes,0,magic()) {
        Some(a)=>match name(bytes,a,description.profile@){
            Some(b)=>match word(bytes,b,2,description.version as u128){
                Some(c)=>match word(bytes,c,4,description.root as u128){
                    Some(d)=>match word(bytes,d,4,description.definitions@.len() as u128){
                        Some(e)=>definitions(bytes,e,description.definitions@,description.definitions@.len()),None=>None,
                    },None=>None,
                },None=>None,
            },None=>None,
        },None=>None,
    }
}
pub open spec fn encoding_matches(bytes:Seq<u8>,description:Description,max_bytes:u64)->bool{
    bytes.len() as u64 <= max_bytes && schema_end(bytes,description)==Some(bytes.len() as usize)
}
}
