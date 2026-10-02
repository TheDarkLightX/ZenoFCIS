//! Bounded import of canonical finite programs for direct runtime evaluation.
//!
//! Import validates the closed IR again; it does not itself grant application
//! authority or upgrade the synthesis certificate.

use crate::SynthesisError;
use crate::finite::{Domain, Error, Op, PROFILE, Program};
use alloc::vec::Vec;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Hash32, decode_value};
use zeno_fcis_value::{Value, ValueLimits};

/// Binds a mounted runtime to the exact importer and finite evaluator source.
pub fn evaluator_hash() -> Result<Hash32, SynthesisError> {
    let mut source = Vec::new();
    source.extend_from_slice(include_bytes!("finite/ir.rs"));
    source.extend_from_slice(include_bytes!("finite/evaluation/mod.rs"));
    source.extend_from_slice(include_bytes!("finite/evaluation/spec.rs"));
    source.extend_from_slice(include_bytes!("finite/evaluation/admission/mod.rs"));
    source.extend_from_slice(include_bytes!("finite/evaluation/admission/spec.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/mod.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/meter.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/spec.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/input_view.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/input_view/spec.rs"));
    source.extend_from_slice(include_bytes!("finite/execution_v2/record_execution.rs"));
    source.extend_from_slice(include_bytes!(
        "finite/execution_v2/record_execution/spec.rs"
    ));
    source.extend_from_slice(include_bytes!("finite/canonical_v2/mod.rs"));
    source.extend_from_slice(include_bytes!("finite/canonical_v2/spec.rs"));
    source.extend_from_slice(crate::finite::V2_EXECUTION_PROFILE.as_bytes());
    source.extend_from_slice(crate::finite::V2_RECORD_PROFILE.as_bytes());
    source.extend_from_slice(crate::finite::V2_RECORD_EXECUTION_PROFILE.as_bytes());
    source.extend_from_slice(include_bytes!("finite_runtime.rs"));
    crate::hash_bytes("zeno-fcis/finite-runtime-source", &source)
}

/// Imports a bounded canonical finite program and rechecks its complete
/// type, topology, and result shape before it can be evaluated.
pub fn import_program(bytes: &[u8]) -> Result<Program, Error> {
    let invalid = || Error::Invalid("program-encoding");
    let value = decode_value(
        bytes,
        DecodeLimits {
            max_input_bytes: 64 * 1024,
            value: ValueLimits {
                max_depth: 4,
                max_nodes: 2048,
                max_payload_bytes: 128,
                max_collection_len: 256,
            },
        },
    )
    .map_err(|_| invalid())?;
    let Value::Tuple(fields) = &value else {
        return Err(invalid());
    };
    let [
        Value::Text(profile),
        Value::Tuple(schema),
        Value::Tuple(nodes),
        Value::Tuple(roots),
    ] = fields.as_ref()
    else {
        return Err(invalid());
    };
    if profile.as_ref() != PROFILE {
        return Err(invalid());
    }
    let [Value::Tuple(inputs), Value::Tuple(outputs)] = schema.as_ref() else {
        return Err(invalid());
    };
    let parse_domain = |value: &Value| -> Result<Domain, Error> {
        let Value::Tuple(parts) = value else {
            return Err(invalid());
        };
        let [Value::Bool(boolean), Value::I128(min), Value::I128(max)] = parts.as_ref() else {
            return Err(invalid());
        };
        let (min, max) = (
            i64::try_from(*min).map_err(|_| invalid())?,
            i64::try_from(*max).map_err(|_| invalid())?,
        );
        if *boolean {
            if (min, max) != (0, 1) {
                return Err(invalid());
            }
            Ok(Domain::Bool)
        } else {
            Ok(Domain::Int { min, max })
        }
    };
    let inputs = inputs
        .iter()
        .map(parse_domain)
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = outputs
        .iter()
        .map(parse_domain)
        .collect::<Result<Vec<_>, _>>()?;
    let parse_op = |value: &Value| -> Result<Op, Error> {
        let Value::Tuple(parts) = value else {
            return Err(invalid());
        };
        let args = parts
            .iter()
            .map(|part| match part {
                Value::I128(value) => Ok(*value),
                _ => Err(invalid()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let id = |value: i128| u16::try_from(value).map_err(|_| invalid());
        match args.as_slice() {
            [0, a] => Ok(Op::Input(id(*a)?)),
            [1, a] => Ok(Op::Int(i64::try_from(*a).map_err(|_| invalid())?)),
            [2, 0] => Ok(Op::Bool(false)),
            [2, 1] => Ok(Op::Bool(true)),
            [3, a, b] => Ok(Op::Add(id(*a)?, id(*b)?)),
            [4, a, b] => Ok(Op::Sub(id(*a)?, id(*b)?)),
            [5, a, b] => Ok(Op::Eq(id(*a)?, id(*b)?)),
            [6, a, b] => Ok(Op::Lt(id(*a)?, id(*b)?)),
            [7, a, b] => Ok(Op::And(id(*a)?, id(*b)?)),
            [8, a] => Ok(Op::Not(id(*a)?)),
            [9, c, a, b] => Ok(Op::Select(id(*c)?, id(*a)?, id(*b)?)),
            _ => Err(invalid()),
        }
    };
    let nodes = nodes.iter().map(parse_op).collect::<Result<Vec<_>, _>>()?;
    let roots = roots
        .iter()
        .map(|value| match value {
            Value::U128(value) => u16::try_from(*value).map_err(|_| invalid()),
            _ => Err(invalid()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let program = Program::try_new(inputs, outputs, nodes, roots)?;
    if program.value().canonical_bytes().map_err(|_| invalid())? != bytes {
        return Err(invalid());
    }
    Ok(program)
}
