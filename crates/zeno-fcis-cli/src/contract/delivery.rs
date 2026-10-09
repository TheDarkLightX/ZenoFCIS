//! Bounded delivery observations compiled to the existing library law graph.
//! No candidate is predicted here: every observation reads the actual frame.

use std::collections::BTreeMap;

use super::declarations::{Declarations, Kind as TypeKind};
use super::graph::{Atom, Graph, Kind, LawGraph, Observation, Op, Ref};
use super::rules::{DeliveryLaw, Rules};

/// Bind each declared projection and return the mandatory length check.
/// Eager law evaluation makes guards on the observations themselves essential.
pub(super) fn bind(
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
    let within = graph.op(Op::Not(over.index), Kind::Bool)?;
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
                    TypeKind::I128 { .. }
                ) {
                    return Err(format!(
                        "channel {} payload field {field} must be an I128 integer",
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
        let selected_channel = graph.int(i128::from(channel.id))?;
        let zero = graph.int(0)?;
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
            let matches = graph.op(Op::And(exists.index, equal.index), Kind::Bool)?;
            let amount = match observation.field {
                None => graph.op(Op::Select(matches.index, one.index, zero.index), Kind::Int)?,
                // Keep the raw atom. ToI128 would coerce Bool, Sum or U128;
                // checked I128 Add instead refuses every wrong active type.
                Some(field) => graph.observe(
                    Observation::OutboxPayload(index, field),
                    Kind::Raw,
                    Some(matches),
                    Atom::I128(0),
                )?,
            };
            total = graph.op(Op::Add(total.index, amount.index), Kind::Int)?;
        }
        graph.bind(name.clone(), total);
    }
    Ok(within)
}

#[cfg(test)]
mod tests;
