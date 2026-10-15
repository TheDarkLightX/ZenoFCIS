//! Erased mathematical meaning of the scalar SMT fragment over exact term data.
//! This is a specification, not a second executable program evaluator.
//! Runtime/encoder correspondence and SMT certificate checking are separate.
use super::Term;
use vstd::prelude::*;

verus! {
broadcast use {
    vstd::seq::lemma_seq_index_decreases,
    vstd::std_specs::vec::axiom_vec_decreases_to_view,
};

pub enum Value {
    Integer(int),
    Boolean(bool),
}

pub(crate) proof fn integer_literal(value: i128, environment: Map<Seq<char>, Value>)
    ensures term(Term::Int(value), environment) == Some(Value::Integer(value as int)),
{
    reveal_with_fuel(term, 1);
}

#[verifier::opaque]
pub open spec fn term(term: Term, environment: Map<Seq<char>, Value>) -> Option<Value>
    decreases term,
{
    match term {
        Term::Int(n) => Some(Value::Integer(n as int)),
        Term::Bool(b) => Some(Value::Boolean(b)),
        Term::Name(name) => if environment.contains_key(name@) { Some(environment[name@]) } else { None },
        Term::Add(a, b) => match (self::term(*a, environment), self::term(*b, environment)) {
            (Some(Value::Integer(x)), Some(Value::Integer(y))) => Some(Value::Integer(x + y)),
            _ => None,
        },
        Term::Sub(a, b) => match (self::term(*a, environment), self::term(*b, environment)) {
            (Some(Value::Integer(x)), Some(Value::Integer(y))) => Some(Value::Integer(x - y)),
            _ => None,
        },
        Term::Eq(a, b) => match (self::term(*a, environment), self::term(*b, environment)) {
            (Some(Value::Integer(x)), Some(Value::Integer(y))) => Some(Value::Boolean(x == y)),
            (Some(Value::Boolean(x)), Some(Value::Boolean(y))) => Some(Value::Boolean(x == y)),
            _ => None,
        },
        Term::Lt(a, b) => match (self::term(*a, environment), self::term(*b, environment)) {
            (Some(Value::Integer(x)), Some(Value::Integer(y))) => Some(Value::Boolean(x < y)),
            _ => None,
        },
        Term::Le(a, b) => match (self::term(*a, environment), self::term(*b, environment)) {
            (Some(Value::Integer(x)), Some(Value::Integer(y))) => Some(Value::Boolean(x <= y)),
            _ => None,
        },
        Term::Not(a) => match self::term(*a, environment) {
            Some(Value::Boolean(b)) => Some(Value::Boolean(!b)), _ => None,
        },
        Term::And(items) => junction(items@, items@.len(), environment, true),
        Term::Or(items) => junction(items@, items@.len(), environment, false),
        Term::Ite(c, a, b) => match self::term(*c, environment) {
            Some(Value::Boolean(true)) => self::term(*a, environment),
            Some(Value::Boolean(false)) => self::term(*b, environment),
            _ => None,
        },
        // These operators are not emitted by the profile-1 scalar encoder.
        Term::Mul(_, _) | Term::Div(_, _) => None,
    }
}

#[verifier::opaque]
pub open spec fn junction(items: Seq<Term>, count: nat,
    environment: Map<Seq<char>, Value>, conjunction: bool) -> Option<Value>
    recommends count <= items.len(),
    decreases items, count,
{
    if count == 0 { Some(Value::Boolean(conjunction)) }
    else if count > items.len() { None }
    else {
        match (junction(items, (count - 1) as nat, environment, conjunction),
            term(items[count as int - 1], environment)) {
            (Some(Value::Boolean(x)), Some(Value::Boolean(y))) =>
                Some(Value::Boolean(if conjunction { x && y } else { x || y })),
            _ => None,
        }
    }
}

/// The two-operand case of the actual folded conjunction constructor. Kept
/// with the term owner so callers need not expose its private fold machinery.
pub(crate) proof fn conjunction_pair(items: Seq<Term>, output: Term,
    environment: Map<Seq<char>, Value>)
    requires items.len() == 2,
        super::folding::folded(items, true, output),
        match (term(items[0], environment), term(items[1], environment)) {
            (Some(Value::Boolean(_)), Some(Value::Boolean(_))) => true, _ => false,
        },
    ensures term(output, environment) == match (term(items[0], environment), term(items[1], environment)) {
        (Some(Value::Boolean(left)), Some(Value::Boolean(right))) => Some(Value::Boolean(left && right)),
        _ => None,
    },
{
    assert forall|i: int| 0 <= i < items.len() implies
        match term(#[trigger] items[i], environment) {
            Some(Value::Boolean(_)) => true, _ => false,
        } by {
        if i != 0 { assert(i == 1); }
    }
    super::folding::folded_meaning(items, true, output, environment);
    broadcast use vstd::seq::group_seq_lemmas;
    reveal_with_fuel(junction, 1);
    super::folding::junction_push(Seq::empty(), items[0], environment, true);
    let first = Seq::empty().push(items[0]);
    super::folding::junction_push(first, items[1], environment, true);
    assert(items =~= first.push(items[1]));
}

/// Thin owner wrapper over the already proved private junction append lemma.
pub(crate) proof fn disjunction_push(items: Seq<Term>, item: Term,
    environment: Map<Seq<char>, Value>)
    ensures junction(items.push(item), items.len() + 1, environment, false) ==
        match (junction(items, items.len(), environment, false), term(item, environment)) {
            (Some(Value::Boolean(left)), Some(Value::Boolean(right))) =>
                Some(Value::Boolean(left || right)),
            _ => None,
        },
{
    super::folding::junction_push(items, item, environment, false);
}

/// The final actual Term::or call can use a public typed premise without
/// exposing the private folding module or weakening its constructor contract.
pub(crate) proof fn disjunction_meaning(items: Seq<Term>, output: Term,
    environment: Map<Seq<char>, Value>)
    requires super::folding::folded(items, false, output),
        forall|i: int| 0 <= i < items.len() ==>
            match term(#[trigger] items[i], environment) {
                Some(Value::Boolean(_)) => true, _ => false,
            },
    ensures term(output, environment) == junction(items, items.len(), environment, false),
{
    assert(super::folding::booleans(items, environment));
    super::folding::folded_meaning(items, false, output, environment);
}


/// Names the existing actual Term::and result contract without exposing folding.
pub(crate) open spec fn conjunction_result(items: Seq<Term>, output: Term) -> bool {
    super::folding::folded(items, true, output)
}

/// Thin owner wrapper; a Boolean-list premise is retained for partial denotation.
pub(crate) proof fn conjunction_meaning(items: Seq<Term>, output: Term,
    environment: Map<Seq<char>, Value>)
    requires conjunction_result(items, output),
        forall|i: int| 0 <= i < items.len() ==>
            match term(#[trigger] items[i], environment) {
                Some(Value::Boolean(_)) => true, _ => false,
            },
    ensures term(output, environment) == junction(items, items.len(), environment, true),
        match term(output, environment) { Some(Value::Boolean(_)) => true, _ => false },
{
    assert(super::folding::booleans(items, environment));
    super::folding::boolean_prefix(items, items.len(), environment, true);
    super::folding::folded_meaning(items, true, output, environment);
}

/// One step of the same-sequence fold; callers need not unfold its predecessor.
proof fn conjunction_step(items: Seq<Term>, count: nat,
    environment: Map<Seq<char>, Value>)
    requires 0 < count <= items.len(),
    ensures junction(items, count, environment, true) ==
        match (junction(items, (count - 1) as nat, environment, true),
            term(items[count as int - 1], environment)) {
            (Some(Value::Boolean(left)), Some(Value::Boolean(right))) =>
                Some(Value::Boolean(left && right)),
            _ => None,
        },
{
    broadcast use vstd::seq::group_seq_lemmas;
    let prefix = items.subrange(0, count as int - 1);
    let last = items[count as int - 1];
    assert forall|i: int| 0 <= i < count - 1 implies
        #[trigger] items[i] == prefix[i] by {}
    super::folding::junction_prefix(items, prefix, (count - 1) as nat, environment, true);
    let extended = prefix.push(last);
    assert forall|i: int| 0 <= i < count implies
        #[trigger] items[i] == extended[i] by {
        if i < count - 1 { assert(extended[i] == prefix[i]); }
        else { assert(i == count - 1); }
    }
    super::folding::junction_prefix(items, extended, count, environment, true);
    super::folding::junction_push(prefix, last, environment, true);
}

proof fn conjunction_true_prefix(items: Seq<Term>, count: nat,
    environment: Map<Seq<char>, Value>)
    requires count <= items.len(),
        forall|i: int| 0 <= i < count ==>
            match term(#[trigger] items[i], environment) {
                Some(Value::Boolean(_)) => true, _ => false,
            },
    ensures junction(items, count, environment, true) == Some(Value::Boolean(
        forall|i: int| 0 <= i < count ==>
            term(#[trigger] items[i], environment) == Some(Value::Boolean(true)))),
    decreases count,
{
    if count > 0 {
        let last = count as int - 1;
        assert(0 <= last < count);
        assert(match term(items[last], environment) {
            Some(Value::Boolean(_)) => true, _ => false,
        });
        conjunction_true_prefix(items, (count - 1) as nat, environment);
        conjunction_step(items, count, environment);
        let prefix_true = forall|i: int| 0 <= i < count - 1 ==>
            term(#[trigger] items[i], environment) == Some(Value::Boolean(true));
        assert(junction(items, (count - 1) as nat, environment, true)
            == Some(Value::Boolean(prefix_true)));
        assert((forall|i: int| 0 <= i < count ==>
            term(#[trigger] items[i], environment) == Some(Value::Boolean(true)))
            == ((forall|i: int| 0 <= i < count - 1 ==>
                term(#[trigger] items[i], environment) == Some(Value::Boolean(true)))
                && term(items[count as int - 1], environment) == Some(Value::Boolean(true)))) by {}
        if let Some(Value::Boolean(last_value)) = term(items[last], environment) {
            assert(junction(items, count, environment, true) == Some(Value::Boolean(
                prefix_true && last_value)));
        }
    } else {
        assert(count == 0);
        reveal_with_fuel(junction, 1);
    }
}

/// A defined folded conjunction is true exactly when every actual operand is.
pub(crate) proof fn conjunction_truth(items: Seq<Term>, output: Term,
    environment: Map<Seq<char>, Value>)
    requires conjunction_result(items, output),
        forall|i: int| 0 <= i < items.len() ==>
            match term(#[trigger] items[i], environment) {
                Some(Value::Boolean(_)) => true, _ => false,
            },
    ensures term(output, environment) == Some(Value::Boolean(
        forall|i: int| 0 <= i < items.len() ==>
            term(#[trigger] items[i], environment) == Some(Value::Boolean(true)))),
{
    conjunction_meaning(items, output, environment);
    conjunction_true_prefix(items, items.len(), environment);
}


}
