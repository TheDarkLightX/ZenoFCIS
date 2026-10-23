//! Bounded delivery observations compiled to the existing library law graph.
//! No candidate is predicted here: every observation reads the actual frame.

use std::collections::BTreeMap;

use super::declarations::{Declarations, Kind as TypeKind};
use super::graph::{Atom, Graph, Kind, LawGraph, Observation, Op, Ref};
use super::rules::{DeliveryLaw, Rules};

/// Bind each declared projection and return the mandatory length check.
/// Eager law evaluation makes guards on the observations themselves essential.
pub fn bind(
    graph: &mut LawGraph<'_>,
    declarations: &Declarations,
    rules: &Rules,
    law: &DeliveryLaw,
) -> Result<Ref, String> {
    let maximum = rules
        .cases
        .iter()
        .map(|case| case.outbox.len())
        .max()
        .unwrap_or(0);
    let bound =
        usize::try_from(law.max_deliveries).map_err(|_| "delivery bound does not fit usize")?;
    if maximum > bound {
        return Err(format!(
            "max_deliveries {} is below the policy's maximum {maximum}",
            law.max_deliveries
        ));
    }
    let length = graph.observe(Observation::OutboxLength, Kind::Int, None, Atom::I128(0))?;
    let limit = graph.int(i128::from(law.max_deliveries))?;
    let over = graph.op(Op::Lt(limit.index, length.index), Kind::Bool)?;
    let mut within = graph.op(Op::Not(over.index), Kind::Bool)?;
    let mut payload_channels = BTreeMap::new();
    for (name, observation) in &law.observations {
        let channel = declarations
            .channel(observation.channel)
            .map_err(|error| error.to_string())?;
        if let Some(previous) = payload_channels.insert(channel.payload, channel.id)
            && previous != channel.id
        {
            return Err(format!(
                "payload type {} names multiple channels in one law; use separate laws",
                channel.payload
            ));
        }
        let expected = match observation.field {
            None => format!("outbox.{}", channel.payload),
            Some(field) => {
                let fields = declarations
                    .fields(channel.payload)
                    .map_err(|error| error.to_string())?;
                let declared = fields
                    .iter()
                    .find(|declared| declared.id == field)
                    .ok_or_else(|| {
                        format!("channel {} payload has no field {field}", channel.id)
                    })?;
                if !matches!(
                    declarations
                        .kind(declared.type_id)
                        .map_err(|error| error.to_string())?,
                    TypeKind::I128 { .. } | TypeKind::U128 { .. } | TypeKind::Sum(_)
                ) {
                    return Err(format!(
                        "channel {} payload field {field} must be an integer or a declared variant",
                        channel.id
                    ));
                }
                format!("outbox.{}.{field}", channel.payload)
            }
        };
        if name != &expected {
            return Err(format!(
                "delivery observation `{name}` must be `{expected}` for channel {}",
                channel.id
            ));
        }
        let unsigned = match observation.field {
            Some(id) => declarations
                .fields(channel.payload)
                .ok()
                .and_then(|fields| fields.iter().find(|f| f.id == id))
                .and_then(|f| declarations.kind(f.type_id).ok())
                .is_some_and(|k| matches!(k, TypeKind::U128 { .. })),
            None => false,
        };
        let sum_type = observation.field.and_then(|id| {
            declarations
                .fields(channel.payload)
                .ok()?
                .iter()
                .find(|f| f.id == id)
                .and_then(|f| match declarations.kind(f.type_id).ok()? {
                    TypeKind::Sum(variants) => Some((f.type_id, variants)),
                    _ => None,
                })
        });
        let kind = if unsigned { Kind::Unsigned } else { Kind::Int };
        let selected_channel = graph.int(i128::from(channel.id))?;
        let zero = if unsigned {
            graph.uint(0)
        } else {
            graph.int(0)?
        };
        let one = graph.int(1)?;
        let mut total = zero;
        // Even a zero bound retains an inactive observation in the canonical
        // policy, binding the selected payload field ID as well as its channel.
        for index in 0..bound.max(1) {
            let ordinal = graph.int(index as i128)?;
            let exists = if bound == 0 {
                graph.bool(false)
            } else {
                graph.op(Op::Lt(ordinal.index, length.index), Kind::Bool)?
            };
            let actual_channel = graph.observe(
                Observation::OutboxChannel(index),
                Kind::Raw,
                Some(exists),
                Atom::I128(0),
            )?;
            let equal = graph.op(
                Op::Eq(actual_channel.index, selected_channel.index),
                Kind::Bool,
            )?;
            let mut matches = graph.op(Op::And(exists.index, equal.index), Kind::Bool)?;
            if let Some(destination) = &observation.destination {
                let actual = graph.observe(
                    Observation::OutboxDestination(index),
                    Kind::Raw,
                    Some(matches),
                    Atom::Text(Vec::new()),
                )?;
                let expected = graph.literal(Atom::Text(destination.as_bytes().to_vec()));
                let same = graph.op(Op::Eq(actual.index, expected.index), Kind::Bool)?;
                matches = graph.op(Op::And(matches.index, same.index), Kind::Bool)?;
            }
            let amount = match observation.field {
                None => graph.op(Op::Select(matches.index, one.index, zero.index), Kind::Int)?,
                // Keep the raw atom. ToI128 would coerce Bool, Sum or U128;
                // checked I128 Add instead refuses every wrong active type.
                Some(field) if sum_type.is_some() => {
                    let (type_id, variants) = sum_type.ok_or("missing variant declaration")?;
                    let raw = graph.observe(
                        Observation::OutboxPayload(index, field),
                        Kind::Raw,
                        Some(matches),
                        Atom::I128(0),
                    )?;
                    let mut value = graph.int(0)?;
                    let mut known = graph.bool(false);
                    for variant in variants {
                        let expected = graph.literal(Atom::Sum {
                            type_id,
                            variant: variant.id,
                        });
                        let same = graph.op(Op::Eq(raw.index, expected.index), Kind::Bool)?;
                        let number = graph.int(i128::from(variant.id))?;
                        value = graph
                            .op(Op::Select(same.index, number.index, value.index), Kind::Int)?;
                        let yes = graph.bool(true);
                        known =
                            graph.op(Op::Select(same.index, yes.index, known.index), Kind::Bool)?;
                    }
                    // No coercions: active Bool/I128/unknown variants cannot impersonate a sum.
                    let yes = graph.bool(true);
                    let valid = graph.op(
                        Op::Select(matches.index, known.index, yes.index),
                        Kind::Bool,
                    )?;
                    within = graph.op(Op::And(within.index, valid.index), Kind::Bool)?;
                    value
                }
                Some(field) => graph.observe(
                    Observation::OutboxPayload(index, field),
                    Kind::Raw,
                    Some(matches),
                    if unsigned {
                        Atom::U128(0)
                    } else {
                        Atom::I128(0)
                    },
                )?,
            };
            total = graph.op(Op::Add(total.index, amount.index), kind)?;
        }
        graph.bind(name.clone(), total);
    }
    Ok(within)
}
