//! Live finite synthesis. All authoritative work is performed by library checkers.
use serde_json::{Value, json};
use zeno_fcis_site_common::{DemoInstance, Refusal, Stage};
use zeno_fcis_synthesis::finite::{self as f, Domain, Op, Program, Slot};

/// Shared descriptor compiler and decision boundary.
pub mod ui;

mod inventory;
mod money;
mod names;
mod search;

const PROBLEM: &str =
    include_str!("../../../crates/zeno-fcis-cli/templates/agent-treasury-guard/synthesis.json");
type Result<T> = std::result::Result<T, String>;
fn checked<T, E: std::fmt::Debug>(value: std::result::Result<T, E>) -> Result<T> {
    value.map_err(|error| format!("{error:?}"))
}
fn field<T>(value: Option<T>) -> Result<T> {
    value.ok_or_else(|| "invalid pinned problem".into())
}
fn domains(fields: &Value) -> Result<Vec<Domain>> {
    field(fields.as_array())?
        .iter()
        .map(|v| match v["type"]["kind"].as_str() {
            Some("bool") => Ok(Domain::Bool),
            Some("int") => Ok(Domain::Int {
                min: i128::from(field(v["type"]["min"].as_i64())?),
                max: i128::from(field(v["type"]["max"].as_i64())?),
            }),
            _ => Err("invalid domain".into()),
        })
        .collect()
}
fn op(v: &Value) -> Result<Op> {
    let n = |i| field(v[i].as_u64()).and_then(|n| checked(u16::try_from(n)));
    Ok(match field(v[0].as_str())? {
        "input" => Op::Input(n(1)?),
        "int" => Op::Int(i128::from(field(v[1].as_i64())?)),
        "bool" => Op::Bool(field(v[1].as_bool())?),
        "not" => Op::Not(n(1)?),
        "add" => Op::Add(n(1)?, n(2)?),
        "sub" => Op::Sub(n(1)?, n(2)?),
        "eq" => Op::Eq(n(1)?, n(2)?),
        "lt" => Op::Lt(n(1)?, n(2)?),
        "and" => Op::And(n(1)?, n(2)?),
        "select" => Op::Select(n(1)?, n(2)?, n(3)?),
        _ => return Err("invalid pinned operation".into()),
    })
}
fn roots(v: &Value) -> Result<Vec<u16>> {
    field(v.as_array())?
        .iter()
        .map(|n| checked(u16::try_from(field(n.as_u64())?)))
        .collect()
}
fn problem() -> Result<(Value, f::Contract, f::Sketch)> {
    let v: Value = checked(serde_json::from_str(PROBLEM))?;
    let inputs = domains(&v["inputs"])?;
    let outputs = domains(&v["outputs"])?;
    let relation = checked(Program::try_new(
        [inputs.clone(), outputs.clone()].concat(),
        vec![Domain::Bool],
        field(v["contract"]["nodes"].as_array())?
            .iter()
            .map(op)
            .collect::<Result<_>>()?,
        roots(&v["contract"]["roots"])?,
    ))?;
    let contract = checked(f::Contract::try_new(
        inputs.clone(),
        outputs.clone(),
        relation,
    ))?;
    let slots = field(v["sketch"]["nodes"].as_array())?
        .iter()
        .map(|n| {
            if n.is_array() {
                Ok(Slot::Fixed(op(n)?))
            } else {
                Ok(Slot::Choice {
                    id: checked(u32::try_from(field(n["hole"].as_u64())?))?,
                    alternatives: field(n["alternatives"].as_array())?
                        .iter()
                        .map(op)
                        .collect::<Result<_>>()?,
                })
            }
        })
        .collect::<Result<_>>()?;
    let sketch = checked(f::Sketch::try_new(
        inputs,
        outputs,
        slots,
        roots(&v["sketch"]["roots"])?,
    ))?;
    Ok((v, contract, sketch))
}
fn witness(w: &f::Witness) -> Value {
    json!({"input":w.input,"output":w.output})
}
fn candidate(number: u64) -> Result<Value> {
    if number >= 8 {
        return Err("candidate must be 0 through 7".into());
    }
    let (v, contract, _) = problem()?;
    let choices = [(number >> 2) & 1, (number >> 1) & 1, number & 1];
    let nodes = field(v["sketch"]["nodes"].as_array())?
        .iter()
        .map(|n| {
            if n.is_array() {
                op(n)
            } else {
                let id = field(n["hole"].as_u64())?;
                let index = checked(usize::try_from(id - 1))?;
                op(&n["alternatives"][checked(usize::try_from(choices[index]))?])
            }
        })
        .collect::<Result<_>>()?;
    let program = checked(Program::try_new(
        domains(&v["inputs"])?,
        domains(&v["outputs"])?,
        nodes,
        roots(&v["sketch"]["roots"])?,
    ))?;
    let mut count = 0;
    for input in checked(contract.input_space())? {
        count += 1;
        let output = checked(program.evaluate(&input))?;
        if !checked(contract.holds(&input, &output))? {
            return Ok(
                json!({"candidate":number,"choices":choices,"checked":count,"status":"refuted","witness":{"input":input,"output":output}}),
            );
        }
    }
    Ok(
        json!({"candidate":number,"choices":choices,"checked":count,"status":"selected","witness":null}),
    )
}
fn synthesize() -> Result<Value> {
    let (v, contract, sketch) = problem()?;
    let result = checked(f::synthesize(
        &contract,
        &sketch,
        f::Budget {
            max_assignments: 8,
            max_steps: 100_000_000,
        },
    ))?;
    let f::Outcome::Selected {
        program,
        certificate,
        cases,
        first_counterexample,
    } = result
    else {
        return Err("no selected program".into());
    };
    let mut passed = 0;
    for case in &cases {
        if checked(program.evaluate(&case.input))? != case.output
            || !checked(contract.holds(&case.input, &case.output))?
        {
            return Err("synthesis replay mismatch".into());
        }
        passed += 1;
    }
    Ok(
        json!({"act":"synthesis","domain":checked(contract.input_space())?.cardinality(),"passed":passed,
        "assignments":certificate.evaluated(),"first_counterexample":first_counterexample.as_ref().map(witness),
        "inputs":v["inputs"].as_array().map(|fs|fs.iter().map(|f|f["name"].clone()).collect::<Vec<_>>()),
        "nodes":program.nodes().iter().enumerate().map(|(i,op)|format!("{i}: {op:?}")).collect::<Vec<_>>(),
        "names":names::treasury()?,"roots":program.roots(),"trace":format!("{:?}",certificate.trace_hash())}),
    )
}
/// Runs one bounded, explicit request. No retained decision state or clock.
/// # Errors
/// Returns parsing, domain or library checker refusals.
pub fn run(request: &Value) -> Result<Value> {
    let object = request.as_object().ok_or("request must be an object")?;
    let act = request["act"].as_str().ok_or("act must be a string")?;
    let allowed: &[&str] = match act {
        "candidate" => &["act", "number"],
        "inventory" => &["act", "mutant"],
        "inventory_rows" => &["act", "mutant", "start", "count"],
        "inventory_detail" => &["act", "mutant", "ordinal"],
        "money" => &["act", "amount", "fee_bps", "reserve_in", "reserve_out"],
        "synthesis" | "search" => &["act"],
        _ => return Err("unknown factory act".into()),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err("unexpected request field".into());
    }
    match act {
        "candidate" => candidate(field(request["number"].as_u64())?),
        "synthesis" => synthesize(),
        "search" => search::run(),
        "money" => money::run(request),
        "inventory" | "inventory_rows" | "inventory_detail" => {
            let id = match request.get("mutant") {
                None => "none",
                Some(id) => id.as_str().ok_or("mutant must be a string")?,
            };
            let index =
                |key| field(request[key].as_u64()).and_then(|n| checked(usize::try_from(n)));
            match act {
                "inventory_rows" => inventory::rows(id, index("start")?, index("count")?),
                "inventory_detail" => inventory::detail(id, index("ordinal")?),
                _ => inventory::check(id),
            }
        }
        _ => Err("unknown factory act".into()),
    }
}
struct Factory {
    // Exclusive bounded preparation bytes. These are never decisions or publication state.
    prepared: String,
}
impl DemoInstance for Factory {
    fn step(&mut self, input: &str) -> std::result::Result<Value, Refusal> {
        let result =
            checked(serde_json::from_str::<Value>(input)).and_then(|v| match v["act"].as_str() {
                Some("ui_append") => {
                    if v.as_object().is_none_or(|o| o.len() != 2) {
                        return Err("UiPreparationFields".into());
                    }
                    let chunk = v["chunk"].as_str().ok_or("UiPreparationChunk")?;
                    if self.prepared.len() + chunk.len() > 65_536 {
                        return Err("UiPreparationLimit".into());
                    }
                    self.prepared.push_str(chunk);
                    Ok(json!({"prepared_bytes":self.prepared.len()}))
                }
                Some("ui_decide") => {
                    if v.as_object().is_none_or(|o| o.len() != 2) {
                        return Err("UiDecisionFields".into());
                    }
                    ui::decide(
                        &checked(serde_json::from_str::<Value>(&self.prepared))?,
                        &v["input"],
                    )
                }
                _ => run(&v),
            });
        result.map_err(|e| Refusal::new(Stage::Input, e))
    }
    fn state(&self) -> std::result::Result<Value, Refusal> {
        Ok(json!({"steps":0,"authority":"none"}))
    }
}
/// Resets bounded transport; this demo publishes no decision or receipt.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn demo_reset() -> u32 {
    zeno_fcis_site_common::abi::reset_instance(|| {
        Ok(Box::new(Factory {
            prepared: String::new(),
        }))
    })
}
