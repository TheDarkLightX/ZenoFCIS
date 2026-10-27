//! Small usage vocabulary and invocation-local derivations. LLM-generated.
use super::*;

/// Fixed usage schema, separate from the compatible structural schema.
pub const USAGE_SCHEMA: &str = "zeno-fcis/usage-knowledge/1";
pub(super) const RULES: &str = r#"
isa(X,Z) :- isa(X,Y), isa(Y,Z).
usage_replaces(X,Z) :- supersedes(X,Z).
usage_replaces(X,Z) :- supersedes(X,Y), usage_replaces(Y,Z).
usage_known(K,V) :- task(K,V), V != unknown.
usage_match(K,V) :- usage_known(K,V).
usage_match(K,any) :- usage_known(K,_).
usage_match(K,V) :- usage_known(K,S), isa(S,V), K != version, K != environment.
usage_missing(L,K,V) :- scope(L,K,V), not usage_match(K,V).
usage_topic(L,S) :- lesson(L,S).
usage_topic(L,S) :- lesson(L,C), isa(S,C).
usage_unknown_environment(L) :- usage_topic(L,S), usage_known(subject,S), blocker(L,_), not usage_known(environment,_).
usage_scope(L,S) :- usage_topic(L,S), usage_known(subject,S), not usage_missing(L,_,_).
usage_tier_one(L) :- strength(L,proved).
usage_tier_one(L) :- strength(L,refuted).
usage_tier_one(L) :- strength(L,certified).
usage_ready(L) :- record(L,promoted,usage-v1,_,_,_), usage_validated(L), not usage_tier_one(L).
usage_active_replace(N,O) :- supersedes(N,O), usage_ready(N).
usage_active_replace(N,O) :- supersedes(N,M), usage_ready(N), usage_active_replace(M,O).
usage_defeated(O,S) :- usage_active_replace(N,O), usage_scope(N,S), usage_scope(O,S).
usage_applies(L,S) :- usage_ready(L), usage_scope(L,S), not usage_defeated(L,S), not usage_clashing(L,S).
usage_clashing(A,S) :- conflict(A,B), usage_ready(A), usage_ready(B), usage_scope(A,S), usage_scope(B,S), not usage_active_replace(A,B), not usage_active_replace(B,A).
usage_clashing(B,S) :- conflict(A,B), usage_ready(A), usage_ready(B), usage_scope(A,S), usage_scope(B,S), not usage_active_replace(A,B), not usage_active_replace(B,A).
"#;
const VOCABULARY: [(&str, usize); 14] = [
    ("lesson", 2),
    ("scope", 3),
    ("strength", 2),
    ("effect", 2),
    ("evidence", 3),
    ("isa", 2),
    ("limit", 4),
    ("blocker", 2),
    ("cost", 3),
    ("prefer", 3),
    ("supersedes", 2),
    ("conflict", 2),
    ("record", 6),
    ("task", 2),
];
pub(super) const METADATA: [&str; 10] = [
    "lesson", "scope", "strength", "effect", "evidence", "limit", "blocker", "cost", "prefer",
    "record",
];
/// Replay result classification; missing or stale evidence never enables advice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum EvidenceStatus {
    Validated,
    Pending,
    Stale,
    Failed,
}
/// Explicit source-bound replay input, independent of lesson assertions.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Validation {
    pub lesson: String,
    pub statement: Vec<Fact>,
    pub recipe_sha256: String,
    pub dependency_sha256: String,
    pub validation_sha256: String,
    pub mode: String,
    pub family: String,
    pub last_validation: u64,
    pub status: EvidenceStatus,
}
/// Both sides of an identically keyed layer override, retained for diagnosis.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Override {
    pub key: String,
    pub earlier_layer: Option<usize>,
    pub later_layer: usize,
    pub earlier: Vec<Fact>,
    pub later: Vec<Fact>,
}
/// Only integer ordering is admitted; equality still supports text constants.
pub(super) fn compare(op: &str, a: &Value, b: &Value, w: &Work) -> Result<bool, Stop> {
    if op == "=" {
        return Ok(a == b);
    }
    if op == "!=" {
        return Ok(a != b);
    }
    let (Value::Integer(a), Value::Integer(b)) = (a, b) else {
        return Err(w.stop("kb-comparison-type", false));
    };
    Ok(match op {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        _ => false,
    })
}
/// Owned ground conclusion. Values are data, never executable operations.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(missing_docs)]
pub struct Fact {
    pub predicate: String,
    pub arguments: Vec<Value>,
}
/// A grounded negative lookup; None denotes an existential anonymous argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Pattern {
    pub predicate: String,
    pub arguments: Vec<Option<Value>>,
}
/// One typed comparison checked against the frozen relation snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Comparison {
    pub operator: &'static str,
    pub left: Value,
    pub right: Value,
}
/// One derivation; premises precede their conclusion in an explanation.
/// Rule numbers index the input rules followed by the fixed usage rules.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Derivation {
    pub fact: Fact,
    pub rule: Option<usize>,
    pub premises: Vec<Fact>,
    pub absent: Vec<Pattern>,
    pub comparisons: Vec<Comparison>,
    pub source_layer: Option<usize>,
    pub bindings: Binding,
}
/// Finite acyclic trace for a single answer, with explicit closed-world absences.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Explanation {
    pub conclusion: Fact,
    pub trace: Vec<Derivation>,
}
/// All and only the current, scope-matching lessons and their structured metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Advice {
    pub lessons: Vec<Value>,
    pub facts: Vec<Fact>,
    pub task_facts: Vec<Fact>,
    pub explanations: Vec<Explanation>,
    pub exclusions: Vec<Exclusion>,
    pub unknowns: Vec<Fact>,
    pub pending: Vec<Value>,
    pub defeated: Vec<Explanation>,
    pub problems: Vec<&'static str>,
}
/// A complete reason why a known public step was excluded.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Exclusion {
    pub step: String,
    pub lesson: Value,
    pub reason: &'static str,
    pub enforced: bool,
    pub constraint: Fact,
    pub explanation: Explanation,
    pub constraint_trace: Explanation,
    pub observed: Vec<Explanation>,
}
/// Declared deterministic frontier rank and its supporting facts.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct StepRank {
    pub step: String,
    pub preference_penalty: u64,
    pub declared_work: u64,
    pub explanations: Vec<Explanation>,
}
pub(super) type Proofs = BTreeMap<Fact, Derivation>;

pub(super) fn fact(pred: &str, row: &[Value]) -> Fact {
    Fact {
        predicate: pred.to_string(),
        arguments: row.to_vec(),
    }
}
fn grounded(a: &Atom, b: &Binding, w: &Work) -> Result<Pattern, Stop> {
    let arguments = a
        .args
        .iter()
        .map(|t| match t {
            Term::Var(n) if n == "_" => Ok(None),
            _ => value(t, b, w).map(Some),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Pattern {
        predicate: a.pred.clone(),
        arguments,
    })
}
pub(super) fn record(
    proofs: &mut Proofs,
    db: &Database,
    conclusion: Fact,
    source: Option<(usize, &Rule, &Binding)>,
    w: &mut Work,
) -> Result<(), Stop> {
    let mut d = Derivation {
        fact: conclusion.clone(),
        rule: None,
        premises: Vec::new(),
        absent: Vec::new(),
        comparisons: Vec::new(),
        source_layer: None,
        bindings: Binding::new(),
    };
    if let Some((index, r, b)) = source {
        d.rule = Some(index);
        d.bindings = b.clone();
        for l in &r.body {
            w.charge(1)?;
            match l {
                Literal::Positive(a) => {
                    let mut found = None;
                    if let Some(rows) = db.get(&a.pred) {
                        for row in rows {
                            // New conclusions cannot justify themselves, even when another
                            // older witness for an anonymous literal exists.
                            let f = fact(&a.pred, row);
                            if proofs.contains_key(&f) && matching(a, row, b, w)?.is_some() {
                                found = Some(f);
                                break;
                            }
                        }
                    }
                    d.premises
                        .push(found.ok_or_else(|| w.stop("kb-trace-premise", false))?);
                }
                Literal::Negative(a) => d.absent.push(grounded(a, b, w)?),
                Literal::Compare(op, a, c) => d.comparisons.push(Comparison {
                    operator: op,
                    left: value(a, b, w)?,
                    right: value(c, b, w)?,
                }),
            }
        }
    }
    proofs.insert(conclusion, d);
    Ok(())
}
pub(super) fn explain(f: Fact, proofs: &Proofs, w: &mut Work) -> Result<Explanation, Stop> {
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_PROVENANCE").is_some() {
        return Ok(Explanation {
            conclusion: f,
            trace: Vec::new(),
        });
    }
    let mut stack = vec![(f.clone(), false)];
    let mut seen = BTreeSet::new();
    let mut trace = Vec::new();
    while let Some((next, expanded)) = stack.pop() {
        w.charge(next.arguments.len() + 1)?;
        if seen.contains(&next) {
            continue;
        }
        let d = proofs
            .get(&next)
            .ok_or_else(|| w.stop("kb-trace-missing", false))?;
        if expanded {
            seen.insert(next);
            let bytes = d
                .fact
                .arguments
                .iter()
                .chain(d.premises.iter().flat_map(|f| f.arguments.iter()))
                .map(|v| match v {
                    Value::Text(s) => s.len(),
                    Value::Integer(_) => 8,
                })
                .sum::<usize>();
            w.charge(bytes + d.premises.len() + d.bindings.len() + 1)?;
            w.bound(trace.len() < w.limits.rows, "kb-trace-budget")?;
            trace.push(d.clone());
        } else {
            stack.push((next, true));
            stack.extend(d.premises.iter().rev().map(|p| (p.clone(), false)));
        }
    }
    Ok(Explanation {
        conclusion: f,
        trace,
    })
}
pub(super) fn schema(a: &Atom, w: &Work) -> Result<(), Stop> {
    if a.pred.starts_with("usage_") || a.pred == "task" {
        return Err(w.stop("kb-reserved-predicate", false));
    }
    if VOCABULARY
        .iter()
        .any(|(p, n)| *p == a.pred && *n != a.args.len())
    {
        return Err(w.stop("kb-usage-arity", false));
    }
    Ok(())
}
pub(super) fn task(source: &str, w: &mut Work) -> Result<Vec<Atom>, Stop> {
    let mut parser = Parser {
        tokens: tokenize(source, w)?,
        i: 0,
    };
    let (facts, rules) = parser.program(w)?;
    if !rules.is_empty() || facts.iter().any(|a| a.pred != "task" || a.args.len() != 2) {
        return Err(w.stop("kb-task-facts-only", false));
    }
    if !facts
        .iter()
        .any(|a| a.args[0] == Term::Const(text("subject")))
    {
        return Err(w.stop("kb-task-subject-required", false));
    }
    let mut scalar = BTreeMap::<String, Value>::new();
    for a in &facts {
        let Term::Const(Value::Text(key)) = &a.args[0] else {
            return Err(w.stop("kb-task-key", false));
        };
        let Term::Const(val) = &a.args[1] else {
            return Err(w.stop("kb-task-value", false));
        };
        let named = matches!(
            key.as_str(),
            "subject"
                | "tool"
                | "version"
                | "family"
                | "environment"
                | "capability"
                | "prerequisite"
        );
        let numeric = key.rsplit_once('-').is_some_and(|(metric, unit)| {
            !metric.is_empty() && matches!(unit, "count" | "bytes" | "work")
        });
        if !named && !numeric {
            return Err(w.stop("kb-task-key", false));
        }
        if named && !matches!(val,Value::Text(s) if !s.is_empty() && s!="any")
            || numeric && !matches!(val,Value::Integer(n) if *n>=0) && val != &text("unknown")
        {
            return Err(w.stop("kb-task-value", false));
        }
        if matches!(key.as_str(), "tool" | "version" | "family" | "environment") || numeric {
            if scalar.get(key).is_some_and(|v| v != val) {
                return Err(w.stop("kb-task-conflict", false));
            }
            scalar.insert(key.clone(), val.clone());
        }
    }
    Ok(facts)
}
fn has(db: &Database, pred: &str, args: &[Value]) -> bool {
    db.get(pred).is_some_and(|rows| rows.contains(args))
}
fn named_rows<'a>(
    db: &'a Database,
    pred: &str,
    id: &'a Value,
) -> impl Iterator<Item = &'a Vec<Value>> {
    db.get(pred)
        .into_iter()
        .flatten()
        .filter(move |r| r.first() == Some(id))
}
fn string(v: &Value) -> Option<&str> {
    if let Value::Text(s) = v {
        Some(s)
    } else {
        None
    }
}
pub(super) fn check(db: &Database, w: &mut Work) -> Result<Vec<&'static str>, Stop> {
    let mut problems = BTreeSet::new();
    for (pred, n) in VOCABULARY {
        for row in db.get(pred).into_iter().flatten() {
            w.charge(row.len() + 1)?;
            if row.len() != n {
                problems.insert("kb-usage-arity");
                continue;
            }
            if row.first().and_then(string).is_none() {
                problems.insert("kb-usage-type");
            }
            if METADATA.contains(&pred)
                && pred != "lesson"
                && !named_rows(db, "lesson", &row[0]).any(|_| true)
            {
                problems.insert("kb-unknown-lesson");
            }
            let text_fields: &[usize] = match pred {
                "isa" | "supersedes" | "conflict" | "blocker" => &[1],
                "prefer" => &[1, 2],
                "record" => &[1, 2, 3, 4, 5],
                _ => &[],
            };
            if text_fields
                .iter()
                .any(|&i| string(&row[i]).is_none_or(str::is_empty))
            {
                problems.insert("kb-usage-type");
            }
            match pred {
                "blocker" if matches!(string(&row[1]), Some("unknown" | "any")) => {
                    problems.insert("kb-blocker-environment");
                }
                "limit" | "cost" => {
                    if !matches!(row[2],Value::Integer(n) if n >= 0) || string(&row[1]).is_none() {
                        problems.insert("kb-usage-number");
                    }
                    if pred == "limit"
                        && !matches!(string(&row[3]), Some("count" | "bytes" | "work"))
                    {
                        problems.insert("kb-limit-unit");
                    }
                }
                "evidence" => {
                    if string(&row[1]).is_none_or(str::is_empty)
                        || string(&row[2]).is_none_or(|s| {
                            s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                    {
                        problems.insert("kb-evidence-format");
                    }
                }
                "strength" => {
                    if !matches!(
                        string(&row[1]),
                        Some(
                            "declared"
                                | "observed"
                                | "measured"
                                | "preregistered"
                                | "proposed"
                                | "proved"
                                | "refuted"
                                | "certified"
                        )
                    ) {
                        problems.insert("kb-strength");
                    }
                }
                "effect" => {
                    if !matches!(
                        string(&row[1]),
                        Some("helps" | "hurts" | "neutral" | "defect" | "limit")
                    ) {
                        problems.insert("kb-effect");
                    }
                }
                "scope" => {
                    if !matches!(
                        string(&row[1]),
                        Some(
                            "tool"
                                | "version"
                                | "family"
                                | "environment"
                                | "capability"
                                | "prerequisite"
                        )
                    ) || string(&row[2]).is_none_or(|v| v == "unknown" || v.is_empty())
                    {
                        problems.insert("kb-scope-key");
                    }
                }
                "record"
                    if string(&row[3]).is_none_or(|s| {
                        s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit())
                    }) || !matches!(
                        string(&row[1]),
                        Some("proposed" | "promoted" | "rejected")
                    ) || string(&row[2]) != Some("usage-v1")
                        || !matches!(
                            string(&row[4]),
                            Some(
                                "observation"
                                    | "exact-precondition"
                                    | "reproduced-blocker"
                                    | "proved"
                                    | "refuted"
                                    | "certified"
                            )
                        ) =>
                {
                    problems.insert("kb-record-schema");
                }
                _ => {}
            }
        }
    }
    for row in db.get("strength").into_iter().flatten() {
        if matches!(
            row.get(1).and_then(string),
            Some("proved" | "refuted" | "certified")
        ) && !has(db, "usage_validated", &[row[0].clone()])
        {
            problems.insert("kb-fact-replay-required");
        }
    }
    for row in db.get("lesson").into_iter().flatten() {
        w.charge(1)?;
        if string(&row[1]).is_none() {
            problems.insert("kb-usage-type");
        }
        for (pred, code) in [
            ("evidence", "kb-lesson-without-evidence"),
            ("strength", "kb-lesson-without-strength"),
            ("effect", "kb-lesson-without-effect"),
        ] {
            if !named_rows(db, pred, &row[0]).any(|_| true) {
                problems.insert(code);
            }
        }
        if named_rows(db, "lesson", &row[0]).count() != 1
            || named_rows(db, "strength", &row[0]).count() != 1
            || named_rows(db, "effect", &row[0]).count() != 1
        {
            problems.insert("kb-lesson-ambiguous");
        }
        if has(db, "usage_replaces", &[row[0].clone(), row[0].clone()]) {
            problems.insert("kb-supersedes-cycle");
        }
    }
    for pred in ["supersedes", "conflict"] {
        for row in db.get(pred).into_iter().flatten() {
            w.charge(1)?;
            if row
                .iter()
                .any(|id| !named_rows(db, "lesson", id).any(|_| true))
            {
                problems.insert("kb-unknown-lesson");
            }
        }
    }
    for row in db.get("isa").into_iter().flatten() {
        w.charge(1)?;
        if row[0] == row[1] {
            problems.insert("kb-isa-cycle");
        }
    }
    for row in db.get("lesson").into_iter().flatten() {
        if string(&row[0]).is_none_or(|id| {
            id.is_empty()
                || id.len() > 96
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }) {
            problems.insert("kb-lesson-id");
        }
        for key in ["tool", "version", "family", "environment"] {
            if named_rows(db, "scope", &row[0])
                .filter(|s| s[1] == text(key))
                .count()
                > 1
            {
                problems.insert("kb-scope-ambiguous");
            }
        }
        if named_rows(db, "record", &row[0]).count() != 1 {
            problems.insert("kb-lesson-record");
        }
    }
    for pair in db.get("conflict").into_iter().flatten() {
        w.charge(1)?;
        let (a, b) = (&pair[0], &pair[1]);
        if scopes_overlap(db, a, b, w)?
            && !has(db, "usage_replaces", &[a.clone(), b.clone()])
            && !has(db, "usage_replaces", &[b.clone(), a.clone()])
        {
            problems.insert("kb-unresolved-conflict");
        }
    }
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_CONFLICT").is_some() {
        problems.remove("kb-unresolved-conflict");
    }
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_EVIDENCE").is_some() {
        problems.remove("kb-lesson-without-evidence");
    }
    Ok(problems.into_iter().collect())
}
pub(super) fn advise(db: &Database, proofs: &Proofs, w: &mut Work) -> Result<Advice, Stop> {
    let mut lessons = BTreeSet::new();
    let mut explanations = Vec::new();
    for row in db.get("usage_applies").into_iter().flatten() {
        w.charge(1)?;
        lessons.insert(row[0].clone());
        explanations.push(explain(fact("usage_applies", row), proofs, w)?);
    }
    let mut facts = Vec::new();
    for pred in METADATA {
        for row in db.get(pred).into_iter().flatten() {
            w.charge(1)?;
            if lessons.contains(&row[0]) {
                facts.push(fact(pred, row));
            }
        }
    }
    facts.sort();
    let subjects: BTreeSet<_> = db
        .get("task")
        .into_iter()
        .flatten()
        .filter(|r| r[0] == text("subject"))
        .map(|r| r[1].clone())
        .collect();
    let relevant: BTreeSet<_> = db
        .get("usage_topic")
        .into_iter()
        .flatten()
        .filter(|r| subjects.contains(&r[1]))
        .map(|r| r[0].clone())
        .collect();
    let mut unknowns: Vec<_> = db
        .get("usage_missing")
        .into_iter()
        .flatten()
        .filter(|r| relevant.contains(&r[0]))
        .map(|r| fact("usage_missing", r))
        .collect();
    unknowns.extend(
        db.get("usage_unknown_environment")
            .into_iter()
            .flatten()
            .filter(|r| relevant.contains(&r[0]))
            .map(|r| fact("usage_unknown_environment", r)),
    );
    let pending = db
        .get("lesson")
        .into_iter()
        .flatten()
        .filter(|r| relevant.contains(&r[0]) && !has(db, "usage_ready", &[r[0].clone()]))
        .map(|r| r[0].clone())
        .collect();
    let defeated = db
        .get("usage_defeated")
        .into_iter()
        .flatten()
        .map(|r| explain(fact("usage_defeated", r), proofs, w))
        .collect::<Result<Vec<_>, _>>()?;
    let problems = check(db, w)?;
    if !problems.is_empty() {
        lessons.clear();
        facts.clear();
        explanations.clear();
    }
    for f in &unknowns {
        explanations.push(explain(f.clone(), proofs, w)?);
    }
    let excluded = if problems.is_empty() {
        exclusions(db, proofs, w)?
    } else {
        Vec::new()
    };
    let mut task_facts = Vec::new();
    for row in db.get("task").into_iter().flatten() {
        w.charge(
            row.iter()
                .map(|v| match v {
                    Value::Text(s) => s.len(),
                    Value::Integer(_) => 8,
                })
                .sum::<usize>()
                + 1,
        )?;
        task_facts.push(fact("task", row));
    }
    Ok(Advice {
        task_facts,
        lessons: lessons.into_iter().collect(),
        facts,
        explanations,
        exclusions: excluded,
        unknowns,
        pending,
        defeated,
        problems,
    })
}
pub(super) fn exclusions(
    db: &Database,
    proofs: &Proofs,
    w: &mut Work,
) -> Result<Vec<Exclusion>, Stop> {
    let mut out = Vec::new();
    for row in db.get("usage_applies").into_iter().flatten() {
        let actionable = named_rows(db, "record", &row[0]).any(|r| {
            matches!(
                string(&r[4]),
                Some("exact-precondition" | "reproduced-blocker")
            )
        });
        if !actionable {
            continue;
        }
        for pred in ["blocker", "limit"] {
            for constraint in named_rows(db, pred, &row[0]) {
                w.charge(1)?;
                let reason = if pred == "blocker" {
                    if !has(db, "task", &[text("environment"), constraint[1].clone()]) {
                        continue;
                    }
                    "environment-blocked"
                } else {
                    let unit = string(&constraint[3]).unwrap_or("");
                    let size_key = text(&alloc::format!(
                        "{}-{}",
                        string(&constraint[1]).unwrap_or("size"),
                        unit
                    ));
                    let sizes: Vec<_> = named_rows(db, "task", &size_key).map(|r| &r[1]).collect();
                    if sizes.is_empty() || sizes.iter().any(|v|matches!(v,Value::Text(s) if s=="unknown")) { "size-unknown" }
                    else if sizes.iter().any(|n| !matches!((n,&constraint[2]),(Value::Integer(n),Value::Integer(max)) if *n >= 0 && n <= max)) { "size-limit" }
                    else { continue; }
                };
                let Some(step) = string(&row[1]) else {
                    continue;
                };
                out.push(Exclusion {
                    step: step.to_string(),
                    lesson: row[0].clone(),
                    reason,
                    enforced: reason != "size-unknown",
                    constraint: fact(pred, constraint),
                    constraint_trace: explain(fact(pred, constraint), proofs, w)?,
                    observed: db
                        .get("task")
                        .into_iter()
                        .flatten()
                        .filter(|r| {
                            if pred == "blocker" {
                                r[0] == text("environment")
                            } else {
                                r[0] == text(&alloc::format!(
                                    "{}-{}",
                                    string(&constraint[1]).unwrap_or("size"),
                                    string(&constraint[3]).unwrap_or("")
                                ))
                            }
                        })
                        .map(|r| explain(fact("task", r), proofs, w))
                        .collect::<Result<Vec<_>, _>>()?,
                    explanation: explain(fact("usage_applies", row), proofs, w)?,
                });
            }
        }
    }
    out.sort_by(|a, b| (&a.step, &a.lesson, a.reason).cmp(&(&b.step, &b.lesson, b.reason)));
    Ok(out)
}
pub(super) fn order(
    db: &Database,
    step: &Step,
    w: &mut Work,
) -> Result<(u64, u64, &'static str), Stop> {
    let (mut penalty, mut cost) = (0u64, 0u64);
    let mut preferred = BTreeSet::new();
    for row in db.get("usage_applies").into_iter().flatten() {
        for c in named_rows(db, "cost", &row[0]) {
            w.charge(1)?;
            if string(&row[1]) == Some(step.name())
                && string(&c[1]) == Some("work")
                && let Value::Integer(n) = c[2]
            {
                cost = cost.saturating_add(n as u64);
            }
        }
        if preferred.insert(row[0].clone())
            && (has(db, "effect", &[row[0].clone(), text("helps")])
                || has(db, "effect", &[row[0].clone(), text("neutral")]))
        {
            for p in named_rows(db, "prefer", &row[0]) {
                w.charge(1)?;
                if string(&p[2]) == Some(step.name()) {
                    penalty = penalty.saturating_add(1);
                }
            }
        }
    }
    Ok((penalty, cost, step.name()))
}

fn scopes_overlap(db: &Database, a: &Value, b: &Value, w: &mut Work) -> Result<bool, Stop> {
    for left in named_rows(db, "scope", a) {
        for right in named_rows(db, "scope", b) {
            w.charge(1)?;
            if left[1] != right[1]
                || left[2] == right[2]
                || left[2] == text("any")
                || right[2] == text("any")
            {
                continue;
            }
            if matches!(string(&left[1]), Some("version" | "environment")) {
                return Ok(false);
            }
            if matches!(string(&left[1]), Some("tool" | "family")) {
                let inherited = has(db, "isa", &[left[2].clone(), right[2].clone()])
                    || has(db, "isa", &[right[2].clone(), left[2].clone()]);
                let mut common = false;
                for r in db.get("isa").into_iter().flatten() {
                    w.charge(1)?;
                    if r[1] == left[2] && has(db, "isa", &[r[0].clone(), right[2].clone()]) {
                        common = true;
                        break;
                    }
                }
                if !inherited && !common {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
pub(super) fn ranking(
    db: &Database,
    proofs: &Proofs,
    step: &Step,
    w: &mut Work,
) -> Result<StepRank, Stop> {
    let (preference_penalty, declared_work, _) = order(db, step, w)?;
    let ids: BTreeSet<_> = db
        .get("usage_applies")
        .into_iter()
        .flatten()
        .map(|r| r[0].clone())
        .collect();
    let mut explanations = Vec::new();
    for row in db.get("usage_applies").into_iter().flatten() {
        if ids.contains(&row[0]) {
            explanations.push(explain(fact("usage_applies", row), proofs, w)?);
        }
    }
    for pred in ["cost", "prefer"] {
        for row in db.get(pred).into_iter().flatten() {
            w.charge(1)?;
            if ids.contains(&row[0]) {
                explanations.push(explain(fact(pred, row), proofs, w)?);
            }
        }
    }
    Ok(StepRank {
        step: step.name().into(),
        preference_penalty,
        declared_work,
        explanations,
    })
}
