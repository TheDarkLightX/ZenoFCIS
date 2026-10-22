//! Exact integer terms for the compiled numeric graph and eager law programs.
//! All builders are invocation-local; no input, policy or result aliases mutate.
use super::*;
use crate::contract::graph::{Atom as Literal, LawOp, Observation as O};
use crate::contract::model::{Domain, Expr, InputLeaf, ScalarDomain};
use zeno_fcis_spec::LawScope;
use zeno_fcis_synthesis::finite::{V2IntegerWidth as Width, V2NumericOp as N};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Bool,
    Signed,
    Unsigned,
    Sum(u32),
    Text(Vec<u8>),
}
#[derive(Clone, Debug)]
pub(super) struct Value {
    pub(super) kind: Kind,
    pub(super) term: Term,
}
pub(super) fn uint(n: u128) -> Term {
    if let Ok(n) = i128::try_from(n) {
        return Term::Int(n);
    }
    Term::add(
        Term::mul(Term::Int((n >> 64) as i128), Term::Int(1i128 << 64)),
        Term::Int((n as u64) as i128),
    )
}
pub(super) fn literal(a: &Literal) -> Value {
    let (kind, term) = match a {
        Literal::Bool(b) => (Kind::Bool, Term::Bool(*b)),
        Literal::I128(n) => (Kind::Signed, Term::Int(*n)),
        Literal::U128(n) => (Kind::Unsigned, uint(*n)),
        Literal::Sum { type_id, variant } => (Kind::Sum(*type_id), Term::Int((*variant).into())),
        Literal::Text(t) => (Kind::Text(t.clone()), Term::Int(0)),
    };
    Value { kind, term }
}
pub(super) fn equal(a: &Value, b: &Value) -> Term {
    if a.kind != b.kind {
        Term::Bool(false)
    } else {
        Term::eq(a.term.clone(), b.term.clone())
    }
}
fn number(n: usize) -> Value {
    Value {
        kind: Kind::Signed,
        term: Term::Int(n as i128),
    }
}
fn boolean(term: Term) -> Value {
    Value {
        kind: Kind::Bool,
        term,
    }
}
fn cast(v: Value, unsigned: bool, guards: &mut Vec<Term>) -> Result<Value, ContractError> {
    let term = match (&v.kind, unsigned) {
        (Kind::Bool, false) => Term::ite(v.term, Term::Int(1), Term::Int(0)),
        (Kind::Sum(_), false) | (Kind::Signed, false) | (Kind::Unsigned, true) => v.term,
        (Kind::Unsigned, false) => {
            guards.push(Term::le(v.term.clone(), Term::Int(i128::MAX)));
            v.term
        }
        (Kind::Signed, true) => {
            guards.push(Term::le(Term::Int(0), v.term.clone()));
            v.term
        }
        _ => return Err(error("unsupported numeric cast")),
    };
    Ok(Value {
        kind: if unsigned {
            Kind::Unsigned
        } else {
            Kind::Signed
        },
        term,
    })
}
fn range(t: &Term, kind: &Kind) -> Result<Term, ContractError> {
    match kind {
        Kind::Signed => Ok(Term::within(t, i128::MIN, i128::MAX)),
        Kind::Unsigned => Ok(Term::and(vec![
            Term::le(Term::Int(0), t.clone()),
            Term::le(t.clone(), uint(u128::MAX)),
        ])),
        _ => Err(error("arithmetic on a non-integer")),
    }
}
fn arithmetic(
    op: &str,
    a: Value,
    b: Value,
    guards: &mut Vec<Term>,
) -> Result<Value, ContractError> {
    if a.kind != b.kind {
        return Err(error("mixed arithmetic kinds"));
    }
    let term = match op {
        "+" => Term::add(a.term, b.term),
        "-" => Term::sub(a.term, b.term),
        "*" => Term::mul(a.term, b.term),
        "floor" | "ceil" => {
            guards.push(Term::not(Term::eq(b.term.clone(), Term::Int(0))));
            if op == "floor" {
                Term::floor_div(a.term, b.term)
            } else {
                Term::ceil_div(a.term, b.term)
            }
        }
        _ => return Err(error("unknown arithmetic operation")),
    };
    guards.push(range(&term, &a.kind)?);
    Ok(Value { kind: a.kind, term })
}
fn fused(
    a: Value,
    b: Value,
    d: Value,
    up: bool,
    signed: bool,
    guards: &mut Vec<Term>,
) -> Result<Value, ContractError> {
    if a.kind != b.kind || a.kind != d.kind {
        return Err(error("mixed fused arithmetic kinds"));
    }
    guards.push(Term::lt(Term::Int(0), d.term.clone()));
    if !signed {
        for t in [&a.term, &b.term] {
            guards.push(Term::le(Term::Int(0), t.clone()));
        }
    }
    let product = Term::mul(a.term, b.term);
    let term = if up {
        Term::ceil_div(product, d.term)
    } else {
        Term::floor_div(product, d.term)
    };
    guards.push(range(&term, &a.kind)?);
    Ok(Value { kind: a.kind, term })
}
fn products(a: [Value; 3], b: [Value; 3], guards: &mut Vec<Term>) -> Result<Value, ContractError> {
    let kind = &a[0].kind;
    for v in a.iter().chain(&b) {
        if &v.kind != kind || !matches!(v.kind, Kind::Signed | Kind::Unsigned) {
            return Err(error("mixed product comparison kinds"));
        }
        guards.push(Term::le(Term::Int(0), v.term.clone()));
    }
    let product = |v: [Value; 3]| {
        Term::mul(
            Term::mul(v[0].term.clone(), v[1].term.clone()),
            v[2].term.clone(),
        )
    };
    Ok(boolean(Term::le(product(b), product(a))))
}
fn select(c: Value, a: Value, b: Value) -> Result<Value, ContractError> {
    if c.kind != Kind::Bool || a.kind != b.kind {
        return Err(error("ill-typed selection"));
    }
    Ok(Value {
        kind: a.kind,
        term: Term::ite(c.term, a.term, b.term),
    })
}
fn comparison(op: &str, a: Value, b: Value) -> Result<Value, ContractError> {
    if op == "eq" {
        return Ok(boolean(equal(&a, &b)));
    }
    if a.kind != b.kind {
        return Err(error("mixed comparison kinds"));
    }
    Ok(boolean(if op == "lt" {
        Term::lt(a.term, b.term)
    } else {
        Term::and(vec![a.term, b.term])
    }))
}
fn save(script: &mut Script, name: &str, v: Value) -> Value {
    Value {
        term: script.define(
            name,
            if v.kind == Kind::Bool {
                Sort::Bool
            } else {
                Sort::Int
            },
            v.term,
        ),
        kind: v.kind,
    }
}
fn numeric(
    op: N,
    values: &[Value],
    inputs: &[Value],
    guards: &mut Vec<Term>,
) -> Result<Value, ContractError> {
    let at = |i: u16| {
        values
            .get(i as usize)
            .cloned()
            .ok_or_else(|| error("missing numeric operand"))
    };
    Ok(match op {
        N::Input(i) => inputs
            .get(i as usize)
            .cloned()
            .ok_or_else(|| error("missing numeric input"))?,
        N::I128(n) => literal(&Literal::I128(n)),
        N::U128(n) => literal(&Literal::U128(n)),
        N::Bool(b) => boolean(Term::Bool(b)),
        N::ToI128(a) => cast(at(a)?, false, guards)?,
        N::ToU128(a) => cast(at(a)?, true, guards)?,
        N::Add(w, a, b)
        | N::Sub(w, a, b)
        | N::Mul(w, a, b)
        | N::DivFloor(w, a, b)
        | N::DivCeil(w, a, b) => {
            let (a, b) = (at(a)?, at(b)?);
            let expected = match w {
                Width::Signed128 => Kind::Signed,
                Width::Unsigned128 => Kind::Unsigned,
                _ => return Err(error("unsupported arithmetic width")),
            };
            if a.kind != expected || b.kind != expected {
                return Err(error("arithmetic width differs from operands"));
            }
            arithmetic(
                match op {
                    N::Add(..) => "+",
                    N::Sub(..) => "-",
                    N::Mul(..) => "*",
                    N::DivFloor(..) => "floor",
                    _ => "ceil",
                },
                a,
                b,
                guards,
            )?
        }
        N::Eq(a, b) => comparison("eq", at(a)?, at(b)?)?,
        N::Lt(a, b) => comparison("lt", at(a)?, at(b)?)?,
        N::And(a, b) => comparison("and", at(a)?, at(b)?)?,
        N::Not(a) => boolean(Term::not(at(a)?.term)),
        N::Select(c, a, b) => select(at(c)?, at(a)?, at(b)?)?,
        N::MulDivFloor(a, b, d)
        | N::MulDivCeil(a, b, d)
        | N::SignedMulDivFloor(a, b, d)
        | N::NonnegativeMulDivFloor(_, a, b, d)
        | N::NonnegativeMulDivCeil(_, a, b, d) => fused(
            at(a)?,
            at(b)?,
            at(d)?,
            matches!(op, N::MulDivCeil(..) | N::NonnegativeMulDivCeil(..)),
            matches!(op, N::SignedMulDivFloor(..)),
            guards,
        )?,
        N::ProductsGe(a, b) | N::NonnegativeProductsGe(a, b) => products(
            [at(a[0])?, at(a[1])?, at(a[2])?],
            [at(b[0])?, at(b[1])?, at(b[2])?],
            guards,
        )?,
        _ => return Err(error("unsupported numeric instruction")),
    })
}
pub(super) fn admits(v: &Value, d: &Domain) -> Term {
    match (d, &v.kind) {
        (Domain::Bool, Kind::Bool) => Term::Bool(true),
        (Domain::I128 { min, max }, Kind::Signed) => Term::within(&v.term, *min, *max),
        (Domain::U128 { min, max }, Kind::Unsigned) => Term::and(vec![
            Term::le(uint(*min), v.term.clone()),
            Term::le(v.term.clone(), uint(*max)),
        ]),
        (Domain::Sum { type_id, variants }, Kind::Sum(actual)) if type_id == actual => {
            Term::member(
                &v.term,
                &variants.iter().map(|n| i128::from(*n)).collect::<Vec<_>>(),
            )
        }
        (Domain::Text, Kind::Text(t)) => Term::Bool(t.is_ascii()),
        _ => Term::Bool(false),
    }
}
fn reify(v: Value, leaf: &InputLeaf, guards: &mut Vec<Term>) -> Result<Value, ContractError> {
    Ok(match leaf {
        InputLeaf::Bool => {
            if v.kind == Kind::Bool {
                v
            } else {
                guards.push(Term::within(&v.term, 0, 1));
                boolean(Term::eq(v.term, Term::Int(1)))
            }
        }
        InputLeaf::I128 { min, max } => {
            guards.push(Term::within(&v.term, *min, *max));
            Value {
                kind: Kind::Signed,
                ..v
            }
        }
        InputLeaf::U128 { min, max } => {
            let v = Value {
                kind: Kind::Unsigned,
                ..v
            };
            guards.push(admits(
                &v,
                &Domain::U128 {
                    min: *min,
                    max: *max,
                },
            ));
            v
        }
        InputLeaf::Sum { type_id, variants } => {
            guards.push(Term::member(
                &v.term,
                &variants.iter().map(|n| i128::from(*n)).collect::<Vec<_>>(),
            ));
            Value {
                kind: Kind::Sum(*type_id),
                ..v
            }
        }
    })
}
pub(super) struct Encoding {
    pub(super) label: String,
    pub(super) script: Script,
    pub(super) inputs: Vec<Value>,
    pub(super) program_ok: Term,
    pub(super) arithmetic_ok: Term,
    pub(super) outputs: Vec<Value>,
    pub(super) cases: Vec<Case>,
    pub(super) lawful: Term,
    pub(super) edges: Vec<Term>,
}
pub(super) struct Case {
    pub(super) selected: Term,
    pub(super) constructed: Term,
    pub(super) post: Vec<(u16, Value)>,
    pub(super) laws: Vec<(u32, Term)>,
}
impl Encoding {
    pub(super) fn new(
        c: &Contract<'_>,
        prefix: &str,
        mut script: Script,
        inputs: Option<Vec<Value>>,
    ) -> Result<Self, ContractError> {
        let inputs = match inputs {
            Some(v) => v,
            None => {
                let mut values = Vec::new();
                for (i, d) in c.input_domains.iter().enumerate() {
                    let name = format!("x{i}");
                    let (kind, term) = match d {
                        ScalarDomain::Bool => (
                            Kind::Bool,
                            Term::eq(
                                script.constant(&name, Values::Range { min: 0, max: 1 }),
                                Term::Int(1),
                            ),
                        ),
                        ScalarDomain::Int { min, max } => (
                            Kind::Signed,
                            script.constant(
                                &name,
                                Values::Range {
                                    min: *min,
                                    max: *max,
                                },
                            ),
                        ),
                        ScalarDomain::Unsigned { min, max } => {
                            if *max <= i128::MAX as u128 {
                                (
                                    Kind::Unsigned,
                                    script.constant(
                                        &name,
                                        Values::Range {
                                            min: *min as i128,
                                            max: *max as i128,
                                        },
                                    ),
                                )
                            } else {
                                let hi = script.constant(
                                    &format!("x{i}h"),
                                    Values::Range {
                                        min: 0,
                                        max: u64::MAX.into(),
                                    },
                                );
                                let lo = script.constant(
                                    &format!("x{i}l"),
                                    Values::Range {
                                        min: 0,
                                        max: u64::MAX.into(),
                                    },
                                );
                                let t = Term::add(Term::mul(hi, Term::Int(1i128 << 64)), lo);
                                script.assert(Term::and(vec![
                                    Term::le(uint(*min), t.clone()),
                                    Term::le(t.clone(), uint(*max)),
                                ]));
                                (Kind::Unsigned, t)
                            }
                        }
                    };
                    if let InputLeaf::Sum { variants, .. } =
                        super::super::unsigned::input_leaf(c, i)
                    {
                        script.assert(Term::member(
                            &term,
                            &variants.iter().map(|n| i128::from(*n)).collect::<Vec<_>>(),
                        ));
                    }
                    values.push(Value { kind, term });
                }
                values
            }
        };
        let mut nodes = Vec::new();
        let mut guards = Vec::new();
        for (i, op) in c.nodes.iter().enumerate() {
            let v = numeric(policy::numeric_op(op)?, &nodes, &inputs, &mut guards)?;
            nodes.push(save(&mut script, &format!("{prefix}n{i}"), v));
        }
        let arithmetic_ok = Term::and(guards.clone());
        let mut outputs = Vec::new();
        for (r, leaf) in c.program_roots.iter().zip(&c.output_types) {
            let v = nodes
                .get(*r)
                .cloned()
                .ok_or_else(|| error("missing output root"))?;
            outputs.push(reify(v, leaf, &mut guards)?);
        }
        // Every declared scalar output domain is checked before reification.
        for (v, d) in outputs.iter().zip(&c.output_domains) {
            let t = if v.kind == Kind::Bool {
                Term::ite(v.term.clone(), Term::Int(1), Term::Int(0))
            } else {
                v.term.clone()
            };
            guards.push(match d {
                ScalarDomain::Bool => Term::within(&t, 0, 1),
                ScalarDomain::Int { min, max } => Term::within(&t, *min, *max),
                ScalarDomain::Unsigned { min, max } => Term::and(vec![
                    Term::le(uint(*min), t.clone()),
                    Term::le(t, uint(*max)),
                ]),
            });
        }
        let edges = arithmetic_edges(&guards);
        let program_ok = Term::and(guards);
        let mut result = Self {
            label: prefix.to_owned(),
            script,
            inputs,
            outputs,
            program_ok,
            arithmetic_ok,
            cases: Vec::new(),
            lawful: Term::Bool(true),
            edges,
        };
        let mut pre = Vec::new();
        for l in &c.laws {
            if is_state(l) {
                pre.push(result.law(c, l, None, &format!("{prefix}pre{}n", l.id))?);
            }
        }
        result.lawful = Term::and(pre);
        for (i, branch) in c.branches.iter().enumerate() {
            let selected = Term::eq(result.outputs[0].term.clone(), Term::Int(i as i128));
            let mut constructed = vec![result.program_ok.clone()];
            let mut post = Vec::new();
            for p in &branch.assignments {
                let v = result.resolve(c, &p.value)?;
                constructed.push(admits(&v, &p.domain));
                post.push((p.field, v));
            }
            for p in &branch.outbox {
                let channel = c
                    .channels
                    .iter()
                    .find(|ch| ch.id == p.channel)
                    .ok_or_else(|| error("missing channel"))?;
                constructed.push(admits(&literal(&p.destination), &channel.destination));
                for (id, e) in &p.payload {
                    let domain = &channel
                        .payload
                        .iter()
                        .find(|(field, _)| field == id)
                        .ok_or_else(|| error("missing payload schema"))?
                        .1;
                    constructed.push(admits(&result.resolve(c, e)?, domain));
                }
            }
            let mut laws = Vec::new();
            for l in &c.laws {
                if applicable(l, branch.class, branch.reason) {
                    let term = result.law(c, l, Some(i), &format!("{prefix}b{i}l{}n", l.id))?;
                    laws.push((l.id, term));
                }
            }
            result.cases.push(Case {
                selected,
                constructed: Term::and(constructed),
                post,
                laws,
            });
        }
        Ok(result)
    }
    pub(super) fn input(&self, c: &Contract<'_>, s: Source, id: Option<u16>) -> Option<Value> {
        let i = c.bindings.iter().position(|b| *b == (s, id))?;
        let v = self.inputs[i].clone();
        Some(match super::super::unsigned::input_leaf(c, i) {
            InputLeaf::Sum { type_id, .. } => Value {
                kind: Kind::Sum(*type_id),
                ..v
            },
            _ => v,
        })
    }
    pub(super) fn resolve(&self, c: &Contract<'_>, e: &Expr) -> Result<Value, ContractError> {
        match e {
            Expr::Constant(a) => Ok(literal(a)),
            Expr::Output(i) => self
                .outputs
                .get(*i)
                .cloned()
                .ok_or_else(|| error("missing output")),
            Expr::Root(s) => self.input(c, *s, None).ok_or_else(|| error("missing root")),
            Expr::Input(s, id) => self
                .input(c, *s, Some(*id))
                .ok_or_else(|| error("missing input")),
        }
    }
    fn observe(
        &self,
        c: &Contract<'_>,
        o: O,
        case: Option<usize>,
    ) -> Result<Option<Value>, ContractError> {
        let input = |s, id| self.input(c, s, id);
        let Some(i) = case else {
            return Ok(match o {
                O::Post(id) | O::Initial(id) => input(Source::State, Some(id)),
                _ => None,
            });
        };
        let b = &c.branches[i];
        let post = |id| {
            b.assignments
                .iter()
                .find(|p| p.field == id)
                .map(|p| self.resolve(c, &p.value))
                .transpose()
        };
        Ok(match o {
            O::Pre(id) => input(Source::State, Some(id)),
            O::Command(id) => input(Source::Command, Some(id)),
            O::Context(id) => input(Source::Context, Some(id)),
            O::Post(id) => post(id)?,
            O::PreRoot => input(Source::State, None),
            O::CommandRoot => input(Source::Command, None),
            O::ContextRoot => input(Source::Context, None),
            O::Initial(_) | O::PostRoot => None,
            O::Class => Some(number(b.class.code() as usize)),
            O::HasReason => Some(boolean(Term::Bool(b.reason.is_some()))),
            O::Reason => b.reason.map(|n| number(n as usize)),
            O::PostLength => Some(number(b.assignments.len())),
            O::EffectLength => Some(number(0)),
            O::OutboxLength => Some(number(b.outbox.len())),
            O::PatchLength => {
                let mut t = Term::Int(0);
                for p in &b.assignments {
                    let before = input(Source::State, Some(p.field))
                        .ok_or_else(|| error("missing patch pre"))?;
                    t = Term::add(
                        t,
                        Term::ite(
                            equal(&before, &self.resolve(c, &p.value)?),
                            Term::Int(0),
                            Term::Int(1),
                        ),
                    );
                }
                Some(Value {
                    kind: Kind::Signed,
                    term: t,
                })
            }
            O::OutboxOrdinal(n) => b.outbox.get(n).map(|p| number(p.ordinal as usize)),
            O::OutboxChannel(n) => b.outbox.get(n).map(|p| number(p.channel as usize)),
            O::OutboxDestination(n) => b.outbox.get(n).map(|p| literal(&p.destination)),
            O::OutboxIdempotency(n) => b
                .outbox
                .get(n)
                .map(|p| literal(&Literal::U128(p.idempotency))),
            O::OutboxPayload(n, id) => match b
                .outbox
                .get(n)
                .and_then(|p| p.payload.iter().find(|(f, _)| *f == id))
            {
                Some((_, e)) => Some(self.resolve(c, e)?),
                None => None,
            },
        })
    }
    pub(super) fn law(
        &mut self,
        c: &Contract<'_>,
        law: &crate::contract::model::Law,
        case: Option<usize>,
        prefix: &str,
    ) -> Result<Term, ContractError> {
        let mut nodes: Vec<Value> = Vec::new();
        // A transition law can read computed outputs only after successful
        // eager execution/reification. Disable its auxiliary relations when
        // that prefix failed, so later laws cannot erase an earlier trap.
        let mut guards = if case.is_some() {
            vec![self.program_ok.clone()]
        } else {
            Vec::new()
        };
        for (i, op) in law.nodes.iter().enumerate() {
            let at = |i: usize| {
                nodes
                    .get(i)
                    .cloned()
                    .ok_or_else(|| error("missing law operand"))
            };
            let v = match op {
                LawOp::Literal(a) => literal(a),
                LawOp::Observe(o) => match self.observe(c, *o, case)? {
                    Some(v) => v,
                    // An unavailable observation ends this eager law with
                    // Undefined. No later operand acquires a fabricated kind.
                    None => return Ok(Term::Bool(false)),
                },
                LawOp::ObserveWhen(g, o, default) => {
                    let guard = at(*g)?;
                    let default = literal(default);
                    match self.observe(c, *o, case)? {
                        Some(v) => select(guard, v, default)?,
                        None => {
                            guards.push(Term::not(guard.term));
                            default
                        }
                    }
                }
                LawOp::ToI128(a) => cast(at(*a)?, false, &mut guards)?,
                LawOp::ToU128(a) => cast(at(*a)?, true, &mut guards)?,
                LawOp::Add(a, b) | LawOp::Sub(a, b) | LawOp::Mul(a, b) | LawOp::Div(_, a, b) => {
                    arithmetic(
                        match op {
                            LawOp::Add(..) => "+",
                            LawOp::Sub(..) => "-",
                            LawOp::Mul(..) => "*",
                            LawOp::Div(crate::contract::expr::Rounding::Floor, ..) => "floor",
                            _ => "ceil",
                        },
                        at(*a)?,
                        at(*b)?,
                        &mut guards,
                    )?
                }
                LawOp::MulDiv(_, a, b, d) | LawOp::SignedMulDivFloor(a, b, d) => fused(
                    at(*a)?,
                    at(*b)?,
                    at(*d)?,
                    matches!(op, LawOp::MulDiv(crate::contract::expr::Rounding::Ceil, ..)),
                    matches!(op, LawOp::SignedMulDivFloor(..)),
                    &mut guards,
                )?,
                LawOp::ProductsGe(a, b) => products(
                    [at(a[0])?, at(a[1])?, at(a[2])?],
                    [at(b[0])?, at(b[1])?, at(b[2])?],
                    &mut guards,
                )?,
                LawOp::Eq(a, b) => comparison("eq", at(*a)?, at(*b)?)?,
                LawOp::Lt(a, b) => comparison("lt", at(*a)?, at(*b)?)?,
                LawOp::And(a, b) => comparison("and", at(*a)?, at(*b)?)?,
                LawOp::Not(a) => boolean(Term::not(at(*a)?.term)),
                LawOp::Select(g, a, b) => select(at(*g)?, at(*a)?, at(*b)?)?,
            };
            nodes.push(save(&mut self.script, &format!("{prefix}{i}"), v));
        }
        let root = nodes
            .get(law.root)
            .ok_or_else(|| error("missing law root"))?;
        if root.kind != Kind::Bool {
            return Err(error("non-Boolean law root"));
        }
        for edge in arithmetic_edges(&guards) {
            if !self.edges.contains(&edge) {
                self.edges.push(edge);
            }
        }
        guards.push(root.term.clone());
        Ok(Term::and(guards))
    }
}

/// Seek equality and both neighbours of every checked arithmetic boundary.
/// These are model-generation questions, followed by concrete native parity;
/// an unreachable boundary must receive two unsat answers, never be skipped.
fn arithmetic_edges(guards: &[Term]) -> Vec<Term> {
    fn visit(t: &Term, out: &mut Vec<Term>) {
        match t {
            Term::And(ts) | Term::Or(ts) => {
                for t in ts {
                    visit(t, out)
                }
            }
            Term::Le(a, b) | Term::Lt(a, b) | Term::Eq(a, b) => {
                for delta in [-1, 0, 1] {
                    let edge = Term::eq((**a).clone(), Term::add((**b).clone(), Term::Int(delta)));
                    if !matches!(edge, Term::Bool(_)) && !out.contains(&edge) {
                        out.push(edge);
                    }
                }
            }
            Term::Not(t) => visit(t, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for g in guards {
        visit(g, &mut out)
    }
    out
}
pub(super) fn is_state(l: &crate::contract::model::Law) -> bool {
    l.genesis
        && matches!(l.scope, LawScope::Always | LawScope::Committing)
        && l.kind != crate::contract::rules::LawKind::InitialCondition
}
pub(super) fn applicable(
    l: &crate::contract::model::Law,
    class: Class,
    reason: Option<u32>,
) -> bool {
    if l.kind == crate::contract::rules::LawKind::InitialCondition {
        return false;
    }
    match l.scope {
        LawScope::Always => true,
        LawScope::Accept => class == Class::Accept,
        LawScope::Reject => class == Class::Reject,
        LawScope::CommittedFailure => class == Class::CommittedFailure,
        LawScope::Committing => class != Class::Reject,
        LawScope::RejectReason(r) => class == Class::Reject && reason == Some(r),
    }
}
