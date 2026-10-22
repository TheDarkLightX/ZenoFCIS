//! Expand table forms during rules loading, before ordinary scalar validation.
use super::{ContractError, Json, parse};
use crate::internals::expr::{self, TableDeclarations};
use std::collections::BTreeSet;

pub(super) fn expand(project: &str, source: &str) -> Result<String, ContractError> {
    let mut remaining = 1usize << 20;
    if source.len() > remaining {
        return Err(ContractError::new(
            "v2/policy.json",
            "table source exceeds text budget",
        ));
    }
    let declarations = TableDeclarations::read(project)
        .map_err(|reason| ContractError::new("project.zeno", reason))?;
    let mut json = parse(source)?;
    if let Some(Json::Object(variables)) = member(&mut json, "variables") {
        for (name, value) in variables {
            expression(
                value,
                &declarations,
                &format!("variables.{name}"),
                &mut remaining,
            )?;
        }
    }
    if let Some(Json::Array(cases)) = member(&mut json, "cases") {
        for (index, case) in cases.iter_mut().enumerate() {
            let place = format!("cases[{index}]");
            if let Some(when) = member(case, "when") {
                expression(
                    when,
                    &declarations,
                    &format!("{place}.when"),
                    &mut remaining,
                )?;
            }
            if let Some(Json::Object(fields)) = member(case, "post") {
                let mut expanded = Vec::new();
                let mut seen = BTreeSet::new();
                for (key, mut value) in std::mem::take(fields) {
                    let place = format!("{place}.post.{key}");
                    expression(&mut value, &declarations, &place, &mut remaining)?;
                    let writes = if key.contains('[') {
                        let ast = super::expression(&value, &place)?;
                        declarations
                            .writes(&key, ast)
                            .map_err(|reason| ContractError::new(&place, reason))?
                            .into_iter()
                            .map(|(field, ast)| (field.to_string(), Json::Text(expr::render(&ast))))
                            .collect()
                    } else {
                        vec![(key, value)]
                    };
                    for (key, value) in writes {
                        if let Json::Text(text) = &value {
                            charge(&mut remaining, text.len(), &place)?;
                        }
                        if !seen.insert(key.clone()) {
                            return Err(ContractError::new(
                                &place,
                                format!("overlapping write to field {key}"),
                            ));
                        }
                        expanded.push((key, value));
                    }
                }
                *fields = expanded;
            }
            if let Some(Json::Array(outbox)) = member(case, "outbox") {
                for delivery in outbox {
                    if let Some(Json::Object(payload)) = member(delivery, "payload") {
                        for (_, value) in payload {
                            expression(
                                value,
                                &declarations,
                                &format!("{place}.outbox.payload"),
                                &mut remaining,
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(json.render())
}

fn member<'a>(json: &'a mut Json, name: &str) -> Option<&'a mut Json> {
    let Json::Object(entries) = json else {
        return None;
    };
    entries
        .iter_mut()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}
fn expression(
    value: &mut Json,
    tables: &TableDeclarations,
    place: &str,
    remaining: &mut usize,
) -> Result<(), ContractError> {
    if let Json::Text(text) = value {
        let ast = expr::parse_tables(text, tables)
            .map_err(|reason| ContractError::new(format!("v2/policy.json {place}"), reason))?;
        *text = expr::render(&ast);
        charge(remaining, text.len(), place)?;
    }
    Ok(())
}

fn charge(remaining: &mut usize, bytes: usize, place: &str) -> Result<(), ContractError> {
    *remaining = remaining
        .checked_sub(bytes)
        .ok_or_else(|| ContractError::new(place, "table rule expansion exceeds text budget"))?;
    Ok(())
}
