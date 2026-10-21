//! Native finite-contract vectors. This tool grants no application authority.
use serde_json::{Value, json};
use std::{env, fs, path::Path, process::ExitCode};
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{domain, evaluate::Framer},
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::v2_composition as c;
fn checked<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, String> {
    r.map_err(|e| format!("{e:?}"))
}
fn run() -> Result<Value, String> {
    let args = env::args().collect::<Vec<_>>();
    let dir = Path::new(args.get(1).ok_or("contract directory required")?);
    let mutant = args.get(2).map_or("", String::as_str);
    let text = checked(fs::read_to_string(dir.join("project.zeno")))?;
    let mut data: Value = checked(serde_json::from_str(&checked(fs::read_to_string(
        dir.join("v2/policy.json"),
    ))?))?;
    match mutant {
        "missing-region" => data["cases"][0]["post"]["112"] = json!("7"),
        "stale-epoch" => {
            let guard = data["cases"][2]["when"]
                .as_str()
                .ok_or("guard")?
                .replace("epoch_match == 1 && ", "");
            data["cases"][2]["when"] = json!(guard);
        }
        "false-claim" => data["cases"][0]["when"] = json!("true"),
        "false-enable" => data["cases"][0]["when"] = json!("false"),
        "" => {}
        _ => return Err("unknown mutant".into()),
    }
    let rules = checked(Rules::read(&data.to_string()))?;
    let declarations = checked(Declarations::read(&text, &rules.leaf_bindings))?;
    let positions = checked(domain::positions(&declarations))?;
    let inputs = checked(domain::input_set(
        &positions,
        &rules,
        &declarations,
        &[],
        100_000,
    ))?;
    if inputs.domain_size != Some(checked(u128::try_from(inputs.len()))?) {
        return Err("not a full domain".into());
    }
    let schema = checked(schema::encode(&declarations))?;
    let commitment = checked(schema_commitment(&schema))?;
    let contract = checked(Contract::build(&declarations, &rules, commitment))?;
    let framer = Framer::new(&positions, &contract);
    checked(policy::with_authority(
        &contract,
        &schema,
        |authority| -> Result<Value, String> {
            let mut genesis = inputs.tuple(0).to_vec();
            for (index, p) in positions.iter().enumerate() {
                if index < contract.genesis.len()
                    && let Some(field) = p.field
                    && let Some(value) = rules.genesis.get(&field)
                {
                    genesis[index] = match value {
                        zeno_fcis_contract::internals::rules::Constant::Int(v) => *v,
                        zeno_fcis_contract::internals::rules::Constant::Bool(_) => {
                            return Err("noninteger genesis".into());
                        }
                    };
                }
            }
            let envelopes = framer.frame(&genesis);
            let report = authority.genesis(&envelopes[0]);
            if report.result().is_err() {
                return Err(format!("genesis law refused: {:?}", report.diagnostics()));
            }
            let mut rows = Vec::new();
            for input in inputs.tuples() {
                let frame = framer.frame(input);
                let report = authority.evaluate(c::Raw {
                    state: &frame[0],
                    command: &frame[1],
                    context: &frame[2],
                });
                let candidate = report.result().map_err(|_| {
                    format!("law violation at {input:?}: {:?}", report.diagnostics())
                })?;
                let post = candidate
                    .post()
                    .iter()
                    .map(|f| match f.value {
                        c::Atom::I128(v) => Ok(json!(v)),
                        _ => Err("noninteger model field".to_owned()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                rows.push(json!([input, format!("{:?}", candidate.class()), post]));
            }
            Ok(
                json!({"domain":inputs.len(),"positions":positions.iter().map(|p|p.name.clone()).collect::<Vec<_>>(),"rows":rows}),
            )
        },
    ))?
}
fn main() -> ExitCode {
    match run() {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("presentation finite check: {e}");
            ExitCode::FAILURE
        }
    }
}
