//! Actual ordered-output comparison used by both split and unsplit queries.
use crate::symbolic::program::Encoded;
use crate::symbolic::smt::Term;
#[cfg(verus_keep_ghost)]
use crate::term::denotation::{self, Value};
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
pub(super) open spec fn ready(left: Encoded, right: Encoded,
    environment: Map<Seq<char>, Value>) -> bool {
    (forall|i: int| 0 <= i < left.outputs@.len() ==>
        match denotation::term(#[trigger] left.outputs@[i], environment) {
            Some(Value::Integer(_)) => true, _ => false,
        }) && (forall|i: int| 0 <= i < right.outputs@.len() ==>
        match denotation::term(#[trigger] right.outputs@[i], environment) {
            Some(Value::Integer(_)) => true, _ => false,
        }) && match (denotation::term(right.trap, environment),
            denotation::term(right.admitted, environment)) {
            (Some(Value::Boolean(_)), Some(Value::Boolean(_))) => true, _ => false,
        }
}

pub(super) open spec fn returns_outputs(right: Encoded,
    environment: Map<Seq<char>, Value>) -> bool {
    denotation::term(right.trap, environment) == Some(Value::Boolean(false))
        && denotation::term(right.admitted, environment) == Some(Value::Boolean(true))
}

pub(super) open spec fn equal_outputs(left: Encoded, right: Encoded,
    environment: Map<Seq<char>, Value>) -> bool {
    left.outputs@.len() == right.outputs@.len()
        && forall|i: int| 0 <= i < left.outputs@.len() ==>
            denotation::term(#[trigger] left.outputs@[i], environment)
                == denotation::term(right.outputs@[i], environment)
}

pub(super) open spec fn items_meaning(items: Seq<Term>, count: nat,
    left: Encoded, right: Encoded, environment: Map<Seq<char>, Value>) -> bool {
    items.len() == count + 1 && count <= left.outputs@.len() && count <= right.outputs@.len()
        && denotation::term(items[0], environment)
            == Some(Value::Boolean(returns_outputs(right, environment)))
        && forall|i: int| 1 <= i <= count ==>
            denotation::term(#[trigger] items[i], environment)
                == Some(Value::Boolean(denotation::term(left.outputs@[i - 1], environment)
                    == denotation::term(right.outputs@[i - 1], environment)))
}

/// Ordered wire equality is the actual runtime result equality whenever the
/// original returns outputs. This uses the existing runtime/encoder theorem;
/// it does not introduce a second executable evaluator.
pub(super) proof fn runtime_equality(
    original: crate::symbolic::program::Program<'_>,
    candidate: crate::symbolic::program::Program<'_>,
    left: Encoded, right: Encoded, raw: Seq<i64>,
    environment: Map<Seq<char>, Value>,
)
    requires
        crate::symbolic::program::bridge::outcome(original.inputs@, original.outputs@,
            original.nodes@, original.roots@, raw, left.outputs@, left.trap, left.admitted, environment),
        crate::symbolic::program::bridge::outcome(candidate.inputs@, candidate.outputs@,
            candidate.nodes@, candidate.roots@, raw, right.outputs@, right.trap, right.admitted, environment),
        left.outputs@.len() == right.outputs@.len(),
    ensures ready(left, right, environment),
        returns_outputs(left, environment) ==
            crate::evaluation::spec::execution(original.inputs@, original.outputs@,
                original.nodes@, original.roots@, raw).is_ok(),
        returns_outputs(right, environment) ==
            crate::evaluation::spec::execution(candidate.inputs@, candidate.outputs@,
                candidate.nodes@, candidate.roots@, raw).is_ok(),
        returns_outputs(left, environment) ==>
            (returns_outputs(right, environment) && equal_outputs(left, right, environment)) ==
                (crate::evaluation::spec::execution(original.inputs@, original.outputs@,
                    original.nodes@, original.roots@, raw)
                    == crate::evaluation::spec::execution(candidate.inputs@, candidate.outputs@,
                        candidate.nodes@, candidate.roots@, raw)),
{
    let original_result = crate::evaluation::spec::execution(original.inputs@, original.outputs@,
        original.nodes@, original.roots@, raw);
    let candidate_result = crate::evaluation::spec::execution(candidate.inputs@, candidate.outputs@,
        candidate.nodes@, candidate.roots@, raw);
    if let (Ok(original_values), Ok(candidate_values)) = (original_result, candidate_result) {
        crate::symbolic::program::bridge::output_values_equal(left.outputs@, right.outputs@,
            original_values, candidate_values, environment);
    }
}
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    forall|environment: Map<Seq<char>, Value>|
        ready(*left, *right, environment) && left.outputs@.len() == right.outputs@.len() ==>
            denotation::term(result, environment) == Some(Value::Boolean(
                returns_outputs(*right, environment) && equal_outputs(*left, *right, environment))),
))]
pub(super) fn same(left: &Encoded, right: &Encoded) -> Term {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(<Term as View>::view); }
    let mut items = vec![right.ok()];
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= left.outputs@.len(), index <= right.outputs@.len(),
            items@.len() == index + 1,
            forall|environment: Map<Seq<char>, Value>| ready(*left, *right, environment) ==>
                items_meaning(items@, index as nat, *left, *right, environment),
        decreases left.outputs@.len() - index,
    ))]
    while index < left.outputs.len() && index < right.outputs.len() {
        let left_term = left.outputs[index].clone();
        let right_term = right.outputs[index].clone();
        #[cfg(verus_keep_ghost)]
        let compared = verus_exec_expr!(Ghost((left_term@, right_term@)));
        let equal = Term::eq(left_term, right_term);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, Value>| ready(*left, *right, environment) implies
                denotation::term(equal, environment) == Some(Value::Boolean(
                    denotation::term(left.outputs@[index as int], environment)
                        == denotation::term(right.outputs@[index as int], environment))) by {
                reveal(<Term as View>::view);
                assert(match denotation::term(left.outputs@[index as int], environment) {
                    Some(Value::Integer(_)) => true, _ => false,
                });
                assert(match denotation::term(right.outputs@[index as int], environment) {
                    Some(Value::Integer(_)) => true, _ => false,
                });
                assert(denotation::term(compared@.0, environment)
                    == denotation::term(left.outputs@[index as int], environment));
                assert(denotation::term(compared@.1, environment)
                    == denotation::term(right.outputs@[index as int], environment));
                assert(denotation::term(equal, environment) ==
                    match (denotation::term(compared@.0, environment),
                        denotation::term(compared@.1, environment)) {
                        (Some(Value::Integer(a)), Some(Value::Integer(b))) => Some(Value::Boolean(a == b)),
                        (Some(Value::Boolean(a)), Some(Value::Boolean(b))) => Some(Value::Boolean(a == b)),
                        _ => None,
                    });
            }
        }
        #[cfg(verus_keep_ghost)]
        let before = verus_exec_expr!(Ghost(items@));
        items.push(equal);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, Value>| ready(*left, *right, environment) implies
                items_meaning(items@, index as nat + 1, *left, *right, environment) by {
                assert(items_meaning(before@, index as nat, *left, *right, environment));
                assert forall|i: int| 1 <= i <= index + 1 implies
                    denotation::term(#[trigger] items@[i], environment)
                        == Some(Value::Boolean(denotation::term(left.outputs@[i - 1], environment)
                            == denotation::term(right.outputs@[i - 1], environment))) by {
                    if i <= index {
                        assert(items@[i] == before@[i]);
                        assert(denotation::term(before@[i], environment)
                            == Some(Value::Boolean(denotation::term(left.outputs@[i - 1], environment)
                                == denotation::term(right.outputs@[i - 1], environment))));
                    }
                    else {
                        assert(i == index + 1);
                        assert(items@[i] == equal);
                    }
                }
            }
        }
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    let arguments = verus_exec_expr!(Ghost(items@));
    let result = Term::and(items);
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, Value>|
            ready(*left, *right, environment) && left.outputs@.len() == right.outputs@.len() implies
                denotation::term(result, environment) == Some(Value::Boolean(
                    returns_outputs(*right, environment) && equal_outputs(*left, *right, environment))) by {
            assert(index == left.outputs@.len());
            assert(items_meaning(arguments@, index as nat, *left, *right, environment));
            assert forall|i: int| 0 <= i < arguments@.len() implies
                match denotation::term(#[trigger] arguments@[i], environment) {
                    Some(Value::Boolean(_)) => true, _ => false,
                } by {}
            denotation::conjunction_truth(arguments@, result, environment);
            assert((forall|i: int| 0 <= i < arguments@.len() ==>
                denotation::term(#[trigger] arguments@[i], environment) == Some(Value::Boolean(true)))
                == (returns_outputs(*right, environment) && equal_outputs(*left, *right, environment))) by {
                if returns_outputs(*right, environment) && equal_outputs(*left, *right, environment) {
                    assert forall|i: int| 0 <= i < arguments@.len() implies
                        denotation::term(#[trigger] arguments@[i], environment) == Some(Value::Boolean(true)) by {
                        if i > 0 { assert(0 <= i - 1 < left.outputs@.len()); }
                    }
                }
                if forall|i: int| 0 <= i < arguments@.len() ==>
                    denotation::term(#[trigger] arguments@[i], environment) == Some(Value::Boolean(true)) {
                    assert(returns_outputs(*right, environment));
                    assert forall|i: int| 0 <= i < left.outputs@.len() implies
                        denotation::term(#[trigger] left.outputs@[i], environment)
                            == denotation::term(right.outputs@[i], environment) by {
                        assert(1 <= i + 1 <= index);
                        assert(denotation::term(arguments@[i + 1], environment) == Some(Value::Boolean(true)));
                    }
                }
            }
        }
    }
    result
}
