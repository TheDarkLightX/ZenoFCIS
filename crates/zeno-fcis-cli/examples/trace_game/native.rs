//! Owned finite tables obtained from the existing native authority boundaries.
use super::engine::Outcome;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_contract::checked_composition::{
    self as composition, DomainSources, JointComposition, LeafSources, OutcomeKind,
};
use zeno_fcis_contract::internals::{
    declarations::Declarations,
    model::Contract,
    policy,
    review::{domain, evaluate},
    rules::Rules,
    schema, schema_commitment,
};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    V2Resource, v2_authority::PublicationOutcome, v2_composition as c,
};

pub(crate) fn hash(bytes: &[u8]) -> String {
    evaluate::hex(RustCryptoSha256::hash(bytes).as_bytes())
}

pub(crate) fn product(domains: &[Vec<i128>]) -> Vec<Vec<i128>> {
    domains.iter().fold(vec![vec![]], |rows, domain| {
        rows.into_iter()
            .flat_map(|row| {
                domain.iter().map(move |value| {
                    let mut next = row.clone();
                    next.push(*value);
                    next
                })
            })
            .collect()
    })
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Row {
    pub(crate) state: Vec<i128>,
    pub(crate) request: Vec<i128>,
    pub(crate) outcome: Outcome,
    pub(crate) complete_native: String,
    pub(crate) usage_steps: u64,
    pub(crate) usage_reads: u64,
    pub(crate) publication_replayed: bool,
    pub(crate) refusal_repeated: bool,
}

#[derive(Debug)]
pub(crate) struct Table {
    pub(crate) rows: Vec<Row>,
    lookup: BTreeMap<Vec<i128>, usize>,
    pub(crate) sources: Value,
    pub(crate) initialization: Value,
    pub(crate) composition_check: Option<Value>,
}

impl Table {
    pub(crate) fn new(
        rows: Vec<Row>,
        sources: Value,
        initialization: Value,
        composition_check: Option<Value>,
    ) -> Self {
        let lookup = rows
            .iter()
            .enumerate()
            .map(|(i, row)| (row.state.iter().chain(&row.request).copied().collect(), i))
            .collect();
        Self {
            rows,
            lookup,
            sources,
            initialization,
            composition_check,
        }
    }
    pub(crate) fn step(&self, state: &[i128], request: &[i128]) -> Result<Outcome, String> {
        if self
            .rows
            .first()
            .is_none_or(|row| row.state.len() != state.len() || row.request.len() != request.len())
        {
            return Err("native-table-arity".into());
        }
        let key: Vec<_> = state.iter().chain(request).copied().collect();
        self.lookup
            .get(&key)
            .map(|i| self.rows[*i].outcome.clone())
            .ok_or_else(|| "successor-or-request-outside-native-table".into())
    }
    pub(crate) fn json(&self) -> Value {
        json!({"sources":self.sources,"initialization":self.initialization,
            "composition_check":self.composition_check,"rows":self.rows})
    }
}

pub(crate) fn pins(source: &LeafSources) -> Result<[String; 3], String> {
    let rules = Rules::read(&source.rules).map_err(|e| e.to_string())?;
    let declarations =
        Declarations::read(&source.project, &rules.leaf_bindings).map_err(|e| e.to_string())?;
    let schema = schema::encode(&declarations).map_err(|e| e.to_string())?;
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let program =
        policy::with_authority(&contract, &schema, |authority| hash(authority.identity()))
            .map_err(|e| e.to_string())?;
    Ok([
        hash(source.project.as_bytes()),
        hash(source.rules.as_bytes()),
        program,
    ])
}

pub(crate) fn authority_table(source: &LeafSources) -> Result<Table, String> {
    let rules = Rules::read(&source.rules).map_err(|e| e.to_string())?;
    let declarations =
        Declarations::read(&source.project, &rules.leaf_bindings).map_err(|e| e.to_string())?;
    let schema = schema::encode(&declarations).map_err(|e| e.to_string())?;
    let contract = Contract::build(
        &declarations,
        &rules,
        schema_commitment(&schema).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let positions = domain::positions(&declarations).map_err(|e| e.to_string())?;
    let width = declarations
        .state_fields()
        .map_err(|e| e.to_string())?
        .len();
    let domains = positions
        .iter()
        .map(|p| p.domain.values_within(16).ok_or("wide fixture".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    policy::with_authority(&contract, &schema, |authority| {
        let framer = evaluate::Framer::new(&positions, &contract);
        let genesis=rules.genesis.values().map(|value|match value{
            zeno_fcis_contract::internals::rules::Constant::Bool(v)=>i128::from(*v),
            zeno_fcis_contract::internals::rules::Constant::Int(v)=>*v,
        }).collect::<Vec<_>>();
        if genesis.len()!=width{return Err("genesis-state-arity".into());}
        let mut genesis_tuple=domains.iter().map(|d|d[0]).collect::<Vec<_>>();
        genesis_tuple[..width].copy_from_slice(&genesis);
        let frames = framer.frame(&genesis_tuple);
        let mut initialization = match authority.publish_genesis(&frames[0]) {
            PublicationOutcome::Commit(p) => {
                let replay = authority.replay_genesis_publication(&frames[0], p.subject());
                if !matches!(replay, PublicationOutcome::Commit(ref q) if q.subject() == p.subject()) {
                    return Err("genesis-replay".into());
                }
                json!({"genesis":genesis,"subject_sha256":hash(p.subject()),"replayed":true})
            }
            _ => return Err("genesis-refused".into()),
        };
        if let Some(other)=domains[0].iter().copied().find(|v|*v!=genesis[0]){
            let mut wrong=genesis_tuple.clone();wrong[0]=other;let wrong=framer.frame(&wrong);
            if !matches!(authority.publish_genesis(&wrong[0]),PublicationOutcome::Refused{..}){return Err("wrong-genesis-state-not-refused".into());}
            initialization["wrong_genesis_refused"]=json!(true);
        }
        genesis_tuple.clear(); // invocation scratch has no retained aliases
        let mut rows = Vec::new();
        let mut altered_subject_refused=false;
        for tuple in product(&domains) {
            let state = tuple[..width].to_vec();
            let outcome = evaluate::evaluate(authority, &framer, &tuple);
            let (kind, reason, post) = match &outcome {
                evaluate::Outcome::Decision(d) => (
                    d.class.name().to_owned(), d.reason,
                    if d.class.name() == "Reject" { state.clone() } else {
                        d.post.iter().map(|(_, v)| v.number().ok_or("non-scalar native output".to_owned())).collect::<Result<Vec<_>,_>>()?
                    }),
                evaluate::Outcome::Refused(_) => ("Refused".into(), None, state.clone()),
            };
            let frames = framer.frame(&tuple);
            let raw = c::Raw { state:&frames[0], command:&frames[1], context:&frames[2] };
            let published = authority.publish(raw);
            let usage = published.evaluation().usage();
            let subject = published.evaluation().subject().ok();
            let replayed = if let Some(subject) = subject {
                if !altered_subject_refused {
                    let mut altered=subject.to_vec();
                    if let Some(byte)=altered.last_mut(){*byte^=1;}
                    if !matches!(authority.replay_publication(raw,&altered),PublicationOutcome::Refused{..}){return Err("altered-outcome-bytes-not-detected".into());}
                    altered_subject_refused=true;
                }
                let replay = authority.replay_publication(raw, subject);
                replay.evaluation().subject().ok() == Some(subject)
                    && format!("{:?}", replay.evaluation().result()) == format!("{:?}", published.evaluation().result())
            } else { false };
            let refusal_repeated=subject.is_none() && matches!(published,PublicationOutcome::Refused{..})
                && format!("{:?}",authority.evaluate(raw).result())==format!("{:?}",published.evaluation().result());
            if !replayed && !refusal_repeated { return Err("native-publication-replay".into()); }
            let complete_native=format!("{outcome:?}; patch={:?}; subject_sha256={:?}; reads={:?}",
                published.evaluation().result().ok().map(|candidate|format!("{:?}",candidate.patch())),subject.map(hash),published.evaluation().reads());
            rows.push(Row { state, request:tuple[width..].to_vec(),
                outcome:Outcome {kind,reason,post,component:None,events:vec![]},
                complete_native, usage_steps:usage.used(V2Resource::Step),
                usage_reads:usage.used(V2Resource::Read), publication_replayed:replayed,refusal_repeated });
        }
        initialization["altered_subject_refused"]=json!(altered_subject_refused);
        if source.project.contains("law 508 no_unintended_failure"){
            use evaluate::{LawSet,Proposed};
            use zeno_fcis_contract::internals::rules::Class;
            let selected=(500..=508).collect::<Vec<_>>();
            let laws=LawSet::new(authority.descriptor(),&selected);
            let states=product(&domains[..width]);
            let mut checked=0usize;let mut zero=0usize;let mut ambiguous=0usize;
            for tuple in product(&domains){
                let mut lawful=0;
                for reason in 200..=203{
                    lawful+=usize::from(laws.on_transition(&framer,&tuple,&Proposed{class:Class::Reject,reason:Some(reason),post:&tuple[..width]}).0);
                }
                for post in &states{
                    lawful+=usize::from(laws.on_transition(&framer,&tuple,&Proposed{class:Class::Accept,reason:None,post}).0);
                }
                // The independent failure law is constant false for every
                // reason/post. Check its application as well as its source form.
                if laws.on_transition(&framer,&tuple,&Proposed{class:Class::CommittedFailure,reason:Some(200),post:&tuple[..width]}).0{return Err("no-failure-law".into());}
                zero+=usize::from(lawful==0);ambiguous+=usize::from(lawful>1);checked+=1;
            }
            if zero!=0||ambiguous!=0{return Err(format!("declared-law-completeness:{zero}:{ambiguous}"));}
            initialization["gap_relation"]=json!({"mechanism":"existing gap-judge LawSet evaluator","declared_laws":selected,"excluded_framework_law":991,
                "raw_inputs":checked,"zero_lawful_outcomes":zero,"ambiguous_outcomes":ambiguous,"failure_exclusion":"declared constant-false on-failure law; no delivery/collection inheritance"});
        }
        Ok(Table::new(rows, json!({"project":source.project,"rules":source.rules}), initialization, None))
    }).map_err(|e| e.to_string())?
}

pub(crate) fn joint(
    manifest: &str,
    sources: &[LeafSources],
    requests: &[Vec<i128>],
) -> Result<Table, String> {
    let input = DomainSources {
        manifest,
        components: sources,
    };
    let checked = composition::check_domain(&input).map_err(|e| e.to_string())?;
    let check_json = checked.json();
    if checked.verdict() != composition::Verdict::Proved {
        return Err(format!("composition-obligations:{check_json}"));
    }
    let joint = JointComposition::from_sources(&input).map_err(|e| e.to_string())?;
    let leaf_publications = sources
        .iter()
        .map(authority_table)
        .collect::<Result<Vec<_>, _>>()?;
    let mut altered = sources.to_vec();
    if let Some(first) = altered.first_mut() {
        first.rules.push(' ');
    }
    if JointComposition::from_sources(&DomainSources {
        manifest,
        components: &altered,
    })
    .is_ok()
    {
        return Err("altered-leaf-binding-not-detected".into());
    }
    let domains: Vec<_> = joint
        .state_layout()
        .iter()
        .map(|slot| slot.domain.values())
        .collect();
    let mut rows = Vec::new();
    for state in product(&domains) {
        for request in requests {
            let outcome = joint
                .joint_step(&state, request)
                .map_err(|e| e.to_string())?;
            let (kind, reason, component) = match &outcome.kind {
                OutcomeKind::Accept => ("Accept", None, None),
                OutcomeKind::Reject { component, reason } => {
                    ("Reject", *reason, Some(component.clone()))
                }
                OutcomeKind::CommittedFailure { component, reason } => {
                    ("CommittedFailure", *reason, Some(component.clone()))
                }
                OutcomeKind::Refused { component, .. } => {
                    ("Refused", None, Some(component.clone()))
                }
                _ => return Err("unknown-joint-outcome".into()),
            };
            rows.push(Row {
                state: state.clone(),
                request: request.clone(),
                outcome: Outcome {
                    kind: kind.into(),
                    reason,
                    component,
                    post: outcome.successor.clone(),
                    events: vec![],
                },
                complete_native: outcome.json().to_string(),
                usage_steps: 0,
                usage_reads: 0,
                // Separate complete leaf publication/replay tables are retained below.
                publication_replayed: kind != "Refused",
                refusal_repeated: kind == "Refused"
                    && joint
                        .joint_step(&state, request)
                        .is_ok_and(|again| again == outcome),
            });
        }
    }
    Ok(Table::new(
        rows,
        json!({"manifest":manifest,"leaves":sources.iter().map(|s|json!({"project":s.project,"rules":s.rules})).collect::<Vec<_>>()}),
        json!({"genesis":joint.genesis(),"leaf_publications":leaf_publications.iter().map(Table::json).collect::<Vec<_>>(),"altered_binding_refused":true}),
        Some(check_json),
    ))
}

pub(crate) fn leaf(
    name: &str,
    state: &[(&str, &str)],
    command: &[(&str, &str)],
    context: &[(&str, &str)],
    cases: Value,
    genesis: Value,
) -> LeafSources {
    let mut project = format!(
        "zeno 1;\nproject 1 {name};\nnamespace 2 fixture;\ntype 100 state State;\ntype 101 command Command;\ntype 102 context Context;\ntype 105 int Units in 0..=2;\ntype 106 bool Flag;\ntype 107 int Operation in 0..=5;\n"
    );
    let mut variables = serde_json::Map::new();
    for (root, base, fields, prefix) in [
        (100, 110, state, "pre"),
        (101, 120, command, "command"),
        (102, 130, context, "context"),
    ] {
        for (i, (field, ty)) in fields.iter().enumerate() {
            let id = base + i;
            let type_id = match *ty {
                "bool" => 106,
                "op" => 107,
                _ => 105,
            };
            project.push_str(&format!("field {id} {root} {field} {type_id};\n"));
            variables.insert((*field).into(), json!(format!("{prefix}.{root}.{id}")));
        }
    }
    let mut ordered_reasons = Vec::new();
    if let Some(rows) = cases.as_array() {
        for row in rows {
            if let Some(reason) = row["reason"].as_u64()
                && !ordered_reasons.contains(&reason)
            {
                ordered_reasons.push(reason);
            }
        }
    }
    for (i, name) in ["unauthorized", "stale", "unavailable", "guarded"]
        .iter()
        .enumerate()
    {
        if cases.as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["reason"].as_u64() == Some((200 + i) as u64))
        }) {
            let rank = ordered_reasons
                .iter()
                .position(|r| *r == (200 + i) as u64)
                .unwrap_or(0);
            project.push_str(&format!("reason {} {name} precedence {rank};\n", 200 + i));
        }
    }
    project.push_str("component 400 machine { owns 100; reads pre.100; writes post.100; contexts context.102; budget steps 4096; }\nmerge [400];\nlaw 500 bounded on commit, genesis = post.100.110 >= 0;\n");
    let rules = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":name,
        "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"106":["Bool"]},
        "variables":variables,"cases":cases,"genesis":genesis,"law_kinds":{"500":"StateInvariant"}});
    LeafSources {
        project,
        rules: rules.to_string(),
    }
}

pub(crate) fn case(when: &str, class: &str, reason: Option<u32>, post: Value) -> Value {
    json!({"when":when,"class":class,"reason":reason,"post":post,"outbox":[]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn results_are_owned_and_arity_cannot_alias_a_key() {
        let row = Row {
            state: vec![1],
            request: vec![0],
            outcome: Outcome {
                kind: "Reject".into(),
                reason: Some(200),
                component: None,
                post: vec![1],
                events: vec![],
            },
            complete_native: "test".into(),
            usage_steps: 7,
            usage_reads: 1,
            publication_replayed: true,
            refusal_repeated: false,
        };
        let table = Table::new(vec![row], json!({}), json!({}), None);
        let mut result = table.step(&[1], &[0]).unwrap_or_else(|e| panic!("{e}"));
        result.post[0] = 9;
        assert_eq!(
            table
                .step(&[1], &[0])
                .unwrap_or_else(|e| panic!("{e}"))
                .post,
            [1]
        );
        assert!(table.step(&[1, 0], &[]).is_err());
        assert_eq!(table.rows[0].usage_steps, 7);
    }
}
