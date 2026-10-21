//! Finite game table acquisition through the library Authority; no solver.
#![forbid(unsafe_code)]
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    time::Instant,
};
use zeno_fcis_contract::internals::{
    declarations::{Declarations, Source},
    model::Contract,
    policy,
    review::{
        domain,
        evaluate::{self, Framer, LawSet, Outcome, Proposed},
    },
    rules::{Class, Constant, LawKind, Rules},
    schema, schema_commitment,
};
use zeno_fcis_synthesis::finite::v2_laws::{Failure, Verdict};
use zeno_fcis_synthesis::finite::{
    V2Resource, v2_authority::PublicationOutcome, v2_composition as c,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Event {
    actor: usize,
    request: Vec<i128>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    project: String,
    policy: String,
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    formula: String,
    #[serde(default)]
    guards: Vec<String>,
}

fn run(request: Request) -> Result<Value, String> {
    let started = Instant::now();
    let rules = Rules::read(&request.policy).map_err(|e| e.to_string())?;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        return Err("retained lineages unsupported; export a scalar contract first".into());
    }
    let declarations =
        Declarations::read(&request.project, &rules.leaf_bindings).map_err(|e| e.to_string())?;
    let positions = domain::positions(&declarations).map_err(|e| e.to_string())?;
    let width = positions
        .iter()
        .filter(|p| p.source == Source::State)
        .count();
    let schema = schema::encode(&declarations).map_err(|e| e.to_string())?;
    let commitment = schema_commitment(&schema).map_err(|e| e.to_string())?;
    let contract = Contract::build(&declarations, &rules, commitment).map_err(|e| e.to_string())?;
    let mut values = Vec::new();
    let mut size = 1usize;
    for p in &positions[..width] {
        let v = p.domain.values_within(10000).ok_or("state domain budget")?;
        size = size.checked_mul(v.len()).ok_or("state domain budget")?;
        if size > 10000 {
            return Err("state domain budget".into());
        }
        values.push(v);
    }
    let genesis: Vec<i128> = positions[..width]
        .iter()
        .map(|p| match p.field.and_then(|id| rules.genesis.get(&id)) {
            Some(Constant::Int(n)) => Ok(*n),
            Some(Constant::Bool(b)) => Ok(i128::from(*b)),
            _ => Err("scalar genesis field required"),
        })
        .collect::<Result<_, _>>()?;
    if !values.iter().zip(&genesis).all(|(v, n)| v.contains(n)) {
        return Err("genesis domain".into());
    }
    let metadata = json!({"schema":"zeno-fcis/game-native/1", "positions":positions.iter().map(|p|
        json!({"name":p.name,"domain":p.domain.json()})).collect::<Vec<_>>(), "genesis":numbers(&genesis)});
    if request.events.is_empty() {
        return Ok(metadata);
    }
    if size
        .checked_mul(request.events.len())
        .is_none_or(|n| n > 100000)
    {
        return Err("native table edge budget".into());
    }
    for event in &request.events {
        if event.actor >= 16 || event.request.len() != positions.len() - width {
            return Err("event encoding".into());
        }
        for (p, n) in positions[width..].iter().zip(&event.request) {
            let inside = match &p.domain {
                domain::LeafDomain::Bool => matches!(n, 0 | 1),
                domain::LeafDomain::Int { min, max } => min <= n && n <= max,
                domain::LeafDomain::Sum { variants, .. } => {
                    variants.iter().any(|v| i128::from(*v) == *n)
                }
            };
            if !inside {
                return Err(format!("capability outside domain: {}", p.name));
            }
        }
    }
    // Observation laws belong to a distinct descriptor. They never govern the
    // native Authority's admission. Reasons are constants in totalized frames.
    let mut extended = request.project.clone();
    let mut observer_rules = rules.clone();
    let mut next_id = request
        .project
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u32>().ok())
        .chain([1000, rules.failure_law, rules.reject_law])
        .max()
        .unwrap_or(1000);
    let mut add = |formula: &str| -> Result<u32, String> {
        if formula.is_empty()
            || [";", "\n", "\r", "//", "/*", "{", "}"]
                .iter()
                .any(|s| formula.contains(s))
        {
            return Err("one formula required".into());
        }
        next_id = next_id.checked_add(1).ok_or("law ID exhausted")?;
        extended.push_str(&format!(
            "\nlaw {next_id} game_observation_{next_id} on accept = {formula};\n"
        ));
        observer_rules
            .law_kinds
            .insert(next_id, LawKind::AuthoritySubjectRecipient);
        Ok(next_id)
    };
    let mut objective_ids = BTreeMap::new();
    for reason in std::iter::once(0).chain(declarations.reasons.iter().copied()) {
        objective_ids.insert(
            reason,
            add(&request
                .formula
                .replace("decision_reason", &reason.to_string()))?,
        );
    }
    let guard_ids = request
        .guards
        .iter()
        .map(|g| add(g))
        .collect::<Result<Vec<_>, _>>()?;
    let observer_declarations =
        Declarations::read(&extended, &rules.leaf_bindings).map_err(|e| e.to_string())?;
    let observer_contract = Contract::build(&observer_declarations, &observer_rules, commitment)
        .map_err(|e| e.to_string())?;
    let framer = Framer::new(&positions, &contract);
    let observed = policy::with_authority(&observer_contract, &schema, |observer| {
        policy::with_authority(&contract, &schema, |authority| -> Result<Value,String> {
            let mut initial = genesis.clone();
            initial.extend(positions[width..].iter().map(|p|p.domain.least().unwrap_or(0)));
            let frames = framer.frame(&initial);
            match authority.publish_genesis(&frames[0]) {
                PublicationOutcome::Commit(p) => {
                    if !matches!(authority.replay_genesis_publication(&frames[0],p.subject()),
                        PublicationOutcome::Commit(ref q) if q.subject()==p.subject()) {
                        return Err("native genesis replay refused".into());
                    }
                },
                _ => return Err("native genesis refused".into()),
            }
            let test = |id, tuple: &[i128], post: &[i128], outbox: &[evaluate::Delivery]| -> Result<bool,String> {
                let laws = LawSet::new(observer.descriptor(), &[id]);
                let (_, diagnostics) = laws.on_transition_with_outbox(&framer, tuple,
                    &Proposed{class:Class::Accept,reason:None,post}, outbox);
                match diagnostics.first().map(|d| d.verdict) {
                    Some(Verdict::Satisfied) => Ok(true),
                    Some(Verdict::Refused(Failure::Violated)) => Ok(false),
                    _ => Err("undefined game formula".into()),
                }
            };
            let mut rows = Vec::new();
            for ordinal in 0..size {
                let mut at = ordinal;
                let mut pre = vec![0; width];
                for (slot, domain) in pre.iter_mut().zip(&values).rev() { *slot=domain[at%domain.len()]; at/=domain.len(); }
                for (event_id,event) in request.events.iter().enumerate() {
                    if started.elapsed().as_secs_f64() >= 30.0 { return Err("native table time budget".into()); }
                    let mut tuple = pre.clone(); tuple.extend_from_slice(&event.request);
                    let outcome = evaluate::evaluate(authority, &framer, &tuple);
                    let frames = framer.frame(&tuple);
                    let raw=c::Raw{state:&frames[0],command:&frames[1],context:&frames[2]};
                    let published=authority.publish(raw);
                    let usage=published.evaluation().usage();
                    let subject=published.evaluation().subject().ok();
                    let replayed=subject.is_some_and(|subject| {
                        let replay=authority.replay_publication(raw,subject);
                        replay.evaluation().subject().ok()==Some(subject) &&
                            format!("{:?}",replay.evaluation().result())==format!("{:?}",published.evaluation().result()) &&
                            replay.evaluation().usage()==usage
                    });
                    let refusal_repeated=subject.is_none() && matches!(published,PublicationOutcome::Refused{..}) && {
                        let again=authority.evaluate(raw);
                        format!("{:?}",again.result())==format!("{:?}",published.evaluation().result()) && again.usage()==usage
                    };
                    if !replayed && !refusal_repeated { return Err("native outcome replay refused".into()); }
                    let mut post = pre.clone();
                    let (class,reason,outbox,details) = match &outcome {
                        Outcome::Refused(r) => ("Refused",None,&[][..],json!({"refusal":r.class.name(),"law":r.law})),
                        Outcome::Decision(d) => {
                            if d.class != Class::Reject {
                                for (slot,p) in post.iter_mut().zip(&positions[..width]) {
                                    *slot=d.post.iter().find(|(id,_)| Some(*id)==p.field).and_then(|(_,v)|v.number()).ok_or("non-scalar successor")?;
                                }
                            }
                            (d.class.name(),d.reason,d.outbox.as_slice(),json!({"outbox_sha256":evaluate::hex(&d.outbox_digest)}))
                        }
                    };
                    let holds = test(objective_ids[&reason.unwrap_or(0)], &tuple, &post, outbox)?;
                    let blocked = test(objective_ids[&0], &tuple, &pre, &[])?;
                    let enabled = guard_ids.iter().map(|id| test(*id,&tuple,&pre,&[])).collect::<Result<Vec<_>,_>>()?;
                    rows.push(json!({"pre":numbers(&pre),"event":event_id,"post":numbers(&post),"class":class,"reason":reason,
                        "details":details,"holds":holds,"blocked_holds":blocked,"guards":enabled,
                        "publication_replayed":replayed,"refusal_repeated":refusal_repeated,
                        "usage_steps":usage.used(V2Resource::Step),"usage_reads":usage.used(V2Resource::Read)}));
                }
            }
            let mut result=metadata.clone(); result["rows"]=json!(rows);
            result["genesis_replayed"]=json!(true); Ok(result)
        }).map_err(|e| e.to_string())?
    }).map_err(|e| e.to_string())??;
    Ok(observed)
}

fn numbers(values: &[i128]) -> Vec<String> {
    values.iter().map(ToString::to_string).collect()
}

fn main() {
    let mut raw = String::new();
    let result = io::stdin()
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            if raw.len() > 4 * 1024 * 1024 {
                Err("native request size".into())
            } else {
                serde_json::from_str(&raw).map_err(|e| e.to_string())
            }
        })
        .and_then(run);
    match result {
        Ok(value) => println!("{value}"),
        Err(reason) => {
            eprintln!("{reason}");
            std::process::exit(2);
        }
    }
}
