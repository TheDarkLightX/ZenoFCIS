//! Owned typed-state projections only. This module is not a raw ingress decoder.

use super::types::{Hash, Observation, Source};
use super::{Failure, Meter, Resource, step};
use alloc::{vec, vec::Vec};
use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_value::{Field, Value};

pub(super) fn record(values: Vec<Value>) -> Result<Value, Failure> {
    let fields = values
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let id = u16::try_from(i + 1).map_err(|_| Failure::Canonical)?;
            Ok(Field::new(id, v))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Value::record_canonical(fields).map_err(|_| Failure::Canonical)
}
pub(super) fn blob(bytes: &[u8]) -> Result<Value, Failure> {
    Value::bytes(bytes.to_vec()).map_err(|_| Failure::Canonical)
}
pub(super) fn integers(xs: &[u128]) -> Result<Value, Failure> {
    Value::tuple(xs.iter().copied().map(Value::unsigned).collect()).map_err(|_| Failure::Canonical)
}
pub(super) fn bytes(value: &Value) -> Result<Vec<u8>, Failure> {
    value.canonical_bytes().map_err(|_| Failure::Canonical)
}
pub(super) fn hash(domain: &[u8], value: &Value) -> Result<Hash, Failure> {
    Ok(*RustCryptoSha256::hash_parts(&[domain, &bytes(value)?]).as_bytes())
}
pub(super) fn observation(o: &Observation) -> Result<Value, Failure> {
    record(vec![
        blob(&o.id)?,
        blob(&o.admission)?,
        blob(o.reporter.bytes())?,
        blob(o.source.bytes())?,
        blob(&o.feed)?,
        blob(&o.profile)?,
        blob(&o.query)?,
        Value::unsigned(u128::from(o.round)),
        Value::unsigned(o.value.raw()),
        Value::unsigned(u128::from(o.epoch)),
        Value::unsigned(u128::from(o.seconds)),
    ])
}
pub(super) fn source(s: &Source) -> Result<Value, Failure> {
    record(vec![
        blob(s.id.bytes())?,
        blob(s.operator.bytes())?,
        blob(s.venue.bytes())?,
        blob(s.family.bytes())?,
        blob(s.transport.bytes())?,
        blob(s.jurisdiction.bytes())?,
    ])
}
pub(super) fn visit(m: &mut Meter) -> Result<(), Failure> {
    step(m)?;
    m.charge(Resource::Read, 1)?;
    Ok(())
}
/// Data-independent allowance for native scratch copies and bounded equality/
/// membership passes. Direct visits and canonical passes are charged as well.
/// This oracle tariff is logical work, not elapsed time or allocator memory.
pub(super) fn reserve_scans(rows: usize, m: &mut Meter) -> Result<(), Failure> {
    let n = u64::try_from(rows)
        .map_err(|_| Failure::Arithmetic)?
        .checked_add(4)
        .ok_or(Failure::Arithmetic)?;
    let scans = n
        .checked_mul(n)
        .and_then(|x| x.checked_mul(4))
        .ok_or(Failure::Arithmetic)?;
    m.charge(Resource::Step, scans)?;
    m.charge(Resource::Read, scans)?;
    m.charge(
        Resource::Byte,
        n.checked_mul(4096).ok_or(Failure::Arithmetic)?,
    )?;
    Ok(())
}
pub(super) fn charge(value: &Value, m: &mut Meter) -> Result<u64, Failure> {
    step(m)?;
    let length = u64::try_from(value.encoded_length().map_err(|_| Failure::Canonical)?)
        .map_err(|_| Failure::Arithmetic)?;
    m.charge(Resource::Byte, length)?;
    Ok(length)
}
