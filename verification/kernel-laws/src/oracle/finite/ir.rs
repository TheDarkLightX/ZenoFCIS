use alloc::vec::Vec;
use zeno_fcis_codec::EncodeError;
use zeno_fcis_synthesis::finite::Domain;
use zeno_fcis_value::Value;

pub(super) fn admitted(domains: &[Domain], values: &[i64]) -> bool {
    if domains.len() != values.len() {
        return false;
    }
    let mut index = 0usize;
    while index < domains.len() {
        if !domains[index].contains(values[index]) {
            return false;
        }
        index += 1;
    }
    true
}

pub(super) fn tuple(values: Vec<Value>) -> Result<Value, EncodeError> {
    Value::tuple(values).map_err(EncodeError::InvalidValue)
}
