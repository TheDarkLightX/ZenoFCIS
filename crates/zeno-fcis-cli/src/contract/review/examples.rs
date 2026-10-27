//! The owner's reviewed decision examples: read with the decision-examples
//! grammar every application built from a contract compiles, compared with
//! the library's decision, and written back as proposed examples.
//!
//! The grammar is `contract-app/src/examples.rs`, the file `zeno-fcis new
//! --contract` copies into each application, compiled here unchanged, so a
//! file the review reads is a file the application reads alike. A line is
//! `inputs | class reason post | deliveries`; the module documentation of
//! the grammar gives every rule.

use zeno_fcis_synthesis::finite::v2_composition as c;

use super::super::ContractError;
use super::super::rules::Class;
use super::evaluate::{Decision, Outcome};

#[path = "../../../contract-app/src/examples.rs"]
mod grammar;

pub(super) const FILE: &str = "tests/decision-examples.txt";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Example {
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) inputs: Vec<i128>,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) post: Vec<i128>,
    /// Each delivery's channel and payload values.
    pub(super) outbox: Vec<(u32, Vec<i128>)>,
    pub(super) ordinals: Vec<Option<(u32, u128)>>,
    pub(super) expected_refusal: Option<String>,
}

/// Parses every example line against the shape the contract's descriptor
/// declares, as the application does.
///
/// # Errors
/// Returns the first malformed line, or a value outside its domain.
pub(super) fn parse(
    text: &str,
    descriptor: &c::Descriptor<'_>,
) -> Result<Vec<Example>, ContractError> {
    let shape =
        grammar::Shape::of(descriptor).map_err(|reason| ContractError::new(FILE, reason))?;
    // Technical refusals are scenario observations, rather than decisions
    // an application may commit. The ordinary numeric shape still validates
    // their inputs and unchanged state.
    let mut refusals = std::collections::BTreeMap::new();
    let normalized = text
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if line.trim_start().starts_with('#') {
                return line.to_owned();
            }
            let mut parts = line.split('|').map(str::to_owned).collect::<Vec<_>>();
            if parts.len() >= 3 {
                let at = parts.len() - 2;
                let words = parts[at].split_whitespace().collect::<Vec<_>>();
                if words.first() == Some(&"refused") && words.len() >= 2 {
                    refusals.insert(index + 1, words[1].to_owned());
                    parts[at] = format!(" reject - {} ", words[2..].join(" "));
                    return parts.join("|");
                }
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    if refusals.values().any(|name| {
        !super::evaluate::RefusalClass::ALL
            .iter()
            .any(|c| c.name() == name)
    }) {
        return Err(ContractError::new(FILE, "unknown refusal class"));
    }
    let lines = grammar::parse(&normalized, &shape)
        .map_err(|error| ContractError::new(format!("{FILE} line {}", error.line), error.reason))?;
    let texts: Vec<&str> = text.lines().collect();
    lines
        .into_iter()
        .map(|line| {
            // Every input domain is within the 64-bit program range.
            let inputs = line
                .inputs
                .iter()
                .map(|value| Ok(*value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Example {
                line: line.number,
                text: texts
                    .get(line.number - 1)
                    .copied()
                    .unwrap_or_default()
                    .to_owned(),
                inputs,
                class: match line.class {
                    grammar::Class::Accept => Class::Accept,
                    grammar::Class::Reject => Class::Reject,
                    grammar::Class::Failure => Class::CommittedFailure,
                },
                reason: line.reason,
                post: line.post,
                outbox: line.outbox,
                ordinals: line.ordinals,
                expected_refusal: refusals.remove(&line.number),
            })
        })
        .collect()
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
    if let Some(class) = &example.expected_refusal {
        return if matches!(outcome, Outcome::Refused(r) if r.class.name() == class)
            && example.post == example.inputs[..state_width]
            && example.outbox.is_empty()
        {
            Ok(())
        } else {
            Err(format!(
                "expected technical refusal {class} with unchanged state and no deliveries"
            ))
        };
    }
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
        let pre = example.inputs[..state_width.min(example.inputs.len())]
            .iter()
            .copied();
        if !example.post.iter().copied().eq(pre) {
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
        if post.as_deref() != Some(example.post.as_slice()) {
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
        if let Some((ordinal, idempotency)) = example.ordinals.get(number).copied().flatten() {
            if delivery.ordinal != ordinal {
                return Err(format!("delivery {number} ordinal differs"));
            }
            if delivery.idempotency != super::evaluate::Value::U128(idempotency) {
                return Err(format!("delivery {number} idempotency ordinal differs"));
            }
        }
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
        if values.as_deref() != Some(payload.as_slice()) {
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
pub(super) fn render(inputs: &[i128], decision: &Decision, state_width: usize) -> Option<String> {
    render_with_ordinals(inputs, decision, state_width, false)
}

pub(super) fn render_exact(
    inputs: &[i128],
    decision: &Decision,
    state_width: usize,
) -> Option<String> {
    render_with_ordinals(inputs, decision, state_width, true)
}

fn render_with_ordinals(
    inputs: &[i128],
    decision: &Decision,
    state_width: usize,
    exact: bool,
) -> Option<String> {
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
                let idempotency = match delivery.idempotency {
                    super::evaluate::Value::U128(n) => n,
                    _ => return None,
                };
                let mut words = vec![if exact {
                    format!("{}@{}:{}", delivery.channel, delivery.ordinal, idempotency)
                } else {
                    delivery.channel.to_string()
                }];
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
pub(super) fn join(values: &[i128]) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}
