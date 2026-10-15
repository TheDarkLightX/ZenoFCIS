//! The actual output-difference query constructor used by the finite partition.
use super::{Prepared, base, same};
#[cfg(verus_keep_ghost)]
use super::{comparison, spec};
use crate::symbolic::program::Program;
use crate::symbolic::smt::Term;
#[cfg(verus_keep_ghost)]
use crate::term::denotation::{self, Value};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    match result {
        Ok(prepared) => super::printed(prepared) && spec::query(prepared.witness@,
            spec::Piece::Outputs(match first { Some(value) => Some(value as int), None => None }),
            original, candidate),
        Err(_) => true,
    },
))]
pub(super) fn outputs(
    original: Program<'_>,
    candidate: Program<'_>,
    first: Option<i128>,
) -> Result<Prepared, crate::symbolic::Unsupported> {
    let purpose = match first {
        Some(value) => format!("case {value}"),
        None => "defined outputs".to_owned(),
    };
    let (mut script, left, right) = base(original, candidate, &purpose)?;
    script.assert(left.ok());
    if let Some(value) = first {
        let Some(output) = left.outputs.first() else {
            return Err(crate::symbolic::Unsupported::new(
                "a split query needs a first output",
            ));
        };
        script.assert(Term::eq(output.clone(), Term::Int(value)));
    }
    script.assert(Term::not(same(&left, &right)));
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, Value>, raw: Seq<i64>|
            spec::context(script, original, candidate, raw, environment) implies
                spec::assertions_hold(script, environment) == spec::expected(
                    spec::Piece::Outputs(match first { Some(value) => Some(value as int), None => None }),
                    original, candidate, raw) by {
            reveal(<Term as View>::view);
            assert(spec::pair_meaning(script.definitions@, original, candidate, left, right));
            assert(left.outputs@.len() == right.outputs@.len());
            comparison::runtime_equality(original, candidate, left, right, raw, environment);
            assert(denotation::term(script.assertions@[0], environment)
                == Some(Value::Boolean(comparison::returns_outputs(left, environment))));
            let difference_index = match first { Some(_) => 2int, None => 1int };
            assert(denotation::term(script.assertions@[difference_index], environment)
                == Some(Value::Boolean(!(comparison::returns_outputs(right, environment)
                    && comparison::equal_outputs(left, right, environment)))));
            if let Some(value) = first {
                denotation::integer_literal(value, environment);
                assert(denotation::term(script.assertions@[1], environment)
                    == Some(Value::Boolean(denotation::term(left.outputs@[0], environment)
                        == Some(Value::Integer(value as int)))));
                if let Ok(values) = spec::execution(original, raw) {
                    assert(values.len() == left.outputs@.len());
                    assert(denotation::term(left.outputs@[0], environment)
                        == Some(Value::Integer(values[0] as int)));
                }
            }
            assert forall|i: int| 0 <= i < script.assertions@.len() implies
                match denotation::term(#[trigger] script.assertions@[i], environment) {
                    Some(Value::Boolean(_)) => true, _ => false,
                } by {
                if i != 0 && i != difference_index { assert(i == 1 && first.is_some()); }
            }
        }
        assert(spec::query(script,
            spec::Piece::Outputs(match first { Some(value) => Some(value as int), None => None }),
            original, candidate));
    }
    let (label, meaning) = match first {
        Some(value) => (
            format!("case-{value}"),
            format!("inputs on which the original returns outputs with first output {value}"),
        ),
        None => (
            "outputs".to_owned(),
            "inputs on which the original returns outputs".to_owned(),
        ),
    };
    Ok(Prepared::new(label, meaning, script))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    match result {
        Ok(prepared) => super::printed(prepared) && spec::query(prepared.witness@,
            if arithmetic { spec::Piece::Arithmetic } else { spec::Piece::OutputDomain },
            original, candidate),
        Err(_) => true,
    },
))]
pub(super) fn failure(
    original: Program<'_>,
    candidate: Program<'_>,
    arithmetic: bool,
) -> Result<Prepared, crate::symbolic::Unsupported> {
    let purpose = if arithmetic {
        "arithmetic failure"
    } else {
        "output domain failure"
    };
    let (mut script, left, right) = base(original, candidate, purpose)?;
    if arithmetic {
        script.assert(left.trap.clone());
        script.assert(Term::not(right.trap.clone()));
    } else {
        script.assert(left.out_of_domain());
        script.assert(Term::not(right.out_of_domain()));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, Value>, raw: Seq<i64>|
            spec::context(script, original, candidate, raw, environment) implies
                spec::assertions_hold(script, environment) == spec::expected(
                    if arithmetic { spec::Piece::Arithmetic } else { spec::Piece::OutputDomain },
                    original, candidate, raw) by {
            reveal(<Term as View>::view);
            assert(spec::pair_meaning(script.definitions@, original, candidate, left, right));
            comparison::runtime_equality(original, candidate, left, right, raw, environment);
            let error = if arithmetic { crate::evaluation::Failure::Arithmetic }
                else { crate::evaluation::Failure::OutputDomain };
            assert(denotation::term(script.assertions@[0], environment)
                == Some(Value::Boolean(spec::execution(original, raw) == Err(error))));
            assert(denotation::term(script.assertions@[1], environment)
                == Some(Value::Boolean(spec::execution(candidate, raw) != Err(error))));
        }
        assert(spec::query(script,
            if arithmetic { spec::Piece::Arithmetic } else { spec::Piece::OutputDomain },
            original, candidate));
    }
    let (label, meaning) = if arithmetic {
        (
            "arithmetic-failure",
            "inputs on which the original fails with Arithmetic",
        )
    } else {
        (
            "output-domain-failure",
            "inputs on which the original fails with OutputDomain",
        )
    };
    Ok(Prepared::new(label.to_owned(), meaning.to_owned(), script))
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    match result {
        Ok(prepared) => super::printed(prepared)
            && spec::query(prepared.witness@, spec::Piece::Control, original, candidate),
        Err(_) => true,
    },
))]
pub(super) fn control(
    original: Program<'_>,
    candidate: Program<'_>,
) -> Result<Prepared, crate::symbolic::Unsupported> {
    let (mut script, left, right) = base(
        original,
        candidate,
        "planted control: the candidate's first output plus one",
    )?;
    script.assert(left.ok());
    script.assert(right.ok());
    if let (Some(a), Some(b)) = (left.outputs.first(), right.outputs.first()) {
        script.observe("original_first", a.clone());
        script.observe("candidate_first", b.clone());
        script.assert(Term::not(Term::eq(
            Term::add(b.clone(), Term::Int(1)),
            a.clone(),
        )));
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, Value>, raw: Seq<i64>|
            spec::context(script, original, candidate, raw, environment) implies
                spec::assertions_hold(script, environment)
                    == spec::expected(spec::Piece::Control, original, candidate, raw) by {
            reveal(<Term as View>::view);
            assert(spec::pair_meaning(script.definitions@, original, candidate, left, right));
            comparison::runtime_equality(original, candidate, left, right, raw, environment);
            assert(denotation::term(script.assertions@[0], environment)
                == Some(Value::Boolean(comparison::returns_outputs(left, environment))));
            assert(denotation::term(script.assertions@[1], environment)
                == Some(Value::Boolean(comparison::returns_outputs(right, environment))));
            if left.outputs@.len() > 0 && right.outputs@.len() > 0 {
                denotation::integer_literal(1, environment);
                if let (Ok(a), Ok(b)) = (spec::execution(original, raw), spec::execution(candidate, raw)) {
                    assert(a.len() > 0 && b.len() > 0);
                    assert(denotation::term(script.assertions@[2], environment)
                        == Some(Value::Boolean(b[0] as int + 1 != a[0] as int)));
                }
            }
        }
        assert(spec::query(script, spec::Piece::Control, original, candidate));
    }
    Ok(Prepared::new(
        "control-shifted-first-output".to_owned(),
        "the candidate with its first output plus one".to_owned(),
        script,
    ))
}
