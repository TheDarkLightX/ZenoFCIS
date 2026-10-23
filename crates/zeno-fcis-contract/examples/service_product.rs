//! Offline shell for product/reference checks; never supplies publication authority.
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead},
    path::Path,
};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_contract::{
    checked_composition::{DomainSources, JointComposition, LeafSources, check_domain},
    internals::{
        declarations::Declarations, model::Contract, policy, rules::Rules, schema,
        schema_commitment,
    },
};
use zeno_fcis_crypto::RustCryptoSha256;
fn hash(bytes: &[u8]) -> String {
    RustCryptoSha256::hash(bytes)
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let root = Path::new(args.get(2).ok_or("mode and directory required")?);
    let text = fs::read_to_string(root.join("composition.json"))?;
    let manifest: Value = serde_json::from_str(&text)?;
    let sources: Vec<LeafSources> = manifest["components"]
        .as_array()
        .ok_or("components")?
        .iter()
        .map(|entry| {
            let dir = root.join(entry["path"].as_str().unwrap_or(""));
            Ok(LeafSources {
                project: fs::read_to_string(dir.join("project.zeno"))?,
                rules: fs::read_to_string(dir.join("v2/policy.json"))?,
            })
        })
        .collect::<Result<_, io::Error>>()?;
    if args[1] == "pins" {
        let mut pins = Vec::new();
        for source in &sources {
            let rules = Rules::read(&source.rules)?;
            let declarations = Declarations::read(&source.project, &rules.leaf_bindings)?;
            let schema = schema::encode(&declarations)?;
            let commitment = schema_commitment(&schema)?;
            let contract = Contract::build(&declarations, &rules, commitment)?;
            let program =
                policy::with_authority(&contract, &schema, |authority| hash(authority.identity()))?;
            pins.push(json!({"project_sha256":hash(source.project.as_bytes()),"policy_sha256":hash(source.rules.as_bytes()),"program_sha256":program, "schema_sha256":hash(&schema)}));
        }
        println!("{}", json!(pins));
        return Ok(());
    }
    let inputs = DomainSources {
        manifest: &text,
        components: &sources,
    };
    if args[1] == "check" {
        println!("{}", check_domain(&inputs)?.json());
        return Ok(());
    }
    let service = if args[1] == "service" {
        Some(
            zeno_fcis_contract::checked_composition::service::ServiceAuthority::from_sources(
                &fs::read_to_string(root.join("service.json"))?,
                &sources,
            )?,
        )
    } else {
        None
    };
    let joint = if service.is_none() {
        Some(JointComposition::from_sources(&inputs)?)
    } else {
        None
    };
    for (ordinal, line) in io::stdin().lock().lines().enumerate() {
        if ordinal % 1000 == 0 {
            eprintln!("product rows decided: {ordinal}");
        }
        let row: Value = serde_json::from_str(&line?)?;
        let read = |key: &str| -> Result<Vec<i128>, Box<dyn std::error::Error>> {
            row[key]
                .as_array()
                .ok_or("array")?
                .iter()
                .map(|n| n.as_i64().map(i128::from).ok_or_else(|| "integer".into()))
                .collect()
        };
        if let Some(service) = &service {
            use zeno_fcis_contract::checked_composition::service::ServiceRequest;
            let state = service.state(&read("state")?)?;
            let request = ServiceRequest::new("reference", "row", &read("request")?)?;
            let (kind, post) = match service.decide(&state, &request, 0) {
                Ok(publication) => (
                    json!({"kind":"Accept"}),
                    publication.post().values().to_vec(),
                ),
                Err(refusal) => {
                    let parts: Vec<&str> = refusal.reason.split(':').collect();
                    let kind = if parts.first() == Some(&"reject") {
                        json!({"kind":"Reject","component":parts[1],"reason":parts[2].parse::<u32>()?})
                    } else {
                        json!({"kind":"Refused","class":refusal.reason})
                    };
                    (kind, state.values().to_vec())
                }
            };
            println!("{}", json!({"outcome":kind,"successor":post}));
        } else if let Some(joint) = &joint {
            let outcome = joint
                .joint_step(&read("state")?, &read("request")?)
                .map_err(|e| e.reason)?;
            println!("{}", outcome.json());
        }
    }
    Ok(())
}
