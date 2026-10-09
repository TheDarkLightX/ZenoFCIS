//! The decision-examples grammar. Every application built from a contract
//! and `zeno-fcis contract review` compile this same file, so both read an
//! examples file alike: the same examples, or the same error on the same
//! line.
//!
//! A line that is blank, or whose first non-blank character is `#`, is
//! skipped. Every other line is one example: `|`-separated sections,
//!
//! ```text
//! inputs | class reason post | deliveries
//! ```
//!
//! - The last section holds the deliveries and the one before it the
//!   decision. Every earlier section holds input numbers, read in order, so
//!   the inputs may be split over several sections. Together they are the
//!   state fields, then the command, then the context, in the order the
//!   program reads them, one number each.
//! - The decision is the class, `accept`, `reject` or `failure`; then the
//!   reason, a number or `-`; then one number per state field, the successor
//!   state. A reject repeats the pre-state.
//! - The deliveries are `-`, or deliveries separated by `;`. A delivery is a
//!   declared channel's ID followed by one number per payload field. When
//!   the contract declares exactly one channel, the payload numbers alone
//!   are also a delivery on it; the count of numbers tells the two forms
//!   apart.
//! - A number is a decimal integer: 0 or 1 for a boolean, the value of an
//!   integer, the variant ID of a sum. Each lies in the declared domain of
//!   its input, state field or payload field.

use zeno_fcis_synthesis::finite::{V2InputLeaf as InputLeaf, v2_composition as c};

/// The decision class an example names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Class {
    Accept,
    Reject,
    Failure,
}

/// One example line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Line {
    /// Line number in the file, from 1.
    pub(crate) number: usize,
    /// State fields, then the command, then the context.
    pub(crate) inputs: Vec<i128>,
    pub(crate) class: Class,
    pub(crate) reason: Option<u32>,
    /// Successor state fields; a reject repeats the pre-state.
    pub(crate) post: Vec<i128>,
    /// Each delivery's channel and payload numbers.
    pub(crate) outbox: Vec<(u32, Vec<i128>)>,
}

/// The first malformed line and what is wrong with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Error {
    pub(crate) line: usize,
    pub(crate) reason: String,
}

/// The numbers a value may be written as.
#[derive(Debug)]
enum Domain {
    Bool,
    Int {
        min: i128,
        max: i128,
    },
    Variants(Vec<u16>),
    /// A value with no number form, such as text.
    Unwritten,
}

impl Domain {
    fn of_leaf(leaf: &InputLeaf) -> Self {
        match leaf {
            InputLeaf::Bool => Self::Bool,
            InputLeaf::I128 { min, max } => Self::Int {
                min: i128::from(*min),
                max: i128::from(*max),
            },
            InputLeaf::Sum { variants, .. } => {
                Self::Variants(variants.iter().map(|variant| variant.id).collect())
            }
            _ => Self::Unwritten,
        }
    }

    fn of_payload(domain: &c::Domain<'_>) -> Self {
        match domain {
            c::Domain::Bool => Self::Bool,
            c::Domain::I128 { min, max } => Self::Int {
                min: *min,
                max: *max,
            },
            c::Domain::Sum { variants, .. } => Self::Variants(variants.to_vec()),
            _ => Self::Unwritten,
        }
    }

    fn check(&self, value: i128, name: &str) -> Result<i128, String> {
        let admitted = match self {
            Self::Bool => value == 0 || value == 1,
            Self::Int { min, max } => (*min..=*max).contains(&value),
            Self::Variants(variants) => variants.iter().any(|id| i128::from(*id) == value),
            Self::Unwritten => {
                return Err(format!(
                    "{name} has no number form, so `{value}` cannot be written for it"
                ));
            }
        };
        if admitted {
            Ok(value)
        } else {
            Err(format!("`{value}` is outside the domain of {name}"))
        }
    }
}

/// One program input: a root field, or a whole scalar root.
#[derive(Debug)]
struct Input {
    root: &'static str,
    field: Option<u16>,
    domain: Domain,
}

impl Input {
    /// `state field 120`, or `command` for a scalar root; `prefix` names the
    /// successor's fields `post-state field 120`.
    fn name(&self, prefix: &str) -> String {
        let root = if prefix.is_empty() { self.root } else { prefix };
        match self.field {
            Some(field) => format!("{root} field {field}"),
            None => root.to_owned(),
        }
    }
}

/// What a contract declares that example lines are checked against.
#[derive(Debug)]
pub(crate) struct Shape {
    /// Every input, in program order; the state fields come first.
    inputs: Vec<Input>,
    /// How many of the inputs are state fields.
    state: usize,
    /// Each declared channel's ID and its payload fields' IDs and domains.
    channels: Vec<(u32, Vec<(u16, Domain)>)>,
}

impl Shape {
    /// The shape the contract's descriptor declares.
    ///
    /// # Errors
    /// Returns a root whose schema is neither a record nor one value.
    pub(crate) fn of(descriptor: &c::Descriptor<'_>) -> Result<Self, String> {
        let mut inputs = Vec::new();
        let mut state = 0;
        for (root, schema) in [
            ("state", descriptor.state),
            ("command", descriptor.command),
            ("context", descriptor.context),
        ] {
            match schema {
                c::Schema::Record(fields) => inputs.extend(fields.iter().map(|field| Input {
                    root,
                    field: Some(field.id),
                    domain: Domain::of_leaf(&field.leaf),
                })),
                c::Schema::Leaf(leaf) => inputs.push(Input {
                    root,
                    field: None,
                    domain: Domain::of_leaf(leaf),
                }),
                other => return Err(format!("unsupported {root} schema {other:?}")),
            }
            if root == "state" {
                state = inputs.len();
            }
        }
        let channels = descriptor
            .channels
            .iter()
            .map(|channel| {
                (
                    channel.id,
                    channel
                        .payload
                        .iter()
                        .map(|field| (field.field, Domain::of_payload(&field.domain)))
                        .collect(),
                )
            })
            .collect();
        Ok(Self {
            inputs,
            state,
            channels,
        })
    }

    /// Checks `value` against the declared domain of input `index`: the
    /// state fields, then the command, then the context, in program order.
    ///
    /// # Errors
    /// Returns an index past the inputs, or a value outside the domain.
    pub(crate) fn check_input(&self, index: usize, value: i128) -> Result<i128, String> {
        let input = self
            .inputs
            .get(index)
            .ok_or_else(|| format!("the contract reads {} inputs", self.inputs.len()))?;
        input.domain.check(value, &input.name(""))
    }
}

/// Parses every example line of `text`.
///
/// # Errors
/// Returns the first malformed line, or a number outside its domain.
pub(crate) fn parse(text: &str, shape: &Shape) -> Result<Vec<Line>, Error> {
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let number = index + 1;
        lines.push(example(line, number, shape).map_err(|reason| Error {
            line: number,
            reason,
        })?);
    }
    Ok(lines)
}

fn example(line: &str, number: usize, shape: &Shape) -> Result<Line, String> {
    let sections: Vec<&str> = line.split('|').map(str::trim).collect();
    let (input_sections, decision, deliveries) = match sections.as_slice() {
        [input_sections @ .., decision, deliveries] if !input_sections.is_empty() => {
            (input_sections, *decision, *deliveries)
        }
        _ => return Err("expected `inputs | class reason post | deliveries`".to_owned()),
    };
    let inputs = numbers(&input_sections.join(" "))?;
    if inputs.len() != shape.inputs.len() {
        return Err(format!(
            "{} input numbers; the contract reads {}",
            inputs.len(),
            shape.inputs.len()
        ));
    }
    for (index, value) in inputs.iter().enumerate() {
        shape.check_input(index, *value)?;
    }
    let mut words = decision.split_whitespace();
    let class = match words.next() {
        Some("accept") => Class::Accept,
        Some("reject") => Class::Reject,
        Some("failure") => Class::Failure,
        _ => return Err("the decision starts with accept, reject or failure".to_owned()),
    };
    let reason = match words.next() {
        Some("-") => None,
        Some(word) => Some(
            word.parse::<u32>()
                .map_err(|_| format!("`{word}` is not a reason"))?,
        ),
        None => return Err("missing reason".to_owned()),
    };
    let post = numbers(&words.collect::<Vec<_>>().join(" "))?;
    if post.len() != shape.state {
        return Err(format!(
            "{} post-state numbers; the state has {} fields",
            post.len(),
            shape.state
        ));
    }
    for (value, input) in post.iter().zip(&shape.inputs) {
        input.domain.check(*value, &input.name("post-state"))?;
    }
    let outbox = if deliveries == "-" {
        Vec::new()
    } else {
        deliveries
            .split(';')
            .map(|delivery| self::delivery(&numbers(delivery)?, &shape.channels))
            .collect::<Result<_, _>>()?
    };
    Ok(Line {
        number,
        inputs,
        class,
        reason,
        post,
        outbox,
    })
}

fn numbers(text: &str) -> Result<Vec<i128>, String> {
    text.split_whitespace()
        .map(|word| {
            word.parse::<i128>()
                .map_err(|_| format!("`{word}` is not a number"))
        })
        .collect()
}

/// A declared channel followed by its payload numbers, or, when exactly one
/// channel is declared, its payload numbers alone.
fn delivery(
    numbers: &[i128],
    channels: &[(u32, Vec<(u16, Domain)>)],
) -> Result<(u32, Vec<i128>), String> {
    let payload = |id: u32, fields: &[(u16, Domain)], values: &[i128]| {
        values
            .iter()
            .zip(fields)
            .map(|(value, (field, domain))| {
                domain.check(*value, &format!("channel {id} field {field}"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|values| (id, values))
    };
    if let Some((first, values)) = numbers.split_first()
        && let Some((id, domains)) = channels.iter().find(|(id, _)| i128::from(*id) == *first)
        && domains.len() == values.len()
    {
        return payload(*id, domains, values);
    }
    if let [(id, domains)] = channels
        && !numbers.is_empty()
        && domains.len() == numbers.len()
    {
        return payload(*id, domains, numbers);
    }
    Err(
        "a delivery is a declared channel followed by its payload numbers, or the payload numbers alone when one channel is declared"
            .to_owned(),
    )
}
