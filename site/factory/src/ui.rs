//! Descriptor inspection and stateless decisions through the actual V2 Authority.
//! Preparation bytes are transport data; each invocation binds a fresh immutable contract.
use crate::{Result, checked};
use serde_json::{Value, json};
use zeno_fcis_contract::internals::{
    declarations::{Declarations, Kind, Source},
    model::Contract,
    policy,
    review::{
        domain::{self, LeafDomain, Position},
        evaluate::Framer,
    },
    rules::{Constant, Rules},
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::{v2_authority::Authority, v2_composition as c};

fn model<T>(
    source: &Value,
    f: impl FnOnce(&Declarations, &Rules, &[Position], &Authority<'_>, &Framer) -> Result<T>,
) -> Result<T> {
    if source
        .as_object()
        .is_none_or(|o| o.len() != 2 || !o.contains_key("project") || !o.contains_key("policy"))
    {
        return Err("UiSourceFields".into());
    }
    let project = source["project"].as_str().ok_or("UiMissingProject")?;
    let rules = checked(Rules::read(
        source["policy"].as_str().ok_or("UiMissingPolicy")?,
    ))?;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        return Err("UiRequiresStandaloneContract".into());
    }
    let declarations = checked(Declarations::read(project, &rules.leaf_bindings))?;
    let positions = checked(domain::positions(&declarations))?;
    let schema = checked(schema::encode(&declarations))?;
    let contract = checked(Contract::build(
        &declarations,
        &rules,
        checked(schema_commitment(&schema))?,
    ))?;
    let framer = Framer::new(&positions, &contract);
    checked(policy::with_authority(&contract, &schema, |authority| {
        f(&declarations, &rules, &positions, authority, &framer)
    }))?
}

fn tuple(positions: &[Position], values: &Value) -> Result<Vec<i128>> {
    let values = values.as_array().ok_or("UiInvalidTuple")?;
    if values.len() != positions.len() {
        return Err("UiTupleWidth".into());
    }
    positions
        .iter()
        .zip(values)
        .map(|(p, v)| {
            let text = v.as_str().ok_or("UiIntegerMustBeString")?;
            let n = checked(text.parse::<i128>())?;
            if n.to_string() != text {
                return Err(format!("UiNoncanonicalInteger: {}", p.name));
            }
            let valid = match &p.domain {
                LeafDomain::Bool => n == 0 || n == 1,
                LeafDomain::Int { min, max } => n >= *min && n <= *max,
                LeafDomain::Sum { variants, .. } => {
                    u16::try_from(n).is_ok_and(|n| variants.contains(&n))
                }
            };
            if !valid {
                return Err(format!("UiOutsideDomain: {}", p.name));
            }
            Ok(n)
        })
        .collect()
}

fn scalar(atom: c::Atom<'_>) -> Result<String> {
    match atom {
        c::Atom::Bool(v) => Ok(u8::from(v).to_string()),
        c::Atom::I128(v) => Ok(v.to_string()),
        c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => Ok(variant.to_string()),
        _ => Err("UiUnsupportedStateType".into()),
    }
}

/// Grouping provenance from the existing checked table expansion, matched to
/// the actual scalar input positions. This adds no state or decision semantics.
fn fixed_tables(project: &str, positions: &[Position]) -> Result<Option<Value>> {
    let expansion = checked(zeno_fcis_spec::expand_tables(
        project,
        zeno_fcis_spec::SourceLimits::default(),
    ))?;
    let mut tables = Vec::new();
    for table in expansion.tables.iter().filter(|table| table.owner == 100) {
        let mut entries = Vec::new();
        for offset in 0..table.size {
            let field = table.id.checked_add(offset).ok_or("UiTableFieldBinding")?;
            let bound: Vec<_> = positions
                .iter()
                .enumerate()
                .filter(|(_, p)| p.source == Source::State && p.field.map(u32::from) == Some(field))
                .collect();
            let [(index, position)] = bound.as_slice() else {
                return Err("UiTableFieldBinding".into());
            };
            if position.type_id != table.field_type
                || position.name != format!("pre.{}.{}", table.owner, field)
            {
                return Err("UiTableFieldBinding".into());
            }
            entries.push(json!({"field":field.to_string(),"path":position.name,"index":index}));
        }
        tables.push(json!({"id":table.id.to_string(),"owner":table.owner.to_string(),
            "name":table.name,"fieldType":table.field_type.to_string(),"size":table.size,"entries":entries}));
    }
    Ok((!tables.is_empty()).then(|| json!({"format":"zenoui/native-fixed-tables/1","tables":tables})))
}

/// Inspects exact schema domains and authored names; never guesses presentation metadata.
/// # Errors
/// Refuses unsupported contract shapes or invalid policies.
pub fn describe(source: &Value) -> Result<Value> {
    model(source, |d, rules, positions, authority, framer| {
        let mut initial: Vec<i128> = positions
            .iter()
            .map(|p| p.domain.least().ok_or("UiEmptyDomain"))
            .collect::<std::result::Result<_, _>>()?;
        for (index, p) in positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == Source::State)
        {
            initial[index] = match rules
                .genesis
                .get(&p.field.unwrap_or(0))
                .ok_or("UiMissingGenesis")?
            {
                Constant::Int(v) => *v,
                Constant::Bool(v) => i128::from(*v),
                Constant::U128(_) => return Err("UiUnsupportedUnsignedGenesis".into()),
            };
        }
        let frame = framer.frame(&initial);
        if authority.genesis(&frame[0]).result().is_err() {
            return Err("UiInvalidGenesis".into());
        }
        let fields: Vec<Value> = positions
            .iter()
            .enumerate()
            .map(|(index, p)| {
                let mut item = p.json();
                item["index"] = json!(index);
                item["initial"] = json!(initial[index].to_string());
                item["name"] = json!(
                    p.field
                        .and_then(|id| d.types.values().find_map(|t| match &t.form {
                            zeno_fcis_contract::internals::declarations::Form::Record(fs) =>
                                fs.iter().find(|f| f.id == id).map(|f| f.name.clone()),
                            _ => None,
                        }))
                        .unwrap_or_else(|| d.types[&p.type_id].name.clone())
                );
                item["path"] = json!(p.name);
                item["type_name"] = json!(d.types[&p.type_id].name);
                if let Ok(Kind::Sum(variants)) = d.kind(p.type_id) {
                    item["options"] = json!(
                        variants
                            .iter()
                            .map(|v| json!({"value":v.id.to_string(),"name":v.name}))
                            .collect::<Vec<_>>()
                    );
                }
                item
            })
            .collect();
        let mut descriptor = json!({"fields":fields,"reasons":d.notes.reasons.iter().map(|(id,(name,_))| json!({"id":id.to_string(),"name":name})).collect::<Vec<_>>(),"domain":domain::domain_size(positions).map(|v|v.to_string()),"enablement":enablement_hole()?});
        if let Some(tables) = fixed_tables(source["project"].as_str().ok_or("UiMissingProject")?, positions)? {
            descriptor["nativeTables"] = tables;
        }
        Ok(descriptor)
    })
}

/// Recompiles and revalidates the exact submitted tuple. No cached decision is authoritative.
/// # Errors
/// Refuses invalid input, source, schema, policy or law evaluation.
pub fn decide(source: &Value, input: &Value) -> Result<Value> {
    model(source, |_, _, positions, authority, framer| {
        let values = tuple(positions, input)?;
        let frames = framer.frame(&values);
        let report = authority.evaluate(c::Raw {
            state: &frames[0],
            command: &frames[1],
            context: &frames[2],
        });
        let Ok(candidate) = report.result() else {
            return Ok(
                json!({"input":input,"class":"Refused","reason":null,"post":[],"checked":"1","total":"1"}),
            );
        };
        let post = if format!("{:?}", candidate.class()) == "Reject" {
            Vec::new()
        } else {
            positions
                .iter()
                .filter(|p| p.source == Source::State)
                .map(|p| {
                    let f = candidate
                        .post()
                        .iter()
                        .find(|f| Some(f.id) == p.field)
                        .ok_or("UiMissingPostField")?;
                    scalar(f.value)
                })
                .collect::<Result<Vec<_>>>()?
        };
        Ok(
            json!({"input":input,"class":format!("{:?}",candidate.class()),"reason":candidate.reason().map(|v|v.to_string()),"post":post,"checked":"1","total":"1"}),
        )
    })
}

/// Exhausts the schema product, including states whose decisions the laws refuse.
/// # Errors
/// Fails closed if the full product exceeds two million tuples; no boundary sample is substituted.
pub fn finite(source: &Value) -> Result<Value> {
    model(source, |_, _, positions, authority, framer| {
        let size = domain::domain_size(positions)
            .filter(|v| *v <= 2_000_000)
            .ok_or("UiDomainTooLarge")?;
        let domains = positions
            .iter()
            .map(|p| p.domain.values_within(2_000_000).ok_or("UiDomainTooLarge"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut classes = String::with_capacity(checked(usize::try_from(size))?);
        let mut values = vec![0; positions.len()];
        for ordinal in 0..size {
            let mut remainder = ordinal;
            for (i, domain) in domains.iter().enumerate().rev() {
                values[i] = domain[checked(usize::try_from(remainder % domain.len() as u128))?];
                remainder /= domain.len() as u128;
            }
            let frames = framer.frame(&values);
            let report = authority.evaluate(c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            });
            classes.push(match report.result() {
                Ok(c) => match format!("{:?}", c.class()).as_str() {
                    "Accept" => 'A',
                    "Reject" => 'R',
                    _ => 'F',
                },
                Err(_) => 'X',
            });
        }
        Ok(json!({"domain":size.to_string(),"classes":classes}))
    })
}

// The factory's finite synthesis loop proposes; the relation judges every tuple.
// The first candidate enables everything and is refuted before selection.
fn enablement_hole() -> Result<Value> {
    use zeno_fcis_synthesis::finite::{self as f, Domain, Op, Program, Slot};
    let inputs = vec![Domain::Bool, Domain::Bool];
    let outputs = vec![Domain::Bool];
    let relation = checked(Program::try_new(
        vec![Domain::Bool; 3],
        outputs.clone(),
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::And(0, 1),
            Op::Eq(2, 3),
        ],
        vec![4],
    ))?;
    let contract = checked(f::Contract::try_new(
        inputs.clone(),
        outputs.clone(),
        relation,
    ))?;
    let sketch = checked(f::Sketch::try_new(
        inputs,
        outputs,
        vec![
            Slot::Fixed(Op::Input(0)),
            Slot::Fixed(Op::Input(1)),
            Slot::Choice {
                id: 1,
                alternatives: vec![Op::Bool(true), Op::And(0, 1)],
            },
        ],
        vec![2],
    ))?;
    let result = checked(f::synthesize(
        &contract,
        &sketch,
        f::Budget {
            max_assignments: 2,
            max_steps: 10_000,
        },
    ))?;
    let f::Outcome::Selected {
        program,
        certificate,
        first_counterexample,
        ..
    } = result
    else {
        return Err("UiEnablementHoleUnfilled".into());
    };
    let witness = first_counterexample.ok_or("UiMissingPolicyCounterexample")?;
    let mut table = Vec::new();
    for input in checked(contract.input_space())? {
        let output = checked(program.evaluate(&input))?;
        if !checked(contract.holds(&input, &output))? {
            return Err("UiEnablementJudgeRefused".into());
        }
        table.push(output == [1]);
    }
    Ok(
        json!({"table":table,"assignments":certificate.evaluated().to_string(),"counterexample":{"input":witness.input,"output":witness.output},"checked":"4"}),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn source() -> Value {
        json!({"project": include_str!("../../zenoui/test-projects/wide-counter/project.zeno"), "policy":include_str!("../../zenoui/test-projects/wide-counter/v2/policy.json")})
    }
    #[test]
    fn exact_wide_domain_and_real_decisions() {
        let source = source();
        let d = describe(&source).unwrap();
        assert_eq!(d["fields"][0]["domain"]["min"], "9007199254740993");
        assert_eq!(d["enablement"]["table"], json!([false, false, false, true]));
        assert_eq!(d["enablement"]["assignments"], "2");
        let input = json!(["9007199254740994", "120", "130"]);
        let before = source.clone();
        let result = decide(&source, &input).unwrap();
        assert_eq!(source, before);
        assert_eq!(input, json!(["9007199254740994", "120", "130"]));
        assert_eq!(result["class"], "Accept");
        assert_eq!(result["post"], json!(["9007199254740995"]));
        let rejected = decide(&source, &json!(["9007199254740994", "120", "131"])).unwrap();
        assert_eq!(rejected["class"], "Reject");
        assert_eq!(rejected["reason"], "200");
        assert_eq!(rejected["post"], json!([]));
    }
    #[test]
    fn invalid_inputs_fail_before_framing() {
        for input in [
            json!([9_007_199_254_740_994_u64, "120", "130"]),
            json!(["09007199254740994", "120", "130"]),
            json!(["9007199254740996", "120", "130"]),
            json!(["9007199254740994", "999", "130"]),
        ] {
            assert!(decide(&source(), &input).is_err());
        }
        let mut extra = source();
        extra["script"] = json!("anything");
        assert!(describe(&extra).is_err());
    }
    #[test]
    fn finite_is_the_full_exact_domain() {
        let checked = finite(&source()).unwrap();
        assert_eq!(checked["domain"], "18");
        assert_eq!(checked["classes"].as_str().unwrap().len(), 18);
    }
    #[test]
    fn authored_library_table_binds_native_positions_and_real_decisions() {
        let source = json!({
            "project": include_str!("../../zenoui/test-projects/fixed-table-library/project.zeno"),
            "policy": include_str!("../../zenoui/test-projects/fixed-table-library/v2/policy.json")
        });
        let descriptor = describe(&source).unwrap();
        let fields = descriptor["fields"].as_array().unwrap();
        let tables = descriptor["nativeTables"]["tables"].as_array().unwrap();
        assert_eq!(descriptor["nativeTables"]["format"], "zenoui/native-fixed-tables/1");
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0]["id"], "110");
        assert_eq!(tables[0]["owner"], "100");
        assert_eq!(tables[0]["fieldType"], "105");
        assert_eq!(tables[0]["size"], 6);
        for entry in tables[0]["entries"].as_array().unwrap() {
            let index = entry["index"].as_u64().unwrap() as usize;
            assert_eq!(fields[index]["source"], "state");
            assert_eq!(fields[index]["path"], entry["path"]);
            assert_eq!(fields[index]["field"].as_u64().unwrap().to_string(), entry["field"]);
            assert_eq!(fields[index]["type"], 105);
        }
        let mut input: Vec<Value> = fields.iter().map(|f| f["initial"].clone()).collect();
        let position = |path: &str| fields.iter().position(|f| f["path"] == path).unwrap();
        input[position("command.101.120")] = json!("150");
        input[position("command.101.121")] = json!("0");
        input[position("command.101.122")] = json!("1");
        input[position("context.102")] = json!("130");
        let before: Vec<_> = fields.iter().filter(|f| f["source"] == "state").map(|f| f["initial"].clone()).collect();
        let borrowed = decide(&source, &json!(input)).unwrap();
        assert_eq!(borrowed["class"], "Accept");
        let mut at = 0;
        for (index, field) in fields.iter().enumerate() {
            if field["source"] == "state" {
                input[index] = borrowed["post"][at].clone();
                at += 1;
            }
        }
        assert_eq!(input[position("pre.100.110")], "1");
        input[position("command.101.120")] = json!("151");
        let returned = decide(&source, &json!(input)).unwrap();
        assert_eq!(returned["class"], "Accept");
        assert_eq!(returned["post"], json!(before));
        input[position("context.102")] = json!("131");
        let visitor = decide(&source, &json!(input)).unwrap();
        assert_eq!(visitor["class"], "Reject");
        assert_eq!(visitor["reason"], "200");
        assert_eq!(visitor["post"], json!([]));
    }
}
