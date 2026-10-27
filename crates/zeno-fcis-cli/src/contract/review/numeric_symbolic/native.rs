//! Native Authority replay and observable comparisons, with no rule interpreter.
use super::encode::{Value, equal, literal};
use super::*;
use crate::contract::graph::Atom as Literal;
use zeno_fcis_synthesis::finite::v2_composition as c;

pub(super) fn evaluate(
    c: &Contract<'_>,
    schema: &[u8],
    row: &[Atom<'static>],
) -> Result<evaluate::Outcome, ContractError> {
    let f = unsigned::frame(c, row)?;
    let raw = || c::Raw {
        state: &f[0],
        command: &f[1],
        context: &f[2],
    };
    if c.unsigned {
        policy::with_numeric_authority(c, schema, |a| evaluate::from_evaluation(&a.evaluate(raw())))
    } else {
        policy::with_authority(c, schema, |a| evaluate::from_evaluation(&a.evaluate(raw())))
    }
}
pub(super) fn with_evaluator<R>(
    c: &Contract<'_>,
    schema: &[u8],
    f: impl FnOnce(&dyn Fn(&[Atom<'static>]) -> Result<evaluate::Outcome, ContractError>) -> R,
) -> Result<R, ContractError> {
    if c.unsigned {
        policy::with_numeric_authority(c, schema, |a| {
            f(&|row| {
                let frames = unsigned::frame(c, row)?;
                Ok(evaluate::from_evaluation(&a.evaluate(c::Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                })))
            })
        })
    } else {
        policy::with_authority(c, schema, |a| {
            f(&|row| {
                let frames = unsigned::frame(c, row)?;
                Ok(evaluate::from_evaluation(&a.evaluate(c::Raw {
                    state: &frames[0],
                    command: &frames[1],
                    context: &frames[2],
                })))
            })
        })
    }
}
fn value(v: &evaluate::Value) -> Result<Value, ContractError> {
    Ok(literal(&match v {
        evaluate::Value::Bool(b) => Literal::Bool(*b),
        evaluate::Value::I128(n) => Literal::I128(*n),
        evaluate::Value::U128(n) => Literal::U128(*n),
        evaluate::Value::Sum { type_id, variant } => Literal::Sum {
            type_id: *type_id,
            variant: *variant,
        },
        evaluate::Value::Text(t) => Literal::Text(t.clone()),
        _ => return Err(error("unsupported native observable")),
    }))
}
fn successful(case: &encode::Case) -> Term {
    Term::and(case.laws.iter().map(|(_, t)| t.clone()).collect())
}
fn refused(case: &encode::Case, id: u32) -> Term {
    let mut prefix = vec![case.selected.clone(), case.constructed.clone()];
    for (other, t) in &case.laws {
        if *other == id {
            prefix.push(Term::not(t.clone()));
            return Term::and(prefix);
        }
        prefix.push(t.clone());
    }
    Term::Bool(false)
}
pub(super) fn matches(
    c: &Contract<'_>,
    e: &Encoding,
    actual: &evaluate::Outcome,
) -> Result<Term, ContractError> {
    match actual {
        evaluate::Outcome::Refused(r) if r.class == evaluate::RefusalClass::Law => Ok(Term::or(
            e.cases
                .iter()
                .map(|case| refused(case, r.law.unwrap_or(0)))
                .collect(),
        )),
        evaluate::Outcome::Refused(r) if r.class == evaluate::RefusalClass::Arithmetic => {
            Ok(Term::not(e.arithmetic_ok.clone()))
        }
        evaluate::Outcome::Refused(r) if r.class == evaluate::RefusalClass::Domain => {
            Ok(Term::and(vec![
                e.arithmetic_ok.clone(),
                Term::not(Term::or(
                    e.cases
                        .iter()
                        .map(|case| {
                            Term::and(vec![case.selected.clone(), case.constructed.clone()])
                        })
                        .collect(),
                )),
            ]))
        }
        evaluate::Outcome::Refused(r) => {
            Err(error(format!("unencoded native refusal: {}", r.text)))
        }
        evaluate::Outcome::Decision(d) => {
            let mut cases = Vec::new();
            for (b, case) in c.branches.iter().zip(&e.cases) {
                if b.class != d.class
                    || b.reason != d.reason
                    || case.post.len() != d.post.len()
                    || b.outbox.len() != d.outbox.len()
                {
                    continue;
                }
                let mut terms = vec![
                    case.selected.clone(),
                    case.constructed.clone(),
                    successful(case),
                ];
                for ((field, v), (id, a)) in case.post.iter().zip(&d.post) {
                    terms.push(Term::Bool(field == id));
                    terms.push(equal(v, &value(a)?));
                }
                for (plan, a) in b.outbox.iter().zip(&d.outbox) {
                    terms.push(Term::Bool(
                        plan.ordinal == a.ordinal
                            && plan.channel == a.channel
                            && plan.payload.len() == a.payload.len(),
                    ));
                    terms.push(equal(&literal(&plan.destination), &value(&a.destination)?));
                    terms.push(equal(
                        &literal(&Literal::U128(plan.idempotency)),
                        &value(&a.idempotency)?,
                    ));
                    for ((field, expr), (id, a)) in plan.payload.iter().zip(&a.payload) {
                        terms.push(Term::Bool(field == id));
                        terms.push(equal(&e.resolve(c, expr)?, &value(a)?));
                    }
                }
                cases.push(Term::and(terms));
            }
            Ok(Term::or(cases))
        }
    }
}
/// Conservative equality: technical refusal details are not collapsed into
/// equality. Such pairs remain UNKNOWN unless native replay distinguishes them.
pub(super) fn equivalent(
    a: &Contract<'_>,
    ae: &Encoding,
    b: &Contract<'_>,
    be: &Encoding,
) -> Result<Term, ContractError> {
    let mut equalities = Vec::new();
    for (ab, ac) in a.branches.iter().zip(&ae.cases) {
        for (bb, bc) in b.branches.iter().zip(&be.cases) {
            if ab.class != bb.class
                || ab.reason != bb.reason
                || ac.post.len() != bc.post.len()
                || ab.outbox.len() != bb.outbox.len()
            {
                continue;
            }
            let mut terms = vec![
                ac.selected.clone(),
                bc.selected.clone(),
                ac.constructed.clone(),
                bc.constructed.clone(),
                successful(ac),
                successful(bc),
            ];
            for ((aid, av), (bid, bv)) in ac.post.iter().zip(&bc.post) {
                terms.push(Term::Bool(aid == bid));
                terms.push(equal(av, bv));
            }
            for (ap, bp) in ab.outbox.iter().zip(&bb.outbox) {
                terms.push(Term::Bool(
                    ap.ordinal == bp.ordinal
                        && ap.channel == bp.channel
                        && ap.idempotency == bp.idempotency
                        && ap.payload.len() == bp.payload.len(),
                ));
                terms.push(equal(&literal(&ap.destination), &literal(&bp.destination)));
                for ((aid, av), (bid, bv)) in ap.payload.iter().zip(&bp.payload) {
                    terms.push(Term::Bool(aid == bid));
                    terms.push(equal(&ae.resolve(a, av)?, &be.resolve(b, bv)?));
                }
            }
            equalities.push(Term::and(terms));
        }
    }
    Ok(Term::or(equalities))
}
pub(super) fn examples(
    c: &Contract<'_>,
    schema: &[u8],
    source: Option<&str>,
) -> Result<Json, ContractError> {
    let source = source.unwrap_or("");
    if !c.unsigned {
        let positions = super::super::domain::positions(c.declarations)?;
        let framer = super::super::evaluate::Framer::new(&positions, c);
        return policy::with_authority(c, schema, |a| {
            let parsed = super::super::examples::parse(source, a.descriptor())?;
            let mut failures = Vec::new();
            for example in &parsed {
                let actual = super::super::evaluate::evaluate(a, &framer, &example.inputs);
                if let Err(reason) =
                    super::super::examples::agrees(example, &actual, c.genesis.len())
                {
                    failures.push(json!({"line":example.line,"reason":reason}));
                }
            }
            Ok(
                json!({"status":if failures.is_empty(){"holds"}else{"refuted"},"count":parsed.len(),"disagreements":failures}),
            )
        })?;
    }
    policy::with_numeric_authority(c, schema, |a| {
        let parsed = unsigned::examples::parse(source, a.descriptor())
            .map_err(|e| error(format!("example {}: {}", e.line, e.reason)))?;
        let mut failures = Vec::new();
        for example in &parsed {
            let actual = evaluate(c, schema, &example.inputs)?;
            let matches = match &actual {
                evaluate::Outcome::Decision(d) => {
                    d.class
                        == match example.class {
                            unsigned::examples::Class::Accept => Class::Accept,
                            unsigned::examples::Class::Reject => Class::Reject,
                            unsigned::examples::Class::Failure => Class::CommittedFailure,
                        }
                        && d.reason == example.reason
                        && if d.class == Class::Reject {
                            d.post.is_empty() && example.post == example.inputs[..c.genesis.len()]
                        } else {
                            d.post.len() == example.post.len()
                                && d.post
                                    .iter()
                                    .zip(&example.post)
                                    .all(|((_, a), b)| unsigned::same_value(a, *b))
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
                                        .all(|((_, a), b)| unsigned::same_value(a, *b))
                            })
                }
                _ => false,
            };
            if !matches {
                failures
                    .push(json!({"line":example.number,"authority":unsigned::normalized(&actual)}));
            }
        }
        Ok(
            json!({"status":if failures.is_empty(){"holds"}else{"refuted"},"count":parsed.len(),"disagreements":failures}),
        )
    })?
}
