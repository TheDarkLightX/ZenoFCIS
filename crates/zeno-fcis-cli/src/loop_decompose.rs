//! `zeno-fcis loop decompose`, steps 1 to 3: an architecture of components
//! in merge order is held against the core's decisions on the whole
//! declared domain.
//!
//! 1. **Required behaviour.** Each input's decision belongs to the component
//!    that owns its reason (an accept to none). A component before that owner
//!    must accept and give the core's successor for the fields it owns; the
//!    owner must make the core's decision; after it nothing is required.
//! 2. **Realizability.** A component decides from the fields it owns and
//!    reads. Two inputs that agree on those but need different behaviour
//!    are an interface counterexample; the smallest set of further reads
//!    that removes every such pair is suggested, and a read owned by a later
//!    component means the architecture needs a reorder. Reasons no
//!    component owns, and committed failures that must change a field a
//!    later component owns, are reported.
//! 3. **Case tables.** A component given without cases gets the core's case
//!    table projected onto it, or a lookup table; a component with cases is
//!    blamed, with a region and an example, wherever it differs from its
//!    required behaviour. A component's decision that commits a successor
//!    outside a field's domain is marked refused, which counts as a
//!    difference.
//!
//! The required behaviour comes from the library Authority's decisions on
//! the core. Steps 2 and 3 are this door's own evaluation of the given
//! tables with the doors' expression evaluator: no component Authority or
//! checker decides them, so their verdicts are proposals (`"decided_by":
//! "door-simulation"`), and an interface counterexample is a witness anyone
//! can recheck. Whether the composed architecture behaves as the core is
//! step 4, the composition check, the only checked verdict; this module
//! does not claim it. Results hold over the declared finite domain only.
//!
//! This module is LLM-generated.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use zeno_fcis_contract::internals::rules::Class;
use zeno_fcis_loop_doors::expr::{Expr, Kind, Op, kind_of, names_of};

use super::judges::{Core, Result, Stop, class_of, label};

/// Most cases a lookup table may have.
const MAX_LOOKUP: usize = 64;
/// A decision code: 0 for an accept, else `class * 2^32 + reason + 1`.
type Code = i64;
/// No case of the component fires.
const NOCASE: Code = -1;
/// The component's Authority refuses its own decision.
const REFUSED: Code = -2;

fn code(class: Class, reason: Option<u32>) -> Code {
    let (class, reason) = label(class, reason);
    if class == 0 {
        0
    } else {
        (i64::from(class) << 32) + i64::from(reason.unwrap_or(0)) + 1
    }
}

fn decode(code: Code) -> (Class, Option<u32>) {
    if code <= 0 {
        return (Class::Accept, None);
    }
    let class = class_of(u8::try_from(code >> 32).unwrap_or(1));
    let reason = u32::try_from((code & 0xFFFF_FFFF) - 1).ok();
    (class, reason)
}

/// Successor values of a component's owned fields, by state position.
type Posts = BTreeMap<usize, Vec<i64>>;

/// One case of a component: a guard and a decision with successors for
/// fields it owns.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PartCase {
    pub(super) when: Expr,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) sets: BTreeMap<usize, Expr>,
}

/// One component, in merge order.
#[derive(Clone, Debug)]
pub(super) struct Part {
    pub(super) name: String,
    pub(super) owns: Vec<usize>,
    pub(super) reads: Vec<usize>,
    pub(super) decides: BTreeSet<u32>,
    pub(super) cases: Vec<PartCase>,
}

/// The architecture after the structural checks.
struct Structure {
    parts: Vec<Part>,
    /// The fields each component decides from, in program order.
    observes: Vec<Vec<usize>>,
    owner_of_field: BTreeMap<usize, usize>,
    owner_of_reason: BTreeMap<u32, usize>,
}

/// What one component must do on each input.
struct Need {
    care_verdict: Vec<bool>,
    verdict: Vec<Code>,
    care_post: Vec<bool>,
    /// Per owned field, by state position: the required successor value.
    post: BTreeMap<usize, Vec<i64>>,
}

fn refuse<T>(reason: impl Into<String>) -> Result<T> {
    Err(Stop::Invalid(reason.into()))
}

/// Reads a `zeno-fcis/loop-architecture/1` document.
pub(super) fn parts(core: &Core, document: &Value) -> Result<Vec<Part>> {
    if document["schema"] != "zeno-fcis/loop-architecture/1" {
        return refuse("architecture-schema");
    }
    let field = |name: &str, part: &str| -> Result<usize> {
        core.names
            .iter()
            .position(|n| n == name)
            .ok_or_else(|| Stop::Invalid(format!("decompose-unknown-field:{part}:{name}")))
    };
    let mut out = Vec::new();
    for item in document["components"]
        .as_array()
        .ok_or_else(|| Stop::Invalid("decompose-needs-components".into()))?
    {
        let name = item["name"]
            .as_str()
            .ok_or_else(|| Stop::Invalid("component-name".into()))?
            .to_owned();
        let list = |key: &str| -> Vec<String> {
            item[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        };
        let owns = list("owns")
            .iter()
            .map(|f| field(f, &name))
            .collect::<Result<Vec<_>>>()?;
        let reads = list("reads")
            .iter()
            .map(|f| field(f, &name))
            .collect::<Result<Vec<_>>>()?;
        let decides = list("decides")
            .iter()
            .map(|r| {
                core.reason_id(r)
                    .ok_or_else(|| Stop::Invalid(format!("decompose-unknown-reason:{r}")))
            })
            .collect::<Result<BTreeSet<_>>>()?;
        let mut cases = Vec::new();
        for case in item["cases"].as_array().into_iter().flatten() {
            let class = match case["class"].as_str() {
                Some("Accept") => Class::Accept,
                Some("Reject") => Class::Reject,
                Some("CommittedFailure") => Class::CommittedFailure,
                other => return refuse(format!("decompose-case-class:{name}:{other:?}")),
            };
            let reason = match case["reason"].as_str() {
                Some(r) => Some(
                    core.reason_id(r)
                        .ok_or_else(|| Stop::Invalid(format!("decompose-unknown-reason:{r}")))?,
                ),
                None => None,
            };
            let mut sets = BTreeMap::new();
            for (f, text) in case["sets"].as_object().into_iter().flatten() {
                let text = text
                    .as_str()
                    .ok_or_else(|| Stop::Invalid(format!("decompose-case-sets:{name}")))?;
                sets.insert(field(f, &name)?, core.read_checked(text)?);
            }
            cases.push(PartCase {
                when: core.read_checked(case["when"].as_str().unwrap_or("true"))?,
                class,
                reason,
                sets,
            });
        }
        out.push(Part {
            name,
            owns,
            reads,
            decides,
            cases,
        });
    }
    Ok(out)
}

/// The structural checks, which evaluate nothing.
fn structure(core: &Core, parts: Vec<Part>) -> Result<Structure> {
    if parts.is_empty() {
        return refuse("decompose-needs-components");
    }
    let mut names = BTreeSet::new();
    let mut owner_of_field = BTreeMap::new();
    for (k, part) in parts.iter().enumerate() {
        if !names.insert(part.name.clone()) {
            return refuse("decompose-duplicate-component");
        }
        for f in &part.owns {
            if *f >= core.state_width {
                return refuse(format!(
                    "decompose-owns-a-non-state-field:{}:{}",
                    part.name, core.names[*f]
                ));
            }
            if owner_of_field.insert(*f, k).is_some() {
                return refuse(format!("decompose-field-owned-twice:{}", core.names[*f]));
            }
        }
    }
    for f in 0..core.state_width {
        if !owner_of_field.contains_key(&f) {
            return refuse(format!("decompose-unowned-state-field:{}", core.names[f]));
        }
    }
    let mut observes = Vec::new();
    let mut owner_of_reason = BTreeMap::new();
    for (k, part) in parts.iter().enumerate() {
        let mut seen: Vec<usize> = part.owns.iter().chain(&part.reads).copied().collect();
        seen.sort_unstable();
        seen.dedup();
        let observed: BTreeSet<&str> = seen.iter().map(|f| core.names[*f].as_str()).collect();
        let mut reasons = part.decides.clone();
        for case in &part.cases {
            let mut read: BTreeSet<String> = names_of(&case.when);
            for value in case.sets.values() {
                read.extend(names_of(value));
            }
            if let Some(missing) = read.iter().find(|n| !observed.contains(n.as_str())) {
                return refuse(format!(
                    "decompose-case-reads-undeclared:{}:{missing}",
                    part.name
                ));
            }
            if kind_of(&case.when, &core.vocab)?.is_some_and(|kind| kind != Kind::Bool) {
                return refuse(format!("decompose-case-kind:{}", part.name));
            }
            if let Some(f) = case.sets.keys().find(|f| !part.owns.contains(f)) {
                return refuse(format!(
                    "decompose-case-sets-unowned:{}:{}",
                    part.name, core.names[*f]
                ));
            }
            if case.class == Class::Reject && !case.sets.is_empty() {
                return refuse(format!("decompose-reject-keeps-the-state:{}", part.name));
            }
            match (case.class, case.reason) {
                (Class::Accept, _) => {}
                (_, None) => return refuse(format!("decompose-case-needs-a-reason:{}", part.name)),
                (_, Some(reason)) => {
                    if !part.decides.is_empty() && !part.decides.contains(&reason) {
                        return refuse(format!(
                            "decompose-case-reason-not-declared:{}:{}",
                            part.name,
                            core.reason_name(Some(reason)).unwrap_or_default()
                        ));
                    }
                    reasons.insert(reason);
                }
            }
        }
        for reason in reasons {
            if owner_of_reason.insert(reason, k).is_some() {
                return refuse(format!(
                    "decompose-reason-owned-twice:{}",
                    core.reason_name(Some(reason)).unwrap_or_default()
                ));
            }
        }
        observes.push(seen);
    }
    Ok(Structure {
        parts,
        observes,
        owner_of_field,
        owner_of_reason,
    })
}

/// The core's code and successor values on each tuple.
struct Monolith {
    code: Vec<Code>,
    class: Vec<Class>,
    reason: Vec<Option<u32>>,
    /// Per state position, the successor value on each tuple.
    post: Vec<Vec<i64>>,
}

fn monolith(core: &Core) -> Result<Monolith> {
    let n = core.tuples();
    let mut out = Monolith {
        code: Vec::with_capacity(n),
        class: Vec::with_capacity(n),
        reason: Vec::with_capacity(n),
        post: vec![Vec::with_capacity(n); core.state_width],
    };
    for ordinal in 0..n {
        let Some(d) = core.decisions[ordinal] else {
            return refuse("decompose-core-has-authority-refusals");
        };
        out.code.push(code(d.class, d.reason));
        out.class.push(d.class);
        out.reason.push(d.reason);
        let state = core.state(d.next);
        for (k, column) in out.post.iter_mut().enumerate() {
            column.push(state[k]);
        }
    }
    Ok(out)
}

/// The required behaviour of every component, and the problems no case
/// table can fix.
fn needs(core: &Core, st: &Structure, mono: &Monolith) -> (Vec<Need>, Vec<Value>) {
    let n = core.tuples();
    let parts = st.parts.len();
    let mut owner = vec![parts; n];
    let mut unowned = vec![false; n];
    let mut problems = Vec::new();
    let mut unowned_reasons: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for o in 0..n {
        if mono.class[o] == Class::Accept {
            continue;
        }
        let reason = mono.reason[o].unwrap_or(0);
        match st.owner_of_reason.get(&reason) {
            Some(k) => owner[o] = *k,
            None => {
                unowned[o] = true;
                let entry = unowned_reasons.entry(reason).or_insert((o, 0));
                entry.1 += 1;
            }
        }
    }
    for (reason, (first, count)) in unowned_reasons {
        problems.push(json!({
            "kind": "unowned-reason",
            "reason": core.reason_name(Some(reason)),
            "ordinal": first,
            "input": core.shown(first),
            "count": count,
        }));
    }
    for (f, k) in &st.owner_of_field {
        let wrong: Vec<usize> = (0..n)
            .filter(|o| {
                mono.class[*o] == Class::CommittedFailure
                    && owner[*o] < *k
                    && mono.post[*f][*o] != core.columns[*f][*o]
            })
            .collect();
        if let Some(first) = wrong.first() {
            problems.push(json!({
                "kind": "failure-changes-a-later-field",
                "field": core.names[*f],
                "component": st.parts[*k].name,
                "ordinal": first,
                "input": core.shown(*first),
                "count": wrong.len(),
            }));
        }
    }
    let needs = (0..parts)
        .map(|k| {
            let care_verdict: Vec<bool> = (0..n).map(|o| owner[o] >= k && !unowned[o]).collect();
            let verdict: Vec<Code> = (0..n)
                .map(|o| if owner[o] == k { mono.code[o] } else { 0 })
                .collect();
            let care_post: Vec<bool> = (0..n)
                .map(|o| {
                    !unowned[o]
                        && (mono.class[o] == Class::Accept
                            || (mono.class[o] == Class::CommittedFailure && owner[o] >= k))
                })
                .collect();
            let post = st.parts[k]
                .owns
                .iter()
                .map(|f| (*f, mono.post[*f].clone()))
                .collect();
            Need {
                care_verdict,
                verdict,
                care_post,
                post,
            }
        })
        .collect();
    (needs, problems)
}

/// The observed-field key of every tuple for `fields`.
fn keys(core: &Core, fields: &[usize]) -> Vec<u64> {
    (0..core.tuples())
        .map(|o| {
            fields.iter().fold(0u64, |key, f| {
                let values = &core.values[*f];
                let at = values
                    .iter()
                    .position(|v| *v == core.columns[*f][o])
                    .unwrap_or(0);
                key.wrapping_mul(values.len() as u64)
                    .wrapping_add(at as u64)
            })
        })
        .collect()
}

/// The first two cared tuples with the same key and different required
/// values.
fn clash(key: &[u64], value: &[i64], care: &[bool]) -> Option<(usize, usize)> {
    let mut seen: BTreeMap<u64, (i64, usize)> = BTreeMap::new();
    let mut best: Option<(usize, usize)> = None;
    for o in (0..key.len()).filter(|o| care[*o]) {
        match seen.get(&key[o]) {
            Some((v, first)) if *v != value[o] => {
                if best.is_none_or(|(a, b)| (*first, o) < (a, b)) {
                    best = Some((*first, o));
                }
            }
            Some(_) => {}
            None => {
                seen.insert(key[o], (value[o], o));
            }
        }
    }
    best
}

/// Keys over `fields` that carry more than one required value.
fn score(core: &Core, need: &Need, fields: &[usize]) -> usize {
    let key = keys(core, fields);
    let mut count = 0;
    let mut tally = |values: &[i64], care: &[bool]| {
        let mut seen: BTreeMap<u64, (i64, bool)> = BTreeMap::new();
        for o in (0..key.len()).filter(|o| care[*o]) {
            let entry = seen.entry(key[o]).or_insert((values[o], false));
            if entry.0 != values[o] {
                entry.1 = true;
            }
        }
        count += seen.values().filter(|(_, many)| *many).count();
    };
    tally(&need.verdict, &need.care_verdict);
    for values in need.post.values() {
        tally(values, &need.care_post);
    }
    count
}

/// The irreducible further reads that make a component's required
/// behaviour a function of what it observes, or `None` when even every
/// field does not.
fn suggest_reads(core: &Core, st: &Structure, need: &Need, k: usize) -> Option<Vec<usize>> {
    let observed = &st.observes[k];
    let candidates: Vec<usize> = (0..core.names.len())
        .filter(|f| !observed.contains(f))
        .collect();
    let with = |extra: &[usize]| -> Vec<usize> {
        let mut fields: Vec<usize> = observed.iter().chain(extra).copied().collect();
        fields.sort_unstable();
        fields
    };
    if score(core, need, &with(&candidates)) > 0 {
        return None;
    }
    let later = |f: usize| st.owner_of_field.get(&f).is_some_and(|owner| *owner > k);
    let mut order = candidates.clone();
    order.sort_by_key(|f| (!later(*f), std::cmp::Reverse(*f)));
    let mut kept: BTreeSet<usize> = candidates.into_iter().collect();
    for f in order {
        kept.remove(&f);
        let trial: Vec<usize> = kept.iter().copied().collect();
        if score(core, need, &with(&trial)) > 0 {
            kept.insert(f);
        }
    }
    Some(kept.into_iter().collect())
}

/// Interface counterexamples: per component, two inputs that agree on what
/// it observes but need a different decision, else a different successor of
/// a field it owns.
fn interfaces(core: &Core, st: &Structure, needs: &[Need]) -> Vec<Value> {
    let mut out = Vec::new();
    for (k, need) in needs.iter().enumerate() {
        let key = keys(core, &st.observes[k]);
        let mut found = clash(&key, &need.verdict, &need.care_verdict)
            .map(|pair| (pair, "decision".to_owned()));
        if found.is_none() {
            for (f, values) in &need.post {
                if let Some(pair) = clash(&key, values, &need.care_post) {
                    found = Some((pair, format!("successor of {}", core.names[*f])));
                    break;
                }
            }
        }
        let Some(((a, b), what)) = found else {
            continue;
        };
        let separating: Vec<&str> = (0..core.names.len())
            .filter(|f| !st.observes[k].contains(f) && core.columns[*f][a] != core.columns[*f][b])
            .map(|f| core.names[f].as_str())
            .collect();
        let suggested = suggest_reads(core, st, need, k);
        let later: Vec<&str> = suggested
            .iter()
            .flatten()
            .filter(|f| st.owner_of_field.get(f).is_some_and(|owner| *owner > k))
            .map(|f| core.names[*f].as_str())
            .collect();
        let required = (what == "decision")
            .then(|| [a, b].map(|o| option(core, need.verdict[o], None)).to_vec());
        out.push(json!({
            "component": st.parts[k].name,
            "pair": [a, b],
            "what": what,
            "inputs": [core.shown(a), core.shown(b)],
            "separating_fields": separating,
            "required": required,
            "suggested_reads": suggested.map(|reads| reads.iter().map(|f| core.names[*f].clone()).collect::<Vec<_>>()),
            "suggested_reads_owned_later": later,
        }));
    }
    out
}

fn option(core: &Core, code: Code, sets: Option<BTreeMap<String, Value>>) -> Value {
    match code {
        NOCASE => json!({"class": "NoCase", "reason": null}),
        REFUSED => json!({"class": "Refused", "reason": "successor-domain"}),
        _ => {
            let (class, reason) = decode(code);
            let mut out = json!({"class": class.name(), "reason": core.reason_name(reason)});
            if let Some(sets) = sets.filter(|s| !s.is_empty()) {
                out["sets"] = Value::Object(sets.into_iter().collect());
            }
            out
        }
    }
}

/// A component's own decisions: the first case that fires, the successors
/// of the fields it owns (unset ones keep their value), and a refusal where
/// a committed successor leaves its field's domain.
fn evaluate_part(core: &Core, part: &Part) -> Result<(Vec<Code>, Posts)> {
    let n = core.tuples();
    let mut codes = vec![NOCASE; n];
    let mut post: BTreeMap<usize, Vec<i64>> = part
        .owns
        .iter()
        .map(|f| (*f, core.columns[*f].clone()))
        .collect();
    let mut decided = vec![false; n];
    for case in &part.cases {
        let fires = core.eval(&case.when)?;
        let sets: BTreeMap<usize, Vec<i64>> = if case.class == Class::Reject {
            BTreeMap::new()
        } else {
            case.sets
                .iter()
                .map(|(f, e)| Ok((*f, core.eval(e)?)))
                .collect::<Result<_>>()?
        };
        for o in 0..n {
            if decided[o] || fires[o] != 1 {
                continue;
            }
            decided[o] = true;
            codes[o] = code(case.class, case.reason);
            for (f, values) in &sets {
                if let Some(column) = post.get_mut(f) {
                    column[o] = values[o];
                }
            }
        }
    }
    for o in 0..n {
        let commits =
            codes[o] == 0 || (codes[o] > 0 && decode(codes[o]).0 == Class::CommittedFailure);
        if commits
            && post
                .iter()
                .any(|(f, values)| !core.values[*f].contains(&values[o]))
        {
            codes[o] = REFUSED;
        }
    }
    Ok((codes, post))
}

/// Where a component differs from its required behaviour.
fn wrong(need: &Need, codes: &[Code], post: &BTreeMap<usize, Vec<i64>>) -> Vec<bool> {
    (0..codes.len())
        .map(|o| {
            (need.care_verdict[o] && codes[o] != need.verdict[o])
                || (need.care_post[o]
                    && need
                        .post
                        .iter()
                        .any(|(f, values)| post.get(f).is_some_and(|p| p[o] != values[o])))
        })
        .collect()
}

/// The core's case table projected onto component `k`, or why it does not
/// project. `compact` drops the cases where a later component rejects after
/// the component's last own decision.
fn project(
    core: &Core,
    st: &Structure,
    k: usize,
    compact: bool,
) -> std::result::Result<Vec<PartCase>, Value> {
    let part = &st.parts[k];
    let mut cases: Vec<(PartCase, bool)> = Vec::new();
    for (i, case) in core.cases.iter().enumerate() {
        let mine = case.class != Class::Accept
            && case
                .reason
                .and_then(|r| st.owner_of_reason.get(&r))
                .is_some_and(|owner| *owner == k);
        if case.class != Class::Accept {
            match case.reason.and_then(|r| st.owner_of_reason.get(&r)) {
                None => {
                    return Err(
                        json!({"case": i, "reason": format!("no component owns {}", core.reason_name(case.reason).unwrap_or_default())}),
                    );
                }
                Some(owner) if *owner < k => continue,
                Some(_) => {}
            }
        }
        let sets: BTreeMap<usize, Expr> = if case.class == Class::Reject {
            BTreeMap::new()
        } else {
            case.post
                .iter()
                .filter(|(f, e)| part.owns.contains(f) && **e != Expr::Var(core.names[**f].clone()))
                .map(|(f, e)| (*f, e.clone()))
                .collect()
        };
        let (class, reason, sets) = if mine {
            let sets = if case.class == Class::CommittedFailure {
                sets
            } else {
                BTreeMap::new()
            };
            (case.class, case.reason, sets)
        } else {
            (Class::Accept, None, sets)
        };
        let loose = case.class == Class::Reject && !mine;
        cases.push((
            PartCase {
                when: case.when.clone(),
                class,
                reason,
                sets,
            },
            loose,
        ));
        if case.when == Expr::Int(1) {
            break;
        }
    }
    if compact && let Some(last_own) = cases.iter().rposition(|(c, _)| c.class != Class::Accept) {
        let mut index = 0;
        cases.retain(|(_, loose)| {
            let keep = index <= last_own || !loose;
            index += 1;
            keep
        });
    }
    // Adjacent cases with the same decision merge.
    let mut merged: Vec<PartCase> = Vec::new();
    for (case, _) in cases {
        match merged.last_mut() {
            Some(last)
                if last.class == case.class
                    && last.reason == case.reason
                    && last.sets == case.sets =>
            {
                last.when = if last.when == Expr::Int(1) || case.when == Expr::Int(1) {
                    Expr::Int(1)
                } else {
                    Expr::bin(Op::Or, last.when.clone(), case.when)
                };
            }
            _ => merged.push(case),
        }
    }
    if let Some(last) = merged.last_mut() {
        last.when = Expr::Int(1);
    }
    let observed: BTreeSet<&str> = st.observes[k]
        .iter()
        .map(|f| core.names[*f].as_str())
        .collect();
    for (i, case) in merged.iter().enumerate() {
        let mut read = names_of(&case.when);
        for value in case.sets.values() {
            read.extend(names_of(value));
        }
        if let Some(missing) = read.iter().find(|n| !observed.contains(n.as_str())) {
            return Err(json!({"case": i, "reason": format!("case {i} reads {missing}")}));
        }
    }
    Ok(merged)
}

/// A lookup table over what component `k` observes, from its required
/// behaviour; `None` when it would need more than [`MAX_LOOKUP`] cases.
fn tabulate(core: &Core, st: &Structure, need: &Need, k: usize) -> Option<Vec<PartCase>> {
    let observed = &st.observes[k];
    let n = core.tuples();
    let care: Vec<bool> = (0..n)
        .map(|o| need.care_verdict[o] || need.care_post[o])
        .collect();
    let accept = |when: Expr| PartCase {
        when,
        class: Class::Accept,
        reason: None,
        sets: BTreeMap::new(),
    };
    if !care.iter().any(|c| *c) {
        return Some(vec![accept(Expr::Int(1))]);
    }
    let key = keys(core, observed);
    let mut order: Vec<u64> = Vec::new();
    let mut groups: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for o in (0..n).filter(|o| care[*o]) {
        let group = groups.entry(key[o]).or_default();
        if group.is_empty() {
            order.push(key[o]);
        }
        group.push(o);
    }
    if order.len() > MAX_LOOKUP {
        return None;
    }
    let mut cases = Vec::new();
    for group_key in order {
        let members = &groups[&group_key];
        let verdict = members
            .iter()
            .find(|o| need.care_verdict[**o])
            .map_or(0, |o| need.verdict[*o]);
        let (class, reason) = decode(verdict);
        let mut sets = BTreeMap::new();
        if class != Class::Reject
            && let Some(o) = members.iter().find(|o| need.care_post[**o])
        {
            for (f, values) in &need.post {
                sets.insert(*f, Expr::Int(values[*o]));
            }
        }
        let first = members[0];
        let when = observed
            .iter()
            .map(|f| {
                let variable = Expr::Var(core.names[*f].clone());
                let value = core.columns[*f][first];
                if core.bools[*f] {
                    if value == 1 {
                        variable
                    } else {
                        Expr::negation(variable)
                    }
                } else {
                    Expr::bin(Op::Eq, variable, Expr::Int(value))
                }
            })
            .reduce(|a, b| Expr::bin(Op::And, a, b))
            .unwrap_or(Expr::Int(1));
        cases.push(PartCase {
            when,
            class,
            reason,
            sets,
        });
    }
    cases.push(accept(Expr::Int(1)));
    Some(cases)
}

fn cases_json(core: &Core, cases: &[PartCase]) -> Value {
    Value::from(
        cases
            .iter()
            .map(|c| {
                json!({
                    "when": core.show(&c.when),
                    "class": c.class.name(),
                    "reason": core.reason_name(c.reason),
                    "sets": c.sets.iter().map(|(f, e)| (core.names[*f].clone(), json!(core.show(e)))).collect::<serde_json::Map<_, _>>(),
                })
            })
            .collect::<Vec<_>>(),
    )
}

/// Steps 1 to 3 for an architecture against the core (the report), and,
/// when the architecture is realizable and every
/// component has a case table (given or synthesized), the components with
/// their tables, for the composed check.
pub(super) fn decompose_with_tables(
    core: &Core,
    parts: Vec<Part>,
) -> Result<(Value, Option<Vec<Part>>)> {
    let st = match structure(core, parts) {
        Ok(st) => st,
        Err(Stop::Invalid(reason)) => {
            return Ok((json!({"verdict": "refused", "reason": reason}), None));
        }
        Err(stop) => return Err(stop),
    };
    let mut order = Vec::new();
    while order.len() < st.parts.len() {
        let next = (0..st.parts.len()).find(|k| {
            !order.contains(k)
                && st.parts[*k].reads.iter().all(|f| {
                    *f >= core.state_width
                        || st.owner_of_field[f] == *k
                        || order.contains(&st.owner_of_field[f])
                })
        });
        let Some(next) = next else { break };
        order.push(next);
    }
    let suggested_order = (order.len() == st.parts.len()).then(|| {
        order
            .iter()
            .map(|k| st.parts[*k].name.clone())
            .collect::<Vec<_>>()
    });
    let mut problems = Vec::new();
    for (k, part) in st.parts.iter().enumerate() {
        for f in &part.reads {
            if *f < core.state_width && st.owner_of_field[f] > k {
                problems.push(json!({"what": "reads-later", "kind": "reads-later", "component": part.name, "field": core.names[*f],
                    "owner": st.parts[st.owner_of_field[f]].name, "suggested_order": suggested_order,
                    "scope": "submitted-component-order", "cyclic_reads": suggested_order.is_none()}));
            }
        }
    }
    if !problems.is_empty() {
        return Ok((
            json!({"verdict": "unrealizable", "structure": problems,
            "decided_by": "door-structure", "authority": "none"}),
            None,
        ));
    }
    let mono = match monolith(core) {
        Ok(mono) => mono,
        Err(Stop::Invalid(reason)) => {
            return Ok((json!({"verdict": "refused", "reason": reason}), None));
        }
        Err(stop) => return Err(stop),
    };
    let (needs, problems) = needs(core, &st, &mono);
    let clashes = interfaces(core, &st, &needs);
    let realizable = problems.is_empty() && clashes.is_empty();
    let local = |k: usize| -> u128 {
        st.observes[k]
            .iter()
            .map(|f| core.values[*f].len() as u128)
            .product()
    };
    let mut components: Vec<Value> = st
        .parts
        .iter()
        .enumerate()
        .map(|(k, part)| {
            json!({
                "name": part.name,
                "owns": part.owns.iter().map(|f| core.names[*f].clone()).collect::<Vec<_>>(),
                "reads": st.observes[k].iter().filter(|f| !part.owns.contains(f)).map(|f| core.names[*f].clone()).collect::<Vec<_>>(),
                "decides": st.owner_of_reason.iter().filter(|(_, owner)| **owner == k).map(|(r, _)| core.reason_name(Some(*r))).collect::<Vec<_>>(),
                "local_domain": local(k).to_string(),
                "cases": part.cases.len(),
            })
        })
        .collect();
    let mut out = json!({
        "decided_by": "door-simulation",
        "domain": core.tuples(),
        "structure": problems,
        "interface": clashes,
        "realizable": realizable,
        "local_domain_total": (0..st.parts.len()).map(local).sum::<u128>().to_string(),
        "composition": "not checked here: the composed check is `loop decompose --compose`",
    });
    let mut filled: Vec<Vec<PartCase>> = st.parts.iter().map(|p| p.cases.clone()).collect();
    let empty: Vec<usize> = (0..st.parts.len())
        .filter(|k| st.parts[*k].cases.is_empty())
        .collect();
    if !empty.is_empty() {
        if !realizable {
            out["verdict"] = json!("unrealizable");
            out["components"] = Value::from(components);
            return Ok((out, None));
        }
        let mut projection = serde_json::Map::new();
        let mut synthesized = Vec::new();
        let mut complete = true;
        for k in empty {
            let mut found: Option<(Vec<PartCase>, &str)> = None;
            let mut why = Value::Null;
            for (how, candidate) in [
                ("projected", project(core, &st, k, false)),
                ("projected-compact", project(core, &st, k, true)),
            ] {
                match candidate {
                    Ok(cases) => {
                        let part = Part {
                            cases: cases.clone(),
                            ..st.parts[k].clone()
                        };
                        let (codes, post) = evaluate_part(core, &part)?;
                        if !wrong(&needs[k], &codes, &post).iter().any(|w| *w) {
                            let better = match &found {
                                None => true,
                                Some((current, _)) => cases.len() < current.len(),
                            };
                            if better {
                                found = Some((cases, how));
                            }
                        }
                    }
                    Err(reason) => {
                        if why.is_null() {
                            why = reason;
                        }
                    }
                }
            }
            if found.is_none()
                && let Some(cases) = tabulate(core, &st, &needs[k], k)
            {
                let part = Part {
                    cases: cases.clone(),
                    ..st.parts[k].clone()
                };
                let (codes, post) = evaluate_part(core, &part)?;
                if !wrong(&needs[k], &codes, &post).iter().any(|w| *w) {
                    found = Some((cases, "lookup-table"));
                }
            }
            match found {
                Some((cases, how)) => {
                    projection.insert(
                        st.parts[k].name.clone(),
                        json!({"cases": cases.len(), "how": how, "table": cases_json(core, &cases)}),
                    );
                    synthesized.push(st.parts[k].name.clone());
                    filled[k] = cases;
                }
                None => {
                    complete = false;
                    projection.insert(
                        st.parts[k].name.clone(),
                        if why.is_null() {
                            json!({"reason": "no exact table within the lookup budget"})
                        } else {
                            why
                        },
                    );
                }
            }
        }
        out["projection"] = Value::Object(projection);
        out["synthesized"] = json!(synthesized);
        if !complete {
            out["verdict"] = json!("realizable");
            out["components"] = Value::from(components);
            return Ok((out, None));
        }
    }
    // Every component has cases: blame each where it differs from its
    // required behaviour.
    let mut blame = Vec::new();
    for (k, cases) in filled.iter().enumerate() {
        let part = Part {
            cases: cases.clone(),
            ..st.parts[k].clone()
        };
        let (codes, post) = evaluate_part(core, &part)?;
        let mask = wrong(&needs[k], &codes, &post);
        components[k]["cases"] = json!(cases.len());
        let count = mask.iter().filter(|w| **w).count();
        if count == 0 {
            continue;
        }
        let Some(first) = mask.iter().position(|w| *w) else {
            continue;
        };
        let need = &needs[k];
        let required_sets = need.care_post[first].then(|| {
            need.post
                .iter()
                .map(|(f, values)| (core.names[*f].clone(), core.value_json(*f, values[first])))
                .collect()
        });
        let commits = codes[first] == 0
            || (codes[first] > 0 && decode(codes[first]).0 == Class::CommittedFailure);
        let actual_sets = commits.then(|| {
            post.iter()
                .map(|(f, values)| (core.names[*f].clone(), core.value_json(*f, values[first])))
                .collect()
        });
        let mut region = core.region_json(&mask)?;
        region["component"] = json!(part.name);
        region["wrong"] = json!(count);
        region["decides_on"] = json!(
            (0..mask.len())
                .filter(|o| need.care_verdict[*o] || need.care_post[*o])
                .count()
        );
        region["required"] = option(core, need.verdict[first], required_sets);
        region["actual"] = option(core, codes[first], actual_sets);
        blame.push(region);
    }
    // A structure problem (a reason no component decides, a failure whose
    // field is owned later) or an interface counterexample leaves inputs
    // no table can get right, and on some of them no component is required
    // to do anything, so blame alone cannot see it.
    out["verdict"] = json!(if !realizable {
        "unrealizable"
    } else if !blame.is_empty() {
        "refuted"
    } else if out.get("synthesized").is_some() {
        "completed"
    } else {
        "meets-requirements"
    });
    if !blame.is_empty() {
        out["blame"] = Value::from(blame);
    }
    out["components"] = Value::from(components);
    if !realizable {
        return Ok((out, None));
    }
    let tables = st
        .parts
        .into_iter()
        .zip(filled)
        .map(|(part, cases)| Part { cases, ..part })
        .collect();
    Ok((out, Some(tables)))
}
