//! Route specialization preserves evaluation.
//!
//! The assembly validator compares an aggregate's decision program with each
//! leaf's by building canonical terms over the aggregate's input positions.
//! On route `k` the aggregate's target input becomes the constant `k`; a
//! leaf's inputs are renamed to the aggregate positions that carry them. The
//! term constructors below fold constants and apply a few identities. A term
//! means what the evaluator specification's `node` computes for its operator.
//!
//! Proved here: on every input the specification executes without failure,
//! each node's value is the value of its canonical term, so two outputs with
//! equal terms have equal values, and two selection chains with the same
//! conditions choose cases the validator maps to each other. The Rust
//! implementation that builds these terms is tested against this definition;
//! it is not proved.

use crate::evaluation::spec::{admitted, execution, node, prefix, projection};
use crate::evaluation::{Domain, Failure, Op};
use alloc::boxed::Box;
use vstd::prelude::*;

verus! {

/// A canonical term; an input is an aggregate input position.
pub(crate) enum Term {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(Box<Term>, Box<Term>),
    Sub(Box<Term>, Box<Term>),
    Mul(Box<Term>, Box<Term>),
    DivFloor(Box<Term>, Box<Term>),
    DivCeil(Box<Term>, Box<Term>),
    MulDivFloor(Box<Term>, Box<Term>, Box<Term>),
    MulDivCeil(Box<Term>, Box<Term>, Box<Term>),
    ProductsGe(Box<(Term, Term, Term)>, Box<(Term, Term, Term)>),
    Eq(Box<Term>, Box<Term>),
    Lt(Box<Term>, Box<Term>),
    And(Box<Term>, Box<Term>),
    Not(Box<Term>),
    Select(Box<Term>, Box<Term>, Box<Term>),
}

/// A binary operator of the evaluator on two operand results.
pub(crate) open spec fn binary(op: Op, left: Result<i64, Failure>, right: Result<i64, Failure>)
    -> Result<i64, Failure>
{
    match left {
        Err(error) => Err(error),
        Ok(a) => match right {
            Err(error) => Err(error),
            Ok(b) => node(op, Seq::empty(), seq![a, b]),
        },
    }
}

/// A three-operand D1 operator, retaining operand refusal order.
pub(crate) open spec fn ternary(op: Op, first: Result<i64, Failure>,
    second: Result<i64, Failure>, third: Result<i64, Failure>) -> Result<i64, Failure>
{
    match first {
        Err(error) => Err(error),
        Ok(a) => match second {
            Err(error) => Err(error),
            Ok(b) => match third {
                Err(error) => Err(error),
                Ok(c) => node(op, Seq::empty(), seq![a, b, c]),
            },
        },
    }
}

/// The value of a term on the aggregate's input `env`, operator by
/// operator as the evaluator specification computes it.
pub(crate) open spec fn denote(term: Term, env: Seq<i64>) -> Result<i64, Failure>
    decreases term,
{
    match term {
        Term::Input(id) => node(Op::Input(id), env, Seq::empty()),
        Term::Int(value) => node(Op::Int(value), env, Seq::empty()),
        Term::Bool(value) => node(Op::Bool(value), env, Seq::empty()),
        Term::Add(a, b) => binary(Op::Add(0, 1), denote(*a, env), denote(*b, env)),
        Term::Sub(a, b) => binary(Op::Sub(0, 1), denote(*a, env), denote(*b, env)),
        Term::Mul(a, b) => binary(Op::Mul(0, 1), denote(*a, env), denote(*b, env)),
        Term::DivFloor(a, b) => binary(Op::DivFloor(0, 1), denote(*a, env), denote(*b, env)),
        Term::DivCeil(a, b) => binary(Op::DivCeil(0, 1), denote(*a, env), denote(*b, env)),
        Term::MulDivFloor(a, b, d) => ternary(Op::MulDivFloor(0, 1, 2),
            denote(*a, env), denote(*b, env), denote(*d, env)),
        Term::MulDivCeil(a, b, d) => ternary(Op::MulDivCeil(0, 1, 2),
            denote(*a, env), denote(*b, env), denote(*d, env)),
        Term::ProductsGe(a, b) => match denote((*a).0, env) {
            Err(error) => Err(error),
            Ok(va) => match denote((*a).1, env) {
                Err(error) => Err(error),
                Ok(vb) => match denote((*a).2, env) {
                    Err(error) => Err(error),
                    Ok(vc) => match denote((*b).0, env) {
                        Err(error) => Err(error),
                        Ok(vd) => match denote((*b).1, env) {
                            Err(error) => Err(error),
                            Ok(ve) => match denote((*b).2, env) {
                                Err(error) => Err(error),
                                Ok(vf) => node(Op::ProductsGe([0, 1, 2], [3, 4, 5]),
                                    Seq::empty(), seq![va, vb, vc, vd, ve, vf]),
                            },
                        },
                    },
                },
            },
        },
        Term::Eq(a, b) => binary(Op::Eq(0, 1), denote(*a, env), denote(*b, env)),
        Term::Lt(a, b) => binary(Op::Lt(0, 1), denote(*a, env), denote(*b, env)),
        Term::And(a, b) => binary(Op::And(0, 1), denote(*a, env), denote(*b, env)),
        Term::Not(a) => match denote(*a, env) {
            Err(error) => Err(error),
            Ok(v) => node(Op::Not(0), Seq::empty(), seq![v]),
        },
        Term::Select(c, a, b) => match denote(*c, env) {
            Err(error) => Err(error),
            Ok(vc) => match denote(*a, env) {
                Err(error) => Err(error),
                Ok(va) => match denote(*b, env) {
                    Err(error) => Err(error),
                    Ok(vb) => node(Op::Select(0, 1, 2), Seq::empty(), seq![vc, va, vb]),
                },
            },
        },
    }
}

/// A term whose value is 0 or 1 wherever it is defined: a Boolean input, a
/// Boolean constant, a comparison, a conjunction, a negation, or a selection
/// between two such terms.
pub(crate) open spec fn boolean(term: Term, bools: spec_fn(u16) -> bool) -> bool
    decreases term,
{
    match term {
        Term::Input(id) => bools(id),
        Term::Int(_) => false,
        Term::Add(_, _) => false,
        Term::Sub(_, _) => false,
        Term::Mul(_, _) | Term::DivFloor(_, _) | Term::DivCeil(_, _)
            | Term::MulDivFloor(_, _, _) | Term::MulDivCeil(_, _, _) => false,
        Term::Select(_, a, b) => boolean(*a, bools) && boolean(*b, bools),
        _ => true,
    }
}

/// Every input position marked Boolean carries 0 or 1.
pub(crate) open spec fn booleans_hold(bools: spec_fn(u16) -> bool, env: Seq<i64>) -> bool {
    forall|id: u16| #[trigger] bools(id) && (id as int) < env.len() ==> 0 <= env[id as int] <= 1
}

// The constructors, in the validator's order of cases.

pub(crate) open spec fn add(a: Term, b: Term) -> Term {
    match (a, b) {
        (Term::Int(x), Term::Int(y)) => if i64::MIN <= x + y <= i64::MAX {
            Term::Int((x + y) as i64)
        } else {
            Term::Add(Box::new(a), Box::new(b))
        },
        _ => Term::Add(Box::new(a), Box::new(b)),
    }
}

pub(crate) open spec fn sub(a: Term, b: Term) -> Term {
    match (a, b) {
        (Term::Int(x), Term::Int(y)) => if i64::MIN <= x - y <= i64::MAX {
            Term::Int((x - y) as i64)
        } else {
            Term::Sub(Box::new(a), Box::new(b))
        },
        _ => Term::Sub(Box::new(a), Box::new(b)),
    }
}

pub(crate) open spec fn mul(a: Term, b: Term) -> Term {
    Term::Mul(Box::new(a), Box::new(b))
}

pub(crate) open spec fn div_floor(a: Term, b: Term) -> Term {
    Term::DivFloor(Box::new(a), Box::new(b))
}

pub(crate) open spec fn div_ceil(a: Term, b: Term) -> Term {
    Term::DivCeil(Box::new(a), Box::new(b))
}

pub(crate) open spec fn mul_div_floor(a: Term, b: Term, d: Term) -> Term {
    Term::MulDivFloor(Box::new(a), Box::new(b), Box::new(d))
}

pub(crate) open spec fn mul_div_ceil(a: Term, b: Term, d: Term) -> Term {
    Term::MulDivCeil(Box::new(a), Box::new(b), Box::new(d))
}

pub(crate) open spec fn products_ge(a: Term, b: Term, c: Term, d: Term, e: Term, f: Term) -> Term {
    Term::ProductsGe(Box::new((a, b, c)), Box::new((d, e, f)))
}

pub(crate) open spec fn eq(a: Term, b: Term) -> Term {
    if a == b {
        Term::Bool(true)
    } else {
        match (a, b) {
            (Term::Int(x), Term::Int(y)) => Term::Bool(x == y),
            (Term::Bool(x), Term::Bool(y)) => Term::Bool(x == y),
            _ => Term::Eq(Box::new(a), Box::new(b)),
        }
    }
}

pub(crate) open spec fn lt(a: Term, b: Term) -> Term {
    match (a, b) {
        (Term::Int(x), Term::Int(y)) => Term::Bool(x < y),
        _ => if a == b { Term::Bool(false) } else { Term::Lt(Box::new(a), Box::new(b)) },
    }
}

/// `true && x` is `x` only for a Boolean `x`: the evaluator's conjunction
/// is 1 exactly when both operands are 1.
pub(crate) open spec fn and(a: Term, b: Term, bools: spec_fn(u16) -> bool) -> Term {
    if a == Term::Bool(false) || b == Term::Bool(false) {
        Term::Bool(false)
    } else if a == Term::Bool(true) && boolean(b, bools) {
        b
    } else if b == Term::Bool(true) && boolean(a, bools) {
        a
    } else if a == b && boolean(a, bools) {
        a
    } else {
        Term::And(Box::new(a), Box::new(b))
    }
}

pub(crate) open spec fn not(a: Term, bools: spec_fn(u16) -> bool) -> Term {
    match a {
        Term::Bool(x) => Term::Bool(!x),
        Term::Not(inner) => if boolean(*inner, bools) { *inner } else { Term::Not(Box::new(a)) },
        _ => Term::Not(Box::new(a)),
    }
}

pub(crate) open spec fn select(c: Term, a: Term, b: Term) -> Term {
    if c == Term::Bool(true) {
        a
    } else if c == Term::Bool(false) {
        b
    } else if a == b {
        a
    } else {
        Term::Select(Box::new(c), Box::new(a), Box::new(b))
    }
}

pub(crate) open spec fn operand(terms: Seq<Term>, index: u16) -> Term {
    if (index as int) < terms.len() { terms[index as int] } else { Term::Bool(false) }
}

/// The canonical term of one node, given its operands' terms; `input` maps
/// the program's input positions to terms.
pub(crate) open spec fn canonical_node(op: Op, input: spec_fn(u16) -> Term,
    bools: spec_fn(u16) -> bool, terms: Seq<Term>) -> Term
{
    match op {
        Op::Input(id) => input(id),
        Op::Int(value) => Term::Int(value),
        Op::Bool(value) => Term::Bool(value),
        Op::Add(a, b) => add(operand(terms, a), operand(terms, b)),
        Op::Sub(a, b) => sub(operand(terms, a), operand(terms, b)),
        Op::Mul(a, b) => mul(operand(terms, a), operand(terms, b)),
        Op::DivFloor(a, b) => div_floor(operand(terms, a), operand(terms, b)),
        Op::DivCeil(a, b) => div_ceil(operand(terms, a), operand(terms, b)),
        Op::MulDivFloor(a, b, d) => mul_div_floor(operand(terms, a), operand(terms, b), operand(terms, d)),
        Op::MulDivCeil(a, b, d) => mul_div_ceil(operand(terms, a), operand(terms, b), operand(terms, d)),
        Op::ProductsGe(a, b) => products_ge(operand(terms, a[0]), operand(terms, a[1]),
            operand(terms, a[2]), operand(terms, b[0]), operand(terms, b[1]), operand(terms, b[2])),
        Op::Eq(a, b) => eq(operand(terms, a), operand(terms, b)),
        Op::Lt(a, b) => lt(operand(terms, a), operand(terms, b)),
        Op::And(a, b) => and(operand(terms, a), operand(terms, b), bools),
        Op::Not(a) => not(operand(terms, a), bools),
        Op::Select(c, a, b) => select(operand(terms, c), operand(terms, a), operand(terms, b)),
    }
}

/// The canonical terms of the first `count` nodes.
pub(crate) open spec fn canonical(nodes: Seq<Op>, input: spec_fn(u16) -> Term,
    bools: spec_fn(u16) -> bool, count: nat) -> Seq<Term>
    decreases count,
{
    if count == 0 {
        Seq::empty()
    } else {
        let terms = canonical(nodes, input, bools, (count - 1) as nat);
        terms.push(canonical_node(nodes[count - 1], input, bools, terms))
    }
}

/// Route `route` of the aggregate: its target input is the route constant.
pub(crate) open spec fn specialized(target: u16, route: i64) -> spec_fn(u16) -> Term {
    |id: u16| if id == target { Term::Int(route) } else { Term::Input(id) }
}

/// A leaf's input `j` is the aggregate's input `position[j]`.
pub(crate) open spec fn renamed(position: Seq<u16>) -> spec_fn(u16) -> Term {
    |id: u16| if (id as int) < position.len() { Term::Input(position[id as int]) } else { Term::Bool(false) }
}

/// The aggregate's Boolean input positions.
pub(crate) open spec fn boolean_inputs(inputs: Seq<Domain>) -> spec_fn(u16) -> bool {
    |id: u16| (id as int) < inputs.len() && inputs[id as int] is Bool
}

/// The case pairs of two selection chains with the same conditions: each
/// aggregate case number with the leaf case number in the same position.
pub(crate) open spec fn selection_pairs(a: Term, l: Term) -> Option<Seq<(i64, i64)>>
    decreases a,
{
    match a {
        Term::Int(x) => match l {
            Term::Int(y) => Some(seq![(x, y)]),
            _ => None,
        },
        Term::Select(c, t, e) => match l {
            Term::Select(c2, t2, e2) => if *c == *c2 {
                match selection_pairs(*t, *t2) {
                    Some(p) => match selection_pairs(*e, *e2) {
                        Some(q) => Some(p + q),
                        None => None,
                    },
                    None => None,
                }
            } else {
                None
            },
            _ => None,
        },
        _ => None,
    }
}

/// The pairs map each aggregate case to one leaf case.
pub(crate) open spec fn functional(pairs: Seq<(i64, i64)>) -> bool {
    forall|p: int, q: int| 0 <= p < pairs.len() && 0 <= q < pairs.len()
        && (#[trigger] pairs[p]).0 == (#[trigger] pairs[q]).0 ==> pairs[p].1 == pairs[q].1
}

/// Boolean terms take the values 0 and 1.
pub(crate) proof fn boolean_values(term: Term, env: Seq<i64>, bools: spec_fn(u16) -> bool, value: i64)
    requires
        boolean(term, bools),
        booleans_hold(bools, env),
        denote(term, env) == Ok::<i64, Failure>(value),
    ensures 0 <= value <= 1,
    decreases term,
{
    hide(denote);
    reveal_with_fuel(denote, 1);
    match term {
        Term::Input(id) => {
            assert(bools(id) && (id as int) < env.len());
        },
        Term::Int(_) => {},
        Term::Bool(_) => {},
        Term::Add(_, _) => {},
        Term::Sub(_, _) => {},
        Term::Mul(_, _) | Term::DivFloor(_, _) | Term::DivCeil(_, _)
            | Term::MulDivFloor(_, _, _) | Term::MulDivCeil(_, _, _) => {},
        Term::ProductsGe(a, b) => {
            match (denote((*a).0, env), denote((*a).1, env), denote((*a).2, env),
                denote((*b).0, env), denote((*b).1, env), denote((*b).2, env)) {
                (Ok(va), Ok(vb), Ok(vc), Ok(vd), Ok(ve), Ok(vf)) => {
                    let operands = seq![va, vb, vc, vd, ve, vf];
                    assert(operands[0] == va && operands[1] == vb && operands[2] == vc
                        && operands[3] == vd && operands[4] == ve && operands[5] == vf);
                },
                _ => {},
            }
        },
        Term::Eq(a, b) => {
            match denote(*a, env) {
                Ok(va) => match denote(*b, env) {
                    Ok(vb) => { assert(seq![va, vb][0] == va && seq![va, vb][1] == vb); },
                    Err(_) => {},
                },
                Err(_) => {},
            }
        },
        Term::Lt(a, b) => {
            match denote(*a, env) {
                Ok(va) => match denote(*b, env) {
                    Ok(vb) => { assert(seq![va, vb][0] == va && seq![va, vb][1] == vb); },
                    Err(_) => {},
                },
                Err(_) => {},
            }
        },
        Term::And(a, b) => {
            match denote(*a, env) {
                Ok(va) => match denote(*b, env) {
                    Ok(vb) => { assert(seq![va, vb][0] == va && seq![va, vb][1] == vb); },
                    Err(_) => {},
                },
                Err(_) => {},
            }
        },
        Term::Not(a) => {
            match denote(*a, env) {
                Ok(va) => { assert(seq![va][0] == va); },
                Err(_) => {},
            }
        },
        Term::Select(c, a, b) => {
            match denote(*c, env) {
                Ok(vc) => match denote(*a, env) {
                    Ok(va) => match denote(*b, env) {
                        Ok(vb) => {
                            assert(seq![vc, va, vb][0] == vc && seq![vc, va, vb][1] == va
                                && seq![vc, va, vb][2] == vb);
                            boolean_values(*a, env, bools, va);
                            boolean_values(*b, env, bools, vb);
                        },
                        Err(_) => {},
                    },
                    Err(_) => {},
                },
                Err(_) => {},
            }
        },
    }
}

/// Constant folds and selection identities retain the evaluator's values.
pub(crate) proof fn folded_constructors_preserve_values(a: Term, b: Term, c: Term,
    env: Seq<i64>, x: i64, y: i64, z: i64)
    requires
        denote(a, env) == Ok::<i64, Failure>(x),
        denote(b, env) == Ok::<i64, Failure>(y),
        denote(c, env) == Ok::<i64, Failure>(z),
    ensures
        denote(add(a, b), env) == node(Op::Add(0, 1), Seq::empty(), seq![x, y]),
        denote(sub(a, b), env) == node(Op::Sub(0, 1), Seq::empty(), seq![x, y]),
        denote(eq(a, b), env) == node(Op::Eq(0, 1), Seq::empty(), seq![x, y]),
        denote(lt(a, b), env) == node(Op::Lt(0, 1), Seq::empty(), seq![x, y]),
        denote(select(c, a, b), env) == node(Op::Select(0, 1, 2), Seq::empty(), seq![z, x, y]),
{
    hide(denote);
    reveal_with_fuel(denote, 1);
    assert(seq![x, y][0] == x && seq![x, y][1] == y);
    assert(seq![z, x, y][0] == z && seq![z, x, y][1] == x && seq![z, x, y][2] == y);
}

/// D1 constructors retain their operands and delegate to the exact D1 node.
pub(crate) proof fn arithmetic_constructors_preserve_values(a: Term, b: Term, c: Term,
    env: Seq<i64>, x: i64, y: i64, z: i64)
    requires
        denote(a, env) == Ok::<i64, Failure>(x),
        denote(b, env) == Ok::<i64, Failure>(y),
        denote(c, env) == Ok::<i64, Failure>(z),
    ensures
        denote(mul(a, b), env) == node(Op::Mul(0, 1), Seq::empty(), seq![x, y]),
        denote(div_floor(a, b), env) == node(Op::DivFloor(0, 1), Seq::empty(), seq![x, y]),
        denote(div_ceil(a, b), env) == node(Op::DivCeil(0, 1), Seq::empty(), seq![x, y]),
        denote(mul_div_floor(a, b, c), env) == node(Op::MulDivFloor(0, 1, 2), Seq::empty(), seq![x, y, z]),
        denote(mul_div_ceil(a, b, c), env) == node(Op::MulDivCeil(0, 1, 2), Seq::empty(), seq![x, y, z]),
{
    // Operand success is already known; no arithmetic expansion is needed.
    hide(node);
    hide(denote);
    reveal_with_fuel(denote, 1);
}

/// Double negation retains its operand only when that operand is Boolean.
pub(crate) proof fn negation_preserves_value(a: Term, env: Seq<i64>,
    bools: spec_fn(u16) -> bool, x: i64)
    requires
        booleans_hold(bools, env),
        denote(a, env) == Ok::<i64, Failure>(x),
    ensures
        denote(not(a, bools), env) == node(Op::Not(0), Seq::empty(), seq![x]),
{
    hide(denote);
    reveal_with_fuel(denote, 1);
    assert(seq![x][0] == x);
    match a {
        Term::Not(inner) => {
            if boolean(*inner, bools) {
                match denote(*inner, env) {
                    Ok(w) => {
                        boolean_values(*inner, env, bools, w);
                        assert(seq![w][0] == w);
                    },
                    Err(_) => {},
                }
            }
        },
        _ => {},
    }
}

/// Each constructor gives the value the evaluator computes for its
/// operator on the operands' values.
pub(crate) proof fn constructors_preserve_values(a: Term, b: Term, c: Term, env: Seq<i64>,
    bools: spec_fn(u16) -> bool, x: i64, y: i64, z: i64)
    requires
        booleans_hold(bools, env),
        denote(a, env) == Ok::<i64, Failure>(x),
        denote(b, env) == Ok::<i64, Failure>(y),
        denote(c, env) == Ok::<i64, Failure>(z),
    ensures
        denote(add(a, b), env) == node(Op::Add(0, 1), Seq::empty(), seq![x, y]),
        denote(sub(a, b), env) == node(Op::Sub(0, 1), Seq::empty(), seq![x, y]),
        denote(mul(a, b), env) == node(Op::Mul(0, 1), Seq::empty(), seq![x, y]),
        denote(div_floor(a, b), env) == node(Op::DivFloor(0, 1), Seq::empty(), seq![x, y]),
        denote(div_ceil(a, b), env) == node(Op::DivCeil(0, 1), Seq::empty(), seq![x, y]),
        denote(mul_div_floor(a, b, c), env) == node(Op::MulDivFloor(0, 1, 2), Seq::empty(), seq![x, y, z]),
        denote(mul_div_ceil(a, b, c), env) == node(Op::MulDivCeil(0, 1, 2), Seq::empty(), seq![x, y, z]),
        denote(eq(a, b), env) == node(Op::Eq(0, 1), Seq::empty(), seq![x, y]),
        denote(lt(a, b), env) == node(Op::Lt(0, 1), Seq::empty(), seq![x, y]),
        denote(and(a, b, bools), env) == node(Op::And(0, 1), Seq::empty(), seq![x, y]),
        denote(not(a, bools), env) == node(Op::Not(0), Seq::empty(), seq![x]),
        denote(select(c, a, b), env) == node(Op::Select(0, 1, 2), Seq::empty(), seq![z, x, y]),
{
    hide(node);
    hide(denote);
    // When no Boolean identity applies, retain the explicit conjunction.
    if a != Term::Bool(false) && b != Term::Bool(false)
        && !(a == Term::Bool(true) && boolean(b, bools))
        && !(b == Term::Bool(true) && boolean(a, bools))
        && !(a == b && boolean(a, bools)) {
        assert(and(a, b, bools) == Term::And(Box::new(a), Box::new(b)));
    }
    folded_constructors_preserve_values(a, b, c, env, x, y, z);
    arithmetic_constructors_preserve_values(a, b, c, env, x, y, z);
    negation_preserves_value(a, env, bools, x);
    // Keep the Boolean conjunction obligation here, including the gate's
    // deliberately invalid unguarded-conjunction control.
    assert(denote(and(a, b, bools), env) == node(Op::And(0, 1), Seq::empty(), seq![x, y])) by {
        reveal_with_fuel(denote, 1);
        reveal(node);
        assert(seq![x, y][0] == x && seq![x, y][1] == y);
        if a == Term::Bool(true) && boolean(b, bools) {
            boolean_values(b, env, bools, y);
        }
        if b == Term::Bool(true) && boolean(a, bools) {
            boolean_values(a, env, bools, x);
        }
        if a == b && boolean(a, bools) {
            boolean_values(a, env, bools, x);
        }
    }
}

/// All six product factors retain their values before D1 compares the exact products.
pub(crate) proof fn products_ge_preserves_values(a: Term, b: Term, c: Term,
    d: Term, e: Term, f: Term, env: Seq<i64>, u: i64, v: i64, w: i64, x: i64, y: i64, z: i64)
    requires
        denote(a, env) == Ok::<i64, Failure>(u),
        denote(b, env) == Ok::<i64, Failure>(v),
        denote(c, env) == Ok::<i64, Failure>(w),
        denote(d, env) == Ok::<i64, Failure>(x),
        denote(e, env) == Ok::<i64, Failure>(y),
        denote(f, env) == Ok::<i64, Failure>(z),
    ensures
        denote(products_ge(a, b, c, d, e, f), env) ==
            node(Op::ProductsGe([0, 1, 2], [3, 4, 5]), Seq::empty(), seq![u, v, w, x, y, z]),
{
    let operands = seq![u, v, w, x, y, z];
    assert(operands[0] == u && operands[1] == v && operands[2] == w
        && operands[3] == x && operands[4] == y && operands[5] == z);
}

/// A node of the program on its operand values is the same operator on
/// canonical operand positions, including all six product factors.
pub(crate) proof fn node_on_operands(op: Op, input: Seq<i64>, values: Seq<i64>)
    ensures
        match op {
            Op::Add(a, b) | Op::Sub(a, b) | Op::Mul(a, b) | Op::DivFloor(a, b)
                | Op::DivCeil(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) =>
                (a as int) < values.len() && (b as int) < values.len() ==>
                node(op, input, values) == node(
                    match op {
                        Op::Add(_, _) => Op::Add(0, 1),
                        Op::Sub(_, _) => Op::Sub(0, 1),
                        Op::Mul(_, _) => Op::Mul(0, 1),
                        Op::DivFloor(_, _) => Op::DivFloor(0, 1),
                        Op::DivCeil(_, _) => Op::DivCeil(0, 1),
                        Op::Eq(_, _) => Op::Eq(0, 1),
                        Op::Lt(_, _) => Op::Lt(0, 1),
                        _ => Op::And(0, 1),
                    },
                    Seq::empty(), seq![values[a as int], values[b as int]]),
            Op::Not(a) => (a as int) < values.len() ==>
                node(op, input, values) == node(Op::Not(0), Seq::empty(), seq![values[a as int]]),
            Op::Select(c, a, b) =>
                (c as int) < values.len() && (a as int) < values.len() && (b as int) < values.len() ==>
                node(op, input, values) == node(Op::Select(0, 1, 2), Seq::empty(),
                    seq![values[c as int], values[a as int], values[b as int]]),
            Op::MulDivFloor(a, b, d) | Op::MulDivCeil(a, b, d) =>
                (a as int) < values.len() && (b as int) < values.len() && (d as int) < values.len() ==>
                node(op, input, values) == node(
                    if op is MulDivCeil { Op::MulDivCeil(0, 1, 2) } else { Op::MulDivFloor(0, 1, 2) },
                    Seq::empty(), seq![values[a as int], values[b as int], values[d as int]]),
            Op::ProductsGe(a, b) =>
                (a[0] as int) < values.len() && (a[1] as int) < values.len()
                    && (a[2] as int) < values.len() && (b[0] as int) < values.len()
                    && (b[1] as int) < values.len() && (b[2] as int) < values.len() ==>
                node(op, input, values) == node(Op::ProductsGe([0, 1, 2], [3, 4, 5]),
                    Seq::empty(), seq![values[a[0] as int], values[a[1] as int], values[a[2] as int],
                        values[b[0] as int], values[b[1] as int], values[b[2] as int]]),
            _ => true,
        },
{
    match op {
        Op::Add(a, b) | Op::Sub(a, b) | Op::Mul(a, b) | Op::DivFloor(a, b)
            | Op::DivCeil(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
            let operands = seq![values[a as int], values[b as int]];
            assert(operands[0] == values[a as int] && operands[1] == values[b as int]);
        },
        Op::Not(a) => {
            let operands = seq![values[a as int]];
            assert(operands[0] == values[a as int]);
        },
        Op::Select(c, a, b) => {
            let operands = seq![values[c as int], values[a as int], values[b as int]];
            assert(operands[0] == values[c as int] && operands[1] == values[a as int]
                && operands[2] == values[b as int]);
        },
        Op::MulDivFloor(a, b, d) | Op::MulDivCeil(a, b, d) => {
            let operands = seq![values[a as int], values[b as int], values[d as int]];
            assert(operands[0] == values[a as int] && operands[1] == values[b as int]
                && operands[2] == values[d as int]);
        },
        Op::ProductsGe(a, b) => {
            let operands = seq![values[a[0] as int], values[a[1] as int], values[a[2] as int],
                values[b[0] as int], values[b[1] as int], values[b[2] as int]];
            assert(operands[0] == values[a[0] as int] && operands[1] == values[a[1] as int]
                && operands[2] == values[a[2] as int] && operands[3] == values[b[0] as int]
                && operands[4] == values[b[1] as int] && operands[5] == values[b[2] as int]);
        },
        _ => {},
    }
}

/// Node `i`'s canonical term has the node's value.
pub(crate) open spec fn node_value(terms: Seq<Term>, env: Seq<i64>, values: Seq<i64>, i: int) -> bool {
    denote(terms[i], env) == Ok::<i64, Failure>(values[i])
}

/// On an input the specification evaluates without failure, every node's
/// value is the value of its canonical term.
pub(crate) proof fn canonical_terms_have_node_values(nodes: Seq<Op>, input: Seq<i64>,
    env: Seq<i64>, inputs: spec_fn(u16) -> Term, bools: spec_fn(u16) -> bool, count: nat,
    values: Seq<i64>)
    requires
        count <= nodes.len(),
        prefix(nodes, input, count) == Ok::<Seq<i64>, Failure>(values),
        forall|id: u16| (id as int) < input.len()
            ==> denote(#[trigger] inputs(id), env) == Ok::<i64, Failure>(input[id as int]),
        booleans_hold(bools, env),
    ensures
        values.len() == count,
        canonical(nodes, inputs, bools, count).len() == count,
        forall|i: int| 0 <= i < count
            ==> #[trigger] node_value(canonical(nodes, inputs, bools, count), env, values, i),
    decreases count,
{
    if count > 0 {
        let last = (count - 1) as nat;
        match prefix(nodes, input, last) {
            Err(_) => {},
            Ok(previous) => {
                canonical_terms_have_node_values(nodes, input, env, inputs, bools, last, previous);
                let terms = canonical(nodes, inputs, bools, last);
                let op = nodes[last as int];
                match node(op, input, previous) {
                    Err(_) => {},
                    Ok(v) => {
                        assert(values == previous.push(v));
                        let term = canonical_node(op, inputs, bools, terms);
                        assert(canonical(nodes, inputs, bools, count) == terms.push(term));
                        node_on_operands(op, input, previous);
                        match op {
                            Op::Input(id) => {},
                            Op::Int(value) => {},
                            Op::Bool(value) => {},
                            Op::Add(a, b) | Op::Sub(a, b) | Op::Mul(a, b) | Op::DivFloor(a, b)
                                | Op::DivCeil(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
                                assert(node_value(terms, env, previous, a as int));
                                assert(node_value(terms, env, previous, b as int));
                                constructors_preserve_values(terms[a as int], terms[b as int], terms[b as int],
                                    env, bools, previous[a as int], previous[b as int], previous[b as int]);
                            },
                            Op::Not(a) => {
                                assert(node_value(terms, env, previous, a as int));
                                constructors_preserve_values(terms[a as int], terms[a as int], terms[a as int],
                                    env, bools, previous[a as int], previous[a as int], previous[a as int]);
                            },
                            Op::Select(c, a, b) => {
                                assert(node_value(terms, env, previous, c as int));
                                assert(node_value(terms, env, previous, a as int));
                                assert(node_value(terms, env, previous, b as int));
                                constructors_preserve_values(terms[a as int], terms[b as int], terms[c as int],
                                    env, bools, previous[a as int], previous[b as int], previous[c as int]);
                            },
                            Op::MulDivFloor(a, b, d) | Op::MulDivCeil(a, b, d) => {
                                assert(node_value(terms, env, previous, a as int));
                                assert(node_value(terms, env, previous, b as int));
                                assert(node_value(terms, env, previous, d as int));
                                constructors_preserve_values(terms[a as int], terms[b as int], terms[d as int],
                                    env, bools, previous[a as int], previous[b as int], previous[d as int]);
                            },
                            Op::ProductsGe(a, b) => {
                                assert(node_value(terms, env, previous, a[0] as int));
                                assert(node_value(terms, env, previous, a[1] as int));
                                assert(node_value(terms, env, previous, a[2] as int));
                                assert(node_value(terms, env, previous, b[0] as int));
                                assert(node_value(terms, env, previous, b[1] as int));
                                assert(node_value(terms, env, previous, b[2] as int));
                                products_ge_preserves_values(terms[a[0] as int], terms[a[1] as int],
                                    terms[a[2] as int], terms[b[0] as int], terms[b[1] as int], terms[b[2] as int],
                                    env, previous[a[0] as int], previous[a[1] as int], previous[a[2] as int],
                                    previous[b[0] as int], previous[b[1] as int], previous[b[2] as int]);
                            },
                        }
                        assert(denote(term, env) == Ok::<i64, Failure>(v));
                        assert forall|i: int| 0 <= i < count
                            implies #[trigger] node_value(canonical(nodes, inputs, bools, count), env, values, i) by {
                            if i < last {
                                assert(node_value(terms, env, previous, i));
                                assert(canonical(nodes, inputs, bools, count)[i] == terms[i]);
                                assert(values[i] == previous[i]);
                            } else {
                                assert(canonical(nodes, inputs, bools, count)[i] == term);
                                assert(values[i] == v);
                            }
                        }
                    },
                }
            },
        }
    }
}

/// Output `i` of the specification's projection is root `i`'s value.
pub(crate) open spec fn reads_root(values: Seq<i64>, roots: Seq<u16>, output: Seq<i64>, i: int) -> bool {
    (roots[i] as int) < values.len() && output[i] == values[roots[i] as int]
}

/// The specification's projection reads each root's value.
pub(crate) proof fn projection_reads_roots(values: Seq<i64>, roots: Seq<u16>, count: nat,
    output: Seq<i64>)
    requires
        count <= roots.len(),
        projection(values, roots, count) == Ok::<Seq<i64>, Failure>(output),
    ensures
        output.len() == count,
        forall|i: int| 0 <= i < count ==> #[trigger] reads_root(values, roots, output, i),
    decreases count,
{
    if count > 0 {
        let last = (count - 1) as nat;
        match projection(values, roots, last) {
            Err(_) => {},
            Ok(previous) => {
                projection_reads_roots(values, roots, last, previous);
                assert(output == previous.push(values[roots[last as int] as int]));
                assert forall|i: int| 0 <= i < count implies #[trigger] reads_root(values, roots, output, i) by {
                    if i < last {
                        assert(reads_root(values, roots, previous, i));
                    }
                }
            },
        }
    }
}

/// Output `i` is the value of the canonical term at root `i`.
pub(crate) open spec fn root_value(terms: Seq<Term>, roots: Seq<u16>, env: Seq<i64>,
    result: Seq<i64>, i: int) -> bool
{
    (roots[i] as int) < terms.len() && denote(terms[roots[i] as int], env) == Ok::<i64, Failure>(result[i])
}

/// An execution's outputs are the canonical terms' values at its roots.
pub(crate) proof fn outputs_are_root_terms(inputs: Seq<Domain>, outputs: Seq<Domain>,
    nodes: Seq<Op>, roots: Seq<u16>, input: Seq<i64>, env: Seq<i64>,
    terms: spec_fn(u16) -> Term, bools: spec_fn(u16) -> bool, result: Seq<i64>)
    requires
        execution(inputs, outputs, nodes, roots, input) == Ok::<Seq<i64>, Failure>(result),
        forall|id: u16| (id as int) < input.len()
            ==> denote(#[trigger] terms(id), env) == Ok::<i64, Failure>(input[id as int]),
        booleans_hold(bools, env),
    ensures
        admitted(inputs, input),
        result.len() == roots.len(),
        forall|i: int| 0 <= i < roots.len()
            ==> #[trigger] root_value(canonical(nodes, terms, bools, nodes.len()), roots, env, result, i),
{
    match prefix(nodes, input, nodes.len()) {
        Err(_) => {},
        Ok(values) => {
            canonical_terms_have_node_values(nodes, input, env, terms, bools, nodes.len(), values);
            let canonical_terms = canonical(nodes, terms, bools, nodes.len());
            match projection(values, roots, roots.len()) {
                Err(_) => {},
                Ok(output) => {
                    projection_reads_roots(values, roots, roots.len(), output);
                    assert(output == result);
                    assert forall|i: int| 0 <= i < roots.len()
                        implies #[trigger] root_value(canonical_terms, roots, env, result, i) by {
                        assert(reads_root(values, roots, output, i));
                        assert(node_value(canonical_terms, env, values, roots[i] as int));
                    }
                },
            }
        },
    }
}

/// Admitted Boolean inputs carry 0 or 1.
pub(crate) proof fn admitted_booleans(inputs: Seq<Domain>, env: Seq<i64>)
    requires admitted(inputs, env),
    ensures booleans_hold(boolean_inputs(inputs), env),
{
    assert forall|id: u16| #[trigger] boolean_inputs(inputs)(id) && (id as int) < env.len()
        implies 0 <= env[id as int] <= 1 by {
        assert(crate::evaluation::spec::contains(inputs[id as int], env[id as int]));
    }
}

/// Route specialization preserves evaluation: when the specification
/// executes the aggregate's program on an input whose target is `route`,
/// each output is the value of its route-specialized canonical term.
pub(crate) proof fn route_specialization_preserves_evaluation(inputs: Seq<Domain>,
    outputs: Seq<Domain>, nodes: Seq<Op>, roots: Seq<u16>, x: Seq<i64>, target: u16,
    route: i64, result: Seq<i64>)
    requires
        execution(inputs, outputs, nodes, roots, x) == Ok::<Seq<i64>, Failure>(result),
        (target as int) < x.len(),
        x[target as int] == route,
    ensures
        result.len() == roots.len(),
        forall|i: int| 0 <= i < roots.len() ==> #[trigger] root_value(
            canonical(nodes, specialized(target, route), boolean_inputs(inputs), nodes.len()),
            roots, x, result, i),
{
    assert(admitted(inputs, x));
    admitted_booleans(inputs, x);
    assert forall|id: u16| (id as int) < x.len() implies
        denote(#[trigger] specialized(target, route)(id), x) == Ok::<i64, Failure>(x[id as int]) by {}
    outputs_are_root_terms(inputs, outputs, nodes, roots, x, x, specialized(target, route),
        boolean_inputs(inputs), result);
}

/// The leaf's input is the aggregate's input read at the leaf's positions.
pub(crate) open spec fn reads_from(y: Seq<i64>, x: Seq<i64>, position: Seq<u16>) -> bool {
    position.len() == y.len() && forall|j: int| #![trigger position[j]] 0 <= j < y.len()
        ==> (position[j] as int) < x.len() && x[position[j] as int] == y[j]
}

/// Renaming preserves evaluation: when the specification executes a leaf's
/// program on the aggregate input read at the leaf's positions, each output
/// is the value of its renamed canonical term on the aggregate input.
pub(crate) proof fn renaming_preserves_evaluation(inputs: Seq<Domain>, x: Seq<i64>,
    leaf_inputs: Seq<Domain>, leaf_outputs: Seq<Domain>, leaf_nodes: Seq<Op>,
    leaf_roots: Seq<u16>, y: Seq<i64>, position: Seq<u16>, result: Seq<i64>)
    requires
        admitted(inputs, x),
        reads_from(y, x, position),
        execution(leaf_inputs, leaf_outputs, leaf_nodes, leaf_roots, y) == Ok::<Seq<i64>, Failure>(result),
    ensures
        result.len() == leaf_roots.len(),
        forall|i: int| 0 <= i < leaf_roots.len() ==> #[trigger] root_value(
            canonical(leaf_nodes, renamed(position), boolean_inputs(inputs), leaf_nodes.len()),
            leaf_roots, x, result, i),
{
    admitted_booleans(inputs, x);
    assert forall|id: u16| (id as int) < y.len() implies
        denote(#[trigger] renamed(position)(id), x) == Ok::<i64, Failure>(y[id as int]) by {
        assert((position[id as int] as int) < x.len() && x[position[id as int] as int] == y[id as int]);
    }
    outputs_are_root_terms(leaf_inputs, leaf_outputs, leaf_nodes, leaf_roots, y, x,
        renamed(position), boolean_inputs(inputs), result);
}

/// A selection term's value from its three parts' values.
pub(crate) proof fn select_value(c: Term, t: Term, e: Term, env: Seq<i64>, value: i64)
    requires denote(Term::Select(Box::new(c), Box::new(t), Box::new(e)), env) == Ok::<i64, Failure>(value),
    ensures
        denote(c, env) is Ok && denote(t, env) is Ok && denote(e, env) is Ok,
        value == if denote(c, env)->Ok_0 == 1 { denote(t, env)->Ok_0 } else { denote(e, env)->Ok_0 },
{
    match denote(c, env) {
        Err(_) => {},
        Ok(vc) => match denote(t, env) {
            Err(_) => {},
            Ok(vt) => match denote(e, env) {
                Err(_) => {},
                Ok(ve) => {
                    let three = seq![vc, vt, ve];
                    assert(three[0] == vc && three[1] == vt && three[2] == ve);
                },
            },
        },
    }
}

/// The pair `(va, vl)` is among the pairs.
pub(crate) open spec fn paired(pairs: Seq<(i64, i64)>, va: i64, vl: i64) -> bool {
    exists|p: int| 0 <= p < pairs.len() && #[trigger] pairs[p] == (va, vl)
}

/// Selection chains with the same conditions choose a recorded pair.
pub(crate) proof fn selections_choose_a_pair(a: Term, l: Term, env: Seq<i64>,
    pairs: Seq<(i64, i64)>, va: i64, vl: i64)
    requires
        selection_pairs(a, l) == Some(pairs),
        denote(a, env) == Ok::<i64, Failure>(va),
        denote(l, env) == Ok::<i64, Failure>(vl),
    ensures paired(pairs, va, vl),
    decreases a,
{
    match a {
        Term::Int(x) => {
            assert(pairs[0] == (va, vl));
        },
        Term::Select(c, t, e) => match l {
            Term::Select(c2, t2, e2) => {
                let ps = selection_pairs(*t, *t2)->Some_0;
                let qs = selection_pairs(*e, *e2)->Some_0;
                assert(*c == *c2);
                assert(pairs == ps + qs);
                assert(a == Term::Select(Box::new(*c), Box::new(*t), Box::new(*e)));
                assert(l == Term::Select(Box::new(*c2), Box::new(*t2), Box::new(*e2)));
                select_value(*c, *t, *e, env, va);
                select_value(*c2, *t2, *e2, env, vl);
                let vc = denote(*c, env)->Ok_0;
                if vc == 1 {
                    selections_choose_a_pair(*t, *t2, env, ps, denote(*t, env)->Ok_0, denote(*t2, env)->Ok_0);
                    let p = choose|p: int| 0 <= p < ps.len() && #[trigger] ps[p] == (va, vl);
                    assert(pairs[p] == ps[p]);
                } else {
                    selections_choose_a_pair(*e, *e2, env, qs, denote(*e, env)->Ok_0, denote(*e2, env)->Ok_0);
                    let q = choose|q: int| 0 <= q < qs.len() && #[trigger] qs[q] == (va, vl);
                    assert(pairs[ps.len() + q] == qs[q]);
                }
            },
            _ => {},
        },
        _ => {},
    }
}

/// The correspondence the validator relies on, on one route: with the
/// aggregate executed on `x` (target `route`) and the leaf on `x` read at
/// its positions, outputs with equal canonical terms are equal, and when the
/// selection outputs' chains give a functional set of case pairs, the leaf
/// selects the case paired with the aggregate's selected case.
pub(crate) proof fn validated_routes_agree(inputs: Seq<Domain>, outputs: Seq<Domain>,
    nodes: Seq<Op>, roots: Seq<u16>, x: Seq<i64>, target: u16, route: i64, result: Seq<i64>,
    leaf_inputs: Seq<Domain>, leaf_outputs: Seq<Domain>, leaf_nodes: Seq<Op>,
    leaf_roots: Seq<u16>, y: Seq<i64>, position: Seq<u16>, leaf_result: Seq<i64>,
    pairs: Seq<(i64, i64)>)
    requires
        execution(inputs, outputs, nodes, roots, x) == Ok::<Seq<i64>, Failure>(result),
        (target as int) < x.len(),
        x[target as int] == route,
        reads_from(y, x, position),
        execution(leaf_inputs, leaf_outputs, leaf_nodes, leaf_roots, y) == Ok::<Seq<i64>, Failure>(leaf_result),
        roots.len() > 0,
        leaf_roots.len() > 0,
        selection_pairs(
            canonical(nodes, specialized(target, route), boolean_inputs(inputs), nodes.len())[roots[0] as int],
            canonical(leaf_nodes, renamed(position), boolean_inputs(inputs), leaf_nodes.len())[leaf_roots[0] as int],
        ) == Some(pairs),
        functional(pairs),
    ensures
        forall|i: int, j: int| 0 <= i < roots.len() && 0 <= j < leaf_roots.len()
            && #[trigger] canonical(nodes, specialized(target, route), boolean_inputs(inputs), nodes.len())[roots[i] as int]
                == #[trigger] canonical(leaf_nodes, renamed(position), boolean_inputs(inputs), leaf_nodes.len())[leaf_roots[j] as int]
            ==> result[i] == leaf_result[j],
        forall|p: int| 0 <= p < pairs.len() && (#[trigger] pairs[p]).0 == result[0] ==> pairs[p].1 == leaf_result[0],
{
    route_specialization_preserves_evaluation(inputs, outputs, nodes, roots, x, target, route, result);
    assert(admitted(inputs, x));
    renaming_preserves_evaluation(inputs, x, leaf_inputs, leaf_outputs, leaf_nodes, leaf_roots, y,
        position, leaf_result);
    let aggregate = canonical(nodes, specialized(target, route), boolean_inputs(inputs), nodes.len());
    let leaf = canonical(leaf_nodes, renamed(position), boolean_inputs(inputs), leaf_nodes.len());
    assert forall|i: int, j: int| 0 <= i < roots.len() && 0 <= j < leaf_roots.len()
        && #[trigger] aggregate[roots[i] as int] == #[trigger] leaf[leaf_roots[j] as int]
        implies result[i] == leaf_result[j] by {
        assert(root_value(aggregate, roots, x, result, i));
        assert(root_value(leaf, leaf_roots, x, leaf_result, j));
    }
    assert(root_value(aggregate, roots, x, result, 0));
    assert(root_value(leaf, leaf_roots, x, leaf_result, 0));
    selections_choose_a_pair(aggregate[roots[0] as int], leaf[leaf_roots[0] as int], x, pairs,
        result[0], leaf_result[0]);
    let chosen = choose|p: int| 0 <= p < pairs.len() && #[trigger] pairs[p] == (result[0], leaf_result[0]);
    assert forall|p: int| 0 <= p < pairs.len() && (#[trigger] pairs[p]).0 == result[0]
        implies pairs[p].1 == leaf_result[0] by {
        assert(pairs[chosen].0 == pairs[p].0);
    }
}

} // verus!
