//! Private preparation oracle using the actual existing finite Program evaluator.
use alloc::vec::Vec;
use zeno_fcis_codec::{Domain, EncodeError, Hash32, commitment};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::SynthesisError;
use zeno_fcis_synthesis::finite::{Error, MAX_INPUTS, MAX_STEPS, Program};
use zeno_fcis_value::Value;
mod evaluation_adapter;
mod ir;
pub(crate) mod preparation;
use ir::tuple;

fn finite_tuple(values: &[i64]) -> Result<Value, zeno_fcis_codec::EncodeError> {
    tuple(
        values
            .iter()
            .map(|value| Value::signed(i128::from(*value)))
            .collect(),
    )
}

fn hash_canonical(
    domain: Domain<'static>,
    value: Result<Vec<u8>, EncodeError>,
) -> Result<Hash32, SynthesisError> {
    let bytes = value.map_err(SynthesisError::Encode)?;
    hash_bytes(domain, &bytes)
}

fn hash_bytes(domain: Domain<'static>, bytes: &[u8]) -> Result<Hash32, SynthesisError> {
    commitment::<RustCryptoSha256>(domain, bytes).map_err(SynthesisError::Encode)
}
