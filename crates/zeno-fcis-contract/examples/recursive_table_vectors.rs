//! Batch diagnostic harness for comparing recursive entries with an external oracle.
//! Each input line is a JSON array in contract input order; each output lists
//! the truth of the declared laws through the bound library law evaluator.

use std::io::{self, BufRead};
use zeno_fcis_contract::internals::{
    declarations::{Declarations, Source},
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{Framer, LawSet, Proposed},
    },
    rules::{Class, Rules},
    schema, schema_commitment,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: recursive_table_vectors PROJECT RULES".into());
    }
    let rules = Rules::read(&std::fs::read_to_string(&args[2])?)?;
    let declarations =
        Declarations::read(&std::fs::read_to_string(&args[1])?, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let positions = domain::positions(&declarations)?;
    let width = positions
        .iter()
        .filter(|p| p.source == Source::State)
        .count();
    let framer = Framer::new(&positions, &contract);
    policy::with_authority(
        &contract,
        &schema,
        |authority| -> Result<(), Box<dyn std::error::Error>> {
            let laws: Vec<_> = declarations
                .laws
                .iter()
                .filter(|l| l.id >= 500)
                .map(|l| LawSet::new(authority.descriptor(), &[l.id]))
                .collect();
            for line in io::stdin().lock().lines() {
                let tuple: Vec<i128> = serde_json::from_str(&line?)?;
                if tuple.len() != positions.len()
                    || tuple.iter().zip(&positions).any(|(v, p)| {
                        !p.domain
                            .values_within(1 << 20)
                            .is_some_and(|vs| vs.contains(v))
                    })
                {
                    return Err("input outside declared finite domain".into());
                }
                let proposed = Proposed {
                    class: Class::Accept,
                    reason: None,
                    post: &tuple[..width],
                };
                let mut values = Vec::new();
                for (law, declared) in laws
                    .iter()
                    .zip(declarations.laws.iter().filter(|l| l.id >= 500))
                {
                    use zeno_fcis_synthesis::finite::v2_laws::{Failure, Verdict};
                    let (_, diagnostics) = law.on_transition(&framer, &tuple, &proposed);
                    let diagnostic = diagnostics
                        .first()
                        .filter(|d| d.id == declared.id)
                        .ok_or("missing selected-law diagnostic")?;
                    values.push(match diagnostic.verdict {
                        Verdict::Satisfied => true,
                        Verdict::Refused(Failure::Violated) => false,
                        other => {
                            return Err(format!("non-Boolean evaluator verdict: {other:?}").into());
                        }
                    });
                }
                println!("{}", serde_json::to_string(&values)?);
            }
            Ok(())
        },
    )??;
    Ok(())
}
