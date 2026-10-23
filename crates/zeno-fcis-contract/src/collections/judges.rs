//! Map induction, concrete keyed scenarios and bounded map reachability.
//!
//! Every invocation owns its scratch and results. No snapshot, caller input or
//! policy is mutated, and no judge result grants publication authority.

use super::runtime::{self, Entries, Input, Key, Layout, RecordValue};
use super::{CheckResult, Limits, Verdict, check, next};
use crate::ContractError;
use crate::internals::{
    declarations::{Declarations, Source},
    model::Contract,
    policy,
    review::evaluate::{self, Framer, LawSet, Outcome, Proposed},
    rules::{Class, Constant, Rules},
    schema, schema_commitment,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use zeno_fcis_spec::{ProjectSpec, ProjectionRoot, SummaryKind};
use zeno_fcis_synthesis::finite::v2_authority::Authority;

fn invalid(reason: impl Into<String>) -> ContractError {
    ContractError::new("collection judges", reason)
}

/// Whether the source declares maps. Malformed declarations remain errors.
pub fn has_maps(project: &str) -> Result<bool, ContractError> {
    Ok(!spec(project)?.collections().maps().is_empty())
}
fn spec(project: &str) -> Result<ProjectSpec, ContractError> {
    zeno_fcis_spec::parse_project(project, Default::default())
        .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
        .map_err(|e| invalid(e.to_string()))
}

/// Check a map predicate by all-size induction, or a transition relation over
/// all admissible views. State predicates must be uniform and input independent.
/// A failed induction can be an unreachable abstract view, not a real run.
pub fn lemma(
    project: &str,
    policy: &str,
    formula: &str,
    transition: bool,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    if formula.trim().is_empty()
        || [";", "\n", "\r", "//", "/*"]
            .iter()
            .any(|s| formula.contains(s))
    {
        return Err(invalid(
            "a lemma must be one formula without terminators or comments",
        ));
    }
    let base = check(project, policy, limits)?;
    if base.verdict() == Verdict::Refuted {
        return Err(invalid(
            "base collection contract is refuted; no lemma claim is available",
        ));
    }
    if base.verdict() != Verdict::Checked {
        return Ok(base);
    }
    let rules = Rules::read(policy)?;
    let highest = project
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse::<u32>().ok())
        .chain([991, rules.failure_law, rules.reject_law])
        .max()
        .unwrap_or(991);
    let id = highest
        .checked_add(1)
        .ok_or_else(|| invalid("no free lemma law ID"))?;
    let scope = if transition {
        "commit"
    } else {
        "commit, genesis"
    };
    let extended = format!("{project}\nlaw {id} map_lemma_{id} on {scope} = {formula};\n");
    let mut rules: Value = serde_json::from_str(policy).map_err(|e| invalid(e.to_string()))?;
    rules["law_kinds"][id.to_string()] = json!(if transition {
        "DebitCreditEffectEquality"
    } else {
        "StateInvariant"
    });
    let mut result = if transition {
        transition_relation(project, policy, &extended, &rules.to_string(), id, limits)?
    } else {
        check(&extended, &rules.to_string(), limits)?
    };
    result.packet["mode"] = json!(if transition {
        "map-transition"
    } else {
        "map-induction"
    });
    result.packet["formula"] = json!(formula);
    result.packet["scope"] = json!(
        "every map size up to capacity under the stated collection premises; abstract refutations need not be reachable"
    );
    if !transition {
        result.packet["verdict"] = json!(match (result.verdict, transition) {
            (Verdict::Checked, false) => "inductive",
            (Verdict::Checked, true) => "checked",
            (Verdict::Refuted, false) => "not-inductive",
            (Verdict::Refuted, true) => "false",
            (Verdict::Unknown, _) => "UNKNOWN",
        });
    }
    result.packet["base_sources"] = base.packet["sources"].clone();
    Ok(result)
}

// Evaluate the relation separately from the base decision. Installing it as
// an admission law would hide the decisions being classified and could judge
// raw payloads instead of the canonical alias-merged successor.
fn transition_relation(
    project: &str,
    policy_source: &str,
    extended: &str,
    extended_policy: &str,
    id: u32,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    use zeno_fcis_synthesis::finite::v2_laws::{Failure, Verdict as LawVerdict};
    let rules = Rules::read(extended_policy)?;
    let declarations = Declarations::read(extended, &rules.leaf_bindings)?;
    let positions = super::domain::positions(&declarations)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    policy::with_authority(&contract, &schema, |authority| {
        let laws = LawSet::new(authority.descriptor(), &[id]);
        let mut held = 0u64;
        let mut failed = 0u64;
        let mut undefined = 0u64;
        let mut false_witness = None;
        let mut undefined_witness = None;
        let mut result = super::check_with_committed_successors(
            project,
            policy_source,
            limits,
            |framer, tuple, decision, merged| {
                let (_, diagnostics) = laws.on_transition_with_outbox(
                    framer,
                    tuple,
                    &Proposed {
                        class: decision.class,
                        reason: decision.reason,
                        post: merged,
                    },
                    &decision.outbox,
                );
                let witness = || {
                    json!({
                        "view": positions.iter().zip(tuple)
                            .map(|(p,v)| (p.name.clone(),v.to_string())).collect::<BTreeMap<_,_>>(),
                        "post": positions.iter().zip(merged)
                            .filter(|(p,_)| p.source == Source::State)
                            .map(|(p,v)| (p.name.clone(),v.to_string())).collect::<BTreeMap<_,_>>(),
                    })
                };
                match diagnostics.first().map(|d| d.verdict) {
                    Some(LawVerdict::Satisfied) => held += 1,
                    Some(LawVerdict::Refused(Failure::Violated)) => {
                        failed += 1;
                        if false_witness.is_none() {
                            false_witness = Some(witness());
                        }
                    }
                    _ => {
                        undefined += 1;
                        if undefined_witness.is_none() {
                            undefined_witness = Some(witness());
                        }
                    }
                }
            },
        )?;
        if result.verdict != Verdict::Checked {
            return Ok(result);
        }
        let admitted = held + failed + undefined;
        let (verdict, name) = if failed != 0 {
            (Verdict::Refuted, "false")
        } else if undefined != 0 {
            (Verdict::Unknown, "undefined")
        } else if admitted == 0 {
            (Verdict::Unknown, "vacuous")
        } else {
            (Verdict::Checked, "checked")
        };
        result.verdict = verdict;
        result.packet["verdict"] = json!(name);
        result.packet["status"] = json!(match verdict {
            Verdict::Unknown => "inconclusive",
            Verdict::Refuted => "refuted",
            Verdict::Checked => "checked",
        });
        if verdict != Verdict::Checked {
            result.packet["claim"] = Value::Null;
        }
        result.packet["result"] = json!({
            "admitted":admitted, "checked":held, "failed":failed,
            "undefined":undefined,
            "counterexample":false_witness.or(undefined_witness),
        });
        // Bind the separately compiled relation as well as the base sources.
        result.packet["relation_sources"] = json!({
            "project_sha256":evaluate::hex(&evaluate::digest(extended.as_bytes())),
            "rules_sha256":evaluate::hex(&evaluate::digest(extended_policy.as_bytes())),
        });
        Ok(result)
    })?
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Snapshot {
    scalars: BTreeMap<u16, i128>,
    maps: BTreeMap<u32, BTreeMap<Key, Vec<u8>>>,
}
impl Entries for Snapshot {
    fn record(&self, map: u32, key: &Key) -> Option<Vec<u8>> {
        self.maps.get(&map)?.get(key).cloned()
    }
}
impl Snapshot {
    fn bytes(&self) -> usize {
        256 + self.scalars.len() * 64
            + self
                .maps
                .values()
                .map(|m| 128 + m.values().map(|b| 128 + b.len()).sum::<usize>())
                .sum::<usize>()
    }
}
struct Entry {
    map: u32,
    key: (Source, u16),
    presence: usize,
    values: Vec<(usize, u32)>,
}
struct Model {
    spec: ProjectSpec,
    layout: Layout,
    rules: Rules,
    declarations: Declarations,
    entries: Vec<Entry>,
    keys: BTreeSet<u32>,
}
struct Step {
    state: Snapshot,
    class: &'static str,
    reason: Option<u32>,
    refusal: Option<String>,
}
impl Model {
    fn read(project: &str, policy: &str) -> Result<Self, ContractError> {
        // Validate syntax, key discipline and empty genesis even when the full
        // view product exceeds the budget. UNKNOWN supplies no proof here.
        check(project, policy, Limits { max_tuples: 0 })?;
        let spec = spec(project)?;
        let layout = Layout::read(project, policy)?;
        let rules = Rules::read(policy)?;
        let declarations = Declarations::read(project, &rules.leaf_bindings)?;
        let positions = layout.positions();
        let at = |id: u32| {
            positions
                .iter()
                .position(|p| p.source == Source::State && p.field.map(u32::from) == Some(id))
                .ok_or_else(|| invalid("entry has no view position"))
        };
        let mut entries = Vec::new();
        for e in spec.collections().entries() {
            let map = spec
                .collections()
                .map(e.map())
                .ok_or_else(|| invalid("map"))?;
            let key = (
                if e.key().root() == ProjectionRoot::Command {
                    Source::Command
                } else {
                    Source::Context
                },
                u16::try_from(e.key().segments()[1].get()).map_err(|_| invalid("key field"))?,
            );
            entries.push(Entry {
                map: map.id().get(),
                key,
                presence: at(e.id().get())?,
                values: map
                    .value_fields()
                    .iter()
                    .enumerate()
                    .map(|(j, (_, t))| Ok((at(e.id().get() + j as u32 + 1)?, t.get())))
                    .collect::<Result<_, ContractError>>()?,
            });
        }
        let keys = spec
            .collections()
            .keys()
            .iter()
            .map(|k| k.id().get())
            .collect();
        Ok(Self {
            spec,
            layout,
            rules,
            declarations,
            entries,
            keys,
        })
    }
    fn bind<T>(
        &self,
        run: impl FnOnce(&Authority<'_>, &Framer) -> Result<T, ContractError>,
    ) -> Result<T, ContractError> {
        let schema = schema::encode(&self.declarations)?;
        let contract =
            Contract::build(&self.declarations, &self.rules, schema_commitment(&schema)?)?;
        let framer = Framer::new(self.layout.positions(), &contract);
        policy::with_authority(&contract, &schema, |a| {
            let start = self.genesis()?;
            let mut command = Input::default();
            let mut context = Input::default();
            for p in self
                .layout
                .positions()
                .iter()
                .filter(|p| p.source != Source::State)
            {
                let input = if p.source == Source::Command {
                    &mut command
                } else {
                    &mut context
                };
                if self.keys.contains(&p.type_id) {
                    input.keys.insert(p.field.unwrap_or(0), [0; 32]);
                } else {
                    input.scalars.insert(
                        p.field.unwrap_or(0),
                        p.domain
                            .least()
                            .ok_or_else(|| invalid("empty input domain"))?,
                    );
                }
            }
            let tuple = self
                .layout
                .frame(&start.scalars, &command, &context, &start)?
                .tuple;
            let ids: Vec<u32> = a
                .descriptor()
                .laws
                .iter()
                .filter(|law| law.genesis)
                .map(|law| law.id)
                .collect();
            if !LawSet::new(a.descriptor(), &ids).on_state(&framer, &tuple) {
                return Err(invalid("empty-map genesis violates a genesis law"));
            }
            run(a, &framer)
        })?
    }
    fn genesis(&self) -> Result<Snapshot, ContractError> {
        let entry_positions: BTreeSet<usize> = self
            .entries
            .iter()
            .flat_map(|e| std::iter::once(e.presence).chain(e.values.iter().map(|(i, _)| *i)))
            .collect();
        let mut scalars = BTreeMap::new();
        for (i, p) in self
            .layout
            .positions()
            .iter()
            .enumerate()
            .filter(|(i, p)| p.source == Source::State && !entry_positions.contains(i))
        {
            let field = p.field.ok_or_else(|| invalid("scalar state field"))?;
            let value = match self.rules.genesis.get(&field) {
                Some(Constant::Int(n)) => *n,
                Some(Constant::Bool(b)) => i128::from(*b),
                _ => {
                    return Err(invalid(format!(
                        "genesis state field {} missing or unsupported at {i}",
                        p.name
                    )));
                }
            };
            scalars.insert(field, value);
        }
        let state = Snapshot {
            scalars,
            maps: self
                .spec
                .collections()
                .maps()
                .iter()
                .map(|m| (m.id().get(), BTreeMap::new()))
                .collect(),
        };
        self.check_state(&state)?;
        Ok(state)
    }
    fn key(&self, entry: &Entry, command: &Input, context: &Input) -> Result<Key, ContractError> {
        let input = if entry.key.0 == Source::Command {
            command
        } else {
            context
        };
        input
            .keys
            .get(&entry.key.1)
            .copied()
            .ok_or_else(|| invalid("missing named key"))
    }
    fn record(
        &self,
        state: &Snapshot,
        map: u32,
        key: &Key,
    ) -> Result<Option<Vec<RecordValue>>, ContractError> {
        state
            .record(map, key)
            .map(|b| runtime::decode_record(&b).ok_or_else(|| invalid("malformed snapshot record")))
            .transpose()
    }
    // Express the concrete successor in the PRE invocation's key classes.
    // Independently reframing it could rename classes and falsify step laws.
    fn merged(
        &self,
        before: &Snapshot,
        after: &Snapshot,
        command: &Input,
        context: &Input,
        tuple: &[i128],
    ) -> Result<Vec<i128>, ContractError> {
        let mut classes = BTreeMap::new();
        for (i, p) in self
            .layout
            .positions()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source != Source::State && self.keys.contains(&p.type_id))
        {
            let input = if p.source == Source::Command {
                command
            } else {
                context
            };
            let key = input
                .keys
                .get(&p.field.unwrap_or(0))
                .ok_or_else(|| invalid("key"))?;
            classes.insert((p.type_id, *key), tuple[i]);
        }
        for entry in &self.entries {
            let key = self.key(entry, command, context)?;
            if let Some(record) = self.record(before, entry.map, &key)? {
                for ((i, t), v) in entry.values.iter().zip(record) {
                    if let RecordValue::Key(k) = v {
                        classes.insert((*t, k), tuple[*i]);
                    }
                }
            }
        }
        let mut merged = tuple.to_vec();
        for (i, p) in self
            .layout
            .positions()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.source == Source::State)
        {
            if let Some(n) = p.field.and_then(|f| after.scalars.get(&f)) {
                merged[i] = *n;
            }
        }
        for entry in &self.entries {
            let key = self.key(entry, command, context)?;
            let record = self.record(after, entry.map, &key)?;
            if record.is_none() {
                super::canonical_absent_view(
                    self.layout.positions(),
                    entry.presence,
                    entry.values.iter().map(|(i, _)| *i),
                    &mut merged,
                );
                continue;
            }
            merged[entry.presence] = 1;
            for (j, (i, t)) in entry.values.iter().enumerate() {
                merged[*i] = match record.as_ref().and_then(|r| r.get(j)) {
                    Some(RecordValue::Number(n)) => *n,
                    Some(RecordValue::Key(k)) => *classes
                        .get(&(*t, *k))
                        .ok_or_else(|| invalid("post key has no pre-invocation class"))?,
                    None => return Err(invalid("missing stored field")),
                };
            }
        }
        Ok(merged)
    }
    fn step(
        &self,
        authority: &Authority<'_>,
        framer: &Framer,
        before: &Snapshot,
        command: &Input,
        context: &Input,
    ) -> Result<Step, ContractError> {
        let framed = self
            .layout
            .frame(&before.scalars, command, context, before)?;
        let refused = |why| Step {
            state: before.clone(),
            class: "Refused",
            reason: None,
            refusal: Some(why),
        };
        let decision = match evaluate::evaluate(authority, framer, &framed.tuple) {
            Outcome::Refused(r) => return Ok(refused(r.text)),
            Outcome::Decision(d) => d,
        };
        if decision.class == Class::Reject {
            return Ok(Step {
                state: before.clone(),
                class: "Reject",
                reason: decision.reason,
                refusal: None,
            });
        }
        let post = decision
            .post
            .iter()
            .map(|(f, v)| {
                v.number()
                    .map(|n| (*f, n))
                    .ok_or_else(|| invalid("nonnumeric post"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let writes = match self.layout.writes(&framed, &post) {
            Ok(w) => w,
            Err(e) => return Ok(refused(e.reason().to_owned())),
        };
        let mut after = before.clone();
        after.scalars = writes.scalars;
        for (map, key, record) in writes.entries {
            let entries = after.maps.get_mut(&map).ok_or_else(|| invalid("map"))?;
            if let Some(record) = record {
                entries.insert(key, record);
            } else {
                entries.remove(&key);
            }
        }
        // Capacity is checked on the complete simultaneous update, so a delete
        // and insert at capacity do not depend on write order.
        if self
            .spec
            .collections()
            .maps()
            .iter()
            .any(|m| after.maps[&m.id().get()].len() as u64 > m.capacity())
        {
            return Ok(refused("capacity".to_owned()));
        }
        let merged = self.merged(before, &after, command, context, &framed.tuple)?;
        let ids: Vec<u32> = self.declarations.laws.iter().map(|l| l.id).collect();
        if !LawSet::new(authority.descriptor(), &ids)
            .on_transition_with_outbox(
                framer,
                &framed.tuple,
                &Proposed {
                    class: decision.class,
                    reason: decision.reason,
                    post: &merged,
                },
                &decision.outbox,
            )
            .0
        {
            return Err(invalid(
                "alias-merged concrete successor violates a declared law",
            ));
        }
        self.check_state(&after)?;
        Ok(Step {
            state: after,
            class: decision.class.name(),
            reason: decision.reason,
            refusal: None,
        })
    }
    fn field(
        &self,
        map: u32,
        field: u32,
        record: &[RecordValue],
    ) -> Result<RecordValue, ContractError> {
        let index = self
            .spec
            .collections()
            .maps()
            .iter()
            .find(|m| m.id().get() == map)
            .and_then(|m| m.value_fields().iter().position(|(f, _)| f.get() == field))
            .ok_or_else(|| invalid("record field"))?;
        record
            .get(index)
            .copied()
            .ok_or_else(|| invalid("record width"))
    }
    fn check_state(&self, state: &Snapshot) -> Result<(), ContractError> {
        let number = |v| {
            if let RecordValue::Number(n) = v {
                Ok(n)
            } else {
                Err(invalid("expected integer"))
            }
        };
        for summary in self.spec.collections().summaries() {
            let mut sum = 0i128;
            for bytes in state.maps[&summary.map().get()].values() {
                let record = runtime::decode_record(bytes).ok_or_else(|| invalid("record"))?;
                let value = match summary.kind() {
                    SummaryKind::Count => 1,
                    SummaryKind::Sum(f) => {
                        number(self.field(summary.map().get(), f.get(), &record)?)?
                    }
                };
                sum = sum
                    .checked_add(value)
                    .ok_or_else(|| invalid("summary overflow"))?;
            }
            if state.scalars.get(&(summary.id().get() as u16)) != Some(&sum) {
                return Err(invalid(format!(
                    "summary {} differs from the concrete map",
                    summary.id().get()
                )));
            }
        }
        for tally in self.spec.collections().tallies() {
            let mut grouped = BTreeMap::<Key, i128>::new();
            for bytes in state.maps[&tally.source().get()].values() {
                let record = runtime::decode_record(bytes).ok_or_else(|| invalid("record"))?;
                let field = |f: u32| self.field(tally.source().get(), f, &record);
                if tally
                    .selector()
                    .map(|f| field(f.get()).and_then(number))
                    .transpose()?
                    .is_some_and(|n| n != 1)
                {
                    continue;
                }
                let RecordValue::Key(key) = field(tally.group().get())? else {
                    return Err(invalid("group key"));
                };
                let weight = tally
                    .weight()
                    .map(|f| field(f.get()).and_then(number))
                    .transpose()?
                    .unwrap_or(1);
                let sum = grouped.entry(key).or_default();
                *sum = sum
                    .checked_add(weight)
                    .ok_or_else(|| invalid("tally overflow"))?;
            }
            for (key, bytes) in &state.maps[&tally.target().get()] {
                let record = runtime::decode_record(bytes).ok_or_else(|| invalid("record"))?;
                let count =
                    number(self.field(tally.target().get(), tally.field().get(), &record)?)?;
                if count != grouped.remove(key).unwrap_or(0) {
                    return Err(invalid(format!(
                        "tally {} drifts on a concrete key",
                        tally.id().get()
                    )));
                }
            }
            if grouped.values().any(|n| *n != 0) {
                return Err(invalid("tally contribution has no target entry"));
            }
        }
        Ok(())
    }
    fn json(&self, state: &Snapshot) -> Result<Value, ContractError> {
        let mut entries = Vec::new();
        for map in self.spec.collections().maps() {
            for (key, bytes) in &state.maps[&map.id().get()] {
                let values = runtime::decode_record(bytes).ok_or_else(|| invalid("record"))?;
                let record: BTreeMap<String, Value> = map
                    .value_fields()
                    .iter()
                    .zip(values)
                    .map(|((f, _), v)| {
                        (
                            f.get().to_string(),
                            match v {
                                RecordValue::Number(n) => json!(n.to_string()),
                                RecordValue::Key(k) => json!(evaluate::hex(&k)),
                            },
                        )
                    })
                    .collect();
                entries
                    .push(json!({"map":map.id().get(),"key":evaluate::hex(key),"record":record}));
            }
        }
        Ok(
            json!({"scalars":state.scalars.iter().map(|(f,n)| (f.to_string(),n.to_string())).collect::<BTreeMap<_,_>>(),"entries":entries}),
        )
    }
    fn input(&self, source: Source, fields: &BTreeMap<u16, Value>) -> Result<Input, ContractError> {
        let mut input = Input::default();
        let positions: Vec<_> = self
            .layout
            .positions()
            .iter()
            .filter(|p| p.source == source)
            .collect();
        if fields.len() != positions.len() {
            return Err(invalid(
                "input must name exactly its declared fields (0 for a scalar root)",
            ));
        }
        for p in positions {
            let field = p.field.unwrap_or(0);
            let value = fields
                .get(&field)
                .ok_or_else(|| invalid(format!("missing {}", p.name)))?;
            if self.keys.contains(&p.type_id) {
                input.keys.insert(field, parse_key(value)?);
            } else {
                input.scalars.insert(field, parse_number(value)?);
            }
        }
        Ok(input)
    }
}
fn parse_number(v: &Value) -> Result<i128, ContractError> {
    match v {
        Value::Number(n) => n
            .to_string()
            .parse()
            .map_err(|_| invalid("integer out of range")),
        Value::String(s) => s.parse().map_err(|_| invalid("expected decimal integer")),
        _ => Err(invalid("expected integer or decimal string")),
    }
}
fn parse_key(v: &Value) -> Result<Key, ContractError> {
    let text = v
        .as_str()
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or_else(|| invalid("keys are 64 lowercase hexadecimal digits"))?;
    let mut key = [0; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|_| invalid("key"))?;
    }
    Ok(key)
}
fn sources(project: &str, policy: &str) -> Value {
    json!({"project_sha256":evaluate::hex(&evaluate::digest(project.as_bytes())),"rules_sha256":evaluate::hex(&evaluate::digest(policy.as_bytes()))})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenarios {
    schema: String,
    scenarios: Vec<Scenario>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    name: String,
    steps: Vec<ScenarioStep>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioStep {
    command: BTreeMap<u16, Value>,
    context: BTreeMap<u16, Value>,
    expect: Expected,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    class: String,
    #[serde(default)]
    reason: Option<u32>,
    #[serde(default)]
    scalars: BTreeMap<u16, Value>,
    #[serde(default)]
    entries: Vec<ExpectedEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedEntry {
    map: u32,
    key: Value,
    record: Option<BTreeMap<u32, Value>>,
}

/// Replay concrete map scenarios from empty genesis. Keys persist across input
/// roles. Expected entries contain complete records, or null for absence.
/// The result contains each owned successor snapshot; no mutant score is claimed.
pub fn scenarios(project: &str, policy: &str, text: &str) -> Result<CheckResult, ContractError> {
    if text.len() > 1 << 20 {
        return Err(invalid("scenario text exceeds 1 MiB"));
    }
    let parsed: Scenarios = serde_json::from_str(text).map_err(|e| invalid(e.to_string()))?;
    if parsed.schema != "zeno-fcis/collection-scenarios/1"
        || parsed.scenarios.is_empty()
        || parsed.scenarios.len() > 128
        || parsed
            .scenarios
            .iter()
            .map(|s| s.steps.len())
            .sum::<usize>()
            > 4096
    {
        return Err(invalid(
            "expected collection-scenarios/1 with 1..128 scenarios and at most 4096 total steps",
        ));
    }
    let model = Model::read(project, policy)?;
    let mut names = BTreeSet::new();
    let (results,failed) = model.bind(|authority,framer| {
        let mut results = Vec::new(); let mut failed = 0; let mut bytes = 0;
        for scenario in parsed.scenarios {
            if scenario.name.trim().is_empty() || !names.insert(scenario.name.clone()) || scenario.steps.is_empty() {return Err(invalid("scenario names are nonempty and unique, and each has steps"));}
            let mut state = model.genesis()?; let mut outcomes = Vec::new(); let mut failure = None;
            for (index,step) in scenario.steps.iter().enumerate() {
                if !["Accept","Reject","CommittedFailure","Refused"].contains(&step.expect.class.as_str()) {return Err(invalid("unknown expected class"));}
                let command = model.input(Source::Command,&step.command)?; let context = model.input(Source::Context,&step.context)?;
                let outcome = model.step(authority,framer,&state,&command,&context)?;
                let mut matches = step.expect.class == outcome.class && step.expect.reason == outcome.reason;
                for (f,value) in &step.expect.scalars { matches &= outcome.state.scalars.get(f) == Some(&parse_number(value)?); }
                for expected in &step.expect.entries {
                    let map = model.spec.collections().maps().iter().find(|m| m.id().get() == expected.map).ok_or_else(|| invalid("unknown expected map"))?;
                    let key = parse_key(&expected.key)?;
                    let record = expected.record.as_ref().map(|fields| {
                        if fields.len()!=map.value_fields().len() {return Err(invalid("expected record must name exactly its fields"));}
                        map.value_fields().iter().map(|(f,t)| {
                            let value = fields.get(&f.get()).ok_or_else(|| invalid("expected record field missing"))?;
                            if model.keys.contains(&t.get()) { Ok(RecordValue::Key(parse_key(value)?)) } else { Ok(RecordValue::Number(parse_number(value)?)) }
                        }).collect::<Result<Vec<_>,ContractError>>().map(|r| runtime::encode_record(&r))
                    }).transpose()?;
                    matches &= outcome.state.record(expected.map,&key) == record;
                }
                let snapshot = model.json(&outcome.state)?;
                let item = json!({"step":index+1,"class":outcome.class,"reason":outcome.reason,"refusal":outcome.refusal,"state":snapshot});
                bytes += item.to_string().len(); if bytes>8<<20 {return Err(invalid("scenario output exceeds 8 MiB"));}
                outcomes.push(item); state = outcome.state;
                if !matches {failure=Some(index+1);break;}
            }
            failed += usize::from(failure.is_some());
            results.push(json!({"name":scenario.name,"start":"empty-map-genesis","verdict":if failure.is_some(){"failed"}else{"passed"},"failure_step":failure,"steps":outcomes}));
        }
        Ok((results,failed))
    })?;
    let verdict = if failed == 0 {
        Verdict::Checked
    } else {
        Verdict::Refuted
    };
    Ok(CheckResult {
        verdict,
        packet: json!({"schema":"zeno-fcis/collection-scenario-result/1","authority":"none","mode":"concrete-maps","sources":sources(project,policy),"status":if failed==0{"passed"}else{"failed"},"failed":failed,"scenarios":results,"mutants":{"status":"not-run","reason":"view mutants have no map-replay score"}}),
    })
}

/// Exhaustive reachability for 1..4 concrete keys per opaque key type, NOT
/// an all-size reachability claim. At most 65,536 snapshots, 64 MiB estimated
/// snapshot storage and the caller's (hard-capped) transition budget.
pub fn profile(
    project: &str,
    policy: &str,
    key_count: u8,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    if !(1..=4).contains(&key_count) {
        return Err(invalid("map profile requires 1..=4 keys per key type"));
    }
    let model = Model::read(project, policy)?;
    let positions: Vec<_> = model
        .layout
        .positions()
        .iter()
        .filter(|p| p.source != Source::State)
        .collect();
    let limit = limits.max_tuples.min(super::MAX_TUPLES);
    let input_size = positions.iter().try_fold(1u128, |n, p| {
        n.checked_mul(if model.keys.contains(&p.type_id) {
            u128::from(key_count)
        } else {
            p.domain.size()?
        })
    });
    // Count and bound before collecting even one large scalar domain.
    let input_too_large = input_size.is_none_or(|n| n == 0 || n > u128::from(limit));
    let input_storage_too_large =
        input_size.is_none_or(|n| n.saturating_mul(256 + positions.len() as u128 * 128) > 32 << 20);
    let domains: Option<Vec<Vec<i128>>> = if input_too_large || input_storage_too_large {
        None
    } else {
        positions
            .iter()
            .map(|p| {
                if model.keys.contains(&p.type_id) {
                    Some((0..i128::from(key_count)).collect())
                } else {
                    p.domain.values_within(limit)
                }
            })
            .collect()
    };
    let mut packet = json!({"schema":"zeno-fcis/collection-profile/1","authority":"none","mode":"concrete-maps","sources":sources(project,policy),"keys_per_type":key_count,"key_universes":model.spec.collections().keys().iter().map(|t| json!({"type":t.id().get(),"keys":(0..key_count).map(|i| {let mut k=[0u8;32];k[31]=i;evaluate::hex(&k)}).collect::<Vec<_>>()})).collect::<Vec<_>>(),"limits":{"transitions":limit,"snapshots":65536,"snapshot_bytes":64<<20},"claim":null});
    let unknown = |mut packet: Value, reason: &str| {
        packet["status"] = json!("UNKNOWN");
        packet["reason"] = json!(reason);
        CheckResult {
            verdict: Verdict::Unknown,
            packet,
        }
    };
    let Some(domains) = domains else {
        return Ok(unknown(packet, "input domain exceeds transition limit"));
    };
    let count = domains
        .iter()
        .try_fold(1u64, |n, v| n.checked_mul(v.len() as u64));
    if count.is_none_or(|n| n == 0 || n > limit) {
        return Ok(unknown(packet, "input domain exceeds transition limit"));
    }
    if count.is_none_or(|n| u128::from(n) * (256 + positions.len() as u128 * 128) > 32 << 20) {
        return Ok(unknown(packet, "input storage limit (32 MiB estimate)"));
    }
    let mut tuples = vec![domains.iter().map(|d| d[0]).collect::<Vec<_>>()];
    let mut digits = vec![0; domains.len()];
    loop {
        let mut tuple = tuples.last().cloned().ok_or_else(|| invalid("inputs"))?;
        if !next(&domains, &mut digits, &mut tuple) {
            break;
        }
        tuples.push(tuple);
    }
    let inputs: Vec<(Input, Input)> = tuples
        .iter()
        .map(|tuple| {
            let mut command = Input::default();
            let mut context = Input::default();
            for (p, value) in positions.iter().zip(tuple) {
                let input = if p.source == Source::Command {
                    &mut command
                } else {
                    &mut context
                };
                let field = p.field.unwrap_or(0);
                if model.keys.contains(&p.type_id) {
                    let mut key = [0; 32];
                    key[31] = *value as u8;
                    input.keys.insert(field, key);
                } else {
                    input.scalars.insert(field, *value);
                }
            }
            (command, context)
        })
        .collect();
    let (states, edges, refused, complete, reason) = model.bind(|authority, framer| {
        let start = model.genesis()?;
        let mut bytes = start.bytes() * 4;
        let mut states = vec![start.clone()];
        let mut ids = BTreeMap::from([(start, 0)]);
        let mut at = 0;
        let mut edges = 0;
        let mut refused = 0;
        while at < states.len() {
            for (command, context) in &inputs {
                if edges >= limit {
                    return Ok((states, edges, refused, false, "transition limit"));
                }
                let step = model.step(authority, framer, &states[at], command, context)?;
                edges += 1;
                refused += u64::from(step.class == "Refused");
                if !ids.contains_key(&step.state) {
                    let size = step.state.bytes() * 4;
                    if states.len() >= 65536 || bytes + size > 64 << 20 {
                        return Ok((states, edges, refused, false, "snapshot storage limit"));
                    }
                    bytes += size;
                    ids.insert(step.state.clone(), states.len());
                    states.push(step.state);
                }
            }
            at += 1;
        }
        Ok((states, edges, refused, true, ""))
    })?;
    packet["measured"] = json!({"discovered_map_states":states.len(),"transitions":edges,"inputs":inputs.len(),"refused":refused});
    if !complete {
        return Ok(unknown(packet, reason));
    }
    packet["status"] = json!("measured");
    packet["states"] = json!({"reachable":states.len()});
    packet["claim"] = json!(
        "all concrete map states reachable from empty-map genesis over exactly the printed finite key universes and declared scalar inputs, under framing, merged laws, invariants and atomic capacity refusal; no all-size reachability or Moore-minimality claim"
    );
    let mut canonical: Vec<String> = states
        .iter()
        .map(|s| model.json(s).map(|v| v.to_string()))
        .collect::<Result<_, _>>()?;
    canonical.sort();
    packet["reachable_states_sha256"] = json!(evaluate::hex(&evaluate::digest(
        canonical.join("\n").as_bytes()
    )));
    Ok(CheckResult {
        verdict: Verdict::Checked,
        packet,
    })
}
