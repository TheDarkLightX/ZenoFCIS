//! Bounded observers paired with the actual finite contract machine. Both
//! state lemmas and run predicates use lemma::induction; no solver or shell
//! behavior is invented here.
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::super::ContractError;
use super::{
    lemma,
    machine::{self, Loaded},
    observations::{self, Expression},
};

pub(crate) const FILE: &str = "v2/run-properties.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    schema: String,
    #[serde(default)]
    observer: BTreeMap<String, Variable>,
    properties: Vec<Property>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Variable {
    min: i128,
    max: i128,
    initial: i128,
    update: String,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Property {
    Predicate {
        name: String,
        formula: String,
    },
    AtMostOnce {
        name: String,
        channel: u32,
        key_field: Option<u16>,
    },
    NeverAfter {
        name: String,
        a: u32,
        b: u32,
    },
}

impl Property {
    fn name(&self) -> &str {
        match self {
            Self::Predicate { name, .. }
            | Self::AtMostOnce { name, .. }
            | Self::NeverAfter { name, .. } => name,
        }
    }
}

fn error(reason: impl Into<String>) -> ContractError {
    ContractError::new(FILE, reason)
}

pub(crate) fn judge(
    project: &str,
    rules: &str,
    text: &str,
    max_tuples: u64,
) -> Result<lemma::Lemmas, ContractError> {
    let strict =
        crate::loop_command::doors::capture::strict_json(text.as_bytes()).map_err(error)?;
    let declaration: Declaration =
        serde_json::from_value(strict).map_err(|e| error(e.to_string()))?;
    if declaration.schema != "zeno-fcis/run-properties/1"
        || declaration.properties.is_empty()
        || declaration.properties.len() > 64
        || declaration.observer.len() > 32
    {
        return Err(error(
            "expected schema zeno-fcis/run-properties/1, 1..64 properties and at most 32 observer fields",
        ));
    }
    let loaded = Loaded::read(project, rules)?;
    let observation = match machine::observe_contract(&loaded, max_tuples)? {
        Ok(o) => o,
        Err(stop) => return Ok(lemma::Lemmas::Inconclusive(stop)),
    };
    if let Err(refusal) = &observation.machine.starting_state {
        return Ok(lemma::Lemmas::NoStartingState(refusal.packet()));
    }
    let mut names = std::collections::BTreeSet::new();
    let mut results = Vec::new();
    for property in &declaration.properties {
        if property.name().is_empty() || !names.insert(property.name()) {
            return Err(error("property names must be nonempty and unique"));
        }
        results.push(check(
            &loaded,
            &observation.machine,
            &declaration.observer,
            property,
            max_tuples,
        )?);
    }
    Ok(lemma::Lemmas::Judged(results))
}

fn check(
    loaded: &Loaded,
    machine: &machine::Machine,
    declared: &BTreeMap<String, Variable>,
    property: &Property,
    max_tuples: u64,
) -> Result<Value, ContractError> {
    let mut variables = declared
        .iter()
        .map(|(n, v)| {
            (
                n.clone(),
                Variable {
                    min: v.min,
                    max: v.max,
                    initial: v.initial,
                    update: v.update.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut keyed = Vec::new();
    let counter = |update: String| Variable {
        min: 0,
        max: 2,
        initial: 0,
        update,
    };
    let channel = |id| {
        loaded
            .declarations
            .channel(id)
            .map_err(|e| error(e.to_string()))
    };
    let formula = match property {
        Property::Predicate { formula, .. } => formula.clone(),
        Property::AtMostOnce {
            channel: id,
            key_field,
            ..
        } => {
            let c = channel(*id)?;
            let counts = if let Some(field) = key_field {
                let f = loaded
                    .declarations
                    .fields(c.payload)?
                    .iter()
                    .find(|f| f.id == *field)
                    .ok_or_else(|| error("unknown key field"))?;
                let domain = super::domain::leaf_domain(&loaded.declarations, f.type_id)?;
                let values = domain
                    .values_within(max_tuples.min(64))
                    .ok_or_else(|| error("key field domain exceeds 64 values"))?;
                values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let name = format!("keycount{index}");
                        keyed.push((name.clone(), *id, *field, value));
                        name
                    })
                    .collect::<Vec<_>>()
            } else {
                vec![format!("outbox.{id}")]
            };
            let mut terms = Vec::new();
            for (index, count) in counts.iter().enumerate() {
                let name = format!("_once{index}");
                variables.insert(
                    name.clone(),
                    counter(format!(
                        "choose(before.{name} + {count} > 1, 2, before.{name} + {count})"
                    )),
                );
                terms.push(format!("observer.{name} <= 1"));
            }
            terms.join(" && ")
        }
        Property::NeverAfter { a, b, .. } => {
            channel(*a)?;
            channel(*b)?;
            variables.insert(
                "_seen".into(),
                Variable {
                    min: 0,
                    max: 1,
                    initial: 0,
                    update: format!("choose(before._seen == 1 || outbox.{a} > 0, 1, 0)"),
                },
            );
            format!("!(before._seen == 1 && outbox.{b} > 0)")
        }
    };
    let mut size = Some(machine.space.tuples() as u128);
    let mut names = observations::names(loaded);
    for (name, v) in &variables {
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || v.min > v.max
            || !(v.min..=v.max).contains(&v.initial)
            || (declared.contains_key(name) && name.starts_with('_'))
        {
            return Err(error(
                "invalid observer name, bounds or initial value; underscore names are reserved",
            ));
        }
        size = size.and_then(|n| {
            v.min
                .abs_diff(v.max)
                .checked_add(1)
                .and_then(|m| n.checked_mul(m))
        });
        names.insert(format!("before.{name}"), false);
        names.insert(format!("observer.{name}"), false);
    }
    if size.is_none_or(|n| n > u128::from(max_tuples)) {
        return Ok(
            json!({"name":property.name(), "verdict":"inconclusive", "reason":"observer-product-over-limit", "size":size.map(|n| n.to_string()), "limit":max_tuples}),
        );
    }
    for (name, ..) in &keyed {
        names.insert(name.clone(), false);
    }
    // Updates are simultaneous: they may read before.*, but never the next
    // observer. Predicates may read both views and the complete decision.
    let mut update_names = names.clone();
    update_names.retain(|n, _| !n.starts_with("observer."));
    let updates = variables
        .values()
        .map(|v| Expression::compile(&v.update, &update_names, false))
        .collect::<Result<Vec<_>, _>>()?;
    let predicate = Expression::compile(&formula, &names, true)?;
    let initial = variables.values().map(|v| v.initial).collect::<Vec<_>>();
    let bind = |values: &mut BTreeMap<String, i128>, before: &[i128], after: &[i128]| {
        for ((name, _), (old, new)) in variables.iter().zip(before.iter().zip(after)) {
            values.insert(format!("before.{name}"), *old);
            values.insert(format!("observer.{name}"), *new);
        }
    };
    let mut genesis_values = BTreeMap::new();
    for (p, value) in loaded
        .positions
        .iter()
        .zip(machine.space.state(machine.genesis))
    {
        genesis_values.insert(p.name.clone(), value);
        genesis_values.insert(p.name.replacen("pre.", "post.", 1), value);
    }
    for n in ["accept", "reject", "failure", "refused"] {
        genesis_values.insert(format!("verdict.{n}"), 0);
    }
    let _ = observations::deliveries(loaded, &[], &mut genesis_values);
    for (n, ..) in &keyed {
        genesis_values.insert(n.clone(), 0);
    }
    bind(&mut genesis_values, &initial, &initial);
    let initial_result = predicate.evaluate(&genesis_values);
    let mut undefined = initial_result.is_none();
    let (reached, failure) = lemma::induction(
        (machine.genesis, initial),
        initial_result == Some(1),
        machine.space.inputs,
        |(state, observer), input| {
            let ordinal = state * machine.space.inputs + input;
            let Some(mut values) = observations::values(loaded, machine, ordinal) else {
                undefined = true;
                return Ok(((*state, observer.clone()), false));
            };
            for (name, channel, field, key) in &keyed {
                let count = machine
                    .outbox_at(ordinal)
                    .iter()
                    .filter(|d| {
                        d.channel == *channel
                            && d.payload
                                .iter()
                                .any(|(id, v)| id == field && v.number() == Some(*key))
                    })
                    .count();
                values.insert(name.clone(), count as i128);
            }
            bind(&mut values, observer, observer);
            let next_observer = updates
                .iter()
                .map(|u| u.evaluate(&values))
                .collect::<Option<Vec<_>>>();
            let Some(next_observer) = next_observer else {
                undefined = true;
                return Ok(((*state, observer.clone()), false));
            };
            if !variables
                .values()
                .zip(&next_observer)
                .all(|(v, n)| (v.min..=v.max).contains(n))
            {
                undefined = true;
                return Ok(((*state, observer.clone()), false));
            }
            bind(&mut values, observer, &next_observer);
            let holds = predicate.evaluate(&values);
            undefined |= holds.is_none();
            Ok((
                (machine.of(*state, input).after(*state), next_observer),
                holds == Some(1),
            ))
        },
    )?;
    let mut result = json!({"name":property.name(), "formula":formula,
        "verdict":if undefined {"undefined"} else if failure.is_some() {"false"} else {"inductive"},
        "initiation":initial_result == Some(1), "reachable_product_states":reached,
        "declared_product_tuples":size.map(|n|n.to_string()),
        "scope":"reachable contract and observer states, exhaustive over the declared finite domains",
        "method":"initiation and reachable consecution by induction",
        "authority":"none"});
    if let Some(trace) = failure {
        result["trace_from_genesis"] = json!(
            trace
                .iter()
                .map(|i| crate::report_numbers::integers(&machine.space.input(*i)))
                .collect::<Vec<_>>()
        );
        result["scenario"] = json!(scenario(loaded, machine, &trace)?);
        result["shortest"] = json!(true);
    }
    Ok(result)
}

fn scenario(
    loaded: &Loaded,
    machine: &machine::Machine,
    trace: &[usize],
) -> Result<Vec<String>, ContractError> {
    loaded.bind(&loaded.rules, |authority, framer| {
        let mut state = machine.genesis;
        let mut lines = vec!["# scenario run-property-violation".into()];
        for input in trace {
            let tuple = machine.space.tuple(state, *input);
            match super::evaluate::evaluate(authority, framer, &tuple) {
                super::evaluate::Outcome::Decision(d) => lines.push(
                    super::examples::render_exact(&tuple, &d, loaded.state_width)
                        .ok_or_else(|| error("witness has no numeric scenario form"))?,
                ),
                super::evaluate::Outcome::Refused(r) => lines.push(format!(
                    "{} | refused {} {} | -",
                    super::examples::join(&tuple),
                    r.class.name(),
                    super::examples::join(&machine.space.state(state))
                )),
            }
            state = machine.of(state, *input).after(state);
        }
        Ok(lines)
    })
}

#[cfg(test)]
mod tests;
