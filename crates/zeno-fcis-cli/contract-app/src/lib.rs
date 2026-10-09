//! An application built from its contract alone.
//!
//! `project.zeno` declares the types, reasons, channels and laws, and
//! `v2/policy.json` the decision rules and the genesis state. `zeno-fcis
//! generate contract` turns them into `src/v2_contract.rs`, `v2/schema.zcve`
//! and `v2/policy.zcve`. The library Authority makes every decision and
//! checks every law. This crate frames inputs, keeps publications in SQLite
//! and delivers the outbox; it holds no decision or law code. `src/cli.rs`
//! is its operational command line.
#![forbid(unsafe_code)]

/// The generated contract: schema, decision program, laws and genesis state.
pub mod v2_contract;

/// The operational command line: `init`, `submit`, `decide`, `state`,
/// `history`, `pending`, `deliver` and `version`.
#[cfg(feature = "sqlite")]
pub mod cli;
mod examples;
#[cfg(feature = "sqlite")]
mod relay;
#[cfg(feature = "sqlite")]
mod session;
#[cfg(feature = "sqlite")]
pub use session::{
    Current, Decision, Delivered, Destination, Entry, Failure, FileDestination, Head, History,
    Outgoing, Sent, Summary, Upgraded, Waiting, audit, current, decide, deliver, deliver_to,
    history, identity, init, journal_path, journey, migrate, pending, preview, submit, upgrade,
};

use zeno_fcis_codec::{CanonicalEncode, Envelope, Hash32};
use zeno_fcis_synthesis::finite::{
    V2InputLeaf as InputLeaf, v2_authority::Authority, v2_composition as c,
};
use zeno_fcis_value::{Field, Value};

/// Errors are text that names the step that refused.
pub type AppResult<T> = Result<T, String>;

/// Binds the checked catalog, then the library Authority.
///
/// # Errors
/// Returns the catalog or Authority refusal.
pub fn authority<'p>(descriptor: &'p c::Descriptor<'p>) -> AppResult<Authority<'p>> {
    v2_contract::checked_authority(descriptor).map_err(|error| format!("bind: {error:?}"))
}

/// One decision example: the input numbers and the complete decision the
/// Authority must make. Numbers are integers as written, 0 or 1 for
/// booleans and variant IDs for sums.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Example {
    /// Line in the examples file.
    pub line: usize,
    /// State fields, then the command, then the context, in the order the
    /// program reads them.
    pub inputs: Vec<i128>,
    /// Decision class.
    pub class: c::Class,
    /// Reason, if the decision has one.
    pub reason: Option<u32>,
    /// Successor state fields; a reject repeats the pre-state.
    pub post: Vec<i128>,
    /// Each delivery's channel and payload fields.
    pub outbox: Vec<(u32, Vec<i128>)>,
}

/// Parses decision examples, one per line: `inputs | class reason post |
/// deliveries`, with the grammar of `src/examples.rs`, which `zeno-fcis
/// contract review` compiles too, against the inputs, state fields and
/// channels this contract declares. Lines whose first non-blank character
/// is `#` are comments.
///
/// # Errors
/// Returns the first malformed line, or a number outside its domain.
pub fn examples(text: &str) -> AppResult<Vec<Example>> {
    let contract = v2_contract::Contract::new();
    let shape = examples::Shape::of(&contract.descriptor())?;
    let lines = examples::parse(text, &shape)
        .map_err(|error| format!("decision example line {}: {}", error.line, error.reason))?;
    Ok(lines
        .into_iter()
        .map(|line| Example {
            line: line.number,
            inputs: line.inputs,
            class: match line.class {
                examples::Class::Accept => c::Class::Accept,
                examples::Class::Reject => c::Class::Reject,
                examples::Class::Failure => c::Class::CommittedFailure,
            },
            reason: line.reason,
            post: line.post,
            outbox: line.outbox,
        })
        .collect())
}

/// How many numbers a root takes: one per record field, or one.
fn width(schema: c::Schema<'_>) -> AppResult<usize> {
    match schema {
        c::Schema::Record(fields) => Ok(fields.len()),
        c::Schema::Leaf(_) => Ok(1),
        other => Err(format!("unsupported root schema {other:?}")),
    }
}

fn leaf_value(leaf: &InputLeaf, number: i128) -> AppResult<Value> {
    match leaf {
        InputLeaf::Bool if number == 0 || number == 1 => Ok(Value::boolean(number == 1)),
        InputLeaf::I128 { .. } => Ok(Value::signed(number)),
        InputLeaf::Sum { type_id, .. } => u16::try_from(number)
            .map(|variant| Value::sum(*type_id, variant, None))
            .map_err(|_| format!("{number} is not a variant of type {type_id}")),
        other => Err(format!("{number} does not fit {other:?}")),
    }
}

fn leaf_atom(leaf: &InputLeaf, number: i128) -> AppResult<c::Atom<'static>> {
    match leaf {
        InputLeaf::Bool if number == 0 || number == 1 => Ok(c::Atom::Bool(number == 1)),
        InputLeaf::I128 { .. } => Ok(c::Atom::I128(number)),
        InputLeaf::Sum { type_id, .. } => u16::try_from(number)
            .map(|variant| c::Atom::Sum {
                type_id: *type_id,
                variant,
            })
            .map_err(|_| format!("{number} is not a variant of type {type_id}")),
        other => Err(format!("{number} does not fit {other:?}")),
    }
}

fn root_value(schema: c::Schema<'_>, numbers: &[i128]) -> AppResult<Value> {
    match schema {
        c::Schema::Record(fields) => Value::record_canonical(
            fields
                .iter()
                .zip(numbers)
                .map(|(field, number)| Ok(Field::new(field.id, leaf_value(&field.leaf, *number)?)))
                .collect::<AppResult<_>>()?,
        )
        .map_err(|error| format!("record: {error:?}")),
        c::Schema::Leaf(leaf) => leaf_value(leaf, numbers.first().copied().unwrap_or_default()),
        other => Err(format!("unsupported root schema {other:?}")),
    }
}

fn frame(binding: &c::FrameBinding, value: Value) -> AppResult<Vec<u8>> {
    Envelope::new(binding.root, Hash32::new(binding.schema), value)
        .canonical_bytes()
        .map_err(|error| format!("frame: {error:?}"))
}

/// The framed state, command and context bytes for an example's inputs.
///
/// # Errors
/// Returns a count or value that does not fit the contract's roots.
pub fn wire(descriptor: &c::Descriptor<'_>, inputs: &[i128]) -> AppResult<[Vec<u8>; 3]> {
    let schemas = [descriptor.state, descriptor.command, descriptor.context];
    let widths = [width(schemas[0])?, width(schemas[1])?, width(schemas[2])?];
    if inputs.len() != widths.iter().sum::<usize>() {
        return Err(format!(
            "{} input numbers; the contract reads {} state, {} command and {} context numbers",
            inputs.len(),
            widths[0],
            widths[1],
            widths[2]
        ));
    }
    let (state, rest) = inputs.split_at(widths[0]);
    let (command, context) = rest.split_at(widths[1]);
    let framing = &v2_contract::FRAMING;
    Ok([
        frame(&framing.state, root_value(schemas[0], state)?)?,
        frame(&framing.command, root_value(schemas[1], command)?)?,
        frame(&framing.context, root_value(schemas[2], context)?)?,
    ])
}

/// The framed state for state field numbers in field order.
///
/// # Errors
/// Returns a count or value that does not fit the state root.
pub fn state(descriptor: &c::Descriptor<'_>, numbers: &[i128]) -> AppResult<Vec<u8>> {
    if numbers.len() != width(descriptor.state)? {
        return Err(format!("{} state numbers", numbers.len()));
    }
    frame(
        &v2_contract::FRAMING.state,
        root_value(descriptor.state, numbers)?,
    )
}

/// The framed genesis state the contract requires.
///
/// # Errors
/// Returns a genesis value with no original form.
pub fn genesis() -> AppResult<Vec<u8>> {
    let fields = v2_contract::GENESIS
        .iter()
        .map(|field| {
            let value = match field.value {
                c::Atom::Bool(value) => Value::boolean(value),
                c::Atom::I128(value) => Value::signed(value),
                c::Atom::Sum { type_id, variant } => Value::sum(type_id, variant, None),
                other => return Err(format!("genesis value {other:?}")),
            };
            Ok(Field::new(field.id, value))
        })
        .collect::<AppResult<_>>()?;
    let state = Value::record_canonical(fields).map_err(|error| format!("genesis: {error:?}"))?;
    frame(&v2_contract::FRAMING.state, state)
}

/// The genesis state's field values, in the order examples write them.
///
/// # Errors
/// Returns a genesis value with no number.
pub fn genesis_numbers() -> AppResult<Vec<i128>> {
    v2_contract::GENESIS
        .iter()
        .map(|field| match field.value {
            c::Atom::Bool(value) => Ok(i128::from(value)),
            c::Atom::I128(value) => Ok(value),
            c::Atom::Sum { variant, .. } => Ok(i128::from(variant)),
            other => Err(format!("genesis value {other:?}")),
        })
        .collect()
}

/// Evaluates an example with the Authority and compares the whole decision.
///
/// # Errors
/// Returns the refusal or the first difference.
pub fn check(authority: &Authority<'_>, example: &Example) -> AppResult<()> {
    let raw = wire(authority.descriptor(), &example.inputs)?;
    let evaluation = authority.evaluate(c::Raw {
        state: &raw[0],
        command: &raw[1],
        context: &raw[2],
    });
    let candidate = evaluation
        .result()
        .map_err(|refusal| format!("line {}: refused: {refusal:?}", example.line))?;
    compare(authority.descriptor(), candidate, example)
}

/// Compares a decision with an example: class, reason, successor state and
/// every delivery.
///
/// # Errors
/// Returns the first difference.
pub fn compare(
    descriptor: &c::Descriptor<'_>,
    candidate: &c::Candidate<'_>,
    example: &Example,
) -> AppResult<()> {
    let differ = |what: &str| Err(format!("line {}: {what} differs", example.line));
    if candidate.class() != example.class {
        return differ("class");
    }
    if candidate.reason() != example.reason {
        return differ("reason");
    }
    let c::Schema::Record(fields) = descriptor.state else {
        return Err("the state root must be a record".to_owned());
    };
    let state = example.inputs.get(..fields.len()).unwrap_or_default();
    let post = if example.class == c::Class::Reject {
        // A reject publishes no successor; the example repeats the pre-state.
        if example.post != state {
            return differ("rejected post-state");
        }
        Vec::new()
    } else {
        if example.post.len() != fields.len() {
            return differ("post-state length");
        }
        fields
            .iter()
            .zip(&example.post)
            .map(|(field, number)| {
                Ok(c::Field {
                    id: field.id,
                    value: leaf_atom(&field.leaf, *number)?,
                })
            })
            .collect::<AppResult<Vec<_>>>()?
    };
    if candidate.post() != post.as_slice() {
        return differ("post-state");
    }
    if candidate.outbox().len() != example.outbox.len() {
        return differ("delivery count");
    }
    for (delivery, (channel, numbers)) in candidate.outbox().iter().zip(&example.outbox) {
        let declared = descriptor
            .channels
            .iter()
            .find(|declared| declared.id == *channel)
            .ok_or_else(|| format!("line {}: channel {channel} is not declared", example.line))?;
        if delivery.channel != *channel || declared.payload.len() != numbers.len() {
            return differ("delivery channel");
        }
        for ((field, typed), number) in delivery.payload.iter().zip(declared.payload).zip(numbers) {
            let expected = match typed.domain {
                c::Domain::Bool if *number == 0 || *number == 1 => c::Atom::Bool(*number == 1),
                c::Domain::I128 { .. } => c::Atom::I128(*number),
                c::Domain::Sum { type_id, .. } => c::Atom::Sum {
                    type_id,
                    variant: u16::try_from(*number).map_err(|_| "bad variant".to_owned())?,
                },
                _ => return differ("payload domain"),
            };
            if field.id != typed.field || field.value != expected {
                return differ("payload");
            }
        }
    }
    Ok(())
}
