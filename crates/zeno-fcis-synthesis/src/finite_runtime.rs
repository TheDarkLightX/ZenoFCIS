//! Bounded import of canonical finite programs for direct runtime evaluation.
//!
//! Import validates the closed IR again; it does not itself grant application
//! authority or upgrade the synthesis certificate.

use crate::SynthesisError;
use crate::finite::{
    ARITHMETIC_EXTENDED_PROFILE, ARITHMETIC_PROFILE, Domain, EXTENDED_PROFILE, Error, Op, PROFILE,
    Program, V2IntegerWidth as ScalarWidth,
};
use alloc::vec::Vec;
use zeno_fcis_codec::{CanonicalEncode, DecodeLimits, Hash32, decode_value};
use zeno_fcis_value::{Value, ValueLimits, ValueRef};

mod legacy {
    //! Immutable historical scalar data adapted to the existing Signed64 engine.
    use super::{
        I64_ARITHMETIC_EXTENDED_PROFILE, I64_ARITHMETIC_PROFILE, I64_EXTENDED_PROFILE, I64_PROFILE,
        program_value,
    };
    use crate::finite::{
        ARITHMETIC_EXTENDED_PROFILE, ARITHMETIC_PROFILE, Domain, EXTENDED_PROFILE, Error, Op,
        Program, V2ExecutionFailure as Failure, V2IntegerWidth as Width, V2Limits,
        V2NumericAtom as Atom, V2NumericDomain as NumericDomain, V2NumericFailure,
        V2NumericOp as NumericOp, V2NumericProgram, V2Resource, V2Usage, execute_numeric_v2,
        v2_zero_limits,
    };
    use alloc::vec::Vec;
    use zeno_fcis_codec::EncodeError;
    use zeno_fcis_value::Value;

    /// A historical i64 program. Its owned graph is never exposed as an i128 Program.
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct LegacyProgram {
        program: Program,
    }

    /// Owned legacy results and usage computed by the library's numeric evaluator.
    /// This reporting value grants no transition or publication authority.
    #[derive(Debug, Eq, PartialEq)]
    pub struct LegacyOutcome {
        result: Result<Vec<i64>, Failure>,
        usage: V2Usage,
    }

    impl LegacyOutcome {
        /// Actual usage, also retained on arithmetic, domain and budget refusal.
        #[must_use]
        pub fn usage(&self) -> V2Usage {
            self.usage
        }

        /// Consumes the immutable report.
        pub fn into_parts(self) -> (Result<Vec<i64>, Failure>, V2Usage) {
            (self.result, self.usage)
        }
    }

    impl LegacyProgram {
        pub(super) fn try_from_program(program: Program) -> Result<Self, Error> {
            for domain in program.inputs().iter().chain(program.outputs()) {
                let (min, max) = domain.bounds();
                if i64::try_from(min).is_err() || i64::try_from(max).is_err() {
                    return Err(Error::Invalid("program-encoding"));
                }
            }
            for op in program.nodes() {
                if matches!(op, Op::SignedMulDivFloor(..)) {
                    return Err(Error::Invalid("program-encoding"));
                }
                if let Op::Int(value) = op {
                    if i64::try_from(*value).is_err() {
                        return Err(Error::Invalid("program-encoding"));
                    }
                }
            }
            Ok(Self { program })
        }

        /// The original shape-selected i64 profile.
        #[must_use]
        pub fn profile(&self) -> &'static str {
            match self.program.profile() {
                ARITHMETIC_PROFILE => I64_ARITHMETIC_PROFILE,
                ARITHMETIC_EXTENDED_PROFILE => I64_ARITHMETIC_EXTENDED_PROFILE,
                EXTENDED_PROFILE => I64_EXTENDED_PROFILE,
                _ => I64_PROFILE,
            }
        }

        /// Canonical program data retaining the original profile and signed atoms.
        pub fn value(&self) -> Result<Value, EncodeError> {
            program_value(&self.program, true)
        }

        /// Eager checked-i64 evaluation, including unused nodes and both select arms.
        pub fn evaluate(&self, input: &[i64]) -> Result<Vec<i64>, Error> {
            self.execute_v2(
                input,
                v2_zero_limits().with_limit(V2Resource::Step, u64::MAX),
            )
            .into_parts()
            .0
            .map_err(|error| match error {
                Failure::InputDomain => Error::Invalid("input-domain"),
                Failure::Reference => Error::Invalid("node-reference"),
                Failure::Arithmetic => Error::Arithmetic,
                Failure::OutputDomain => Error::Invalid("output-domain"),
                Failure::Budget(_) => Error::Limit("step-budget"),
            })
        }

        /// Executes using the existing typed evaluator and a fresh private meter.
        /// Adapter construction and allocation are outside logical Step accounting.
        /// No application precondition or caller-supplied usage is accepted.
        #[must_use]
        pub fn execute_v2(&self, input: &[i64], limits: V2Limits) -> LegacyOutcome {
            let domain = |d: &Domain| match *d {
                Domain::Bool => NumericDomain::Bool,
                Domain::Int { min, max } => NumericDomain::I128 { min, max },
            };
            let inputs: Vec<_> = self.program.inputs().iter().map(domain).collect();
            let outputs: Vec<_> = self.program.outputs().iter().map(domain).collect();
            let nodes: Vec<_> = self
                .program
                .nodes()
                .iter()
                .map(|op| match *op {
                    Op::Input(i) => NumericOp::Input(i),
                    Op::Int(v) => NumericOp::I128(v),
                    Op::Bool(v) => NumericOp::Bool(v),
                    Op::Add(a, b) => NumericOp::Add(Width::Signed64, a, b),
                    Op::Sub(a, b) => NumericOp::Sub(Width::Signed64, a, b),
                    Op::Eq(a, b) => NumericOp::Eq(a, b),
                    Op::Lt(a, b) => NumericOp::Lt(a, b),
                    Op::And(a, b) => NumericOp::And(a, b),
                    Op::Not(a) => NumericOp::Not(a),
                    Op::Select(c, a, b) => NumericOp::Select(c, a, b),
                    Op::Mul(a, b) => NumericOp::Mul(Width::Signed64, a, b),
                    Op::DivFloor(a, b) => NumericOp::DivFloor(Width::Signed64, a, b),
                    Op::DivCeil(a, b) => NumericOp::DivCeil(Width::Signed64, a, b),
                    Op::MulDivFloor(a, b, d) => {
                        NumericOp::NonnegativeMulDivFloor(Width::Signed64, a, b, d)
                    }
                    Op::MulDivCeil(a, b, d) => {
                        NumericOp::NonnegativeMulDivCeil(Width::Signed64, a, b, d)
                    }
                    Op::ProductsGe(a, b) => NumericOp::NonnegativeProductsGe(a, b),
                    // Private construction rejects this; the importer also refuses tag 16.
                    Op::SignedMulDivFloor(a, b, d) => NumericOp::SignedMulDivFloor(a, b, d),
                })
                .collect();
            let values: Vec<_> = input
                .iter()
                .enumerate()
                .map(|(i, value)| match (self.program.inputs().get(i), *value) {
                    (Some(Domain::Bool), 0) => Atom::Bool(false),
                    (Some(Domain::Bool), 1) => Atom::Bool(true),
                    _ => Atom::I128(i128::from(*value)),
                })
                .collect();
            let (result, usage) = execute_numeric_v2(
                V2NumericProgram {
                    inputs: &inputs,
                    outputs: &outputs,
                    nodes: &nodes,
                    roots: self.program.roots(),
                },
                &values,
                limits,
            )
            .into_parts();
            let result = result
                .map_err(|error| match error {
                    V2NumericFailure::InputDomain => Failure::InputDomain,
                    V2NumericFailure::Reference => Failure::Reference,
                    V2NumericFailure::Kind | V2NumericFailure::Arithmetic => Failure::Arithmetic,
                    V2NumericFailure::OutputDomain => Failure::OutputDomain,
                    V2NumericFailure::Budget(error) => Failure::Budget(error),
                })
                .and_then(|values| {
                    values
                        .into_iter()
                        .map(|value| match value {
                            Atom::Bool(value) => Ok(i64::from(value)),
                            Atom::I128(value) => {
                                i64::try_from(value).map_err(|_| Failure::Arithmetic)
                            }
                            _ => Err(Failure::Arithmetic),
                        })
                        .collect()
                });
            LegacyOutcome { result, usage }
        }
    }
}

pub use legacy::{LegacyOutcome, LegacyProgram};

/// Historical checked-i64 profile; its integer atoms already have 16-byte payloads.
pub const I64_PROFILE: &str = "zeno-fcis/finite-i64/1";
/// Historical extended checked-i64 profile.
pub const I64_EXTENDED_PROFILE: &str = "zeno-fcis/finite-i64/2";
/// Historical D1 checked-i64 arithmetic under the small shape caps.
pub const I64_ARITHMETIC_PROFILE: &str = "zeno-fcis/finite-i64/3";
/// Historical D1 checked-i64 arithmetic under the extended shape caps.
pub const I64_ARITHMETIC_EXTENDED_PROFILE: &str = "zeno-fcis/finite-i64/4";

/// A decoded scalar program with its arithmetic route retained explicitly.
/// Reading this data grants neither transition nor historical replay authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportedScalarProgram {
    /// Historical eager checked-i64 semantics, with unchanged canonical bytes.
    I64(LegacyProgram),
    /// Eager checked-i128 semantics.
    I128(Program),
}

/// Binds a mounted runtime to the exact importer and finite evaluator source.
pub fn evaluator_hash() -> Result<Hash32, SynthesisError> {
    let mut source = crate::finite::semantic_source_bytes();
    source.extend_from_slice(crate::finite::V2_EXECUTION_PROFILE.as_bytes());
    source.extend_from_slice(crate::finite::V2_RECORD_PROFILE.as_bytes());
    // The importer file also contains the complete legacy adapter. Its native
    // source is part of the existing approved source membership.
    let importer = include_bytes!("finite_runtime.rs");
    source.extend_from_slice(&(importer.len() as u64).to_be_bytes());
    source.extend_from_slice(importer);
    // The generated digest also binds the approved closure outside these
    // files, including the value and codec crates the importer decodes with.
    source.extend_from_slice(&crate::finite::v2_authority::EVALUATOR);
    crate::hash_bytes(zeno_fcis_codec::domains::FINITE_RUNTIME_SOURCE, &source)
}

/// Imports a bounded canonical finite program and rechecks its complete
/// type, topology, and result shape before it can be evaluated.
pub fn import_program(bytes: &[u8]) -> Result<Program, Error> {
    decode_program(bytes, Some(ScalarWidth::Signed128), false).map(|(program, _)| program)
}

/// Imports either scalar width, including the explicitly bounded extended route.
/// A historical profile is never reinterpreted as an i128 program.
pub fn import_scalar_program(bytes: &[u8]) -> Result<ImportedScalarProgram, Error> {
    let (program, legacy) = decode_program(bytes, None, true)?;
    if legacy {
        LegacyProgram::try_from_program(program).map(ImportedScalarProgram::I64)
    } else {
        Ok(ImportedScalarProgram::I128(program))
    }
}

/// Reads an i64 program without changing its profile, bytes or overflow rules.
pub fn import_i64_program(bytes: &[u8]) -> Result<LegacyProgram, Error> {
    let (program, _) = decode_program(bytes, Some(ScalarWidth::Signed64), true)?;
    LegacyProgram::try_from_program(program)
}

fn decode_program(
    bytes: &[u8],
    width: Option<ScalarWidth>,
    allow_extended: bool,
) -> Result<(Program, bool), Error> {
    let invalid = || Error::Invalid("program-encoding");
    let value = decode_value(
        bytes,
        DecodeLimits {
            max_input_bytes: if allow_extended {
                512 * 1024
            } else {
                64 * 1024
            },
            value: ValueLimits {
                max_depth: 4,
                max_nodes: if allow_extended { 65536 } else { 4096 },
                max_payload_bytes: 128,
                max_collection_len: if allow_extended { 4096 } else { 256 },
            },
        },
    )
    .map_err(|_| invalid())?;
    let ValueRef::Tuple(fields) = value.view() else {
        return Err(invalid());
    };
    let [profile, schema, nodes, roots] = fields else {
        return Err(invalid());
    };
    let (
        ValueRef::Text(profile),
        ValueRef::Tuple(schema),
        ValueRef::Tuple(nodes),
        ValueRef::Tuple(roots),
    ) = (profile.view(), schema.view(), nodes.view(), roots.view())
    else {
        return Err(invalid());
    };
    let (legacy, extended, arithmetic) = match profile {
        PROFILE | ARITHMETIC_PROFILE
            if width.is_none() || width == Some(ScalarWidth::Signed128) =>
        {
            (false, false, profile == ARITHMETIC_PROFILE)
        }
        EXTENDED_PROFILE | ARITHMETIC_EXTENDED_PROFILE
            if allow_extended && (width.is_none() || width == Some(ScalarWidth::Signed128)) =>
        {
            (false, true, profile == ARITHMETIC_EXTENDED_PROFILE)
        }
        I64_PROFILE | I64_ARITHMETIC_PROFILE
            if width.is_none() || width == Some(ScalarWidth::Signed64) =>
        {
            (true, false, profile == I64_ARITHMETIC_PROFILE)
        }
        I64_EXTENDED_PROFILE | I64_ARITHMETIC_EXTENDED_PROFILE
            if allow_extended && (width.is_none() || width == Some(ScalarWidth::Signed64)) =>
        {
            (true, true, profile == I64_ARITHMETIC_EXTENDED_PROFILE)
        }
        _ => return Err(invalid()),
    };
    // Dispatch may inspect an extended envelope, but a profile-1 program still
    // passes its original decoder limits before structural admission. This
    // preserves malformed-profile-1 refusal precedence as well as valid bytes.
    if allow_extended && !extended {
        return decode_program(bytes, width, false);
    }
    if extended && !arithmetic {
        // Profiles /2 predate seven-word arithmetic nodes. Retain their exact decoder cap.
        decode_value(
            bytes,
            DecodeLimits {
                max_input_bytes: 512 * 1024,
                value: ValueLimits {
                    max_depth: 4,
                    max_nodes: 32768,
                    max_payload_bytes: 128,
                    max_collection_len: 4096,
                },
            },
        )
        .map_err(|_| invalid())?;
    }
    let [inputs, outputs] = schema else {
        return Err(invalid());
    };
    let (ValueRef::Tuple(inputs), ValueRef::Tuple(outputs)) = (inputs.view(), outputs.view())
    else {
        return Err(invalid());
    };
    let parse_domain = |value: &Value| -> Result<Domain, Error> {
        let ValueRef::Tuple(parts) = value.view() else {
            return Err(invalid());
        };
        let [boolean, min, max] = parts else {
            return Err(invalid());
        };
        let (ValueRef::Bool(boolean), ValueRef::I128(min), ValueRef::I128(max)) =
            (boolean.view(), min.view(), max.view())
        else {
            return Err(invalid());
        };
        if legacy && (i64::try_from(min).is_err() || i64::try_from(max).is_err()) {
            return Err(invalid());
        }
        if boolean {
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
        let ValueRef::Tuple(parts) = value.view() else {
            return Err(invalid());
        };
        let args = parts
            .iter()
            .map(|part| match part.view() {
                ValueRef::I128(value) => Ok(value),
                _ => Err(invalid()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let id = |value: i128| u16::try_from(value).map_err(|_| invalid());
        match args.as_slice() {
            [0, a] => Ok(Op::Input(id(*a)?)),
            [1, a] if !legacy || i64::try_from(*a).is_ok() => Ok(Op::Int(*a)),
            [2, 0] => Ok(Op::Bool(false)),
            [2, 1] => Ok(Op::Bool(true)),
            [3, a, b] => Ok(Op::Add(id(*a)?, id(*b)?)),
            [4, a, b] => Ok(Op::Sub(id(*a)?, id(*b)?)),
            [5, a, b] => Ok(Op::Eq(id(*a)?, id(*b)?)),
            [6, a, b] => Ok(Op::Lt(id(*a)?, id(*b)?)),
            [7, a, b] => Ok(Op::And(id(*a)?, id(*b)?)),
            [8, a] => Ok(Op::Not(id(*a)?)),
            [9, c, a, b] => Ok(Op::Select(id(*c)?, id(*a)?, id(*b)?)),
            [10, a, b] if arithmetic => Ok(Op::Mul(id(*a)?, id(*b)?)),
            [11, a, b] if arithmetic => Ok(Op::DivFloor(id(*a)?, id(*b)?)),
            [12, a, b] if arithmetic => Ok(Op::DivCeil(id(*a)?, id(*b)?)),
            [13, a, b, d] if arithmetic => Ok(Op::MulDivFloor(id(*a)?, id(*b)?, id(*d)?)),
            [14, a, b, d] if arithmetic => Ok(Op::MulDivCeil(id(*a)?, id(*b)?, id(*d)?)),
            [15, a, b, c, d, e, f] if arithmetic => Ok(Op::ProductsGe(
                [id(*a)?, id(*b)?, id(*c)?],
                [id(*d)?, id(*e)?, id(*f)?],
            )),
            [16, a, b, d] if arithmetic && !legacy => {
                Ok(Op::SignedMulDivFloor(id(*a)?, id(*b)?, id(*d)?))
            }
            _ => Err(invalid()),
        }
    };
    let nodes = nodes.iter().map(parse_op).collect::<Result<Vec<_>, _>>()?;
    let roots = roots
        .iter()
        .map(|value| match value.view() {
            ValueRef::U128(value) => u16::try_from(value).map_err(|_| invalid()),
            _ => Err(invalid()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let program = if extended {
        Program::try_new_extended(inputs, outputs, nodes, roots)?
    } else {
        Program::try_new(inputs, outputs, nodes, roots)?
    };
    if program_value(&program, legacy)
        .and_then(|value| value.canonical_bytes())
        .map_err(|_| invalid())?
        != bytes
    {
        return Err(invalid());
    }
    Ok((program, legacy))
}

// Serialization only: preserve the historical profile over the same canonical
// signed atoms. Never return the inner i128 Program from the legacy adapter.
fn program_value(program: &Program, legacy: bool) -> Result<Value, zeno_fcis_codec::EncodeError> {
    let value = program.value()?;
    if !legacy {
        return Ok(value);
    }
    let ValueRef::Tuple(fields) = value.view() else {
        return Err(zeno_fcis_codec::EncodeError::UnsupportedProtocolVariant(
            "scalar-program",
        ));
    };
    let mut fields = fields.to_vec();
    let profile = match program.profile() {
        ARITHMETIC_PROFILE => I64_ARITHMETIC_PROFILE,
        ARITHMETIC_EXTENDED_PROFILE => I64_ARITHMETIC_EXTENDED_PROFILE,
        EXTENDED_PROFILE => I64_EXTENDED_PROFILE,
        _ => I64_PROFILE,
    };
    fields[0] = Value::text_ascii_with_limits(profile.into(), ValueLimits::default())
        .map_err(zeno_fcis_codec::EncodeError::InvalidValue)?;
    Value::tuple(fields).map_err(zeno_fcis_codec::EncodeError::InvalidValue)
}
