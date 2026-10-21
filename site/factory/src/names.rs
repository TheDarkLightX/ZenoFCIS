//! Display metadata read from the same declarations as each template.
use super::{Result, checked};
use serde_json::{Map, Value, json};
use zeno_fcis_contract::internals::{
    declarations::{Declarations, Form},
    rules::Rules,
};

pub(super) fn declared(declarations: &Declarations) -> Value {
    let mut fields = Map::new();
    let mut variants = Map::new();
    let mut sums = Map::new();
    for declared in declarations.types.values() {
        match &declared.form {
            Form::Record(members) => {
                for field in members {
                    fields.insert(field.id.to_string(), json!(field.name));
                }
            }
            Form::Sum(members) => {
                for variant in members {
                    variants.insert(variant.id.to_string(), json!(variant.name));
                }
                sums.insert(
                    declared.name.clone(),
                    json!(members.iter().map(|v| v.name.clone()).collect::<Vec<_>>()),
                );
            }
            Form::Leaf(_) => {}
        }
    }
    json!({"fields":fields,"variants":variants,"sums":sums,
        "reasons":declarations.notes.reasons.iter().map(|(id,(name,_))|(id.to_string(),json!(name))).collect::<Map<_,_>>(),
        "channels":declarations.notes.channels.iter().map(|(id,name)|(id.to_string(),json!(name))).collect::<Map<_,_>>(),
        "laws":declarations.laws.iter().map(|law|(law.id.to_string(),json!(law.name))).collect::<Map<_,_>>()})
}
pub(super) fn treasury() -> Result<Value> {
    let rules = checked(Rules::read(include_str!(
        "../../../crates/zeno-fcis-cli/templates/agent-treasury-guard/v2/policy.json"
    )))?;
    let declarations = checked(Declarations::read(
        include_str!("../../../crates/zeno-fcis-cli/templates/agent-treasury-guard/project.zeno"),
        &rules.leaf_bindings,
    ))?;
    let mut names = declared(&declarations);
    names["reason_ranks"] = json!(
        declarations
            .notes
            .reasons
            .values()
            .map(|(name, precedence)| ((precedence + 1).to_string(), json!(name)))
            .collect::<Map<_, _>>()
    );
    Ok(names)
}
