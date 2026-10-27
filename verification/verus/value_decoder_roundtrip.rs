//! Encoder and decoder theorems at the production call boundary.
use crate::codec::{self, DecodeError, DecodeLimits, EncodeError};
use crate::native_value::decoder_model::*;
use crate::native_value::{Value, ValueLimits};
use alloc::vec::Vec;
use vstd::prelude::*;

verus! {

/// T4 for the actual append API. The complete output must fit its native index type.
pub fn t4_encoder_append(value: &Value, output: &mut Vec<u8>, limits: ValueLimits)
    -> (result: Result<(), EncodeError>)
    requires
        value.representation_valid(),
        wf(value.model(), limits.model(0)),
        old(output)@.len() + spec_encode(value.model()).len() <= usize::MAX,
    ensures
        result.is_ok(),
        final(output)@ == old(output)@ + spec_encode(value.model()),
{
    codec::encode_value_with_limits(value, output, limits)
}

/// T4 with an empty output buffer: successful production encoding is exactly spec_encode.
pub fn t4_encoder_agreement(value: &Value, limits: ValueLimits) -> (bytes: Vec<u8>)
    requires
        value.representation_valid(),
        wf(value.model(), limits.model(0)),
        spec_encode(value.model()).len() <= usize::MAX,
    ensures bytes@ == spec_encode(value.model()),
{
    let mut bytes = Vec::new();
    let result = t4_encoder_append(value, &mut bytes, limits);
    assert(result.is_ok());
    bytes
}

/// T5 carries abstract completeness through the unchanged production correspondence.
pub fn t5_decoder_complete(bytes: &[u8], limits: DecodeLimits, Ghost(value): Ghost<ModelValue>)
    -> (result: Result<Value, DecodeError>)
    requires
        wf(value, limits.model()),
        bytes@ == spec_encode(value),
        spec_encode(value).len() <= limits.max_input_bytes,
    ensures result matches Ok(decoded) && decoded.model() == value && decoded.representation_valid(),
{
    proof! { crate::completeness::spec_t5_complete(value, limits.model()); }
    codec::decode_value(bytes, limits)
}

/// The encoder and decoder compose under their explicit respective bounds.
pub fn production_roundtrip(value: &Value, limits: DecodeLimits) -> (result: Result<Value, DecodeError>)
    requires
        value.representation_valid(),
        wf(value.model(), limits.model()),
        spec_encode(value.model()).len() <= usize::MAX,
        spec_encode(value.model()).len() <= limits.max_input_bytes,
    ensures result matches Ok(decoded) && decoded.model() == value.model() && decoded.representation_valid(),
{
    let bytes = t4_encoder_agreement(value, limits.value);
    t5_decoder_complete(&bytes, limits, Ghost(value.model()))
}

}
