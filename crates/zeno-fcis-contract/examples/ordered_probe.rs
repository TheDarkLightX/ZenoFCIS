//! Batch probe for an independent small-map simulator. No storage or publication.
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::PathBuf;
use zeno_fcis_contract::{
    collections::{Limits, OrderedQuery, check, ordered_key},
    internals::{
        declarations::Declarations,
        model::Contract,
        policy,
        review::{
            domain,
            evaluate::{self, Framer, Outcome},
        },
        rules::Rules,
        schema, schema_commitment,
    },
};
const BOOK: &str = include_str!("../../../examples/collections/order-book/project.zeno");
const RULES: &str = include_str!("../../../examples/collections/order-book/v2/policy.json");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if paths.len() != 2 {
        return Err("usage: ordered_probe INPUT.json OUTPUT.json".into());
    }
    let input: Value = serde_json::from_slice(&std::fs::read(&paths[0])?)?;
    let mut output = json!({"schema":"ordered-small-map-probe/1", "variants":[], "queries":[]});
    for variant in 0..2 {
        let mut raw: Value = serde_json::from_str(RULES)?;
        if variant == 1 {
            raw["cases"][2]["when"] = "context.102.133 == 0 || pre.100.160 == 0".into();
        }
        let text = raw.to_string();
        let verdict = check(BOOK, &text, Limits::default())?;
        let rules = Rules::read(&text)?;
        let decl = Declarations::read(BOOK, &rules.leaf_bindings)?;
        let positions = domain::positions(&decl)?;
        let encoded = schema::encode(&decl)?;
        let contract = Contract::build(&decl, &rules, schema_commitment(&encoded)?)?;
        let framer = Framer::new(&positions, &contract);
        let outcomes = policy::with_authority(
            &contract,
            &encoded,
            |authority| -> Result<Vec<Value>, Box<dyn std::error::Error>> {
                let mut results = Vec::new();
                for case in input["transitions"].as_array().ok_or("transitions")? {
                    let keys: BTreeSet<i128> = case["keys"]
                        .as_array()
                        .ok_or("keys")?
                        .iter()
                        .map(|v| v.as_i64().map(i128::from).ok_or("key"))
                        .collect::<Result<_, _>>()?;
                    let price = i128::from(case["price"].as_i64().ok_or("price")?);
                    let action = i128::from(case["action"].as_i64().ok_or("action")?);
                    let best = ordered_key(&keys, OrderedQuery::Max);
                    let r = best.unwrap_or(price);
                    let ranks: BTreeSet<i128> = [price, r].into_iter().collect();
                    let rank = |k: i128| ranks.iter().position(|v| *v == k).unwrap_or(0) as i128;
                    let tuple: Vec<i128> = positions
                        .iter()
                        .map(|p| match p.name.as_str() {
                            "pre.100.160" | "pre.100.161" => i128::from(keys.contains(&price)),
                            "pre.100.170" | "pre.100.171" => i128::from(best.is_some()),
                            "command.101.130" => action,
                            "command.101.131" => rank(price),
                            "context.102.132" => rank(r),
                            "context.102.133" => i128::from(best.is_some()),
                            other => panic!("unexpected view field {other}"),
                        })
                        .collect();
                    let mut post = keys.clone();
                    let class = match evaluate::evaluate(authority, &framer, &tuple) {
                        Outcome::Refused(_) => "refused".to_owned(),
                        Outcome::Decision(d) => {
                            if let Some(present) = d
                                .post
                                .iter()
                                .find(|(id, _)| *id == 160)
                                .and_then(|(_, v)| v.number())
                            {
                                if present == 1 {
                                    post.insert(price);
                                } else {
                                    post.remove(&price);
                                }
                            }
                            format!("{:?}", d.class)
                        }
                    };
                    results.push(json!({"best":best,"class":class,"post":post}));
                }
                Ok(results)
            },
        )??;
        output["variants"]
            .as_array_mut()
            .ok_or("variants")?
            .push(json!({"verdict":verdict.packet(),"outcomes":outcomes}));
    }
    for q in input["queries"].as_array().ok_or("queries")? {
        let keys: BTreeSet<i128> = q["keys"]
            .as_array()
            .ok_or("keys")?
            .iter()
            .map(|v| v.as_i64().map(i128::from).ok_or("key"))
            .collect::<Result<_, _>>()?;
        let a = i128::from(q["a"].as_i64().ok_or("a")?);
        let b = i128::from(q["b"].as_i64().ok_or("b")?);
        let query = match q["kind"].as_str().ok_or("kind")? {
            "min" => OrderedQuery::Min,
            "max" => OrderedQuery::Max,
            "successor" => OrderedQuery::Successor(a),
            "predecessor" => OrderedQuery::Predecessor(a),
            "range" => OrderedQuery::Range(a, b),
            _ => return Err("query kind".into()),
        };
        output["queries"]
            .as_array_mut()
            .ok_or("queries")?
            .push(json!(ordered_key(&keys, query)));
    }
    std::fs::write(&paths[1], serde_json::to_vec(&output)?)?;
    Ok(())
}
