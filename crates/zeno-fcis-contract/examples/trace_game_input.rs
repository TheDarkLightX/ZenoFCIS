//! Finite game table acquisition through the library Authority; no solver.
#![forbid(unsafe_code)]
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
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

const MAX_REACHABLE_STATES: usize = 10_000;
const MAX_ROWS: usize = 100_000;

fn in_domain(domain: &domain::LeafDomain, number: i128) -> bool {
    match domain {
        domain::LeafDomain::Bool => matches!(number, 0 | 1),
        domain::LeafDomain::Int { min, max } => *min <= number && number <= *max,
        domain::LeafDomain::Sum { variants, .. } => variants
            .iter()
            .any(|variant| i128::from(*variant) == number),
    }
}

fn run(request: Request) -> Result<Value, String> {
    let started = Instant::now();
    let rules = Rules::read(&request.policy).map_err(|e| e.to_string())?;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        return Err("retained-lineage acquisition requires the retained programs and receipt chain; base-policy replay is not exact".into());
    }
    if rules.refuse_unsigned("trace-game acquisition").is_err() {
        return Err("unsigned/typed acquisition requires typed Authority frames; scalar i128 frames are not exact".into());
    }
    let declarations =
        Declarations::read(&request.project, &rules.leaf_bindings).map_err(|e| e.to_string())?;
    if declarations.unsigned() {
        return Err("unsigned/typed acquisition requires typed Authority frames; scalar i128 frames are not exact".into());
    }
    let positions = domain::positions(&declarations).map_err(|e| e.to_string())?;
    let width = positions
        .iter()
        .filter(|p| p.source == Source::State)
        .count();
    let schema = schema::encode(&declarations).map_err(|e| e.to_string())?;
    let commitment = schema_commitment(&schema).map_err(|e| e.to_string())?;
    let contract = Contract::build(&declarations, &rules, commitment).map_err(|e| e.to_string())?;
    let genesis: Vec<i128> = positions[..width]
        .iter()
        .map(|p| match p.field.and_then(|id| rules.genesis.get(&id)) {
            Some(Constant::Int(n)) => Ok(*n),
            Some(Constant::Bool(b)) => Ok(i128::from(*b)),
            _ => Err("scalar genesis field required"),
        })
        .collect::<Result<_, _>>()?;
    if !positions[..width]
        .iter()
        .zip(&genesis)
        .all(|(position, number)| in_domain(&position.domain, *number))
    {
        return Err("genesis domain".into());
    }
    let metadata = json!({"schema":"zeno-fcis/game-native/1", "positions":positions.iter().map(|p|
        json!({"name":p.name,"domain":p.domain.json()})).collect::<Vec<_>>(), "genesis":numbers(&genesis)});
    if request.events.is_empty() {
        return Ok(metadata);
    }
    if request.events.len() > MAX_ROWS {
        return Err("native table edge budget".into());
    }
    for event in &request.events {
        if event.actor >= 16 || event.request.len() != positions.len() - width {
            return Err("event encoding".into());
        }
        for (p, n) in positions[width..].iter().zip(&event.request) {
            if !in_domain(&p.domain, *n) {
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
            // Close the native successor relation from admitted genesis. The
            // later game product fixes a coalition and one guard; this union
            // of event/guard successors conservatively supplies all its rows.
            let mut states = vec![genesis.clone()];
            let mut seen = BTreeSet::from([genesis.clone()]);
            let mut at = 0;
            while at < states.len() {
                let pre = states[at].clone();
                for (event_id,event) in request.events.iter().enumerate() {
                    if rows.len() >= MAX_ROWS {
                        return Err(format!("native table edge budget: {} states, {} rows (limit {MAX_ROWS})",seen.len(),rows.len()));
                    }
                    if started.elapsed().as_secs_f64() >= 30.0 {
                        return Err(format!("native table time budget: {} states, {} rows (limit 30 seconds)",seen.len(),rows.len()));
                    }
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
                    let can_advance = enabled.is_empty() || enabled.iter().any(|enabled| *enabled);
                    rows.push(json!({"pre":numbers(&pre),"event":event_id,"post":numbers(&post),"class":class,"reason":reason,
                        "details":details,"holds":holds,"blocked_holds":blocked,"guards":enabled,
                        "publication_replayed":replayed,"refusal_repeated":refusal_repeated,
                        "usage_steps":usage.used(V2Resource::Step),"usage_reads":usage.used(V2Resource::Read)}));
                    if can_advance && !seen.contains(&post) {
                        if seen.len() >= MAX_REACHABLE_STATES {
                            return Err(format!("native reachable state budget: {} states, {} rows (limit {MAX_REACHABLE_STATES})",seen.len(),rows.len()));
                        }
                        if !positions[..width].iter().zip(&post).all(|(p,n)| in_domain(&p.domain,*n)) {
                            return Err("native successor outside declared state domain".into());
                        }
                        seen.insert(post.clone());
                        states.push(post);
                    }
                }
                at += 1;
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

#[cfg(test)]
mod acquisition_profiles {
    use super::*;

    fn request() -> Request {
        Request {
            project: include_str!("../../../test-data/trace-games/escrow-safe/project.zeno").into(),
            policy: include_str!("../../../test-data/trace-games/escrow-safe/v2/policy.json")
                .into(),
            events: Vec::new(),
            formula: String::new(),
            guards: Vec::new(),
        }
    }

    #[test]
    fn typed_policy_refuses_before_scalar_frames() {
        let mut input = request();
        input.policy = input.policy.replace("policy/2", "policy/4");
        let reason = run(input).unwrap_err();
        assert_eq!(
            reason,
            "unsigned/typed acquisition requires typed Authority frames; scalar i128 frames are not exact"
        );
        assert!(!reason.contains('\n'));
    }

    #[test]
    fn unsigned_declaration_refuses_before_scalar_frames() {
        let mut input = request();
        input
            .project
            .push_str("\ntype 999 u128 Wide in 0..=340282366920938463463374607431768211455;\n");
        let reason = run(input).unwrap_err();
        assert_eq!(
            reason,
            "unsigned/typed acquisition requires typed Authority frames; scalar i128 frames are not exact"
        );
    }

    #[test]
    fn retained_lineage_refuses_before_base_policy_replay() {
        let mut input = request();
        let mut policy: Value = serde_json::from_str(&input.policy).unwrap();
        policy["evolutions"] =
            json!([{"superseded_policy_sha256":"0".repeat(64),"review_sha256":"1".repeat(64)}]);
        input.policy = policy.to_string();
        let reason = run(input).unwrap_err();
        assert_eq!(
            reason,
            "retained-lineage acquisition requires the retained programs and receipt chain; base-policy replay is not exact"
        );
        assert!(!reason.contains('\n'));
    }

    #[test]
    fn large_cartesian_domain_only_charges_reachable_states() {
        let mut input = request();
        input.project = input
            .project
            .replace("Units in 0..=3;", "Units in 0..=1000000;");
        input.events = vec![Event {
            actor: 0,
            request: vec![160, 1, 170, 0],
        }];
        input.formula = "false".into();
        input.guards = vec!["true".into()];
        let result = run(input);
        assert!(
            result.is_ok(),
            "Cartesian size must not consume the reachable-state budget: {result:?}"
        );
        let packet =
            result.unwrap_or_else(|error| panic!("asserted complete acquisition: {error}"));
        let rows = packet["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("rows missing"));
        assert_eq!(rows.len(), 2, "genesis and the one funded state only");
        assert_eq!(rows[0]["class"], "Accept");
        assert_eq!(rows[1]["class"], "Reject");
        assert_eq!(rows[1]["pre"], rows[1]["post"], "reject is a stutter");
        assert!(rows.iter().all(|row| row["publication_replayed"] == true));
    }

    fn counter(max: u32) -> Request {
        Request {
            project: format!("zeno 1; project 1 counter; namespace 2 core;
                type 100 state State; type 101 command Command; type 102 context Context;
                type 105 int Count in 0..={max}; type 106 int Input in 0..=0;
                field 110 100 count 105; field 120 101 action 106; field 130 102 caller 106;
                reason 200 Limit precedence 0; reason 201 Failed precedence 1;
                component 400 core {{ owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 512; }}
                merge [400]; law 500 bounded on commit, genesis = post.100.110 <= {max};
                law 501 failure_step on failure = post.100.110 == pre.100.110 + 1;"),
            policy: json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"counter",
                "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{},"variables":{},
                "cases":[{"when":format!("pre.100.110 == {max}"),"class":"Reject","reason":200,"post":{},"outbox":[]},
                    {"when":true,"class":"CommittedFailure","reason":201,"post":{"110":"pre.100.110 + 1"},"outbox":[]}],
                "genesis":{"110":0},"law_kinds":{"500":"StateInvariant","501":"CommittedFailureEffects"}}).to_string(),
            events: vec![Event { actor: 0, request: vec![0, 0] }],
            formula: "false".into(),
            guards: vec!["true".into()],
        }
    }

    #[test]
    fn committed_failures_advance_and_blocked_guards_stutter() {
        let packet = run(counter(2)).unwrap_or_else(|error| panic!("counter: {error}"));
        let rows = packet["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("rows missing"));
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["class"], "CommittedFailure");
        assert_eq!(rows[1]["class"], "CommittedFailure");
        assert_eq!(rows[1]["post"], json!(["2"]));
        assert_eq!(rows[2]["class"], "Reject");
        assert!(rows.iter().all(|row| row["publication_replayed"] == true));
        let mut blocked = counter(2);
        blocked.guards = vec!["false".into()];
        let packet = run(blocked).unwrap_or_else(|error| panic!("blocked: {error}"));
        let rows = packet["rows"]
            .as_array()
            .unwrap_or_else(|| panic!("rows missing"));
        assert_eq!(
            rows.len(),
            1,
            "a disabled transition must not grow the frontier"
        );
        assert_eq!(rows[0]["guards"], json!([false]));
        assert_eq!(
            rows[0]["post"],
            json!(["1"]),
            "the native decision remains unchanged"
        );
    }

    #[test]
    fn reachable_state_and_row_limits_still_refuse_incomplete_tables() {
        let reason = run(counter(10_000))
            .err()
            .unwrap_or_else(|| panic!("state cap was bypassed"));
        assert!(
            reason.starts_with("native reachable state budget: 10000 states, 10000 rows"),
            "{reason}"
        );
        let mut input = counter(2);
        input.events = (0..=MAX_ROWS)
            .map(|_| Event {
                actor: 0,
                request: vec![0, 0],
            })
            .collect();
        let reason = run(input)
            .err()
            .unwrap_or_else(|| panic!("row cap was bypassed"));
        assert_eq!(reason, "native table edge budget");
    }
}
