//! Bounded stratified Datalog and structural strategy proposals. LLM-generated.
//! Text, policy and request are immutable. No clock, file or executable primitive
//! is available here. A code-consistency result is not a semantic program judge.
use crate::strategy_algebra::{self as A, Artifact, Node, Plan, Step, Stop};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
mod reasoning;
pub use reasoning::{
    Advice, Comparison, Derivation, EvidenceStatus, Exclusion, Explanation, Fact, Override,
    Pattern, StepRank, USAGE_SCHEMA, Validation,
};

/// Neutral facts derived only from the closed public type/plan registry.
pub const EXAMPLE: &str = include_str!("../examples/strategy-example.kb");
/// Version of the public reserved structural schema.
pub const SCHEMA: &str = "zeno-fcis/strategy-knowledge/1";
const MAX_TEXT: usize = 65_536;
const MAX_TOKENS: usize = 8192;
const MAX_PREDICATES: usize = 128;
const MAX_RULES: usize = 128;
const MAX_ARITY: usize = 8;
const MAX_BODY: usize = 16;
const TYPE_WORK: u64 = 10_000;

/// Bound policy for one invocation, shared across parsing, inference and planning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Limits {
    pub work: u64,
    pub rows: usize,
    pub bindings: usize,
    pub states: usize,
    pub steps: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            work: 2_000_000,
            rows: 8192,
            bindings: 4096,
            states: 4096,
            steps: 8,
        }
    }
}
/// Requested operation. Named type strings refer to the closed artifact catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Request {
    Query(String),
    Check,
    /// Apply structured task facts to the current usage lessons.
    Advise(String),
    /// Query with a derivation and an explicit task context.
    Explain {
        query: String,
        task: String,
    },
    /// Constrained public-catalog planning in a declared task context.
    PlanFor {
        have: Vec<String>,
        want: Vec<String>,
        task: String,
    },
    /// After one real step, replan only from the artifacts actually observed.
    Replan {
        have: Vec<String>,
        want: Vec<String>,
        task: String,
        produced: Vec<String>,
    },
    Plan {
        have: Vec<String>,
        want: Vec<String>,
    },
}
/// Datalog constants: quoted strings and lowercase names share text semantics.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[allow(missing_docs)]
pub enum Value {
    Text(String),
    Integer(i128),
}
/// Query variable bindings. Anonymous underscores are omitted.
pub type Binding = BTreeMap<String, Value>;
/// A structural proposal; none of its artifacts are runtime checked values.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Planning {
    pub steps: Vec<Step>,
    pub composition: Plan,
    pub eligible: Vec<String>,
    pub selected: Option<String>,
    pub strategy_plan: Option<Plan>,
    pub exclusions: Vec<Exclusion>,
    pub ranking: Vec<StepRank>,
}
/// Complete bounded result; no partial result is returned on any stop.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Outcome {
    Query(Vec<Binding>),
    Checked(Vec<&'static str>),
    Planned(Box<Planning>),
    Advised(Advice),
    /// Complete exclusions with no partial candidate or impossibility claim.
    NoPlan(Vec<Exclusion>),
}
/// Owned result plus reserved/charged work and finite relation/state counts.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub struct Report {
    pub outcome: Outcome,
    pub work: u64,
    pub rows: usize,
    pub states: usize,
    pub explanations: Vec<Explanation>,
    pub layer_conflicts: usize,
    pub overrides: Vec<Override>,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Term {
    Var(String),
    Const(Value),
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Atom {
    pred: String,
    args: Vec<Term>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Literal {
    Positive(Atom),
    Negative(Atom),
    Compare(&'static str, Term, Term),
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Rule {
    head: Atom,
    body: Vec<Literal>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Name(String),
    Var(String),
    Const(Value),
    Punct(&'static str),
}
type Rows = BTreeSet<Vec<Value>>;
type Database = BTreeMap<String, Rows>;

// Exclusive invocation scratch. Stops retain usage, never the mutable database.
struct Work {
    used: u64,
    limits: Limits,
    rows: usize,
    states: usize,
}
impl Work {
    fn stop(&self, code: &'static str, inconclusive: bool) -> Stop {
        Stop {
            code,
            inconclusive,
            work: self.used,
            trace: Vec::new(),
        }
    }
    fn charge(&mut self, amount: usize) -> Result<(), Stop> {
        self.used = self.used.saturating_add(amount as u64);
        if self.used > self.limits.work {
            return Err(self.stop("kb-work-budget", true));
        }
        Ok(())
    }
    fn bound(&self, ok: bool, code: &'static str) -> Result<(), Stop> {
        if ok {
            Ok(())
        } else {
            Err(self.stop(code, true))
        }
    }
}
fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}
fn hex4(bytes: &[u8], i: &mut usize, w: &Work) -> Result<u16, Stop> {
    let mut n = 0u16;
    for _ in 0..4 {
        let b = *bytes.get(*i).ok_or_else(|| w.stop("kb-string", false))?;
        *i += 1;
        n = n
            .checked_mul(16)
            .and_then(|n| (b as char).to_digit(16).map(|d| n + d as u16))
            .ok_or_else(|| w.stop("kb-string", false))?;
    }
    Ok(n)
}
fn string(source: &str, i: &mut usize, w: &Work) -> Result<String, Stop> {
    let bytes = source.as_bytes();
    *i += 1;
    let mut out = String::new();
    while let Some(&b) = bytes.get(*i) {
        *i += 1;
        match b {
            b'"' => return Ok(out),
            0..=31 => return Err(w.stop("kb-string", false)),
            b'\\' => {
                let e = *bytes.get(*i).ok_or_else(|| w.stop("kb-string", false))?;
                *i += 1;
                out.push(match e {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{8}',
                    b'f' => '\u{c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'u' => {
                        let a = hex4(bytes, i, w)?;
                        let cp = if (0xd800..=0xdbff).contains(&a) {
                            if bytes.get(*i..*i + 2) != Some(b"\\u") {
                                return Err(w.stop("kb-string", false));
                            }
                            *i += 2;
                            let b = hex4(bytes, i, w)?;
                            if !(0xdc00..=0xdfff).contains(&b) {
                                return Err(w.stop("kb-string", false));
                            }
                            0x10000 + ((u32::from(a) - 0xd800) << 10) + (u32::from(b) - 0xdc00)
                        } else {
                            u32::from(a)
                        };
                        char::from_u32(cp).ok_or_else(|| w.stop("kb-string", false))?
                    }
                    _ => return Err(w.stop("kb-string", false)),
                });
            }
            _ => {
                *i -= 1;
                let ch = source[*i..]
                    .chars()
                    .next()
                    .ok_or_else(|| w.stop("kb-string", false))?;
                *i += ch.len_utf8();
                out.push(ch);
            }
        }
        w.bound(out.len() <= 256, "kb-term-budget")?;
    }
    Err(w.stop("kb-string", false))
}
fn tokenize(source: &str, w: &mut Work) -> Result<Vec<Token>, Stop> {
    w.bound(source.len() <= MAX_TEXT, "kb-text-budget")?;
    w.charge(source.len())?;
    let bytes = source.as_bytes();
    let (mut i, mut out) = (0, Vec::new());
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i] == b'%' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        w.bound(out.len() < MAX_TOKENS, "kb-token-budget")?;
        let b = bytes[i];
        let token = if b == b'"' {
            Token::Const(Value::Text(string(source, &mut i, w)?))
        } else if b.is_ascii_digit()
            || (b == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
        {
            let begin = i;
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            Token::Const(Value::Integer(
                source[begin..i]
                    .parse()
                    .map_err(|_| w.stop("kb-integer", false))?,
            ))
        } else if b.is_ascii_uppercase() || b == b'_' || b.is_ascii_lowercase() {
            let begin = i;
            i += 1;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit()
                    || bytes[i] == b'_'
                    || if b.is_ascii_lowercase() {
                        bytes[i].is_ascii_lowercase() || bytes[i] == b'-'
                    } else {
                        bytes[i].is_ascii_alphabetic()
                    })
            {
                i += 1;
            }
            let name = &source[begin..i];
            w.bound(name.len() <= 256, "kb-term-budget")?;
            if b.is_ascii_lowercase() {
                Token::Name(name.to_string())
            } else {
                Token::Var(name.to_string())
            }
        } else {
            let punctuation = if bytes.get(i..i + 2) == Some(b":-") {
                ":-"
            } else if bytes.get(i..i + 2) == Some(b"!=") {
                "!="
            } else if bytes.get(i..i + 2) == Some(b"<=") {
                "<="
            } else if bytes.get(i..i + 2) == Some(b">=") {
                ">="
            } else {
                match b {
                    b'(' => "(",
                    b')' => ")",
                    b',' => ",",
                    b'.' => ".",
                    b'=' => "=",
                    b'<' => "<",
                    b'>' => ">",
                    _ => return Err(w.stop("kb-syntax", false)),
                }
            };
            i += punctuation.len();
            Token::Punct(punctuation)
        };
        out.push(token);
    }
    Ok(out)
}
struct Parser {
    tokens: Vec<Token>,
    i: usize,
}
impl Parser {
    fn punct(&mut self, p: &str) -> bool {
        if matches!(self.tokens.get(self.i), Some(Token::Punct(q)) if *q == p) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, p: &str, w: &Work) -> Result<(), Stop> {
        if self.punct(p) {
            Ok(())
        } else {
            Err(w.stop("kb-syntax", false))
        }
    }
    fn term(&mut self, w: &Work) -> Result<Term, Stop> {
        let term = match self.tokens.get(self.i) {
            Some(Token::Name(s)) => Term::Const(text(s)),
            Some(Token::Var(s)) => Term::Var(s.clone()),
            Some(Token::Const(c)) => Term::Const(c.clone()),
            _ => return Err(w.stop("kb-syntax", false)),
        };
        self.i += 1;
        Ok(term)
    }
    fn atom(&mut self, w: &Work) -> Result<Atom, Stop> {
        let Some(Token::Name(pred)) = self.tokens.get(self.i) else {
            return Err(w.stop("kb-syntax", false));
        };
        let pred = pred.clone();
        self.i += 1;
        let mut args = Vec::new();
        if self.punct("(") && !self.punct(")") {
            loop {
                w.bound(args.len() < MAX_ARITY, "kb-arity-budget")?;
                args.push(self.term(w)?);
                if self.punct(")") {
                    break;
                }
                self.require(",", w)?;
            }
        }
        Ok(Atom { pred, args })
    }
    fn literal(&mut self, w: &Work) -> Result<Literal, Stop> {
        if matches!(self.tokens.get(self.i), Some(Token::Name(n)) if n == "not") {
            self.i += 1;
            return self.atom(w).map(Literal::Negative);
        }
        if let Some(Token::Punct(op @ ("=" | "!=" | "<" | "<=" | ">" | ">="))) =
            self.tokens.get(self.i + 1)
        {
            let op = *op;
            let a = self.term(w)?;
            self.require(op, w)?;
            return Ok(Literal::Compare(op, a, self.term(w)?));
        }
        self.atom(w).map(Literal::Positive)
    }
    fn program(&mut self, w: &mut Work) -> Result<(Vec<Atom>, Vec<Rule>), Stop> {
        let (mut facts, mut rules) = (Vec::new(), Vec::new());
        while self.i < self.tokens.len() {
            w.charge(1)?;
            let head = self.atom(w)?;
            if self.punct(":-") {
                w.bound(rules.len() < MAX_RULES, "kb-rule-budget")?;
                let mut body = Vec::new();
                loop {
                    w.bound(body.len() < MAX_BODY, "kb-body-budget")?;
                    body.push(self.literal(w)?);
                    if !self.punct(",") {
                        break;
                    }
                }
                rules.push(Rule { head, body });
            } else {
                if head.args.iter().any(|a| matches!(a, Term::Var(_))) {
                    return Err(w.stop("kb-fact-variable", false));
                }
                w.bound(facts.len() < 4096, "kb-fact-budget")?;
                facts.push(head);
            }
            self.require(".", w)?;
        }
        Ok((facts, rules))
    }
}
fn vars(args: &[Term]) -> BTreeSet<String> {
    args.iter()
        .filter_map(|t| match t {
            Term::Var(n) if n != "_" => Some(n.clone()),
            _ => None,
        })
        .collect()
}
fn safe(rule: &Rule, w: &mut Work) -> Result<(), Stop> {
    let mut positive = BTreeSet::new();
    let mut needed = vars(&rule.head.args);
    if rule
        .head
        .args
        .iter()
        .any(|a| matches!(a, Term::Var(n) if n == "_"))
    {
        return Err(w.stop("kb-head-wildcard", false));
    }
    for l in &rule.body {
        w.charge(MAX_ARITY)?;
        match l {
            Literal::Positive(a) => positive.extend(vars(&a.args)),
            Literal::Negative(a) => needed.extend(vars(&a.args)),
            Literal::Compare(_, a, b) => {
                if [a, b].iter().any(|t| matches!(t, Term::Var(n) if n == "_")) {
                    return Err(w.stop("kb-comparison-wildcard", false));
                }
                needed.extend(vars(&[a.clone(), b.clone()]));
            }
        }
    }
    w.bound(positive.len() <= 32, "kb-variable-budget")?;
    if !needed.is_subset(&positive) {
        return Err(w.stop("kb-unsafe-rule", false));
    }
    Ok(())
}
fn arity(a: &Atom, arities: &mut BTreeMap<String, usize>, w: &Work) -> Result<(), Stop> {
    if arities.get(&a.pred).is_some_and(|n| *n != a.args.len()) {
        return Err(w.stop("kb-arity-mismatch", false));
    }
    arities.insert(a.pred.clone(), a.args.len());
    w.bound(arities.len() <= MAX_PREDICATES, "kb-predicate-budget")
}
fn strata(rules: &[Rule], w: &mut Work) -> Result<BTreeMap<String, usize>, Stop> {
    let mut levels: BTreeMap<_, _> = rules
        .iter()
        .map(|r| (r.head.pred.clone(), 0usize))
        .collect();
    for _ in 0..=levels.len() {
        let mut changed = false;
        for r in rules {
            for lit in &r.body {
                w.charge(1)?;
                let (a, neg) = match lit {
                    Literal::Positive(a) => (a, 0),
                    Literal::Negative(a) => (a, 1),
                    _ => continue,
                };
                if let Some(dep) = levels.get(&a.pred).copied() {
                    let goal = dep + neg;
                    if let Some(current) = levels.get_mut(&r.head.pred)
                        && *current < goal
                    {
                        *current = goal;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            return Ok(levels);
        }
    }
    Err(w.stop("kb-negation-through-recursion", false))
}
fn value(t: &Term, b: &Binding, w: &Work) -> Result<Value, Stop> {
    match t {
        Term::Const(v) => Ok(v.clone()),
        Term::Var(n) => b
            .get(n)
            .cloned()
            .ok_or_else(|| w.stop("kb-unbound-variable", false)),
    }
}
fn matching(
    a: &Atom,
    row: &[Value],
    bound: &Binding,
    w: &mut Work,
) -> Result<Option<Binding>, Stop> {
    w.charge(a.args.len() + bound.len() + 1)?;
    if a.args.len() != row.len() {
        return Ok(None);
    }
    let mut out = bound.clone();
    for (arg, val) in a.args.iter().zip(row) {
        match arg {
            Term::Var(n) if n == "_" => {}
            Term::Var(n) => {
                if out.get(n).is_some_and(|v| v != val) {
                    return Ok(None);
                }
                out.insert(n.clone(), val.clone());
            }
            Term::Const(c) if c != val => return Ok(None),
            _ => {}
        }
    }
    Ok(Some(out))
}
fn solve(body: &[Literal], db: &Database, w: &mut Work) -> Result<Vec<Binding>, Stop> {
    let mut bindings = vec![Binding::new()];
    w.bound(w.limits.bindings > 0, "kb-binding-budget")?;
    for lit in body {
        let Literal::Positive(a) = lit else { continue };
        let mut next = Vec::new();
        if let Some(rows) = db.get(&a.pred) {
            for binding in &bindings {
                for row in rows {
                    if let Some(b) = matching(a, row, binding, w)? {
                        w.bound(next.len() < w.limits.bindings, "kb-binding-budget")?;
                        next.push(b);
                    }
                }
            }
        }
        bindings = next;
    }
    let mut out = Vec::new();
    for b in bindings {
        let mut keep = true;
        for lit in body {
            w.charge(1)?;
            match lit {
                Literal::Compare(eq, a, c) => {
                    if !reasoning::compare(eq, &value(a, &b, w)?, &value(c, &b, w)?, w)? {
                        keep = false;
                        break;
                    }
                }
                Literal::Negative(a) => {
                    if let Some(rows) = db.get(&a.pred) {
                        for row in rows {
                            if matching(a, row, &b, w)?.is_some() {
                                keep = false;
                                break;
                            }
                        }
                    }
                    if !keep {
                        break;
                    }
                }
                _ => {}
            }
        }
        if keep {
            out.push(b);
        }
    }
    Ok(out)
}
fn add(db: &mut Database, pred: &str, row: Vec<Value>, w: &mut Work) -> Result<bool, Stop> {
    w.charge(row.len() + 1)?;
    let rows = db.entry(pred.to_string()).or_default();
    if rows.contains(&row) {
        return Ok(false);
    }
    w.bound(w.rows < w.limits.rows, "kb-row-budget")?;
    rows.insert(row);
    w.rows += 1;
    Ok(true)
}
fn evaluate(
    facts: &[Atom],
    rules: &[Rule],
    proofs: &mut reasoning::Proofs,
    w: &mut Work,
) -> Result<Database, Stop> {
    let levels = strata(rules, w)?;
    let mut db = Database::new();
    for a in facts {
        let row = a
            .args
            .iter()
            .map(|t| value(t, &Binding::new(), w))
            .collect::<Result<Vec<_>, _>>()?;
        if add(&mut db, &a.pred, row.clone(), w)? {
            reasoning::record(proofs, &db, reasoning::fact(&a.pred, &row), None, w)?;
        }
    }
    let strata: BTreeSet<usize> = levels.values().copied().collect();
    for level in strata {
        loop {
            let mut changed = false;
            for (index, r) in rules.iter().enumerate() {
                w.charge(1)?;
                if levels.get(&r.head.pred) != Some(&level) {
                    continue;
                }
                for b in solve(&r.body, &db, w)? {
                    let row = r
                        .head
                        .args
                        .iter()
                        .map(|t| value(t, &b, w))
                        .collect::<Result<Vec<_>, _>>()?;
                    if add(&mut db, &r.head.pred, row.clone(), w)? {
                        reasoning::record(
                            proofs,
                            &db,
                            reasoning::fact(&r.head.pred, &row),
                            Some((index, r, &b)),
                            w,
                        )?;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
    Ok(db)
}
fn steps(node: &Node, out: &mut BTreeSet<&'static str>) {
    match node {
        Node::Step(s) => {
            out.insert(s.name());
        }
        Node::Seq(ns) => {
            for n in ns {
                steps(n, out);
            }
        }
        Node::Branch { cases, .. } => {
            for n in cases {
                steps(n, out);
            }
        }
        Node::Repeat { body, .. } => steps(body, out),
    }
}
fn same_rows(a: &Rows, b: &Rows) -> bool {
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_CODE").is_some() {
        return true;
    }
    a == b
}
fn typecheck(plan: &Plan, w: &mut Work) -> Result<(), Stop> {
    // Reserve the full existing structural check budget; it is not an estimate.
    w.charge(TYPE_WORK as usize)?;
    match A::check(plan, TYPE_WORK) {
        Ok(r) if r.problems.is_empty() => Ok(()),
        Ok(_) => Err(w.stop("kb-plan-type-error", false)),
        Err(s) => Err(w.stop(s.code, s.inconclusive)),
    }
}
fn consistency(db: &Database, w: &mut Work) -> Result<Vec<&'static str>, Stop> {
    let mut expected = Database::new();
    let mut put = |pred: &str, row: Vec<Value>| {
        expected.entry(pred.to_string()).or_default().insert(row);
    };
    for a in Artifact::ALL {
        put("artifact", vec![text(a.name())]);
    }
    for s in Step::ALL {
        put("loop", vec![text(s.name())]);
        let (consumes, produces) = s.types();
        for a in consumes {
            put("consumes", vec![text(s.name()), text(a.name())]);
        }
        for a in produces {
            put("produces", vec![text(s.name()), text(a.name())]);
        }
    }
    for name in A::NAMED_STRATEGIES {
        let p = A::named(name, 64).ok_or_else(|| w.stop("kb-code-catalog", false))?;
        typecheck(&p, w)?;
        put("strategy", vec![text(name)]);
        for a in p.needs() {
            put("needs", vec![text(name), text(a.name())]);
        }
        for a in p.goal() {
            put("goal", vec![text(name), text(a.name())]);
        }
        let mut used = BTreeSet::new();
        steps(p.node(), &mut used);
        for s in used {
            put("uses", vec![text(name), text(s)]);
        }
    }
    let mut problems = Vec::new();
    for (pred, target) in expected {
        let empty = Rows::new();
        let actual = db.get(&pred).unwrap_or(&empty);
        w.charge(actual.len() * (MAX_ARITY + 1) + target.len() * (MAX_ARITY + 1) + 1)?;
        if pred == "artifact" {
            if actual
                .iter()
                .any(|r| r.len() != 2 || !r.iter().all(|v| matches!(v, Value::Text(_))))
            {
                problems.push("kb-artifact-schema");
            } else {
                let projected: Rows = actual.iter().map(|r| vec![r[0].clone()]).collect();
                if !same_rows(&projected, &target) {
                    problems.push("kb-artifact-code-mismatch");
                }
            }
        } else if !same_rows(actual, &target) {
            problems.push(match pred.as_str() {
                "loop" => "kb-loop-code-mismatch",
                "consumes" => "kb-consumes-code-mismatch",
                "produces" => "kb-produces-code-mismatch",
                "strategy" => "kb-strategy-code-mismatch",
                "uses" => "kb-uses-code-mismatch",
                "needs" => "kb-needs-code-mismatch",
                _ => "kb-goal-code-mismatch",
            });
        }
    }
    Ok(problems)
}
fn artifacts(names: &[String], w: &Work) -> Result<Vec<Artifact>, Stop> {
    w.bound(names.len() <= Artifact::ALL.len(), "kb-type-budget")?;
    names
        .iter()
        .map(|name| {
            Artifact::ALL
                .into_iter()
                .find(|a| a.name() == name)
                .ok_or_else(|| w.stop("kb-unknown-type", false))
        })
        .collect()
}
fn mask(artifacts: &[Artifact]) -> u64 {
    artifacts
        .iter()
        .fold(0, |bits, a| bits | (1 << (*a as u32)))
}
fn plan(
    have: &[String],
    want: &[String],
    db: &Database,
    proofs: &reasoning::Proofs,
    exclusions: Vec<Exclusion>,
    w: &mut Work,
) -> Result<Planning, Stop> {
    let have = artifacts(have, w)?;
    let want = artifacts(want, w)?;
    if want.is_empty() {
        return Err(w.stop("kb-want-required", false));
    }
    let start = mask(&have);
    let goal = mask(&want);
    let mut eligible = Vec::new();
    for name in A::NAMED_STRATEGIES {
        w.charge(1)?;
        let p = A::named(name, 64).ok_or_else(|| w.stop("kb-code-catalog", false))?;
        let mut used = BTreeSet::new();
        steps(p.node(), &mut used);
        if !exclusions
            .iter()
            .any(|e| e.enforced && used.contains(e.step.as_str()))
            && start & mask(p.needs()) == mask(p.needs())
            && mask(p.goal()) & goal == goal
        {
            eligible.push(name.to_string());
        }
    }
    let selected = eligible.first().cloned();
    let strategy_plan = selected.as_deref().and_then(|n| A::named(n, 64));
    let mut sorted = Step::ALL.to_vec();
    let mut ranked = Vec::new();
    for s in sorted {
        ranked.push((reasoning::order(db, &s, w)?, s));
    }
    ranked.sort_by_key(|(rank, _)| *rank);
    sorted = ranked.into_iter().map(|(_, s)| s).collect();
    let ranking = sorted
        .iter()
        .map(|s| reasoning::ranking(db, proofs, s, w))
        .collect::<Result<Vec<_>, _>>()?;
    let mut frontier = vec![(start, Vec::<Step>::new())];
    let mut seen = BTreeSet::from([start]);
    w.bound(w.limits.states > 0, "kb-state-budget")?;
    w.states = 1;
    for depth in 0..=w.limits.steps {
        let mut next = Vec::new();
        for (state, path) in frontier {
            w.charge(1)?;
            if state & goal == goal {
                let composition = Plan::new(
                    have,
                    want,
                    Node::Seq(path.iter().map(|s| Node::Step(*s)).collect()),
                );
                typecheck(&composition, w)?;
                return Ok(Planning {
                    steps: path,
                    composition,
                    eligible,
                    selected,
                    strategy_plan,
                    exclusions,
                    ranking,
                });
            }
            if depth == w.limits.steps {
                continue;
            }
            for s in &sorted {
                w.charge(path.len() + 1)?;
                if exclusions.iter().any(|e| e.enforced && e.step == s.name()) {
                    continue;
                }
                let (needs, makes) = s.types();
                let need = mask(needs);
                let make = mask(makes);
                if state & need != need || state & make == make {
                    continue;
                }
                let s2 = state | make;
                if seen.contains(&s2) {
                    continue;
                }
                w.bound(seen.len() < w.limits.states, "kb-state-budget")?;
                seen.insert(s2);
                w.states += 1;
                let mut p2 = path.clone();
                p2.push(*s);
                next.push((s2, p2));
            }
        }
        frontier = next;
        if frontier.is_empty() {
            break;
        }
    }
    Err(w.stop("kb-bounded-no-plan", true))
}
fn statement(facts: &[Atom], id: &str) -> Vec<Fact> {
    let mut out = facts
        .iter()
        .filter(|a| {
            reasoning::METADATA.contains(&a.pred.as_str())
                && !matches!(a.pred.as_str(), "record" | "evidence")
                && a.args.first() == Some(&Term::Const(text(id)))
        })
        .map(|a| {
            reasoning::fact(
                &a.pred,
                &a.args
                    .iter()
                    .filter_map(|t| {
                        if let Term::Const(v) = t {
                            Some(v.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}
/// Bounded record preparation. Its output is data and measured parse work; it
/// supplies neither replay validation nor a program/publication capability.
pub fn lesson_statement(source: &str, id: &str, limits: Limits) -> Result<(Vec<Fact>, u64), Stop> {
    let mut w = Work {
        used: 0,
        limits,
        rows: 0,
        states: 0,
    };
    if limits.work > 2_000_000 {
        return Err(w.stop("kb-limits-out-of-bounds", false));
    }
    let mut parser = Parser {
        tokens: tokenize(source, &mut w)?,
        i: 0,
    };
    let (facts, _) = parser.program(&mut w)?;
    Ok((statement(&facts, id), w.used))
}
/// Runs one bounded operation, returning independent owned rows/plans.
///
/// Bounds always return an inconclusive typed stop without partial results.
/// A successful code check only compares the reserved schema with the public
/// registry. Program and owner authority remain with the existing runtime gates.
pub fn run(source: &str, request: &Request, limits: Limits) -> Result<Report, Stop> {
    run_layered(&[source], request, limits)
}
/// Ordered explicit layers. Later definitions replace the metadata of an
/// identically named lesson; changed definitions are reported as layer conflicts.
/// No files, clocks, caches, or preparation handles enter this pure invocation.
pub fn run_layered(sources: &[&str], request: &Request, limits: Limits) -> Result<Report, Stop> {
    run_with_evidence(sources, request, limits, &[])
}
/// Validation results are explicit, owned input from the approved shell adapter;
/// they grant advice eligibility only and cannot qualify a program.
pub fn run_with_evidence(
    sources: &[&str],
    request: &Request,
    limits: Limits,
    validations: &[Validation],
) -> Result<Report, Stop> {
    let mut w = Work {
        used: 0,
        limits,
        rows: 0,
        states: 0,
    };
    if limits.work > 2_000_000
        || limits.rows > 8192
        || limits.bindings > 4096
        || limits.states > 4096
        || limits.steps > 16
    {
        return Err(w.stop("kb-limits-out-of-bounds", false));
    }
    w.bound(sources.len() <= 8, "kb-layer-budget")?;
    w.bound(
        sources.iter().map(|s| s.len()).sum::<usize>() <= MAX_TEXT,
        "kb-text-budget",
    )?;
    let (mut facts, mut rules) = (Vec::<Atom>::new(), Vec::<Rule>::new());
    let mut layer_conflicts = 0;
    let mut overrides = Vec::new();
    let mut origins = BTreeMap::<Fact, usize>::new();
    let mut rule_origins = Vec::new();
    for (layer, source) in sources.iter().enumerate() {
        let mut parser = Parser {
            tokens: tokenize(source, &mut w)?,
            i: 0,
        };
        let (incoming, more_rules) = parser.program(&mut w)?;
        for a in &incoming {
            reasoning::schema(a, &w)?;
        }
        for r in &more_rules {
            reasoning::schema(&r.head, &w)?;
            if reasoning::METADATA.contains(&r.head.pred.as_str())
                || r.head.pred == "isa"
                || r.head.pred == "supersedes"
            {
                return Err(w.stop("kb-record-rules-refused", false));
            }
        }
        let ids: BTreeSet<_> = incoming
            .iter()
            .filter(|a| a.pred == "lesson")
            .map(|a| a.args[0].clone())
            .collect();
        for id in &ids {
            let old: BTreeSet<_> = facts
                .iter()
                .filter(|a| {
                    reasoning::METADATA.contains(&a.pred.as_str()) && a.args.first() == Some(id)
                })
                .map(|a| alloc::format!("{a:?}"))
                .collect();
            let new: BTreeSet<_> = incoming
                .iter()
                .filter(|a| {
                    reasoning::METADATA.contains(&a.pred.as_str()) && a.args.first() == Some(id)
                })
                .map(|a| alloc::format!("{a:?}"))
                .collect();
            w.charge(old.len() + new.len() + 1)?;
            if !old.is_empty() && old != new {
                layer_conflicts += 1;
                overrides.push(Override {
                    key: alloc::format!("{id:?}"),
                    earlier_layer: origins
                        .iter()
                        .find(|(f, _)| {
                            f.predicate == "lesson"
                                && f.arguments
                                    .first()
                                    .is_some_and(|v| Some(&Term::Const(v.clone())) == Some(id))
                        })
                        .map(|(_, layer)| *layer),
                    later_layer: layer,
                    earlier: facts
                        .iter()
                        .filter(|a| {
                            reasoning::METADATA.contains(&a.pred.as_str())
                                && a.args.first() == Some(id)
                        })
                        .map(|a| {
                            reasoning::fact(
                                &a.pred,
                                &a.args
                                    .iter()
                                    .filter_map(|t| {
                                        if let Term::Const(v) = t {
                                            Some(v.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect(),
                    later: incoming
                        .iter()
                        .filter(|a| {
                            reasoning::METADATA.contains(&a.pred.as_str())
                                && a.args.first() == Some(id)
                        })
                        .map(|a| {
                            reasoning::fact(
                                &a.pred,
                                &a.args
                                    .iter()
                                    .filter_map(|t| {
                                        if let Term::Const(v) = t {
                                            Some(v.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect(),
                });
            }
        }
        facts.retain(|a| {
            !reasoning::METADATA.contains(&a.pred.as_str())
                || !a.args.first().is_some_and(|id| ids.contains(id))
        });
        for a in &incoming {
            origins.insert(
                reasoning::fact(
                    &a.pred,
                    &a.args
                        .iter()
                        .filter_map(|t| {
                            if let Term::Const(v) = t {
                                Some(v.clone())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>(),
                ),
                layer,
            );
        }
        rule_origins.extend(core::iter::repeat_n(Some(layer), more_rules.len()));
        facts.extend(incoming);
        rules.extend(more_rules);
        w.bound(facts.len() <= 4096, "kb-fact-budget")?;
    }
    let task_source = match request {
        Request::Advise(t)
        | Request::Explain { task: t, .. }
        | Request::PlanFor { task: t, .. }
        | Request::Replan { task: t, .. } => Some(t),
        _ => None,
    };
    if let Some(t) = task_source {
        facts.extend(reasoning::task(t, &mut w)?);
    }
    w.bound(validations.len() <= 128, "kb-validation-budget")?;
    for v in validations {
        w.bound(v.statement.len() <= 4096, "kb-validation-budget")?;
        w.bound(
            v.statement.iter().all(|f| {
                f.arguments.len() <= MAX_ARITY
                    && f.predicate.len() <= 256
                    && f.arguments
                        .iter()
                        .all(|v| !matches!(v,Value::Text(s) if s.len()>256))
            }),
            "kb-validation-budget",
        )?;
        w.charge(
            v.statement
                .iter()
                .map(|f| {
                    f.arguments
                        .iter()
                        .map(|v| match v {
                            Value::Text(s) => s.len(),
                            Value::Integer(_) => 8,
                        })
                        .sum::<usize>()
                        + f.predicate.len()
                        + 1
                })
                .sum::<usize>()
                + 1,
        )?;
        let mut statement = statement(&facts, &v.lesson);
        let mut expected = v.statement.clone();
        statement.sort();
        expected.sort();
        expected.dedup();
        if statement != expected {
            continue;
        }

        if v.last_validation > 0
            && v.status == EvidenceStatus::Validated
            && facts.iter().any(|a| {
                a.pred == "record"
                    && a.args
                        == vec![
                            Term::Const(text(&v.lesson)),
                            Term::Const(text("promoted")),
                            Term::Const(text("usage-v1")),
                            Term::Const(text(&v.dependency_sha256)),
                            Term::Const(text(&v.mode)),
                            Term::Const(text(&v.family)),
                        ]
            })
            && facts.iter().any(|a| {
                a.pred == "evidence"
                    && a.args.first() == Some(&Term::Const(text(&v.lesson)))
                    && a.args.get(2) == Some(&Term::Const(text(&v.recipe_sha256)))
            })
        {
            facts.push(Atom {
                pred: "usage_validated".into(),
                args: vec![Term::Const(text(&v.lesson))],
            });
        }
    }
    if matches!(request, Request::PlanFor { .. } | Request::Replan { .. }) {
        for s in Step::ALL {
            facts.push(Atom {
                pred: "task".into(),
                args: vec![Term::Const(text("subject")), Term::Const(text(s.name()))],
            });
        }
    }
    let mut builtin = Parser {
        tokens: tokenize(reasoning::RULES, &mut w)?,
        i: 0,
    };
    rules.extend(builtin.program(&mut w)?.1);
    w.bound(rules.len() <= MAX_RULES, "kb-rule-budget")?;
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_INHERITANCE").is_some() {
        rules.retain(|r| r.head.pred != "isa");
    }
    #[cfg(test)]
    if std::env::var_os("ZENO_FCIS_KB_STUB_SCOPE").is_some() {
        for r in &mut rules {
            if r.head.pred == "usage_scope" {
                r.body
                    .retain(|l| !matches!(l,Literal::Negative(a) if a.pred=="usage_missing"));
            }
        }
    }
    let mut arities = BTreeMap::new();
    for f in &facts {
        arity(f, &mut arities, &w)?;
    }
    for r in &rules {
        arity(&r.head, &mut arities, &w)?;
        safe(r, &mut w)?;
        for l in &r.body {
            if let Literal::Positive(a) | Literal::Negative(a) = l {
                arity(a, &mut arities, &w)?;
            }
        }
    }
    let mut proofs = reasoning::Proofs::new();
    let db = evaluate(&facts, &rules, &mut proofs, &mut w)?;
    for (f, d) in &mut proofs {
        d.source_layer = match d.rule {
            Some(i) => rule_origins.get(i).copied().flatten(),
            None => origins.get(f).copied(),
        };
    }
    let mut explanations = Vec::new();
    let outcome = match request {
        Request::Query(query) | Request::Explain { query, .. } => {
            let mut p = Parser {
                tokens: tokenize(query, &mut w)?,
                i: 0,
            };
            let a = p.atom(&w)?;
            p.punct(".");
            if p.i != p.tokens.len() {
                return Err(w.stop("kb-query-trailing-input", false));
            }
            arity(&a, &mut arities, &w)?;
            let mut answers = BTreeSet::new();
            for row in db.get(&a.pred).into_iter().flatten() {
                if let Some(b) = matching(&a, row, &Binding::new(), &mut w)? {
                    if !answers.contains(&b) {
                        w.bound(answers.len() < limits.bindings, "kb-query-budget")?;
                        explanations.push(reasoning::explain(
                            reasoning::fact(&a.pred, row),
                            &proofs,
                            &mut w,
                        )?);
                    }
                    answers.insert(b);
                }
            }
            Outcome::Query(answers.into_iter().collect())
        }
        Request::Check => {
            let mut problems = consistency(&db, &mut w)?;
            problems.extend(reasoning::check(&db, &mut w)?);
            for r in db.get("record").into_iter().flatten() {
                w.charge(1)?;
                if r[1] == text("promoted")
                    && !db
                        .get("usage_ready")
                        .is_some_and(|rows| rows.contains(&vec![r[0].clone()]))
                {
                    problems.push("kb-lesson-unvalidated");
                    break;
                }
            }
            if layer_conflicts > 0 {
                problems.push("kb-layer-conflict");
            }
            Outcome::Checked(problems)
        }
        Request::Advise(_) => {
            let mut advice = reasoning::advise(&db, &proofs, &mut w)?;
            if layer_conflicts > 0 {
                advice.problems.push("kb-layer-conflict");
                advice.lessons.clear();
                advice.facts.clear();
                advice.explanations.clear();
                advice.exclusions.clear();
            }
            Outcome::Advised(advice)
        }
        Request::Plan { have, want }
        | Request::PlanFor { have, want, .. }
        | Request::Replan { have, want, .. } => {
            if !consistency(&db, &mut w)?.is_empty() {
                return Err(w.stop("kb-code-inconsistent", false));
            }
            if !reasoning::check(&db, &mut w)?.is_empty() || layer_conflicts > 0 {
                return Err(w.stop("kb-knowledge-inconsistent", false));
            }
            let exclusions = reasoning::exclusions(&db, &proofs, &mut w)?;
            let mut observed = have.clone();
            if let Request::Replan { produced, .. } = request {
                artifacts(produced, &w)?;
                for a in produced {
                    if !observed.contains(a) {
                        observed.push(a.clone());
                    }
                }
            }
            match plan(&observed, want, &db, &proofs, exclusions.clone(), &mut w) {
                Ok(p) => Outcome::Planned(Box::new(p)),
                Err(s)
                    if s.code == "kb-bounded-no-plan"
                        && matches!(request, Request::PlanFor { .. } | Request::Replan { .. }) =>
                {
                    Outcome::NoPlan(exclusions)
                }
                Err(s) => return Err(s),
            }
        }
    };
    Ok(Report {
        outcome,
        work: w.used,
        rows: w.rows,
        states: w.states,
        explanations,
        layer_conflicts,
        overrides,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(s: &str) -> Request {
        Request::Query(s.to_string())
    }
    fn rows(source: &str, query: &str) -> Vec<Binding> {
        match run(source, &request(query), Limits::default())
            .unwrap_or_else(|s| panic!("{s:?}"))
            .outcome
        {
            Outcome::Query(rows) => rows,
            _ => panic!("query"),
        }
    }
    #[test]
    fn kb_recursive_stratified_query_and_anonymous_variables() {
        let source = "edge(a,b). edge(b,c). edge(c,d). blocked(c). path(X,Y):-edge(X,Y). path(X,Z):-edge(X,Y),path(Y,Z). open(X,Y):-path(X,Y),not blocked(Y). other(X,Y):-edge(X,_),edge(_,Y),X!=Y.";
        assert_eq!(rows(source, "path(X,Y)").len(), 6);
        assert_eq!(rows(source, "open(a,c)"), Vec::new());
        assert_eq!(rows(source, "path(a,\"d\")"), vec![Binding::new()]);
        assert_eq!(rows(source, "path(a,_)"), vec![Binding::new()]);
        assert_eq!(
            rows("p(\"\\uD834\\uDD1E\",-3).", "p(X,Y)")[0]["X"],
            text("𝄞")
        );
    }
    #[test]
    fn kb_refuses_unsafe_cycles_arities_and_trailing_queries() {
        for source in [
            "p(X).",
            "p(X):-not q(X).",
            "q(a). p(X):-q(X),not r(X). r(X):-q(X),not p(X).",
            "p(a). p(a,b).",
            "p(_):-q(a).",
            "p(a):-q(a),_=a.",
            "p(\"\\uD800\").",
        ] {
            assert!(
                run(source, &request("p(X)"), Limits::default()).is_err(),
                "{source}"
            );
        }
        assert!(run("p(a).", &request("p(X) q(X)"), Limits::default()).is_err());
        assert!(run("p(a).", &request("p(X). junk"), Limits::default()).is_err());
    }
    #[test]
    fn kb_example_runs_without_any_external_file_and_plans_through_existing_types() {
        let r = run_with_evidence(
            &[EXAMPLE],
            &Request::Check,
            Limits::default(),
            &usage_tests::validations(EXAMPLE),
        )
        .unwrap_or_else(|s| panic!("{s:?}"));
        assert_eq!(r.outcome, Outcome::Checked(Vec::new()));
        let req = Request::Plan {
            have: vec!["original".into(), "vocabulary".into()],
            want: vec!["qualified".into()],
        };
        let r = run(EXAMPLE, &req, Limits::default()).unwrap_or_else(|s| panic!("{s:?}"));
        let Outcome::Planned(p) = r.outcome else {
            panic!("plan");
        };
        assert_eq!(p.selected.as_deref(), Some("split-by-cases"));
        assert_eq!(
            p.steps.iter().map(|s| s.name()).collect::<Vec<_>>(),
            vec![
                "sample",
                "learn-terms",
                "cover",
                "learn-split",
                "assemble",
                "check-program"
            ]
        );
        assert!(
            A::check(&p.composition, TYPE_WORK)
                .unwrap_or_else(|s| panic!("{s:?}"))
                .problems
                .is_empty()
        );
    }
    #[test]
    fn kb_code_comparison_is_load_bearing_and_does_not_execute_extension_facts() {
        let planted = EXAMPLE.replace("needs(diagnose,model).", "needs(diagnose,proposal).");
        let r =
            run(&planted, &Request::Check, Limits::default()).unwrap_or_else(|s| panic!("{s:?}"));
        let Outcome::Checked(problems) = r.outcome else {
            panic!("check");
        };
        assert!(problems.contains(&"kb-needs-code-mismatch"));
        let req = Request::Plan {
            have: vec!["model".into()],
            want: vec!["answer".into()],
        };
        assert!(run(&planted, &req, Limits::default()).is_err());
        let extension = EXAMPLE.to_string() + "custom(a). related(X):-custom(X).";
        assert_eq!(rows(&extension, "related(X)")[0]["X"], text("a"));
    }
    #[test]
    fn kb_work_rows_bindings_states_depth_and_no_path_are_inconclusive() {
        let req = Request::Plan {
            have: vec!["original".into(), "vocabulary".into()],
            want: vec!["qualified".into()],
        };
        for limits in [
            Limits {
                work: 0,
                ..Limits::default()
            },
            Limits {
                rows: 1,
                ..Limits::default()
            },
            Limits {
                bindings: 0,
                ..Limits::default()
            },
            Limits {
                states: 1,
                ..Limits::default()
            },
            Limits {
                steps: 1,
                ..Limits::default()
            },
        ] {
            let source = if limits.bindings == 0 {
                "a(x). p(X):-a(X)."
            } else {
                EXAMPLE
            };
            let r = if limits.bindings == 0 {
                request("p(X)")
            } else {
                req.clone()
            };
            let s = run(source, &r, limits)
                .err()
                .unwrap_or_else(|| panic!("budget escaped"));
            assert!(s.inconclusive, "{s:?}");
        }
        let impossible = Request::Plan {
            have: vec![],
            want: vec!["qualified".into()],
        };
        assert!(
            run(EXAMPLE, &impossible, Limits::default())
                .err()
                .is_some_and(|s| s.inconclusive && s.code == "kb-bounded-no-plan")
        );
    }
    #[test]
    fn kb_result_mutation_cannot_change_inputs_or_prior_results() {
        let source = "edge(a,b).".to_string();
        let before = rows(&source, "edge(X,Y)");
        let mut copy = before.clone();
        copy[0].insert("X".into(), text("changed"));
        assert_ne!(before, copy);
        assert_eq!(rows(&source, "edge(X,Y)"), before);
        assert_eq!(source, "edge(a,b).");
    }
}

#[cfg(test)]
#[path = "strategy_knowledge/usage_tests.rs"]
mod usage_tests;
