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
    let decisions = args.len() == 4 && args[3] == "--decisions";
    if args.len() != 3 && !decisions {
        return Err("usage: recursive_table_vectors PROJECT RULES [--decisions]".into());
    }
    let rules = Rules::read(&std::fs::read_to_string(&args[2])?)?;
    let declarations =
        Declarations::read(&std::fs::read_to_string(&args[1])?, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    if contract.unsigned {
        if decisions {
            return Err("the decision projection harness supports scalar fixtures".into());
        }
        return numeric(&contract, &schema);
    }
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
                if decisions {
                    use zeno_fcis_contract::internals::review::evaluate::{self, Outcome};
                    match evaluate::evaluate(authority, &framer, &tuple) {
                        Outcome::Decision(d) => {
                            if !d.outbox.is_empty() {
                                return Err("decision fixtures must have no outbox".into());
                            }
                            println!(
                                "{}",
                                serde_json::json!({"class":d.class.name(),"reason":d.reason,"post":d.post.iter().map(|(id,v)|serde_json::json!([id,v.json()])).collect::<Vec<_>>(),"outbox":[]})
                            );
                        }
                        Outcome::Refused(r) => {
                            return Err(format!("original Authority refused: {r}").into());
                        }
                    }
                    continue;
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

/// Diagnostic selected-law replay through a bound full-width library descriptor.
fn numeric(contract: &Contract<'_>, schema: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use zeno_fcis_contract::internals::model::{RootSchema, ScalarDomain};
    use zeno_fcis_synthesis::finite::{V2NumericAtom as Atom, v2_composition as c, v2_laws as l};
    policy::with_numeric_authority(
        contract,
        schema,
        |authority| -> Result<(), Box<dyn std::error::Error>> {
            let laws: Vec<_> = contract
                .declarations
                .laws
                .iter()
                .filter(|l| l.id >= 500)
                .map(|l| LawSet::new_numeric(authority.descriptor(), &[l.id]))
                .collect();
            let empty = [];
            let (_, usage, _, _) = l::evaluate(
                &[],
                &[],
                &l::Frame::Genesis {
                    initial: l::RootView::Record(&empty),
                },
                authority.descriptor().limits,
            )
            .into_parts();
            for line in io::stdin().lock().lines() {
                let raw: Vec<serde_json::Value> = serde_json::from_str(&line?)?;
                if raw.len() != contract.bindings.len() {
                    return Err("wrong input width".into());
                }
                let row = raw
                    .iter()
                    .zip(&contract.input_domains)
                    .map(
                        |(v, d)| -> Result<Atom<'static>, Box<dyn std::error::Error>> {
                            let text = v
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string());
                            Ok(match d {
                                ScalarDomain::Bool => Atom::Bool(match text.as_str() {
                                    "0" | "false" => false,
                                    "1" | "true" => true,
                                    _ => return Err("invalid Boolean input".into()),
                                }),
                                ScalarDomain::Int { min, max } => {
                                    let n = text.parse()?;
                                    if !(*min..=*max).contains(&n) {
                                        return Err("input outside domain".into());
                                    }
                                    Atom::I128(n)
                                }
                                ScalarDomain::Unsigned { min, max } => {
                                    let n = text.parse()?;
                                    if !(*min..=*max).contains(&n) {
                                        return Err("input outside domain".into());
                                    }
                                    Atom::U128(n)
                                }
                            })
                        },
                    )
                    .collect::<Result<Vec<_>, _>>()?;
                let fields = |source| {
                    contract
                        .bindings
                        .iter()
                        .zip(&row)
                        .filter(|((s, _), _)| *s == source)
                        .map(|((_, id), v)| c::Field {
                            id: id.unwrap_or(0),
                            value: *v,
                        })
                        .collect::<Vec<_>>()
                };
                let pre = fields(Source::State);
                let command = fields(Source::Command);
                let context = fields(Source::Context);
                fn view<'a>(
                    root: &RootSchema,
                    fields: &'a [c::Field<'static>],
                ) -> l::RootView<'a, 'static> {
                    if matches!(root, RootSchema::Leaf(_)) {
                        l::RootView::Leaf(fields[0].value)
                    } else {
                        l::RootView::Record(fields)
                    }
                }
                let candidate = l::Candidate {
                    class: c::Class::Accept,
                    reason: None,
                    post: view(&contract.roots[0], &pre),
                    patch: &[],
                    effects: &[],
                    outbox: &[],
                    reads: &[],
                    attempts: &[],
                    usage,
                };
                let frame = l::Frame::Transition {
                    pre: view(&contract.roots[0], &pre),
                    command: view(&contract.roots[1], &command),
                    context: view(&contract.roots[2], &context),
                    candidate: &candidate,
                };
                let mut values = Vec::new();
                for (law, declared) in laws
                    .iter()
                    .zip(contract.declarations.laws.iter().filter(|l| l.id >= 500))
                {
                    let (_, diagnostics) = law.on_frame(&frame);
                    values.push(
                        match diagnostics
                            .first()
                            .filter(|d| d.id == declared.id)
                            .ok_or("missing selected-law diagnostic")?
                            .verdict
                        {
                            l::Verdict::Satisfied => true,
                            l::Verdict::Refused(l::Failure::Violated) => false,
                            other => {
                                return Err(
                                    format!("non-Boolean evaluator verdict: {other:?}").into()
                                );
                            }
                        },
                    );
                }
                println!("{}", serde_json::to_string(&values)?);
            }
            Ok(())
        },
    )??;
    Ok(())
}
