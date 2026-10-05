//! The owner's reviewed decision examples: read in the format every
//! application's `tests/decision-examples.txt` uses, compared with the
//! library's decision, and written back as proposed examples.
//!
//! A line is `inputs | class reason post | deliveries`. The inputs may be
//! split over several `|` sections. The class is `accept`, `reject` or
//! `failure`; the reason is a number or `-`; a reject repeats the pre-state
//! as its post-state. Deliveries are `-`, or `;`-separated deliveries, each a
//! declared channel followed by its payload values, or the payload values
//! alone when the contract declares one channel.

use super::super::ContractError;
use super::super::declarations::{Declarations, Source};
use super::super::rules::Class;
use super::domain::{LeafDomain, Position, leaf_domain};
use super::evaluate::{Decision, Outcome};

pub(super) const FILE: &str = "tests/decision-examples.txt";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Example {
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) inputs: Vec<i64>,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) post: Vec<i64>,
    /// Each delivery's channel and payload values.
    pub(super) outbox: Vec<(u32, Vec<i64>)>,
}

/// Parses every example line; comments start with `#`.
///
/// # Errors
/// Returns the first malformed line, or a value outside its domain.
pub(super) fn parse(
    text: &str,
    positions: &[Position],
    declarations: &Declarations,
) -> Result<Vec<Example>, ContractError> {
    let channels: Vec<(u32, Vec<LeafDomain>)> = declarations
        .channels
        .iter()
        .map(|channel| {
            Ok((
                channel.id,
                declarations
                    .fields(channel.payload)?
                    .iter()
                    .map(|field| leaf_domain(declarations, field.type_id))
                    .collect::<Result<_, _>>()?,
            ))
        })
        .collect::<Result<_, ContractError>>()?;
    let state_width = positions
        .iter()
        .filter(|position| position.source == Source::State)
        .count();
    let mut examples = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let error = |what: String| ContractError::new(format!("{FILE} line {number}"), what);
        let sections: Vec<&str> = line.split('|').map(str::trim).collect();
        if sections.len() < 3 {
            return Err(error(
                "expected `inputs | decision | deliveries`".to_owned(),
            ));
        }
        let (decision, deliveries) = (sections[sections.len() - 2], sections[sections.len() - 1]);
        let inputs = numbers(&sections[..sections.len() - 2].join(" ")).map_err(&error)?;
        if inputs.len() != positions.len() {
            return Err(error(format!(
                "{} input numbers; the contract reads {}",
                inputs.len(),
                positions.len()
            )));
        }
        let inputs = inputs
            .iter()
            .zip(positions)
            .map(|(value, position)| {
                if !position.domain.contains(*value) {
                    return Err(error(format!(
                        "`{value}` is outside the domain of `{}`",
                        position.name
                    )));
                }
                i64::try_from(*value).map_err(|_| error(format!("`{value}` is too large")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut words = decision.split_whitespace();
        let class = match words.next() {
            Some("accept") => Class::Accept,
            Some("reject") => Class::Reject,
            Some("failure") => Class::CommittedFailure,
            _ => {
                return Err(error(
                    "the decision starts with accept, reject or failure".to_owned(),
                ));
            }
        };
        let reason = match words.next() {
            Some("-") => None,
            Some(word) => Some(
                word.parse::<u32>()
                    .map_err(|_| error(format!("`{word}` is not a reason")))?,
            ),
            None => return Err(error("missing reason".to_owned())),
        };
        let post = words
            .map(|word| {
                word.parse::<i64>()
                    .map_err(|_| error(format!("`{word}` is not a post-state number")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if post.len() != state_width {
            return Err(error(format!(
                "{} post-state numbers; the state has {state_width} fields",
                post.len()
            )));
        }
        let outbox = if deliveries == "-" {
            Vec::new()
        } else {
            deliveries
                .split(';')
                .map(|delivery| {
                    let numbers = numbers(delivery).map_err(&error)?;
                    self::delivery(&numbers, &channels).map_err(&error)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        examples.push(Example {
            line: number,
            text: line.to_owned(),
            inputs,
            class,
            reason,
            post,
            outbox,
        });
    }
    Ok(examples)
}

fn numbers(text: &str) -> Result<Vec<i128>, String> {
    text.split_whitespace()
        .map(|word| {
            word.parse::<i128>()
                .map_err(|_| format!("`{word}` is not a number"))
        })
        .collect()
}

/// A delivery's channel and payload values: a declared channel followed by
/// its payload, or the payload alone when one channel is declared.
fn delivery(
    numbers: &[i128],
    channels: &[(u32, Vec<LeafDomain>)],
) -> Result<(u32, Vec<i64>), String> {
    let payload = |id: u32, domains: &[LeafDomain], values: &[i128]| {
        values
            .iter()
            .zip(domains)
            .map(|(value, domain)| {
                if domain.contains(*value) {
                    i64::try_from(*value).map_err(|_| format!("`{value}` is too large"))
                } else {
                    Err(format!(
                        "payload value `{value}` is outside its domain on channel {id}"
                    ))
                }
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|values| (id, values))
    };
    if let Some((channel, values)) = numbers.split_first()
        && let Ok(id) = u32::try_from(*channel)
        && let Some((_, domains)) = channels.iter().find(|(declared, _)| *declared == id)
        && domains.len() == values.len()
    {
        return payload(id, domains, values);
    }
    if let [(id, domains)] = channels
        && domains.len() == numbers.len()
    {
        return payload(*id, domains, numbers);
    }
    Err(
        "a delivery is a declared channel followed by its payload values, or the payload values of the only channel"
            .to_owned(),
    )
}

/// The first way a library outcome differs from the example, if any.
/// Deliveries compare by channel and payload values, as the examples write
/// them.
///
/// # Errors
/// Returns the difference.
pub(super) fn agrees(
    example: &Example,
    outcome: &Outcome,
    state_width: usize,
) -> Result<(), String> {
    let decision = match outcome {
        Outcome::Decision(decision) => decision,
        Outcome::Refused(refusal) => return Err(format!("the library refused: {refusal}")),
    };
    if decision.class != example.class {
        return Err(format!(
            "class differs: the example says {}, the library {}",
            example.class.name(),
            decision.class.name()
        ));
    }
    if decision.reason != example.reason {
        return Err(format!(
            "reason differs: the example says {}, the library {}",
            text(example.reason),
            text(decision.reason)
        ));
    }
    if example.class == Class::Reject {
        if example.post != example.inputs[..state_width.min(example.inputs.len())] {
            return Err("the rejected post-state differs from the pre-state".to_owned());
        }
        if !decision.post.is_empty() {
            return Err("the library published a successor for a reject".to_owned());
        }
    } else {
        let post: Option<Vec<i128>> = decision
            .post
            .iter()
            .map(|(_, value)| value.number())
            .collect();
        let expected: Vec<i128> = example
            .post
            .iter()
            .map(|value| i128::from(*value))
            .collect();
        if post.as_deref() != Some(expected.as_slice()) {
            return Err(format!(
                "post-state differs: the example says {:?}, the library {:?}",
                example.post,
                post.unwrap_or_default()
            ));
        }
    }
    if decision.outbox.len() != example.outbox.len() {
        return Err(format!(
            "delivery count differs: the example says {}, the library {}",
            example.outbox.len(),
            decision.outbox.len()
        ));
    }
    for (number, (delivery, (channel, payload))) in
        decision.outbox.iter().zip(&example.outbox).enumerate()
    {
        if delivery.channel != *channel {
            return Err(format!(
                "delivery {number} channel differs: the example says {channel}, the library {}",
                delivery.channel
            ));
        }
        let values: Option<Vec<i128>> = delivery
            .payload
            .iter()
            .map(|(_, value)| value.number())
            .collect();
        let expected: Vec<i128> = payload.iter().map(|value| i128::from(*value)).collect();
        if values.as_deref() != Some(expected.as_slice()) {
            return Err(format!(
                "delivery {number} payload differs: the example says {payload:?}, the library {:?}",
                values.unwrap_or_default()
            ));
        }
    }
    Ok(())
}

fn text(reason: Option<u32>) -> String {
    reason.map_or_else(|| "-".to_owned(), |reason| reason.to_string())
}

/// The example line a decision makes for `inputs`, in the format above; a
/// reject repeats the pre-state. `None` when a value has no number form.
pub(super) fn render(inputs: &[i64], decision: &Decision, state_width: usize) -> Option<String> {
    let class = match decision.class {
        Class::Accept => "accept",
        Class::Reject => "reject",
        Class::CommittedFailure => "failure",
    };
    let post: Vec<String> = if decision.class == Class::Reject {
        inputs[..state_width.min(inputs.len())]
            .iter()
            .map(ToString::to_string)
            .collect()
    } else {
        decision
            .post
            .iter()
            .map(|(_, value)| value.number().map(|number| number.to_string()))
            .collect::<Option<_>>()?
    };
    let deliveries = if decision.outbox.is_empty() {
        "-".to_owned()
    } else {
        decision
            .outbox
            .iter()
            .map(|delivery| {
                let mut words = vec![delivery.channel.to_string()];
                for (_, value) in &delivery.payload {
                    words.push(value.number()?.to_string());
                }
                Some(words.join(" "))
            })
            .collect::<Option<Vec<_>>>()?
            .join("; ")
    };
    Some(format!(
        "{} | {class} {} {} | {deliveries}",
        join(inputs),
        text(decision.reason),
        post.join(" ")
    ))
}

/// Numbers separated by one space.
pub(super) fn join(values: &[i64]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
