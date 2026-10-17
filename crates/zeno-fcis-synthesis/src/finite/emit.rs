//! Pure target adapters. An emitter consumes only closed IR, never its contract.
//! Output is source code, not semantic evidence or runtime authority.

use super::{ARITHMETIC_PROFILE, Error, Op, PROFILE, Program};
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
    /// Human- and agent-readable description of the emitted call ABI.
    ///
    /// Trusted registration must state the exact accepted argument and result
    /// shapes. The default is deliberately uninformative: an unstated ABI is a
    /// reason to read the adapter, never evidence about it.
    fn abi(&self) -> &'static str {
        "unspecified; review this adapter before calling the emitted function"
    }
}

/// Rust module emitter: core-only code with stack-allocated inputs and outputs.
pub struct RustEmitter;
/// Python module emitter: exact built-in integers, with checked i128 intermediates.
pub struct PythonEmitter;
/// JavaScript ES module emitter: `BigInt` values behind an immutable string ABI.
pub struct JavaScriptEmitter;

const RUST_ABI: &str = "transition(input: &[i128]) -> Option<[i128; outputs]>; an out-of-domain input, \
     a checked-arithmetic trap, or an out-of-domain output returns None";
const PYTHON_ABI: &str = "transition(input: tuple[int, ...]) -> tuple[int, ...] | None; the argument must be \
     an exact tuple of built-in int, excluding bool and int subclasses";
const JAVASCRIPT_ABI: &str = "transition(input: string) -> string | null; the argument must be a primitive string of \
     exactly `inputs` canonical signed decimal i128 tokens separated by single ASCII spaces \
     (the empty string for zero fields); the result spells the complete output tuple the same \
     way, and null covers every rejected input, domain, arithmetic, or output case";

fn identity(language: &str) -> Hash32 {
    RustCryptoSha256::hash_parts(&[
        PROFILE.as_bytes(),
        b"\0",
        language.as_bytes(),
        b"\0",
        include_bytes!("emit.rs"),
        ARITHMETIC_PROFILE.as_bytes(),
        include_bytes!("value/scalar.rs"),
        include_bytes!("value/wide.rs"),
    ])
}
impl TargetEmitter for RustEmitter {
    fn target(&self) -> TargetId {
        TargetId {
            language: "rust",
            revision: 3,
            extension: "rs",
        }
    }
    fn identity(&self) -> Hash32 {
        identity("rust")
    }
    fn abi(&self) -> &'static str {
        RUST_ABI
    }
    fn emit(&self, program: &Program) -> Result<String, Error> {
        let mut source = String::from(
            "// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.\n/// Evaluates the generated finite program; no commit authority.\n#[rustfmt::skip]\n",
        );
        writeln!(
            source,
            "pub fn transition(input: &[i128]) -> Option<[i128; {}]> {{",
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
                "    if !({min}_i128..={max}_i128).contains(&input[{i}]) {{ return None; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        for (i, op) in program.nodes().iter().enumerate() {
            let expression = match *op {
                Op::Input(a) => format!("input[{a}]"),
                Op::Int(a) => format!("{a}_i128"),
                Op::Bool(a) => format!("{}_i128", i128::from(a)),
                Op::Add(a, b) => format!("v{a}.checked_add(v{b})?"),
                Op::Sub(a, b) => format!("v{a}.checked_sub(v{b})?"),
                Op::Mul(a, b) => format!("arithmetic::scalar::multiply(v{a},v{b})?"),
                Op::DivFloor(a, b) => format!("arithmetic::scalar::divide(false,v{a},v{b})?"),
                Op::DivCeil(a, b) => format!("arithmetic::scalar::divide(true,v{a},v{b})?"),
                Op::SignedMulDivFloor(a, b, d) => {
                    format!("arithmetic::scalar::signed_mul_div_floor(v{a},v{b},v{d})?")
                }
                Op::MulDivFloor(a, b, d) => {
                    format!("arithmetic::scalar::mul_div(false,v{a},v{b},v{d})?")
                }
                Op::MulDivCeil(a, b, d) => {
                    format!("arithmetic::scalar::mul_div(true,v{a},v{b},v{d})?")
                }
                Op::ProductsGe(a, b) => format!(
                    "i128::from(arithmetic::scalar::products_ge(v{},v{},v{},v{},v{},v{})?)",
                    a[0], a[1], a[2], b[0], b[1], b[2]
                ),
                Op::Eq(a, b) => format!("i128::from(v{a} == v{b})"),
                Op::Lt(a, b) => format!("i128::from(v{a} < v{b})"),
                Op::And(a, b) => format!("i128::from(v{a} == 1 && v{b} == 1)"),
                Op::Not(a) => format!("i128::from(v{a} == 0)"),
                Op::Select(c, a, b) => format!("if v{c} == 1 {{ v{a} }} else {{ v{b} }}"),
            };
            // Some valid graphs have unused nodes. Evaluate them anyway: a dead
            // checked arithmetic node must still trap under this eager profile.
            writeln!(
                source,
                "    let v{i}: i128 = {expression};\n    let _ = v{i};"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        for (d, id) in program.outputs().iter().zip(program.roots()) {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "    if !({min}_i128..={max}_i128).contains(&v{id}) {{ return None; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str("    Some([");
        for id in program.roots() {
            write!(source, "v{id},").map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str("])\n}\n");
        if program.nodes().iter().any(Op::requires_arithmetic_profile) {
            source = source.replacen(PROFILE, program.profile(), 1);
            source.push_str("\n#[allow(dead_code)]\nmod arithmetic {\n    pub(crate) mod wide {\n");
            source.push_str(include_str!("value/wide.rs"));
            source.push_str("\n}\n    pub(crate) mod scalar {\n");
            source.push_str(include_str!("value/scalar.rs"));
            source.push_str("\n}\n}\n");
        }
        Ok(source)
    }
}
impl TargetEmitter for PythonEmitter {
    fn target(&self) -> TargetId {
        TargetId {
            language: "python",
            revision: 3,
            extension: "py",
        }
    }
    fn identity(&self) -> Hash32 {
        identity("python")
    }
    fn abi(&self) -> &'static str {
        PYTHON_ABI
    }
    fn emit(&self, program: &Program) -> Result<String, Error> {
        let mut source = String::from(
            "# Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.\ndef transition(input):\n",
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
            let invalid = match *op {
                Op::SignedMulDivFloor(_, _, d) => Some(format!("v{d} <= 0")),
                Op::DivFloor(_, b) | Op::DivCeil(_, b) => Some(format!("v{b} == 0")),
                Op::MulDivFloor(a, b, d) | Op::MulDivCeil(a, b, d) => {
                    Some(format!("v{a} < 0 or v{b} < 0 or v{d} <= 0"))
                }
                Op::ProductsGe(a, b) => Some(format!(
                    "v{} < 0 or v{} < 0 or v{} < 0 or v{} < 0 or v{} < 0 or v{} < 0",
                    a[0], a[1], a[2], b[0], b[1], b[2]
                )),
                _ => None,
            };
            if let Some(invalid) = invalid {
                writeln!(source, "    if {invalid}:\n        return None")
                    .map_err(|_| Error::Invalid("format"))?;
            }
            let expression = match *op {
                Op::Input(a) => format!("input[{a}]"),
                Op::Int(a) => format!("{a}"),
                Op::Bool(a) => format!("{}", i128::from(a)),
                Op::Add(a, b) => format!("v{a} + v{b}"),
                Op::Sub(a, b) => format!("v{a} - v{b}"),
                Op::Mul(a, b) => format!("v{a} * v{b}"),
                Op::DivFloor(a, b) => format!("v{a} // v{b}"),
                Op::DivCeil(a, b) => format!("-((-v{a}) // v{b})"),
                Op::SignedMulDivFloor(a, b, d) => format!("(v{a} * v{b}) // v{d}"),
                Op::MulDivFloor(a, b, d) => format!("(v{a} * v{b}) // v{d}"),
                Op::MulDivCeil(a, b, d) => format!("(v{a} * v{b} + v{d} - 1) // v{d}"),
                Op::ProductsGe(a, b) => format!(
                    "int(v{} * v{} * v{} >= v{} * v{} * v{})",
                    a[0], a[1], a[2], b[0], b[1], b[2]
                ),
                Op::Eq(a, b) => format!("int(v{a} == v{b})"),
                Op::Lt(a, b) => format!("int(v{a} < v{b})"),
                Op::And(a, b) => format!("int(v{a} == 1 and v{b} == 1)"),
                Op::Not(a) => format!("int(v{a} == 0)"),
                Op::Select(c, a, b) => format!("v{a} if v{c} == 1 else v{b}"),
            };
            writeln!(source, "    v{i} = {expression}").map_err(|_| Error::Invalid("format"))?;
            if matches!(
                op,
                Op::Add(..)
                    | Op::Sub(..)
                    | Op::Mul(..)
                    | Op::DivFloor(..)
                    | Op::DivCeil(..)
                    | Op::MulDivFloor(..)
                    | Op::MulDivCeil(..)
                    | Op::SignedMulDivFloor(..)
            ) {
                writeln!(source,"    if not (-170141183460469231731687303715884105728 <= v{i} <= 170141183460469231731687303715884105727):\n        return None").map_err(|_|Error::Invalid("format"))?;
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
        if program.nodes().iter().any(Op::requires_arithmetic_profile) {
            source = source.replacen(PROFILE, program.profile(), 1);
        }
        Ok(source)
    }
}

/// Fixed part of the emitted ES module: the string ABI admission scanner.
///
/// The scanner never sees a caller-controlled object. It bounds the input by
/// the declared arity, accepts only canonical ASCII spellings, and builds every
/// value with `BigInt`, so no field can pass through `Number` rounding.
const JAVASCRIPT_PRELUDE: &str = r#"// Generated from zeno-fcis/finite-i128/1. Pure code; no commit authority.
// ABI: transition(input) requires a primitive string holding exactly the
// declared number of canonical signed decimal i128 tokens separated by single
// ASCII spaces; zero fields is the empty string. It returns the complete output
// tuple in the same spelling, or null for every rejected input, domain,
// arithmetic, or output case. The immutable string ABI excludes Number rounding
// and caller objects, arrays, proxies, and conversion hooks; every field and
// node value is a BigInt. This module has no imports, eval, Function, ambient
// I/O, global assignment, or source splice, and mutates only local temporaries.
// Unmodified native ECMAScript intrinsics are a stated runtime premise, not a
// claim about an arbitrary monkeypatched runtime.
const I128_MIN = -170141183460469231731687303715884105728n;
const I128_MAX = 170141183460469231731687303715884105727n;
const TOKEN = 40; // signed i128 MIN token length
const DIGITS = 39; // unsigned magnitude of i128 MIN

function parseFields(text, arity) {
  // Bound the work by the declared arity before any scan, slice, or BigInt.
  if (text.length > (arity === 0 ? 0 : arity * (TOKEN + 1) - 1)) { return null; }
  const fields = [];
  let at = 0;
  for (let index = 0; index < arity; index += 1) {
    // Exactly one ASCII space separates tokens: no other whitespace, no run.
    if (index > 0) {
      if (text.charCodeAt(at) !== 32) { return null; }
      at += 1;
    }
    const negative = text.charCodeAt(at) === 45;
    if (negative) { at += 1; }
    const start = at;
    while (at < text.length) {
      const code = text.charCodeAt(at);
      if (code < 48 || code > 57) { break; }
      at += 1;
    }
    // Only ASCII digits reach BigInt: a leading plus, leading zero, negative
    // zero, exponent, hex, or non-ASCII digit leaves an unscanned character.
    const length = at - start;
    if (length === 0 || length > DIGITS) { return null; }
    if (length > 1 && text.charCodeAt(start) === 48) { return null; }
    const magnitude = BigInt(text.slice(start, at));
    if (negative && magnitude === 0n) { return null; }
    const value = negative ? -magnitude : magnitude;
    if (value < I128_MIN || value > I128_MAX) { return null; }
    fields.push(value);
  }
  // A missing token, extra token, trailing space, or trailing character fails.
  if (at !== text.length) { return null; }
  return fields;
}
"#;

impl TargetEmitter for JavaScriptEmitter {
    fn target(&self) -> TargetId {
        TargetId {
            language: "javascript",
            revision: 3,
            extension: "mjs",
        }
    }
    fn identity(&self) -> Hash32 {
        identity("javascript")
    }
    fn abi(&self) -> &'static str {
        JAVASCRIPT_ABI
    }
    fn emit(&self, program: &Program) -> Result<String, Error> {
        let mut source = String::from(JAVASCRIPT_PRELUDE);
        if program.nodes().iter().any(Op::requires_arithmetic_profile) {
            source = source.replacen(PROFILE, program.profile(), 1);
            source.push_str("\nfunction divRounded(a, b, up) {\n  const q = a / b;\n  const r = a % b;\n  if (r === 0n) { return q; }\n  const negative = (a < 0n) !== (b < 0n);\n  return up ? q + (negative ? 0n : 1n) : q - (negative ? 1n : 0n);\n}\n");
        }
        // Reject every non-string before any method, index, or coercion hook
        // can run, then admit the exact declared arity instead of a length.
        writeln!(
            source,
            "\nexport function transition(input) {{\n  if (typeof input !== \"string\") {{ return null; }}\n  const fields = parseFields(input, {});\n  if (fields === null) {{ return null; }}",
            program.inputs().len()
        )
        .map_err(|_| Error::Invalid("format"))?;
        for (i, d) in program.inputs().iter().enumerate() {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "  if (fields[{i}] < {min}n || fields[{i}] > {max}n) {{ return null; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        for (i, op) in program.nodes().iter().enumerate() {
            let invalid = match *op {
                Op::SignedMulDivFloor(_, _, d) => Some(format!("v{d} <= 0n")),
                Op::DivFloor(_, b) | Op::DivCeil(_, b) => Some(format!("v{b} === 0n")),
                Op::MulDivFloor(a, b, d) | Op::MulDivCeil(a, b, d) => {
                    Some(format!("v{a} < 0n || v{b} < 0n || v{d} <= 0n"))
                }
                Op::ProductsGe(a, b) => Some(format!(
                    "v{} < 0n || v{} < 0n || v{} < 0n || v{} < 0n || v{} < 0n || v{} < 0n",
                    a[0], a[1], a[2], b[0], b[1], b[2]
                )),
                _ => None,
            };
            if let Some(invalid) = invalid {
                writeln!(source, "  if ({invalid}) {{ return null; }}")
                    .map_err(|_| Error::Invalid("format"))?;
            }
            let expression = match *op {
                Op::Input(a) => format!("fields[{a}]"),
                Op::Int(a) => format!("{a}n"),
                Op::Bool(a) => format!("{}n", i128::from(a)),
                Op::Add(a, b) => format!("v{a} + v{b}"),
                Op::Sub(a, b) => format!("v{a} - v{b}"),
                Op::Mul(a, b) => format!("v{a} * v{b}"),
                Op::DivFloor(a, b) => format!("divRounded(v{a}, v{b}, false)"),
                Op::DivCeil(a, b) => format!("divRounded(v{a}, v{b}, true)"),
                Op::SignedMulDivFloor(a, b, d) => format!("divRounded(v{a} * v{b},v{d},false)"),
                Op::MulDivFloor(a, b, d) => format!("(v{a} * v{b}) / v{d}"),
                Op::MulDivCeil(a, b, d) => format!("(v{a} * v{b} + v{d} - 1n) / v{d}"),
                Op::ProductsGe(a, b) => format!(
                    "v{} * v{} * v{} >= v{} * v{} * v{} ? 1n : 0n",
                    a[0], a[1], a[2], b[0], b[1], b[2]
                ),
                Op::Eq(a, b) => format!("v{a} === v{b} ? 1n : 0n"),
                Op::Lt(a, b) => format!("v{a} < v{b} ? 1n : 0n"),
                Op::And(a, b) => format!("(v{a} === 1n && v{b} === 1n) ? 1n : 0n"),
                Op::Not(a) => format!("v{a} === 0n ? 1n : 0n"),
                Op::Select(c, a, b) => format!("v{c} === 1n ? v{a} : v{b}"),
            };
            // Every node is evaluated in order, including a dead one: BigInt is
            // unbounded, so each add and sub carries its own i128 trap check.
            writeln!(source, "  const v{i} = {expression};")
                .map_err(|_| Error::Invalid("format"))?;
            if matches!(
                op,
                Op::Add(..)
                    | Op::Sub(..)
                    | Op::Mul(..)
                    | Op::DivFloor(..)
                    | Op::DivCeil(..)
                    | Op::MulDivFloor(..)
                    | Op::MulDivCeil(..)
                    | Op::SignedMulDivFloor(..)
            ) {
                writeln!(
                    source,
                    "  if (v{i} < I128_MIN || v{i} > I128_MAX) {{ return null; }}"
                )
                .map_err(|_| Error::Invalid("format"))?;
            }
        }
        for (d, id) in program.outputs().iter().zip(program.roots()) {
            let (min, max) = d.bounds();
            writeln!(
                source,
                "  if (v{id} < {min}n || v{id} > {max}n) {{ return null; }}"
            )
            .map_err(|_| Error::Invalid("format"))?;
        }
        // BigInt.prototype.toString gives the same canonical decimal spelling
        // the ABI requires of the caller; there is no separator or exponent.
        source.push_str("  return ");
        for (position, id) in program.roots().iter().enumerate() {
            if position > 0 {
                source.push_str(" + \" \" + ");
            }
            write!(source, "v{id}.toString()").map_err(|_| Error::Invalid("format"))?;
        }
        source.push_str(";\n}\n");
        Ok(source)
    }
}
