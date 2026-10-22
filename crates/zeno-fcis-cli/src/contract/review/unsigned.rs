//! Full-width review adapters. Only the library Authority decides transitions;
//! selected law checks use explicit immutable frames, with no publication authority.
use super::super::{
    declarations::{Declarations, Source},
    model::{Contract, InputLeaf, RootSchema},
    policy,
    rules::{Class, Constant, LawKind, Rules},
    schema, schema_commitment,
};
use super::evaluate::{self, LawSet, Outcome};
use super::machine::Inconclusive;
use super::{
    ContractError, Review, ReviewSources, Summary, gaps, lemma, mutants, refusals, table::Table,
};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    self, V2NumericAtom as Atom, V2NumericDomain as Domain, canonical_v2::output,
    v2_composition as c, v2_laws as l,
};
#[path = "../../../contract-app/src/numeric_examples.rs"]
pub(super) mod examples;
#[path = "unsigned_probe.rs"]
mod numeric_probe;
pub(super) fn count_probe(
    project: &str,
    rules: &str,
    condition: &str,
    max: u64,
) -> Result<super::probe::Answer, ContractError> {
    numeric_probe::count_query(project, rules, condition, max)
}
fn error(reason: impl Into<String>) -> ContractError {
    ContractError::new("numeric review", reason)
}
fn count(d: Domain<'_>) -> Option<u128> {
    match d {
        Domain::Bool => Some(2),
        Domain::I128 { min, max } => min.abs_diff(max).checked_add(1),
        Domain::U128 { min, max } => max.checked_sub(min)?.checked_add(1),
        _ => None,
    }
}
fn values(d: Domain<'static>, limit: u64) -> Option<Vec<Atom<'static>>> {
    if count(d)? > u128::from(limit) {
        return None;
    }
    Some(match d {
        Domain::Bool => vec![Atom::Bool(false), Atom::Bool(true)],
        Domain::I128 { min, max } => (min..=max).map(Atom::I128).collect(),
        Domain::U128 { min, max } => (min..=max).map(Atom::U128).collect(),
        _ => return None,
    })
}
fn boundary(d: Domain<'static>) -> Vec<Atom<'static>> {
    match d {
        Domain::Bool => vec![Atom::Bool(false), Atom::Bool(true)],
        Domain::I128 { min, max } => {
            let mut v = vec![min, max];
            v.extend(
                [min.saturating_add(1), max.saturating_sub(1), -1, 0, 1]
                    .into_iter()
                    .filter(|n| min <= *n && *n <= max),
            );
            v.sort_unstable();
            v.dedup();
            v.into_iter().map(Atom::I128).collect()
        }
        Domain::U128 { min, max } => {
            let mut v = vec![min, max];
            v.extend(
                [
                    min.saturating_add(1),
                    max.saturating_sub(1),
                    0,
                    1,
                    i128::MAX as u128,
                    1u128 << 127,
                ]
                .into_iter()
                .filter(|n| min <= *n && *n <= max),
            );
            v.sort_unstable();
            v.dedup();
            v.into_iter().map(Atom::U128).collect()
        }
        _ => Vec::new(),
    }
}
fn product(values: &[Vec<Atom<'static>>], limit: usize) -> Vec<Vec<Atom<'static>>> {
    let mut rows = vec![Vec::new()];
    for vs in values {
        let mut next = Vec::new();
        for row in rows {
            for v in vs {
                if next.len() == limit {
                    break;
                }
                let mut r = row.clone();
                r.push(*v);
                next.push(r)
            }
        }
        rows = next;
    }
    rows
}
struct Space {
    rows: Vec<Vec<Atom<'static>>>,
    states: Vec<Vec<Atom<'static>>>,
    state_width: usize,
    full: bool,
    size: Option<u128>,
}
impl Space {
    fn new(c: &Contract<'_>, limit: u64) -> Result<Self, ContractError> {
        if limit == 0 {
            return Err(error("tuple limit must be positive"));
        }
        let sizes: Vec<_> = c
            .input_domains
            .iter()
            .enumerate()
            .map(|(i, d)| match input_leaf(c, i) {
                InputLeaf::Sum { variants, .. } => Some(variants.len() as u128),
                _ => count(policy::numeric_domain(d)),
            })
            .collect();
        let size = sizes.iter().try_fold(1u128, |n, s| n.checked_mul((*s)?));
        let full = size.is_some_and(|n| n <= u128::from(limit));
        let cols: Vec<_> = c
            .input_domains
            .iter()
            .enumerate()
            .map(|(i, d)| match input_leaf(c, i) {
                InputLeaf::Sum { variants, .. } => variants
                    .iter()
                    .map(|v| Atom::I128(i128::from(*v)))
                    .collect(),
                _ => {
                    let d = policy::numeric_domain(d);
                    if full {
                        values(d, limit).unwrap_or_default()
                    } else {
                        boundary(d)
                    }
                }
            })
            .collect();
        if cols.iter().any(Vec::is_empty) {
            return Err(error("unsupported numeric review domain"));
        }
        let cap =
            usize::try_from(limit.max(1)).map_err(|_| error("tuple limit exceeds host size"))?;
        let rows = if full {
            product(&cols, cap)
        } else {
            // Vary every column about the same base before taking any extra Cartesian rows.
            let base: Vec<_> = cols.iter().map(|c| c[0]).collect();
            let mut rows = vec![base.clone()];
            for (i, col) in cols.iter().enumerate() {
                for v in col {
                    if rows.len() >= cap {
                        break;
                    }
                    let mut row = base.clone();
                    row[i] = *v;
                    if !rows.contains(&row) {
                        rows.push(row)
                    }
                }
            }
            rows
        };
        let state_width = c
            .bindings
            .iter()
            .take_while(|(s, _)| *s == Source::State)
            .count();
        let states = if full {
            product(&cols[..state_width], cap)
        } else {
            Vec::new()
        };
        Ok(Self {
            rows,
            states,
            state_width,
            full,
            size,
        })
    }
}
pub(super) fn input_leaf<'a>(c: &'a Contract<'_>, index: usize) -> &'a InputLeaf {
    let (source, id) = c.bindings[index];
    let root = match source {
        Source::State => 0,
        Source::Command => 1,
        Source::Context => 2,
    };
    match &c.roots[root] {
        RootSchema::Leaf(leaf) => leaf,
        RootSchema::Record(fields) => {
            &fields
                .iter()
                .find(|(field, _)| Some(*field) == id)
                .unwrap_or_else(|| panic!("compiler binding belongs to root"))
                .1
        }
    }
}
pub(super) fn fields(
    c: &Contract<'_>,
    row: &[Atom<'static>],
    source: Source,
) -> Vec<c::Field<'static>> {
    c.bindings
        .iter()
        .zip(row)
        .enumerate()
        .filter(|(_, ((s, _), _))| *s == source)
        .map(|(i, ((_, id), v))| c::Field {
            id: id.unwrap_or(0),
            value: match (input_leaf(c, i), v) {
                (InputLeaf::Sum { type_id, .. }, Atom::I128(n)) => Atom::Sum {
                    type_id: *type_id,
                    variant: u16::try_from(*n).unwrap_or_else(|_| panic!("enumerated variant")),
                },
                _ => *v,
            },
        })
        .collect()
}
pub(super) fn view<'a>(
    c: &Contract<'_>,
    index: usize,
    fields: &'a [c::Field<'static>],
) -> l::RootView<'a, 'static> {
    if matches!(c.roots[index], RootSchema::Leaf(_)) {
        l::RootView::Leaf(fields[0].value)
    } else {
        l::RootView::Record(fields)
    }
}
pub(super) fn frame(
    c: &Contract<'_>,
    row: &[Atom<'static>],
) -> Result<[Vec<u8>; 3], ContractError> {
    let mut result = [Vec::new(), Vec::new(), Vec::new()];
    for (i, source) in [Source::State, Source::Command, Source::Context]
        .into_iter()
        .enumerate()
    {
        let fs = fields(c, row, source);
        let max = c.frame_bytes[i] as usize;
        let data = if matches!(c.roots[i], RootSchema::Leaf(_)) {
            output::encode_atom(fs[0].value, max)
        } else {
            output::encode_record(&fs, max)
        }
        .map_err(|e| error(format!("numeric frame: {e:?}")))?;
        result[i] = output::encode_envelope(100 + i as u32, &c.commitment, &data, max)
            .map_err(|e| error(format!("numeric envelope: {e:?}")))?;
    }
    Ok(result)
}
pub(super) fn atom_json(v: Atom<'_>) -> Value {
    match v {
        Atom::Bool(v) => json!({"kind":"bool","value":v}),
        Atom::I128(v) => json!({"kind":"i128","value":v.to_string()}),
        Atom::U128(v) => json!({"kind":"u128","value":v.to_string()}),
        Atom::Sum { type_id, variant } => json!({"kind":"sum","type_id":type_id,"variant":variant}),
        _ => json!({"kind":"unsupported"}),
    }
}
pub(super) fn normalized(v: &Outcome) -> Value {
    match v {
        Outcome::Refused(r) => json!({"class":"refused","reason":r.text,"law":r.law}),
        Outcome::Decision(d) => {
            json!({"class":d.class.name(),"reason":d.reason,"post_digest":evaluate::hex(&d.post_digest),"outbox_digest":evaluate::hex(&d.outbox_digest),"post":d.post.iter().map(|(id,v)|json!([id,v.json()])).collect::<Vec<_>>(),"outbox":d.outbox.iter().map(|d|json!({"ordinal":d.ordinal,"channel":d.channel,"destination":d.destination.json(),"idempotency":d.idempotency.json(),"payload":d.payload.iter().map(|(id,v)|json!([id,v.json()])).collect::<Vec<_>>()})).collect::<Vec<_>>()})
        }
    }
}
pub(super) fn same_value(a: &evaluate::Value, b: Atom<'_>) -> bool {
    match (a, b) {
        (evaluate::Value::Bool(a), Atom::Bool(b)) => *a == b,
        (evaluate::Value::I128(a), Atom::I128(b)) => *a == b,
        (evaluate::Value::U128(a), Atom::U128(b)) => *a == b,
        (evaluate::Value::Sum { variant, .. }, Atom::I128(v)) => i128::from(*variant) == v,
        (
            evaluate::Value::Sum {
                type_id: a,
                variant: x,
            },
            Atom::Sum {
                type_id: b,
                variant: y,
            },
        ) => *a == b && *x == y,
        _ => false,
    }
}
fn state_ids(d: &c::NumericDescriptor<'_>) -> Vec<u32> {
    evaluate::state_law_ids(d.laws)
}
fn state_check(c: &Contract<'_>, row: &[Atom<'static>], laws: &LawSet<'_>) -> Option<u32> {
    let fs = fields(c, row, Source::State);
    laws.first_unsatisfied_state_law(view(c, 0, &fs))
}
fn evaluate(
    c: &Contract<'_>,
    a: &NumericAuthority<'_>,
    row: &[Atom<'static>],
) -> Result<Outcome, ContractError> {
    let f = frame(c, row)?;
    Ok(evaluate::from_evaluation(&a.evaluate(c::Raw {
        state: &f[0],
        command: &f[1],
        context: &f[2],
    })))
}
pub(super) fn review(
    sources: ReviewSources<'_>,
    max: u64,
    options: Option<gaps::Options<'_>>,
) -> Result<Review, ContractError> {
    let rules = Rules::read_for_project(sources.project, sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let space = Space::new(&contract, max)?;
    policy::with_numeric_authority(&contract, &schema, |authority| {
        let descriptor = authority.descriptor();
        let laws = LawSet::new_numeric(descriptor, &state_ids(descriptor));
        let mut table = Table::default();
        let mut observed = Vec::new();
        let mut rows = Vec::new();
        for tuple in &space.rows {
            let outcome = evaluate(&contract, authority, tuple)?;
            let bad = state_check(&contract, tuple, &laws);
            table.record(&outcome, bad);
            rows.push(json!({"inputs":tuple.iter().copied().map(atom_json).collect::<Vec<_>>(),"outcome":normalized(&outcome)}));
            observed.push(outcome);
        }
        let examples = examples::parse(sources.examples.unwrap_or(""), descriptor)
            .map_err(|e| error(format!("examples line {}: {}", e.line, e.reason)))?;
        let mut disagreements = Vec::new();
        for example in &examples {
            let result = evaluate(&contract, authority, &example.inputs)?;
            let agrees = match &result {
                Outcome::Decision(d) => {
                    d.class
                        == match example.class {
                            examples::Class::Accept => Class::Accept,
                            examples::Class::Reject => Class::Reject,
                            examples::Class::Failure => Class::CommittedFailure,
                        }
                        && d.reason == example.reason
                        && if d.class == Class::Reject {
                            d.post.is_empty() && example.post == example.inputs[..space.state_width]
                        } else {
                            d.post.len() == example.post.len()
                                && d.post
                                    .iter()
                                    .zip(&example.post)
                                    .all(|((_, a), b)| same_value(a, *b))
                        }
                        && d.outbox.len() == example.outbox.len()
                        && d.outbox
                            .iter()
                            .zip(&example.outbox)
                            .all(|(a, (channel, values))| {
                                a.channel == *channel
                                    && a.payload.len() == values.len()
                                    && a.payload
                                        .iter()
                                        .zip(values)
                                        .all(|((_, a), b)| same_value(a, *b))
                            })
                }
                _ => false,
            };
            if !agrees {
                disagreements.push(json!({"line":example.number,"inputs":example.inputs.iter().copied().map(atom_json).collect::<Vec<_>>(),"expected_post":example.post.iter().copied().map(atom_json).collect::<Vec<_>>(),"actual":normalized(&result)}));
            }
        }
        let catalog = if options.is_some() {
            mutants::extended_catalog(&rules, &declarations)
        } else {
            mutants::catalog(&rules, &declarations)
        };
        let mut classified = Vec::new();
        let (mut generator, mut library, mut different, mut equal) = (0, 0, 0, 0);
        for mutant in &catalog {
            match Contract::build(&declarations, &mutant.rules, contract.commitment) {
                Err(e) => {
                    generator += 1;
                    classified.push(
                        json!({"id":mutant.id,"status":"generator-refused","reason":e.reason()}),
                    );
                }
                Ok(c) => {
                    let check = policy::with_numeric_authority(&c, &schema, |a| {
                        for (i, row) in space.rows.iter().enumerate() {
                            if evaluate(&c, a, row)? != observed[i] {
                                return Ok::<_, ContractError>(Some(i));
                            }
                        }
                        Ok(None)
                    });
                    match check {
                        Err(e) => {
                            library += 1;
                            classified.push(json!({"id":mutant.id,"status":"library-refused","reason":e.reason()}));
                        }
                        Ok(result) => {
                            if let Some(i) = result? {
                                different += 1;
                                classified
                                    .push(json!({"id":mutant.id,"status":"distinguished","row":i}));
                            } else {
                                equal += 1;
                                classified.push(json!({"id":mutant.id,"status":if space.full{"equivalent-over-full-domain"}else{"not-distinguished-within-boundary-set"}}));
                            }
                        }
                    }
                }
            }
        }
        let groups = refusals::groups(&table);
        let tally = refusals::Tally::of(&table, &groups);
        let law_findings:Vec<_>=groups.iter().filter(|g|g.unsatisfied.is_none()&&table.refusals[g.refusal as usize].class==evaluate::RefusalClass::Law).map(|g|{
            let refusal=&table.refusals[g.refusal as usize];
            let tuple=&space.rows[g.first];
            let words=tuple.iter().map(|v|match v{Atom::Bool(v)=>u8::from(*v).to_string(),Atom::I128(v)=>v.to_string(),Atom::U128(v)=>v.to_string(),_=>format!("{v:?}")}).collect::<Vec<_>>().join(" ");
            let law=refusal.law.map_or_else(||"a law".to_owned(),|id|format!("law {id}"));
            let line=format!("law refusal: {law} refuses {} inputs whose pre-state satisfies every state law ({}); first {words}",g.count,refusal.text);
            (line,json!({"law":refusal.law,"refusal":refusal.text,"inputs":g.count,"ordinal":g.first,"input":tuple.iter().copied().map(atom_json).collect::<Vec<_>>()}))
        }).collect();
        let law_refusals = law_findings.len();
        let gaps = options
            .map(|o| {
                gap_measure(
                    (sources.project, &schema),
                    &contract,
                    authority,
                    &space,
                    &rules,
                    &observed,
                    o,
                )
            })
            .transpose()?;
        let summary = Summary {
            application: rules.template.clone(),
            construction: if space.full {
                "full-domain"
            } else {
                "boundary-set"
            },
            inputs: space.rows.len(),
            examples: examples.len(),
            disagreements: disagreements.len(),
            mutants: catalog.len(),
            distinguished: different,
            refused_by_generator: generator,
            refused_by_library: library,
            equivalent: if space.full { equal } else { 0 },
            undistinguished: if space.full { 0 } else { equal },
            refusals: tally,
            law_refusals,
            findings: disagreements.len() + law_refusals,
            gaps: gaps.as_ref().map(gaps::Gaps::summary),
        };
        let packet = json!({"schema":"zeno-fcis/contract-review/4","authority":"none","identity":evaluate::hex(authority.identity()),"summary":summary.json(),"domain_size":space.size.map(|s|s.to_string()),"scope":if space.full{"whole declared domain"}else{"boundary probes; no equivalence claim"},"rows":rows,"example_disagreements":disagreements,"law_refusals":law_findings.iter().map(|(_,v)|v).collect::<Vec<_>>(),"mutants":classified,"gaps":gaps.as_ref().map(gaps::Gaps::json)});
        Ok(Review {
            packet: serde_json::to_string(&packet).map_err(|e| error(e.to_string()))? + "\n",
            summary,
            law_refusals: law_findings.into_iter().map(|(line, _)| line).collect(),
            gaps,
        })
    })?
}
fn proposed(
    c: &Contract<'_>,
    row: &[Atom<'static>],
    post: &[Atom<'static>],
    class: c::Class,
    reason: Option<u32>,
    laws: &LawSet<'_>,
    usage: finite::V2Usage,
) -> bool {
    let pre = fields(c, row, Source::State);
    let command = fields(c, row, Source::Command);
    let context = fields(c, row, Source::Context);
    let after: Vec<_> = if class == c::Class::Reject {
        Vec::new()
    } else {
        {
            let mut tuple = row.to_vec();
            tuple[..post.len()].copy_from_slice(post);
            fields(c, &tuple, Source::State)
        }
    };
    let patch: Vec<_> = pre
        .iter()
        .zip(&after)
        .filter(|(a, b)| a.value != b.value)
        .map(|(a, b)| l::Patch {
            field: a.id,
            before: a.value,
            after: b.value,
        })
        .collect();
    let candidate = l::Candidate {
        class,
        reason,
        post: if class == c::Class::Reject {
            l::RootView::Record(&after)
        } else {
            view(c, 0, &after)
        },
        patch: &patch,
        effects: &[],
        outbox: &[],
        reads: &[],
        attempts: &[],
        usage,
    };
    laws.on_frame(&l::Frame::Transition {
        pre: view(c, 0, &pre),
        command: view(c, 1, &command),
        context: view(c, 2, &context),
        candidate: &candidate,
    })
    .0
}
fn actual_laws(
    c: &Contract<'_>,
    a: &NumericAuthority<'_>,
    row: &[Atom<'static>],
    laws: &LawSet<'_>,
) -> Result<Option<(bool, Vec<l::Diagnostic>)>, ContractError> {
    let f = frame(c, row)?;
    let e = a.evaluate(c::Raw {
        state: &f[0],
        command: &f[1],
        context: &f[2],
    });
    let Ok(v) = e.result() else { return Ok(None) };
    let fs = [
        fields(c, row, Source::State),
        fields(c, row, Source::Command),
        fields(c, row, Source::Context),
    ];
    let candidate = l::Candidate {
        class: v.class(),
        reason: v.reason(),
        post: if matches!(c.roots[0], RootSchema::Leaf(_)) {
            l::RootView::Leaf(v.post()[0].value)
        } else {
            l::RootView::Record(v.post())
        },
        patch: v.patch(),
        effects: v.effects(),
        outbox: v.outbox(),
        reads: e.reads(),
        attempts: e.decision_attempts(),
        usage: e.decision_usage().unwrap_or(e.usage()),
    };
    Ok(Some(laws.on_frame(&l::Frame::Transition {
        pre: view(c, 0, &fs[0]),
        command: view(c, 1, &fs[1]),
        context: view(c, 2, &fs[2]),
        candidate: &candidate,
    })))
}
#[derive(Default)]
struct GapCounts {
    tuples: usize,
    refused: usize,
    determined: usize,
    none: usize,
    undetermined: usize,
    admitted: usize,
    caught: usize,
}
impl GapCounts {
    fn json(&self) -> Value {
        json!({"tuples":self.tuples,"refused":self.refused,"determined":self.determined,"undetermined":self.undetermined,"no_admitted_decision":self.none,"admitted":self.admitted,"caught":self.caught})
    }
}
fn gap_measure(
    (project, schema): (&str, &[u8]),
    c: &Contract<'_>,
    a: &NumericAuthority<'_>,
    space: &Space,
    rules: &Rules,
    observed: &[Outcome],
    options: gaps::Options<'_>,
) -> Result<gaps::Gaps, ContractError> {
    if !space.full {
        return Ok(gaps::Gaps::Inconclusive(Inconclusive {
            reason: "domain-exceeds-limit",
            size: space.size,
            limit: options.max_tuples,
        }));
    }
    if rules.cases.iter().any(|c| !c.outbox.is_empty()) {
        return Ok(gaps::Gaps::OutOfScope(
            "candidate enumeration excludes deliveries".into(),
        ));
    }
    let d = a.descriptor();
    let failure = rules
        .cases
        .iter()
        .any(|c| c.class == Class::CommittedFailure);
    let failures = if failure {
        d.reasons
            .iter()
            .filter(|r| r.class == c::Class::CommittedFailure)
            .count()
    } else {
        0
    };
    let rejects = d
        .reasons
        .iter()
        .filter(|r| r.class == c::Class::Reject)
        .count();
    let candidates = (space.states.len() as u128)
        .checked_mul(1 + failures as u128)
        .and_then(|n| n.checked_add(rejects as u128));
    let work = candidates
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(space.rows.len() as u128))
        .and_then(|n| n.checked_mul(c.declarations.laws.len() as u128));
    let needed = match work {
        Some(n) if n <= options.max_work as u128 => n,
        _ => {
            return Ok(gaps::Gaps::Inconclusive(Inconclusive {
                reason: "work-exceeds-limit",
                size: work,
                limit: options.max_work,
            }));
        }
    };
    let before = gap_metrics(schema, c, a, space, rules, observed, needed)?;
    let Some(proposals) = options.proposals else {
        return Ok(gaps::Gaps::Measured(before));
    };
    let lines: Vec<_> = proposals
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .collect();
    if lines.is_empty()
        || lines.iter().any(|l| {
            !l.starts_with("law ")
                || !l.ends_with(';')
                || l.matches(';').count() != 1
                || l.contains("//")
                || l.contains("/*")
        })
    {
        return Ok(gaps::Gaps::InvalidProposals {
            place: "proposed laws".into(),
            reason: "requires one new law declaration per line".into(),
        });
    }
    let source = format!("{project}\n{}\n", lines.join("\n"));
    let extended = (|| {
        let decl = Declarations::read(&source, &rules.leaf_bindings)?;
        let ids: Vec<_> = decl
            .laws
            .iter()
            .filter(|l| !c.declarations.laws.iter().any(|old| old.id == l.id))
            .map(|l| l.id)
            .collect();
        if ids.len() != lines.len() {
            return Err(error("proposals must each introduce one new law"));
        }
        let mut r = rules.clone();
        for id in &ids {
            r.law_kinds.insert(*id, LawKind::FeeAndRounding);
        }
        Ok((decl, r, ids))
    })();
    let (decl, r, ids) = match extended {
        Ok(v) => v,
        Err(e) => {
            return Ok(gaps::Gaps::InvalidProposals {
                place: e.place().into(),
                reason: e.reason().into(),
            });
        }
    };
    let bytes = schema::encode(&decl)?;
    let probe = Contract::build(&decl, &r, schema_commitment(&bytes)?)?;
    policy::with_numeric_authority(&probe, &bytes, |pa| {
        let mut activation = Vec::new();
        for id in &ids {
            let laws = LawSet::new_numeric(pa.descriptor(), &[*id]);
            let mut active = 0;
            for (i, row) in space.rows.iter().enumerate() {
                if let Some((ok, ds)) = actual_laws(c, a, row, &laws)? {
                    if let Some(diag) = ds.iter().find(|diag| diag.id == *id)
                        && !matches!(diag.verdict, l::Verdict::Skipped)
                    {
                        active += 1;
                    }
                    if !ok {
                        return Ok(gaps::Gaps::FalseAlarm(
                            json!({"status":"proposal-false-alarm","law":id,"row":i,"actual":normalized(&observed[i])}),
                        ));
                    }
                }
            }
            activation.push(json!({"id":id,"active":active}));
        }
        let mut after_options = options;
        after_options.proposals = None;
        let after = gap_measure(
            (&source, &bytes),
            &probe,
            pa,
            space,
            &r,
            observed,
            after_options,
        )?;
        if !matches!(after, gaps::Gaps::Measured(_)) {
            return Ok(after);
        }
        let mut result = before.clone();
        result["proposals"] = json!({"laws":activation,"after":after.json()});
        Ok(gaps::Gaps::Measured(result))
    })?
}
fn gap_metrics(
    schema: &[u8],
    c: &Contract<'_>,
    a: &NumericAuthority<'_>,
    space: &Space,
    rules: &Rules,
    observed: &[Outcome],
    work: u128,
) -> Result<Value, ContractError> {
    let d = a.descriptor();
    let ids: Vec<_> = c.declarations.laws.iter().map(|l| l.id).collect();
    let laws = LawSet::new_numeric(d, &ids);
    let state_ids = state_ids(d);
    let state_laws = LawSet::new_numeric(d, &state_ids);
    let valid: Vec<_> = space
        .rows
        .iter()
        .map(|row| state_check(c, row, &state_laws).is_none())
        .collect();
    let mut full = GapCounts::default();
    let mut valid_counts = GapCounts::default();
    let mut activation = [vec![0usize; ids.len()], vec![0usize; ids.len()]];
    let empty = [];
    let (_, usage, _, _) = l::evaluate(
        &[],
        &[],
        &l::Frame::Genesis {
            initial: l::RootView::Record(&empty),
        },
        d.limits,
    )
    .into_parts();
    let mut witnesses = Vec::new();
    let failure = rules
        .cases
        .iter()
        .any(|c| c.class == Class::CommittedFailure);
    for (i, row) in space.rows.iter().enumerate() {
        let reference = actual_laws(c, a, row, &laws)?;
        let mut admitted = 0;
        if let Some((ok, ds)) = &reference {
            if !ok {
                return Err(error("gap reference frame fails declared laws"));
            }
            for (j, id) in ids.iter().enumerate() {
                if ds
                    .iter()
                    .any(|diag| diag.id == *id && !matches!(diag.verdict, l::Verdict::Skipped))
                {
                    activation[0][j] += 1;
                    if valid[i] {
                        activation[1][j] += 1;
                    }
                }
            }
            for post in &space.states {
                if proposed(c, row, post, c::Class::Accept, None, &laws, usage) {
                    admitted += 1
                }
                if failure {
                    for reason in d
                        .reasons
                        .iter()
                        .filter(|r| r.class == c::Class::CommittedFailure)
                    {
                        if proposed(
                            c,
                            row,
                            post,
                            c::Class::CommittedFailure,
                            Some(reason.id),
                            &laws,
                            usage,
                        ) {
                            admitted += 1
                        }
                    }
                }
            }
            for reason in d.reasons.iter().filter(|r| r.class == c::Class::Reject) {
                if proposed(c, row, &[], c::Class::Reject, Some(reason.id), &laws, usage) {
                    admitted += 1
                }
            }
        }
        for counts in std::iter::once(&mut full).chain(if valid[i] {
            Some(&mut valid_counts)
        } else {
            None
        }) {
            counts.tuples += 1;
            if reference.is_none() {
                counts.refused += 1
            } else {
                match admitted {
                    0 => counts.none += 1,
                    1 => counts.determined += 1,
                    _ => counts.undetermined += 1,
                }
            }
        }
        if admitted > 1 && witnesses.len() < 3 {
            witnesses.push(
                json!({"row":i,"admissible_decisions":admitted,"actual":normalized(&observed[i])}),
            );
        }
    }
    let mut mutation = [GapCounts::default(), GapCounts::default()];
    let mut records = Vec::new();
    for mutant in mutants::extended_catalog(rules, c.declarations) {
        let mut status = "generator-refused";
        if let Ok(m) = Contract::build(c.declarations, &mutant.rules, c.commitment) {
            let checked = policy::with_numeric_authority(&m, schema, |ma| {
                let mut changed = [false; 2];
                let mut caught = [false; 2];
                for (i, row) in space.rows.iter().enumerate() {
                    if !matches!(observed[i], Outcome::Decision(_)) {
                        continue;
                    }
                    let result = evaluate(&m, ma, row)?;
                    if result != observed[i] {
                        for j in 0..2 {
                            if j == 0 || valid[i] {
                                changed[j] = true;
                                if matches!(result, Outcome::Refused(_)) {
                                    caught[j] = true
                                }
                            }
                        }
                    }
                }
                Ok::<_, ContractError>((changed, caught))
            });
            if let Ok(result) = checked {
                let (changed, caught) = result?;
                status = if !changed[0] {
                    "equivalent"
                } else if caught[0] {
                    "caught"
                } else {
                    "admitted"
                };
                for j in 0..2 {
                    if changed[j] {
                        if caught[j] {
                            mutation[j].caught += 1
                        } else {
                            mutation[j].admitted += 1
                        }
                    }
                }
            } else {
                status = "library-refused"
            }
        }
        records.push(json!({"id":mutant.id,"status":status}));
    }
    let reject = if let Some((r, reason)) = super::machine::reject_everything(rules) {
        match Contract::build(c.declarations, &r, c.commitment) {
            Err(e) => json!({"status":"generator-refused","reason":e.reason()}),
            Ok(m) => match policy::with_numeric_authority(&m, schema, |ma| {
                let (mut admitted, mut refused) = (0, 0);
                for (i, row) in space.rows.iter().enumerate() {
                    if matches!(observed[i], Outcome::Decision(_)) {
                        match evaluate(&m, ma, row)? {
                            Outcome::Decision(_) => admitted += 1,
                            _ => refused += 1,
                        }
                    }
                }
                Ok::<_, ContractError>((admitted, refused))
            }) {
                Err(e) => json!({"status":"library-refused","reason":e.reason()}),
                Ok(v) => {
                    let (admitted, refused) = v?;
                    json!({"status":"measured","reason":reason,"admitted_where_the_contract_decides":admitted,"refused_where_the_contract_decides":refused})
                }
            },
        }
    } else {
        json!({"status":"no-reject-reason"})
    };
    let inactive: Vec<_> = ids
        .iter()
        .zip(&activation[0])
        .filter(|(_, n)| **n == 0)
        .map(|(id, _)| *id)
        .collect();
    let active_json = |n: usize| {
        ids.iter()
            .zip(&activation[n])
            .map(|(id, count)| (id.to_string(), json!(count)))
            .collect::<serde_json::Map<_, _>>()
    };
    Ok(
        json!({"status":"measured","authority":"none","domain":{"states":space.states.len(),"tuples":space.rows.len(),"valid_tuples":valid.iter().filter(|v|**v).count()},"declared_laws":ids,"work":work.to_string(),"admissible":{"full":full.json(),"valid":valid_counts.json()},"activation":{"full":active_json(0),"valid":active_json(1),"inactive":inactive},"mutation":{"full":mutation[0].json(),"valid":mutation[1].json(),"mutants":records},"reject_everything":reject,"witnesses":witnesses,"proposals":null}),
    )
}
fn state_number(c: &Contract<'_>, state: &[Atom<'static>]) -> Vec<c::Field<'static>> {
    fields(c, state, Source::State)
}
fn next_state(space: &Space, row: usize, outcome: &Outcome) -> Result<usize, ContractError> {
    let before = &space.rows[row][..space.state_width];
    let post = match outcome {
        Outcome::Decision(d) if d.class != Class::Reject => Some(&d.post),
        _ => None,
    };
    space
        .states
        .iter()
        .position(|state| match post {
            Some(fs) => {
                fs.len() == state.len() && fs.iter().zip(state).all(|((_, a), b)| same_value(a, *b))
            }
            None => state.as_slice() == before,
        })
        .ok_or_else(|| error("Authority successor is outside the declared finite state domain"))
}
fn predicate_verdict(ds: &[l::Diagnostic], id: u32) -> Option<bool> {
    match ds.iter().find(|d| d.id == id).map(|d| d.verdict) {
        Some(l::Verdict::Satisfied) => Some(true),
        Some(l::Verdict::Refused(l::Failure::Violated)) => Some(false),
        _ => None,
    }
}
pub(super) fn lemmas(
    project: &str,
    text: &str,
    formulas: &[String],
    max: u64,
    transitions: bool,
) -> Result<lemma::Lemmas, ContractError> {
    let rules = Rules::read_for_project(project, text)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let space = Space::new(&contract, max)?;
    if !space.full {
        return Ok(lemma::Lemmas::Inconclusive(Inconclusive {
            reason: "domain-exceeds-limit",
            size: space.size,
            limit: max,
        }));
    }
    if formulas.is_empty() {
        return Err(error("requires at least one lemma"));
    }
    policy::with_numeric_authority(&contract, &schema, |authority| {
        let mut outcomes = Vec::new();
        for row in &space.rows {
            outcomes.push(evaluate(&contract, authority, row)?)
        }
        let next: Vec<_> = outcomes
            .iter()
            .enumerate()
            .map(|(i, o)| next_state(&space, i, o))
            .collect::<Result<_, _>>()?;
        let genesis: Vec<_> = contract.bindings[..space.state_width]
            .iter()
            .map(|(_, id)| match id.and_then(|id| rules.genesis.get(&id)) {
                Some(Constant::Bool(v)) => Ok(Atom::Bool(*v)),
                Some(Constant::Int(v)) => Ok(Atom::I128(*v)),
                Some(Constant::U128(v)) => Ok(Atom::U128(*v)),
                None => Err(error("missing explicit genesis value")),
            })
            .collect::<Result<_, _>>()?;
        let initial = space
            .states
            .iter()
            .position(|s| s == &genesis)
            .ok_or_else(|| error("genesis outside declared state domain"))?;
        let mut row = space.rows[0].clone();
        row[..space.state_width].copy_from_slice(&genesis);
        let frames = frame(&contract, &row)?;
        if let Err(refusal) = zeno_fcis_contract::internals::genesis::admit(authority, &frames[0]) {
            return Ok(lemma::Lemmas::NoStartingState(refusal.packet()));
        }
        let per_state = space.rows.len() / space.states.len();
        let mut reachable = vec![false; space.states.len()];
        reachable[initial] = true;
        let mut queue = std::collections::VecDeque::from([initial]);
        while let Some(state) = queue.pop_front() {
            for input in 0..per_state {
                let n = next[state * per_state + input];
                if !reachable[n] {
                    reachable[n] = true;
                    queue.push_back(n)
                }
            }
        }
        let mut result = Vec::new();
        for (index, formula) in formulas.iter().enumerate() {
            let id = declarations
                .laws
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(1000)
                .max(1000)
                .checked_add(1 + index as u32)
                .ok_or_else(|| error("lemma ID overflow"))?;
            let applicability = zeno_fcis_spec::LawApplicability::new(
                if transitions {
                    zeno_fcis_spec::LawScope::Always
                } else {
                    zeno_fcis_spec::LawScope::Committing
                },
                !transitions,
            );
            let parsed = zeno_fcis_spec::parse_law_formula(
                project,
                formula,
                applicability,
                zeno_fcis_spec::SourceLimits::default(),
            )
            .map_err(|diagnostics| error(diagnostics.to_string()))?;
            let parsed = super::super::expr::law(&parsed).map_err(error)?;
            let mut r = rules.clone();
            r.law_kinds.insert(
                id,
                if transitions {
                    LawKind::FeeAndRounding
                } else {
                    LawKind::StateInvariant
                },
            );
            let mut d = Declarations::read(project, &r.leaf_bindings)?;
            d.laws.push(super::super::declarations::Law {
                id,
                name: "unsigned_lemma".to_owned(),
                scope: applicability.scope(),
                genesis: applicability.genesis(),
                formula: parsed,
            });
            let bytes = schema::encode(&d)?;
            let probe = Contract::build(&d, &r, schema_commitment(&bytes)?)?;
            let judged = policy::with_numeric_authority(&probe, &bytes, |a| {
                let laws = LawSet::new_numeric(a.descriptor(), &[id]);
                if transitions {
                    let (mut admitted, mut failed, mut undefined, mut checked) =
                        (0usize, 0usize, 0usize, 0usize);
                    let mut witness = None;
                    for (i, row) in space.rows.iter().enumerate() {
                        if !lemma::PreStates::TRANSITION.predicate(true, || {
                            let ids = state_ids(authority.descriptor());
                            let laws = LawSet::new_numeric(authority.descriptor(), &ids);
                            state_check(&contract, row, &laws).is_none()
                        }) {
                            continue;
                        }
                        let f = frame(&contract, row)?;
                        let evaluation = authority.evaluate(c::Raw {
                            state: &f[0],
                            command: &f[1],
                            context: &f[2],
                        });
                        if let Ok(cand) = evaluation.result() {
                            if cand.class() == c::Class::Reject {
                                continue;
                            }
                            admitted += 1;
                            let fs = [
                                fields(&contract, row, Source::State),
                                fields(&contract, row, Source::Command),
                                fields(&contract, row, Source::Context),
                            ];
                            let candidate = l::Candidate {
                                class: cand.class(),
                                reason: cand.reason(),
                                post: if matches!(contract.roots[0], RootSchema::Leaf(_)) {
                                    l::RootView::Leaf(cand.post()[0].value)
                                } else {
                                    l::RootView::Record(cand.post())
                                },
                                patch: cand.patch(),
                                effects: cand.effects(),
                                outbox: cand.outbox(),
                                reads: evaluation.reads(),
                                attempts: evaluation.decision_attempts(),
                                usage: evaluation.decision_usage().unwrap_or(evaluation.usage()),
                            };
                            let (_, ds) = laws.on_frame(&l::Frame::Transition {
                                pre: view(&contract, 0, &fs[0]),
                                command: view(&contract, 1, &fs[1]),
                                context: view(&contract, 2, &fs[2]),
                                candidate: &candidate,
                            });
                            match predicate_verdict(&ds, id) {
                                Some(true) => checked += 1,
                                Some(false) => {
                                    failed += 1;
                                    if witness.is_none() {
                                        witness = Some(i)
                                    }
                                }
                                None => {
                                    undefined += 1;
                                    if witness.is_none() {
                                        witness = Some(i)
                                    }
                                }
                            }
                        }
                    }
                    Ok::<_, ContractError>(
                        json!({"mode":"transition-relation","formula":formula,"verdict":if failed>0{"false"}else if undefined>0{"undefined"}else if admitted==0{"empty-scope"}else{"checked"},"scope":"all admitted committed decisions in the declared finite domain","decided_by":"library-authority-and-law-evaluator","tuples":space.rows.len(),"admitted":admitted,"failed":failed,"undefined":undefined,"checked":checked,"counterexample_row":witness}),
                    )
                } else {
                    let mut holds = Vec::new();
                    for state in &space.states {
                        let fs = state_number(&contract, state);
                        let (_, ds) = laws.on_frame(&l::Frame::Genesis {
                            initial: view(&contract, 0, &fs),
                        });
                        holds.push(predicate_verdict(&ds, id));
                    }
                    let initiation = holds[initial];
                    let mut counter = None;
                    for (i, n) in next.iter().enumerate() {
                        if holds[i / per_state] == Some(true) && holds[*n] != Some(true) {
                            counter = Some(i);
                            break;
                        }
                    }
                    let false_state = holds
                        .iter()
                        .zip(&reachable)
                        .position(|(h, r)| *r && *h == Some(false));
                    let undefined = holds.iter().any(Option::is_none);
                    Ok(
                        json!({"mode":"state-induction","formula":formula,"verdict":if false_state.is_some()||initiation==Some(false){"false"}else if undefined{"undefined"}else if initiation==Some(true)&&counter.is_none(){"inductive"}else{"not-inductive"},"scope":"declared finite state and input domains","initiation":initiation,"consecution":counter.is_none(),"states":space.states.len(),"reachable_states":reachable.iter().filter(|v|**v).count(),"counterexample_row":counter,"false_reachable_state":false_state,"decided_by":"library-authority-and-law-evaluator"}),
                    )
                }
            })??;
            result.push(judged);
        }
        Ok(lemma::Lemmas::Judged(result))
    })?
}

type NumericAuthority<'a> = finite::v2_authority::Authority<'a, c::NumericDescriptor<'a>>;
