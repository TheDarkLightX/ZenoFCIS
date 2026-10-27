//! Runtime compilation and exhaustive review of the inventory contract.
use super::{Result, checked};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{domain, evaluate::Framer},
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{v2_authority::Authority, v2_composition as c};
const PROJECT: &str = include_str!("../../apps/inventory-reservation/project.zeno");
const RULES: &str = include_str!("../../apps/inventory-reservation/v2/policy.json");
const EXAMPLES: &str = include_str!("../../apps/inventory-reservation/tests/decision-examples.txt");
fn mutated(id: &str) -> Result<String> {
    let mut rules: Value = checked(serde_json::from_str(RULES))?;
    match id {
        "none" => {}
        "m0002" => rules["cases"][1]["when"] = json!("action == 150 && quantity >= available"),
        "m0018" => {
            rules["cases"][3]["when"] = json!(
                "(action == 150 && reserved + quantity > 6) || ((action == 151 || action == 153) && available + quantity > 5)"
            );
        }
        "m0040" => rules["cases"][1]["when"] = json!("action == 150"),
        "m0049" => rules["cases"].as_array_mut().ok_or("cases")?.swap(1, 2),
        "m0054" => {
            rules["cases"][0]["reason"] = json!(201);
            rules["cases"][1]["reason"] = json!(200);
        }
        "m0061" => rules["cases"][6]["outbox"] = json!([]),
        _ => return Err("unknown mutant".into()),
    }
    Ok(rules.to_string())
}
fn atom(value: c::Atom<'_>) -> Value {
    match value {
        c::Atom::Bool(v) => json!(v),
        c::Atom::I128(v) => json!(v.to_string()),
        c::Atom::U128(v) => json!(v.to_string()),
        c::Atom::Text(v) => json!(String::from_utf8_lossy(v)),
        c::Atom::Sum { type_id, variant } | c::Atom::Enum { type_id, variant } => {
            json!({"type_id":type_id,"variant":variant})
        }
        other => json!(format!("{other:?}")),
    }
}
fn observe(authority: &Authority<'_>, framer: &Framer, tuple: &[i128]) -> Value {
    let envelopes = framer.frame(tuple);
    let report = authority.evaluate(c::Raw {
        state: &envelopes[0],
        command: &envelopes[1],
        context: &envelopes[2],
    });
    let diagnostics = report
        .diagnostics()
        .iter()
        .map(|law| json!({"id":law.id,"status":format!("{:?}",law.verdict)}))
        .collect::<Vec<_>>();
    match report.result() {
        Err(refusal) => {
            json!({"class":"Refused","refusal":format!("{refusal:?}"),"laws":diagnostics})
        }
        Ok(candidate) => {
            json!({"class":format!("{:?}",candidate.class()),"reason":candidate.reason(),
            "post":candidate.post().iter().map(|f|json!({"field":f.id,"value":atom(f.value)})).collect::<Vec<_>>(),
            "effects":candidate.effects().iter().map(|d|format!("{d:?}")).collect::<Vec<_>>(),
            "outbox":candidate.outbox().iter().map(|d|json!({"ordinal":d.ordinal,"channel":d.channel,
                "destination":atom(d.destination),"payload":d.payload.iter().map(|f|json!({"field":f.id,"value":atom(f.value)})).collect::<Vec<_>>(),"idempotency":atom(d.idempotency)})).collect::<Vec<_>>(),"laws":diagnostics})
        }
    }
}
fn signature(value: &Value) -> Value {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("laws");
    }
    value
}
fn passed(value: &Value) -> bool {
    value["class"] != "Refused"
}
fn witness(input: &[i128], original: &Value, mutant: &Value) -> Value {
    json!({"input":input,"original":original,"mutant":mutant})
}
fn with_inventory<T>(
    id: &str,
    use_models: impl FnOnce(
        &Authority<'_>,
        &Authority<'_>,
        &Framer,
        &domain::InputSet,
        &Value,
    ) -> Result<T>,
) -> Result<T> {
    // Both models are fresh, owned preparation data. Only immutable borrows
    // enter the authority; no contract, input alias or candidate escapes it.
    let rules = checked(Rules::read(RULES))?;
    let declarations = checked(Declarations::read(PROJECT, &rules.leaf_bindings))?;
    let positions = checked(domain::positions(&declarations))?;
    let inputs = checked(domain::input_set(
        &positions,
        &rules,
        &declarations,
        &[],
        864,
    ))?;
    if inputs.domain_size != Some(864) || inputs.len() != 864 {
        return Err("not the full declared domain".into());
    }
    let schema = checked(schema::encode(&declarations))?;
    let commitment = checked(schema_commitment(&schema))?;
    let original = checked(Contract::build(&declarations, &rules, commitment))?;
    let mutant_rules = checked(Rules::read(&mutated(id)?))?;
    let mutant = checked(Contract::build(&declarations, &mutant_rules, commitment))?;
    let framer = Framer::new(&positions, &original);
    let mut names = super::names::declared(&declarations);
    for law in &original.laws {
        if names["laws"].get(law.id.to_string()).is_none() {
            names["laws"][law.id.to_string()] = json!(law.kind.name());
        }
    }
    names["inputs"] = json!(positions.iter().map(|position|json!({"position":position.name,
        "name":position.field.map(|id|names["fields"][id.to_string()].clone()),"domain":position.domain.json()})).collect::<Vec<_>>());
    checked(policy::with_authority(&original, &schema, |authority| {
        policy::with_authority(&mutant, &schema, |other| {
            use_models(authority, other, &framer, &inputs, &names)
        })
    }))?
    .map_err(|error| error.to_string())?
}

pub(super) fn check(id: &str) -> Result<Value> {
    with_inventory(id, |authority, other, framer, inputs, names| {
        let mut first = None;
        for line in EXAMPLES
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        {
            let tuple = line
                .split('|')
                .next()
                .unwrap_or("")
                .split_whitespace()
                .map(str::parse::<i128>)
                .collect::<std::result::Result<Vec<_>, _>>();
            let tuple = checked(tuple)?;
            let before = observe(authority, framer, &tuple);
            let after = observe(other, framer, &tuple);
            if first.is_none() && signature(&before) != signature(&after) {
                first = Some(witness(&tuple, &before, &after));
            }
        }
        let mut original_passed = 0;
        let mut mutant_passed = 0;
        let mut differences = 0;
        let mut law_checks = 0;
        let mut fallback = None;
        let mut first_decided = None;
        let mut counts = std::collections::BTreeMap::<String, u64>::new();
        for tuple in inputs.tuples() {
            let before = observe(authority, framer, tuple);
            let after = observe(other, framer, tuple);
            original_passed += u64::from(passed(&before));
            mutant_passed += u64::from(passed(&after));
            *counts
                .entry(before["class"].as_str().unwrap_or("unknown").to_owned())
                .or_default() += 1;
            law_checks += before["laws"].as_array().map_or(0, |ds| {
                ds.iter().filter(|d| d["status"] == "Satisfied").count()
            });
            if signature(&before) != signature(&after) {
                differences += 1;
                let w = witness(tuple, &before, &after);
                if passed(&before) && first_decided.is_none() {
                    first_decided = Some(w.clone());
                }
                if fallback.is_none() {
                    fallback = Some(w);
                }
            }
        }
        let first = first.or(first_decided).or(fallback);
        Ok(
            json!({"act":"inventory","mutant":id,"domain":inputs.len(),"passed":original_passed,"mutant_passed":mutant_passed,
                "differences":differences,"law_checks":law_checks,"classes":counts,"first_difference":first,
                "positions":names["inputs"].as_array().map(|inputs|inputs.iter().map(|i|i["position"].clone()).collect::<Vec<_>>()),
                "names":names,"law_ids":authority.descriptor().laws.iter().map(|law|law.id).collect::<Vec<_>>(),
                "status":if first.is_some(){"distinguished"}else{"equivalent over the full domain"}}),
        )
    })
}

/// Small complete row reports keep each scalar-ABI reply below its byte cap.
pub(super) fn rows(id: &str, start: usize, count: usize) -> Result<Value> {
    if count == 0 || count > 48 || start >= 864 || start + count > 864 {
        return Err("row range must be within 864 inputs and at most 48 rows".into());
    }
    with_inventory(id, |authority, other, framer, inputs, names| {
        let cells = (start..start+count).map(|ordinal| {
            let input = inputs.tuple(ordinal);
            let original = observe(authority,framer,input);
            let mutant = observe(other,framer,input);
            json!({"ordinal":ordinal,"input":input,"class":original["class"],"mutant_class":mutant["class"],"different":signature(&original)!=signature(&mutant)})
        }).collect::<Vec<_>>();
        Ok(json!({"mutant":id,"start":start,"cells":cells,"names":names}))
    })
}

pub(super) fn detail(id: &str, ordinal: usize) -> Result<Value> {
    if ordinal >= 864 {
        return Err("cell must be within 864 inputs".into());
    }
    with_inventory(id, |authority, other, framer, inputs, _| {
        let input = inputs.tuple(ordinal);
        Ok(witness(
            input,
            &observe(authority, framer, input),
            &observe(other, framer, input),
        ))
    })
}
