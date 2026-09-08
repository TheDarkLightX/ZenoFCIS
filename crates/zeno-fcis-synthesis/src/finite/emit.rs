//! Pure target adapters. An emitter consumes only closed IR, never its contract.
//! Output is source code, not semantic evidence or runtime authority.

use super::{Error, Op, PROFILE, Program};
use alloc::{format, string::String};
use core::fmt::Write;
use zeno_fcis_codec::{CommitmentHasher, Hash32};
use zeno_fcis_crypto::RustCryptoSha256;

/// Stable target identity. Languages are an extension seam, not a core enum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetId {
    /// Target language name.
    pub language: &'static str,
    /// Adapter ABI/emission revision.
    pub revision: u32,
    /// Fixed source extension, chosen by trusted registration.
    pub extension: &'static str,
}
/// A pure adapter from the shared closed theory to one target language.
///
/// Trusted implementations must emit a closed pure subset with no ambient reads,
/// mutation of authoritative inputs, I/O, imports with effects, or source splice.
/// Output replay checks functional conformance; it does not establish purity of
/// a new adapter. Admission of an adapter also requires review of this mechanism.
pub trait TargetEmitter {
    /// Reports the target identity.
    fn target(&self) -> TargetId;
    /// Content identity of the emitter implementation and semantic profile.
    fn identity(&self) -> Hash32;
    /// Emits `transition(input)` with an explicit, checked positional ABI.
    /// Implementations must reject unsupported semantics instead of approximating.
    fn emit(&self, program: &Program) -> Result<String, Error>;
}

/// Rust module emitter: core-only code with stack-allocated inputs and outputs.
pub struct RustEmitter;
/// Python module emitter: exact built-in integers, with checked i64 intermediates.
pub struct PythonEmitter;

fn identity(language: &str) -> Hash32 {
    RustCryptoSha256::hash_parts(&[
        PROFILE.as_bytes(),
        b"\0",
        language.as_bytes(),
        b"\0",
        include_bytes!("emit.rs"),
    ])
}
impl TargetEmitter for RustEmitter {
    fn target(&self) -> TargetId {
        TargetId {
            language: "rust",
            revision: 1,
            extension: "rs",
        }
    }
    fn identity(&self) -> Hash32 {
        identity("rust")
    }
    fn emit(&self, program: &Program) -> Result<String, Error> {
        let mut source = String::from(
            "// Generated from zeno-fcis/finite-i64/1. Pure code; no commit authority.\n#[rustfmt::skip]\n",
        );
        writeln!(
            source,
            "pub fn transition(input: &[i64]) -> Option<[i64; {}]> {{",
            program.outputs().len()
        )
        .map_err(|_| Error::Invalid("format"))?;
        writeln!(
            source,
            "    if input.len() != {} {{ return None; }}",
            program.inputs().len()
        )
        .map_err(|_| Error::Invalid("format"))?;
        for (i, d) in program.inputs().iter().enumerate() {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "    if !({min}_i64..={max}_i64).contains(&input[{i}]) {{ return None; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        for (i, op) in program.nodes().iter().enumerate() {
            let expression = match *op {
                Op::Input(a) => format!("input[{a}]"),
                Op::Int(a) => format!("{a}_i64"),
                Op::Bool(a) => format!("{}_i64", i64::from(a)),
                Op::Add(a, b) => format!("v{a}.checked_add(v{b})?"),
                Op::Sub(a, b) => format!("v{a}.checked_sub(v{b})?"),
                Op::Eq(a, b) => format!("i64::from(v{a} == v{b})"),
                Op::Lt(a, b) => format!("i64::from(v{a} < v{b})"),
                Op::And(a, b) => format!("i64::from(v{a} == 1 && v{b} == 1)"),
                Op::Not(a) => format!("i64::from(v{a} == 0)"),
                Op::Select(c, a, b) => format!("if v{c} == 1 {{ v{a} }} else {{ v{b} }}"),
            };
            // Some valid graphs have unused nodes. Evaluate them anyway: a dead
            // checked arithmetic node must still trap under this eager profile.
            writeln!(
                source,
                "    let v{i}: i64 = {expression};\n    let _ = v{i};"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        for (d, id) in program.outputs().iter().zip(program.roots()) {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "    if !({min}_i64..={max}_i64).contains(&v{id}) {{ return None; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str("    Some([");
        for id in program.roots() {
            write!(source, "v{id},").map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str("])\n}\n");
        Ok(source)
    }
}
impl TargetEmitter for PythonEmitter {
    fn target(&self) -> TargetId {
        TargetId {
            language: "python",
            revision: 1,
            extension: "py",
        }
    }
    fn identity(&self) -> Hash32 {
        identity("python")
    }
    fn emit(&self, program: &Program) -> Result<String, Error> {
        let mut source = String::from(
            "# Generated from zeno-fcis/finite-i64/1. Pure code; no commit authority.\ndef transition(input):\n",
        );
        writeln!(
            source,
            "    if type(input) is not tuple or len(input) != {}:\n        return None",
            program.inputs().len()
        )
        .map_err(|_| Error::Invalid("format"))?;
        for (i, d) in program.inputs().iter().enumerate() {
            let (min, max) = d.bounds();
            writeln!(source,"    if type(input[{i}]) is not int or not ({min} <= input[{i}] <= {max}):\n        return None").map_err(|_|Error::Invalid("format"))?;
        }
        for (i, op) in program.nodes().iter().enumerate() {
            let expression = match *op {
                Op::Input(a) => format!("input[{a}]"),
                Op::Int(a) => format!("{a}"),
                Op::Bool(a) => format!("{}", i64::from(a)),
                Op::Add(a, b) => format!("v{a} + v{b}"),
                Op::Sub(a, b) => format!("v{a} - v{b}"),
                Op::Eq(a, b) => format!("int(v{a} == v{b})"),
                Op::Lt(a, b) => format!("int(v{a} < v{b})"),
                Op::And(a, b) => format!("int(v{a} == 1 and v{b} == 1)"),
                Op::Not(a) => format!("int(v{a} == 0)"),
                Op::Select(c, a, b) => format!("v{a} if v{c} == 1 else v{b}"),
            };
            writeln!(source, "    v{i} = {expression}").map_err(|_| Error::Invalid("format"))?;
            if matches!(op, Op::Add(..) | Op::Sub(..)) {
                writeln!(source,"    if not (-9223372036854775808 <= v{i} <= 9223372036854775807):\n        return None").map_err(|_|Error::Invalid("format"))?;
            }
        }
        for (d, id) in program.outputs().iter().zip(program.roots()) {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "    if not ({min} <= v{id} <= {max}):\n        return None"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str("    return (");
        for id in program.roots() {
            write!(source, "v{id},").map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str(")\n");
        Ok(source)
    }
}
