//! Keyed collections, checked by data independence.
//!
//! A project with maps lowers each map to the entries its commands and
//! contexts name: the view contract the library Authority decides. Keys
//! become classes, one restricted-growth assignment per equality pattern, so
//! the view never holds a key's value, only which named keys are equal.
//!
//! This checker enumerates every admissible view: every class pattern; each
//! named entry absent, with default values, or present with every value of
//! its record; aliased entries of a map equal; summaries no smaller than
//! the entries they must include; and every state law holding on the
//! pre-view. For each, the library Authority decides, and the checker
//! requires:
//! - no refusal: every decision the rules make is lawful;
//! - alias merge: the entries one key names write at most one new value;
//! - summary deltas: each summary changes by exactly the change its named
//!   entries make, counting aliased entries once.
//!
//! An unnamed entry has no view, so no decision reads or writes it: that
//! frame is structural. Given the premises the packet states, the result
//! holds for every number of entries up to each map's capacity and for any
//! key values. `docs/COLLECTIONS.md` gives the argument.

pub mod runtime;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use zeno_fcis_spec::{ProjectionPath, ProjectionRoot, SummaryKind, TypeKind};

use crate::ContractError;
use crate::internals::{
    declarations::{Declarations, Source},
    expr::{self, Ast, Binary},
    model::Contract,
    policy,
    review::{
        domain::{self, Position},
        evaluate::{self, Framer, LawSet, Outcome},
    },
    rules::{Class, Constant, Rules},
    schema, schema_commitment,
};

/// The premises every result rests on; the checker and type checker
/// enforce the first three, and the fourth is the runtime's.
pub const PREMISES: [&str; 4] = [
    "keys are compared only for equality, with keys of the same type",
    "a decision reads and writes only scalar state and the entries its command and context keys name",
    "laws are per entry or over named entries, and aggregates are declared summaries whose changes are checked",
    "the runtime frames each decision from the stored entries at those keys and writes back only them",
];

/// The claim a checked result makes.
pub const CLAIM: &str = "exhaustively checked over every admissible view; given the premises, this covers every number of entries up to each map's capacity and any key values";

/// Default and hard limit on enumerated view tuples, before admissibility.
pub const MAX_TUPLES: u64 = 1 << 22;

/// Caller limits, bounded by the hard limit.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Enumerated view tuples, admissible or not.
    pub max_tuples: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tuples: MAX_TUPLES,
        }
    }
}

/// A collections verdict; UNKNOWN claims nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every admissible view decided lawfully and met every obligation.
    Checked,
    /// One admissible view refuted an obligation or was refused.
    Refuted,
    /// The view domain exceeded the limit.
    Unknown,
}

impl Verdict {
    /// Stable packet spelling.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Checked => "checked",
            Self::Refuted => "refuted",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// Owned result; it grants no runtime authority.
#[derive(Clone, Debug)]
pub struct CheckResult {
    verdict: Verdict,
    packet: Value,
}

impl CheckResult {
    /// The verdict.
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }
    /// The packet, with premises, measurements and any counterexample.
    pub const fn packet(&self) -> &Value {
        &self.packet
    }
}

/// One named entry's view positions.
struct EntryView {
    id: u32,
    map: u32,
    key: usize,
    presence: usize,
    values: Vec<usize>,
    /// Value positions holding stored keys, with their key type.
    stored_keys: Vec<(usize, u32)>,
}

/// One summary's view position and what it adds up.
struct SummaryView {
    id: u32,
    map: u32,
    position: usize,
    /// The value position within each entry, for a sum; None for a count.
    field: Option<usize>,
    /// The summed field's least value, when it is nonnegative.
    nonnegative: bool,
}

/// Checks a collections contract. Invalid declarations, rules that use a
/// key other than by equality, and a genesis with entries are errors.
pub fn check(
    project: &str,
    policy_source: &str,
    limits: Limits,
) -> Result<CheckResult, ContractError> {
    let invalid = |reason: String| ContractError::new("collections", reason);
    let spec = zeno_fcis_spec::parse_project(project, Default::default())
        .and_then(|p| zeno_fcis_spec::elaborate_project(p, Default::default()))
        .map_err(|e| invalid(e.to_string()))?;
    let collections = spec.collections().clone();
    if collections.maps().is_empty() {
        return Err(invalid("declare at least one map".to_owned()));
    }
    let rules = Rules::read_for_project(project, policy_source)?;
    rules.refuse_unsigned("collections binding")?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let positions = domain::positions(&declarations)?;
    let at = |name: String| {
        positions
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| invalid(format!("{name} is not a view input")))
    };
    let path_name = |path: &ProjectionPath| {
        let root = match path.root() {
            ProjectionRoot::Command => "command",
            ProjectionRoot::Context => "context",
            _ => "pre",
        };
        let segments: Vec<String> = path
            .segments()
            .iter()
            .map(|s| s.get().to_string())
            .collect();
        format!("{root}.{}", segments.join("."))
    };
    let key_types: BTreeSet<u32> = collections.keys().iter().map(|k| k.id().get()).collect();
    let root_of = |kind: TypeKind| {
        spec.types()
            .iter()
            .find(|t| t.kind() == kind)
            .map(|t| t.id())
    };
    let input_roots = [
        (root_of(TypeKind::Command), "command"),
        (root_of(TypeKind::Context), "context"),
    ];
    // Key positions: input key fields, then stored keys of named entries.
    let mut keys: BTreeMap<usize, u32> = BTreeMap::new();
    let mut key_names: BTreeMap<String, u32> = BTreeMap::new();
    for field in spec.fields() {
        if !key_types.contains(&field.field_type().get()) {
            continue;
        }
        if let Some((_, root)) = input_roots.iter().find(|(r, _)| *r == Some(field.owner())) {
            let name = format!("{root}.{}.{}", field.owner().get(), field.id().get());
            keys.insert(at(name.clone())?, field.field_type().get());
            key_names.insert(name, field.field_type().get());
        }
    }
    let mut entries = Vec::new();
    for entry in collections.entries() {
        let map = collections
            .map(entry.map())
            .ok_or_else(|| invalid(format!("entry {} names no map", entry.id().get())))?;
        let owner = map.owner().get();
        let mut values = Vec::new();
        let mut stored_keys = Vec::new();
        for (j, (_, field_type)) in map.value_fields().iter().enumerate() {
            let position = at(format!("pre.{owner}.{}", entry.id().get() + j as u32 + 1))?;
            if key_types.contains(&field_type.get()) {
                stored_keys.push((position, field_type.get()));
                key_names.insert(positions[position].name.clone(), field_type.get());
            }
            values.push(position);
        }
        entries.push(EntryView {
            id: entry.id().get(),
            map: map.id().get(),
            key: at(path_name(entry.key()))?,
            presence: at(format!("pre.{owner}.{}", entry.id().get()))?,
            values,
            stored_keys,
        });
    }
    let mut summaries = Vec::new();
    for summary in collections.summaries() {
        let map = collections
            .map(summary.map())
            .ok_or_else(|| invalid(format!("summary {} names no map", summary.id().get())))?;
        let field = match summary.kind() {
            SummaryKind::Count => None,
            SummaryKind::Sum(field) => Some(
                map.value_fields()
                    .iter()
                    .position(|(id, _)| *id == field)
                    .ok_or_else(|| {
                        invalid(format!("summary {} sums no field", summary.id().get()))
                    })?,
            ),
        };
        let nonnegative = field.is_none_or(|j| {
            entries
                .iter()
                .find(|e| e.map == map.id().get())
                .is_none_or(|e| {
                    positions[e.values[j]]
                        .domain
                        .least()
                        .is_some_and(|v| v >= 0)
                })
        });
        summaries.push(SummaryView {
            id: summary.id().get(),
            map: map.id().get(),
            position: at(format!(
                "pre.{}.{}",
                summary.owner().get(),
                summary.id().get()
            ))?,
            field,
            nonnegative,
        });
    }
    refuse_key_values(&rules, &key_names)?;
    let genesis = |position: usize| {
        positions[position]
            .field
            .and_then(|field| rules.genesis.get(&field))
            .map(|value| match value {
                Constant::Bool(b) => i128::from(*b),
                Constant::Int(n) => *n,
                Constant::U128(n) => i128::try_from(*n).unwrap_or(i128::MAX),
            })
    };
    for entry in &entries {
        if genesis(entry.presence) != Some(0) {
            return Err(invalid(format!(
                "genesis: entry {} must be absent, because every map starts empty",
                entry.id
            )));
        }
    }
    for summary in &summaries {
        if genesis(summary.position) != Some(0) {
            return Err(invalid(format!(
                "genesis: summary {} must be 0, the aggregate of an empty map",
                summary.id
            )));
        }
    }

    let values: Vec<Vec<i128>> = positions
        .iter()
        .map(|p| p.domain.values_within(limits.max_tuples.min(MAX_TUPLES)))
        .collect::<Option<_>>()
        .unwrap_or_default();
    let size = domain::domain_size(&positions);
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let framer = Framer::new(&positions, &contract);
    let sha = |s: &str| evaluate::hex(&evaluate::digest(s.as_bytes()));
    let mut packet = json!({
        "schema": "zeno-fcis/collections-check/1",
        "authority": "none",
        "premises": PREMISES,
        "sources": {"project_sha256": sha(project), "rules_sha256": sha(policy_source)},
        "maps": collections.maps().iter().map(|map| json!({
            "id": map.id().get(), "name": map.name().as_str(),
            "capacity": map.capacity().to_string(),
            "entries": entries.iter().filter(|e| e.map == map.id().get()).map(|e| e.id).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "key_types": collections.keys().iter().map(|key| json!({
            "id": key.id().get(), "name": key.name().as_str(), "positions": key.positions(),
            "patterns": bell(key.positions()).to_string(),
        })).collect::<Vec<_>>(),
        "summaries": summaries.iter().map(|s| s.id).collect::<Vec<_>>(),
        "view_domain": size.map(|n| n.to_string()),
        "limits": {"max_tuples": limits.max_tuples.min(MAX_TUPLES)},
    });
    if values.is_empty()
        || size.is_none_or(|n| n == 0 || n > u128::from(limits.max_tuples.min(MAX_TUPLES)))
    {
        packet["status"] = json!(Verdict::Unknown.name());
        packet["claim"] = Value::Null;
        packet["result"] = json!({"reason": "view domain exceeds the tuple limit"});
        return Ok(CheckResult {
            verdict: Verdict::Unknown,
            packet,
        });
    }
    let shape = Shape {
        positions: &positions,
        keys: &keys,
        entries: &entries,
        summaries: &summaries,
    };
    let outcome = policy::with_authority(&contract, &schema, |authority| {
        let state_laws = evaluate::state_laws(authority.descriptor());
        let laws = LawSet::new(authority.descriptor(), &state_laws);
        let mut measured = Measured::default();
        // Genesis: every map empty, so every view at every choice of keys is
        // this one, and every state law must hold on it.
        let start: Vec<i128> = positions
            .iter()
            .enumerate()
            .map(|(i, p)| match p.source {
                Source::State => genesis(i).unwrap_or(values[i][0]),
                _ => values[i][0],
            })
            .collect();
        if !laws.on_state(&framer, &start) {
            return (
                measured,
                Some(json!({
                    "obligation": "state laws at genesis",
                    "detail": "the genesis state breaks a state law",
                    "view": shape.view_json(&start),
                })),
            );
        }
        let mut tuple: Vec<i128> = values.iter().map(|v| v[0]).collect();
        let mut digits = vec![0usize; values.len()];
        loop {
            measured.tuples += 1;
            if shape.admissible(&tuple) && laws.on_state(&framer, &tuple) {
                measured.views += 1;
                match evaluate::evaluate(authority, &framer, &tuple) {
                    Outcome::Refused(refusal) => {
                        measured.refused += 1;
                        return (
                            measured,
                            Some(json!({
                                "obligation": "lawful decision",
                                "detail": refusal.text, "law": refusal.law,
                                "view": shape.view_json(&tuple),
                            })),
                        );
                    }
                    Outcome::Decision(decision) => {
                        if decision.class == Class::Reject {
                            measured.rejected += 1;
                        } else {
                            measured.committed += 1;
                            let post: BTreeMap<u16, i128> = decision
                                .post
                                .iter()
                                .filter_map(|(id, v)| Some((*id, v.number()?)))
                                .collect();
                            if let Some(failure) = shape.obligations(&tuple, &post) {
                                return (
                                    measured,
                                    Some(json!({
                                        "obligation": failure.0, "detail": failure.1,
                                        "view": shape.view_json(&tuple),
                                    })),
                                );
                            }
                        }
                    }
                }
            }
            if !next(&values, &mut digits, &mut tuple) {
                return (measured, None);
            }
        }
    })?;
    let (measured, counterexample) = outcome;
    let verdict = if counterexample.is_some() {
        Verdict::Refuted
    } else {
        Verdict::Checked
    };
    packet["status"] = json!(verdict.name());
    packet["claim"] = if verdict == Verdict::Checked {
        json!(CLAIM)
    } else {
        Value::Null
    };
    packet["measured"] = json!({
        "tuples": measured.tuples, "admissible_views": measured.views,
        "committed": measured.committed, "rejected": measured.rejected, "refused": measured.refused,
    });
    packet["result"] = counterexample.unwrap_or_else(|| json!({"all_admissible_views": true}));
    Ok(CheckResult { verdict, packet })
}

#[derive(Default)]
struct Measured {
    tuples: u64,
    views: u64,
    committed: u64,
    rejected: u64,
    refused: u64,
}

struct Shape<'a> {
    positions: &'a [Position],
    keys: &'a BTreeMap<usize, u32>,
    entries: &'a [EntryView],
    summaries: &'a [SummaryView],
}

impl Shape<'_> {
    /// One assignment per equality pattern; absent entries hold defaults;
    /// aliased entries agree; summaries cover their present entries.
    fn admissible(&self, tuple: &[i128]) -> bool {
        let present = |entry: &EntryView| tuple[entry.presence] == 1;
        // Absent entries hold the least value of each field.
        for entry in self.entries {
            if !present(entry)
                && entry
                    .values
                    .iter()
                    .any(|p| Some(tuple[*p]) != self.positions[*p].domain.least())
            {
                return false;
            }
        }
        // Key classes form a restricted-growth string per key type, over
        // input keys and the stored keys of present entries, in input order.
        let mut stored: BTreeMap<usize, u32> = BTreeMap::new();
        for entry in self.entries.iter().filter(|e| present(e)) {
            stored.extend(entry.stored_keys.iter().copied());
        }
        let mut highest: BTreeMap<u32, i128> = BTreeMap::new();
        for (position, key_type) in self
            .keys
            .iter()
            .chain(stored.iter())
            .collect::<BTreeMap<_, _>>()
        {
            let seen = highest.entry(*key_type).or_insert(-1);
            let class = tuple[*position];
            if class > *seen + 1 {
                return false;
            }
            *seen = (*seen).max(class);
        }
        // Entries of one map at one key are one entry.
        for (i, a) in self.entries.iter().enumerate() {
            for b in &self.entries[i + 1..] {
                if a.map == b.map
                    && tuple[a.key] == tuple[b.key]
                    && (tuple[a.presence] != tuple[b.presence]
                        || a.values
                            .iter()
                            .zip(&b.values)
                            .any(|(x, y)| tuple[*x] != tuple[*y]))
                {
                    return false;
                }
            }
        }
        // A summary includes at least its distinct present entries, when
        // entries add nonnegative amounts.
        for summary in self.summaries.iter().filter(|s| s.nonnegative) {
            let named: i128 = self
                .classes(summary.map, tuple)
                .iter()
                .map(|entry| self.contribution(summary, entry, tuple))
                .sum();
            if tuple[summary.position] < named {
                return false;
            }
        }
        true
    }

    /// One representative entry per key class of a map.
    fn classes(&self, map: u32, tuple: &[i128]) -> Vec<&EntryView> {
        let mut seen = BTreeSet::new();
        self.entries
            .iter()
            .filter(|e| e.map == map && seen.insert(tuple[e.key]))
            .collect()
    }

    fn contribution(&self, summary: &SummaryView, entry: &EntryView, view: &[i128]) -> i128 {
        if view[entry.presence] != 1 {
            return 0;
        }
        summary.field.map_or(1, |j| view[entry.values[j]])
    }

    /// The alias-merge and summary-delta obligations on a committed post.
    fn obligations(
        &self,
        tuple: &[i128],
        post: &BTreeMap<u16, i128>,
    ) -> Option<(&'static str, String)> {
        let read = |position: usize| {
            self.positions[position]
                .field
                .and_then(|field| post.get(&field).copied())
                .unwrap_or(tuple[position])
        };
        // The merged post view of each entry: at most one aliased entry
        // writes a new value.
        let mut merged: Vec<i128> = tuple.to_vec();
        let maps: BTreeSet<u32> = self.entries.iter().map(|e| e.map).collect();
        for map in &maps {
            let classes: BTreeSet<i128> = self
                .entries
                .iter()
                .filter(|e| e.map == *map)
                .map(|e| tuple[e.key])
                .collect();
            for class in classes {
                let members: Vec<&EntryView> = self
                    .entries
                    .iter()
                    .filter(|e| e.map == *map && tuple[e.key] == class)
                    .collect();
                let view = |e: &EntryView| {
                    core::iter::once(read(e.presence))
                        .chain(e.values.iter().map(|p| read(*p)))
                        .collect::<Vec<_>>()
                };
                let before = |e: &EntryView| {
                    core::iter::once(tuple[e.presence])
                        .chain(e.values.iter().map(|p| tuple[*p]))
                        .collect::<Vec<_>>()
                };
                let mut written: Vec<Vec<i128>> = members
                    .iter()
                    .map(|e| view(e))
                    .filter(|v| *v != before(members[0]))
                    .collect();
                written.dedup();
                if written.len() > 1 {
                    let ids: Vec<u32> = members.iter().map(|e| e.id).collect();
                    return Some((
                        "alias merge",
                        format!("entries {ids:?} name one key and write different values"),
                    ));
                }
                if let Some(value) = written.pop() {
                    for e in &members {
                        for (position, v) in core::iter::once(e.presence)
                            .chain(e.values.iter().copied())
                            .zip(&value)
                        {
                            merged[position] = *v;
                        }
                    }
                }
            }
        }
        for summary in self.summaries {
            let entries_delta: i128 = self
                .classes(summary.map, tuple)
                .iter()
                .map(|e| {
                    self.contribution(summary, e, &merged) - self.contribution(summary, e, tuple)
                })
                .sum();
            let summary_delta = read(summary.position) - tuple[summary.position];
            if entries_delta != summary_delta {
                return Some((
                    "summary delta",
                    format!(
                        "summary {} changes by {summary_delta}, its named entries by {entries_delta}",
                        summary.id
                    ),
                ));
            }
        }
        None
    }

    fn view_json(&self, tuple: &[i128]) -> Value {
        json!(
            self.positions
                .iter()
                .zip(tuple)
                .map(|(p, v)| (p.name.clone(), json!(v.to_string())))
                .collect::<BTreeMap<_, _>>()
        )
    }
}

/// Advances the odometer; false after the last tuple.
fn next(values: &[Vec<i128>], digits: &mut [usize], tuple: &mut [i128]) -> bool {
    for index in (0..values.len()).rev() {
        if digits[index] + 1 < values[index].len() {
            digits[index] += 1;
            tuple[index] = values[index][digits[index]];
            return true;
        }
        digits[index] = 0;
        tuple[index] = values[index][0];
    }
    false
}

/// The number of equality patterns of `n` keys: the Bell number.
fn bell(n: u32) -> u128 {
    let mut row = vec![1u128];
    for _ in 0..n {
        let mut next = vec![*row.last().unwrap_or(&1)];
        for value in &row {
            let last = *next.last().unwrap_or(&0);
            next.push(last.saturating_add(*value));
        }
        row = next;
    }
    row[0]
}

/// Refuses rules that use a key other than in `==` or `!=` with another key
/// of its type: a key has no value, order or arithmetic.
/// A stored key field of an entry may be written a key of its type: the
/// entry then holds that key's class.
fn refuse_key_values(rules: &Rules, keys: &BTreeMap<String, u32>) -> Result<(), ContractError> {
    for (index, case) in rules.cases.iter().enumerate() {
        let place = format!("v2/policy.json cases[{index}]");
        let mut expressions = vec![&case.when];
        for (field, ast) in &case.post {
            let stored = keys
                .iter()
                .find(|(name, _)| name.starts_with("pre.") && name.ends_with(&format!(".{field}")))
                .map(|(_, key_type)| *key_type);
            let written = expr::expand(ast, &rules.variables)
                .map_err(|reason| ContractError::new(&place, reason))?;
            match (stored, &written) {
                (Some(key_type), Ast::Name(name)) if keys.get(name) == Some(&key_type) => {}
                (Some(_), _) => {
                    return Err(ContractError::new(
                        &place,
                        format!("a stored key field ({field}) is written only a key of its type"),
                    ));
                }
                (None, _) => expressions.push(ast),
            }
        }
        expressions.extend(case.outbox.iter().flat_map(|d| d.payload.values()));
        for ast in expressions {
            let expanded = expr::expand(ast, &rules.variables)
                .map_err(|reason| ContractError::new(&place, reason))?;
            if misuses_key(&expanded, keys) {
                return Err(ContractError::new(
                    &place,
                    "a key is compared only with == or != against a key of its type",
                ));
            }
        }
    }
    Ok(())
}

fn misuses_key(ast: &Ast, keys: &BTreeMap<String, u32>) -> bool {
    match ast {
        Ast::Circuit(c) => c.formulas().any(|f| misuses_key(f, keys)),
        Ast::Binary(Binary::Eq | Binary::Ne, a, b) => match (a.as_ref(), b.as_ref()) {
            (Ast::Name(x), Ast::Name(y)) if keys.contains_key(x) || keys.contains_key(y) => {
                keys.get(x).is_none() || keys.get(x) != keys.get(y)
            }
            _ => misuses_key(a, keys) || misuses_key(b, keys),
        },
        Ast::Name(name) => keys.contains_key(name),
        Ast::Bool(_) | Ast::Int(_) | Ast::U128(_) | Ast::DecisionReason => false,
        Ast::ToI128(a) | Ast::ToU128(a) | Ast::Not(a) | Ast::Neg(a) => misuses_key(a, keys),
        Ast::Binary(_, a, b) | Ast::Div(_, a, b) => misuses_key(a, keys) || misuses_key(b, keys),
        Ast::Choose(a, b, c)
        | Ast::MulDiv(_, a, b, c)
        | Ast::SignedMulDivFloor(a, b, c)
        | Ast::ScaledMulDiv(_, _, a, b, c, _)
        | Ast::RepeatMulDiv(_, a, b, c, _) => {
            misuses_key(a, keys) || misuses_key(b, keys) || misuses_key(c, keys)
        }
        Ast::ProductsGe(arguments) => arguments.iter().any(|a| misuses_key(a, keys)),
    }
}
