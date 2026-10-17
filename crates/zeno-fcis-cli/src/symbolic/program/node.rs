//! One node's construction as a term of its encoded prefix: pure data in,
//! a typed term or a data refusal out. The caller maps `Failure` to its
//! diagnostics.

use zeno_fcis_synthesis::finite::{Domain, Op};

use crate::symbolic::smt::Term;
#[cfg(verus_keep_ghost)]
use crate::term::denotation;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
    pub(super) open spec fn boolean(kind: Kind) -> bool { matches!(kind, Kind::Bool) }
    pub(super) open spec fn static_kinds(kinds: Seq<Kind>) -> Seq<bool> {
        Seq::new(kinds.len(), |i: int| boolean(kinds[i]))
    }
    spec fn ready(terms: Seq<Term>, kinds: Seq<Kind>, id: u16, wanted: Option<Kind>) -> bool {
        id < terms.len() && id < kinds.len()
            && (wanted.is_none() || wanted == Some(kinds[id as int]))
    }

    pub(super) open spec fn wire_value(kind: Kind, value: i64) -> denotation::Value {
        if boolean(kind) { denotation::Value::Boolean(value == 1) }
        else { denotation::Value::Integer(value as int) }
    }

    pub(crate) open spec fn input_values(
        domains: Seq<Domain>, terms: Seq<Term>, values: Seq<i64>,
        environment: Map<Seq<char>, denotation::Value>,
    ) -> bool {
        crate::evaluation::spec::admitted(domains, values)
            && terms.len() == values.len()
            && forall|i: int| 0 <= i < values.len() ==>
                denotation::term(terms[i], environment) ==
                    Some(denotation::Value::Integer(values[i] as int))
    }

    pub(super) open spec fn prefix_values(
        kinds: Seq<Kind>, terms: Seq<Term>, values: Seq<i64>,
        environment: Map<Seq<char>, denotation::Value>,
    ) -> bool {
        kinds.len() == terms.len() && terms.len() == values.len()
            && forall|i: int| 0 <= i < values.len() ==>
                (!boolean(kinds[i]) || 0 <= values[i] <= 1)
                && denotation::term(terms[i], environment) == Some(wire_value(kinds[i], values[i]))
    }

    pub(super) open spec fn typed_sources(input: Seq<Term>, terms: Seq<Term>, kinds: Seq<Kind>,
        environment: Map<Seq<char>, denotation::Value>) -> bool {
        terms.len() == kinds.len()
            && (forall|i: int| 0 <= i < input.len() ==>
                match #[trigger] denotation::term(input[i], environment) {
                    Some(denotation::Value::Integer(_)) => true, _ => false,
                })
            && (forall|i: int| 0 <= i < terms.len() ==>
                match #[trigger] denotation::term(terms[i], environment) {
                    Some(denotation::Value::Boolean(_)) => boolean(kinds[i]),
                    Some(denotation::Value::Integer(_)) => !boolean(kinds[i]),
                    None => false,
                })
    }

    pub(super) open spec fn has_kind(value: Option<denotation::Value>, kind: Kind) -> bool {
        match value {
            Some(denotation::Value::Boolean(_)) => boolean(kind),
            Some(denotation::Value::Integer(_)) => !boolean(kind),
            None => false,
        }
    }

    /// Equal supplied meanings in two environments, without a second term walk.
    pub(super) open spec fn sources_equal(input: Seq<Term>, terms: Seq<Term>,
        left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>) -> bool {
        (forall|i: int| 0 <= i < input.len() ==>
            denotation::term(input[i], left) == denotation::term(input[i], right))
        && (forall|i: int| 0 <= i < terms.len() ==>
            denotation::term(terms[i], left) == denotation::term(terms[i], right))
    }

    /// A constructed body depends only on its supplied typed source values.
    /// This is conditional environment congruence, not existence of an environment.
    pub(super) open spec fn depends_on(output: Term, input: Seq<Term>, terms: Seq<Term>,
        kinds: Seq<Kind>) -> bool {
        forall|left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>|
            typed_sources(input, terms, kinds, left)
                && typed_sources(input, terms, kinds, right)
                && sources_equal(input, terms, left, right) ==>
                denotation::term(output, left) == denotation::term(output, right)
    }

    spec fn at(terms: Seq<Term>, id: u16, environment: Map<Seq<char>, denotation::Value>)
        -> Option<denotation::Value> {
        if id < terms.len() { denotation::term(terms[id as int], environment) } else { None }
    }

    /// The mathematical value of one SMT expression, before the caller's
    /// checked-i64 guard. This erased relation is connected to the actual
    /// runtime below; it does not execute or authorize an application.
    #[verifier::opaque]
    spec fn expression(op: Op, domains: Seq<Domain>, input: Seq<Term>, terms: Seq<Term>,
        environment: Map<Seq<char>, denotation::Value>) -> Option<denotation::Value> {
        match op {
            Op::Input(id) => if id >= domains.len() { None }
                else if domains[id as int] is Bool {
                    match at(input, id, environment) {
                        Some(denotation::Value::Integer(v)) => Some(denotation::Value::Boolean(v == 1)),
                        _ => None,
                    }
                } else { at(input, id, environment) },
            Op::Int(v) => Some(denotation::Value::Integer(v as int)),
            Op::Bool(v) => Some(denotation::Value::Boolean(v)),
            Op::Add(a, b) | Op::Sub(a, b) | Op::Lt(a, b) =>
                match (at(terms, a, environment), at(terms, b, environment)) {
                    (Some(denotation::Value::Integer(a)), Some(denotation::Value::Integer(b))) =>
                        if matches!(op, Op::Add(..)) { Some(denotation::Value::Integer(a + b)) }
                        else if matches!(op, Op::Sub(..)) { Some(denotation::Value::Integer(a - b)) }
                        else { Some(denotation::Value::Boolean(a < b)) },
                    _ => None,
                },
            Op::Eq(a, b) => match (at(terms, a, environment), at(terms, b, environment)) {
                (Some(denotation::Value::Integer(a)), Some(denotation::Value::Integer(b))) =>
                    Some(denotation::Value::Boolean(a == b)),
                (Some(denotation::Value::Boolean(a)), Some(denotation::Value::Boolean(b))) =>
                    Some(denotation::Value::Boolean(a == b)),
                _ => None,
            },
            Op::And(a, b) => match (at(terms, a, environment), at(terms, b, environment)) {
                (Some(denotation::Value::Boolean(a)), Some(denotation::Value::Boolean(b))) =>
                    Some(denotation::Value::Boolean(a && b)),
                _ => None,
            },
            Op::Not(a) => match at(terms, a, environment) {
                Some(denotation::Value::Boolean(a)) => Some(denotation::Value::Boolean(!a)),
                _ => None,
            },
            Op::Mul(..) | Op::DivFloor(..) | Op::DivCeil(..)
            | Op::MulDivFloor(..) | Op::MulDivCeil(..) | Op::ProductsGe(..) => None,
            Op::Select(c, a, b) => match at(terms, c, environment) {
                Some(denotation::Value::Boolean(true)) => at(terms, a, environment),
                Some(denotation::Value::Boolean(false)) => at(terms, b, environment),
                _ => None,
            },
        }
    }

    /// Relates the constructed term to the existing runtime's node semantics.
    /// Encoding overflow succeeds with an unbounded term; the caller's range
    /// guard, rather than node construction, represents the runtime refusal.
    pub(super) open spec fn refines(
        op: Op, kind: Kind, term: Term, input: Seq<i64>, values: Seq<i64>,
        environment: Map<Seq<char>, denotation::Value>,
    ) -> bool {
        match crate::evaluation::spec::node(op, input, values) {
            Ok(value) => (!boolean(kind) || 0 <= value <= 1)
                && denotation::term(term, environment) == Some(wire_value(kind, value)),
            Err(crate::evaluation::Failure::Arithmetic) => match op {
                Op::Add(a, b) | Op::Sub(a, b) => {
                    let value = if matches!(op, Op::Add(..)) {
                        values[a as int] as int + values[b as int] as int
                    } else { values[a as int] as int - values[b as int] as int };
                    !boolean(kind) && (value < i64::MIN || value > i64::MAX)
                        && denotation::term(term, environment) == Some(denotation::Value::Integer(value))
                },
                _ => false,
            },
            _ => false,
        }
    }

    proof fn expression_refines_runtime(op: Op, domains: Seq<Domain>, input: Seq<Term>,
        terms: Seq<Term>, kinds: Seq<Kind>, kind: Kind, term: Term,
        raw_input: Seq<i64>, previous: Seq<i64>, environment: Map<Seq<char>, denotation::Value>)
        requires !crate::evaluation::spec::new_arithmetic(op),
            input_values(domains, input, raw_input, environment),
            prefix_values(kinds, terms, previous, environment),
            crate::evaluation::admission::spec::kind(op, domains, static_kinds(kinds)) == Ok(boolean(kind)),
            denotation::term(term, environment) == expression(op, domains, input, terms, environment),
        ensures refines(op, kind, term, raw_input, previous, environment),
    {
        reveal(expression);
        reveal(at);
        match op {
            Op::Input(id) => {},
            Op::Int(_) | Op::Bool(_) => {},
            Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
                assert(0 <= a < previous.len() && 0 <= b < previous.len());
                assert(denotation::term(terms[a as int], environment) == Some(wire_value(kinds[a as int], previous[a as int])));
                assert(denotation::term(terms[b as int], environment) == Some(wire_value(kinds[b as int], previous[b as int])));
            },
            Op::Not(a) => {},
            Op::Select(c, a, b) => {},
            Op::Mul(..) | Op::DivFloor(..) | Op::DivCeil(..)
            | Op::MulDivFloor(..) | Op::MulDivCeil(..) | Op::ProductsGe(..) => { assert(false); },
        }
    }

    proof fn integer_one(environment: Map<Seq<char>, denotation::Value>)
        ensures denotation::term(Term::Int(1), environment)
            == Some(denotation::Value::Integer(1)),
    {
        reveal_with_fuel(denotation::term, 1);
    }
}

/// Whether a node is Boolean, as program admission types it.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Bool,
    Int,
}

/// Why a node could not be encoded, as data.
#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Debug, Eq, PartialEq)]
pub(super) enum Failure {
    /// The input's domain is unsupported.
    InputDomain(usize),
    /// No term was supplied for the input.
    MissingInput(usize),
    /// The node id is later or of another kind.
    Operand(u16),
    /// The instruction is unknown to this encoding.
    Instruction,
}

/// Reads an available operand of the requested kind from the encoded prefix.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result.is_ok() == ready(terms@, kinds@, id, wanted),
    match result {
        Ok((term, kind)) => id < terms@.len() && id < kinds@.len()
            && kind == kinds@[id as int]
            && forall|environment: Map<Seq<char>, denotation::Value>|
                denotation::term(term, environment) == denotation::term(terms@[id as int], environment),
        Err(error) => error == Failure::Operand(id),
    },
))]
fn operand(
    terms: &[Term],
    kinds: &[Kind],
    id: u16,
    wanted: Option<Kind>,
) -> Result<(Term, Kind), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(ready); }
    let index = usize::from(id);
    match (terms.get(index), kinds.get(index)) {
        (Some(term), Some(kind))
            if matches!(
                (wanted, *kind),
                (None, _) | (Some(Kind::Bool), Kind::Bool) | (Some(Kind::Int), Kind::Int)
            ) =>
        {
            Ok((term.clone(), *kind))
        }
        _ => Err(Failure::Operand(id)),
    }
}

/// The kind and term of `op`, with operands read from the encoded prefix.
///
/// # Errors
/// `Failure` naming the refusal; the caller adds its position.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    // Exact unsupported refusal for the separately admitted arithmetic extension.
    match result {
        Ok(_) => !crate::evaluation::spec::new_arithmetic(*op),
        Err(Failure::Instruction) => crate::evaluation::spec::new_arithmetic(*op),
        Err(_) => !crate::evaluation::spec::new_arithmetic(*op),
    },
    terms@.len() == kinds@.len() && input@.len() == inputs@.len() ==>
        match result {
            Ok((kind, _)) => crate::evaluation::admission::spec::kind(
                *op, inputs@, static_kinds(kinds@)) == Ok(boolean(kind)),
            Err(Failure::Instruction) => crate::evaluation::spec::new_arithmetic(*op),
            Err(_) => crate::evaluation::admission::spec::kind(
                *op, inputs@, static_kinds(kinds@)).is_err(),
        },
    match result {
        Ok((kind, term)) => forall|environment: Map<Seq<char>, denotation::Value>|
            typed_sources(input@, terms@, kinds@, environment) ==>
                has_kind(denotation::term(term, environment), kind),
        Err(_) => true,
    },
    match result {
        Ok((_, term)) => depends_on(term, input@, terms@, kinds@),
        Err(_) => true,
    },
    match result {
        Ok((kind, term)) => forall|environment: Map<Seq<char>, denotation::Value>,
            raw_input: Seq<i64>, previous: Seq<i64>|
                input_values(inputs@, input@, raw_input, environment)
                    && prefix_values(kinds@, terms@, previous, environment) ==>
                refines(*op, kind, term, raw_input, previous, environment),
        Err(_) => true,
    },
))]
pub(super) fn encode(
    op: &Op,
    inputs: &[Domain],
    input: &[Term],
    terms: &[Term],
    kinds: &[Kind],
) -> Result<(Kind, Term), Failure> {
    #[cfg(verus_keep_ghost)]
    proof! { reveal(<Term as View>::view); }
    if op.requires_arithmetic_profile() {
        return Err(Failure::Instruction);
    }
    let result = match *op {
        Op::Input(id) => {
            let id = usize::from(id);
            let kind = match inputs.get(id) {
                Some(Domain::Bool) => Kind::Bool,
                Some(Domain::Int { .. }) => Kind::Int,
                _ => return Err(Failure::InputDomain(id)),
            };
            let Some(wire) = input.get(id) else {
                return Err(Failure::MissingInput(id));
            };
            let wire = wire.clone();
            match kind {
                Kind::Bool => (Kind::Bool, Term::eq(wire, Term::Int(1))),
                Kind::Int => (Kind::Int, wire),
            }
        }
        Op::Int(value) => (Kind::Int, Term::Int(i128::from(value))),
        Op::Bool(value) => (Kind::Bool, Term::Bool(value)),
        Op::Add(a, b) | Op::Sub(a, b) => {
            let (a, _) = operand(terms, kinds, a, Some(Kind::Int))?;
            let (b, _) = operand(terms, kinds, b, Some(Kind::Int))?;
            #[cfg(verus_keep_ghost)]
            let arguments = verus_exec_expr!(Ghost((a@, b@)));
            let term = if matches!(op, Op::Add(..)) {
                Term::add(a, b)
            } else {
                Term::sub(a, b)
            };
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    typed_sources(input@, terms@, kinds@, environment) implies
                    denotation::term(term, environment) == expression(*op, inputs@, input@, terms@, environment) by {
                    reveal(expression);
                    reveal(at);
                    reveal(typed_sources);
                    match *op {
                        Op::Add(a, b) | Op::Sub(a, b) => {
                            assert(denotation::term(arguments@.0, environment) == denotation::term(terms@[a as int], environment));
                            assert(denotation::term(arguments@.1, environment) == denotation::term(terms@[b as int], environment));
                        },
                        _ => {},
                    }
                    reveal_with_fuel(denotation::term, 1);
                }
            }
            (Kind::Int, term)
        }
        Op::Eq(a, b) => {
            let (a, kind) = operand(terms, kinds, a, None)?;
            let (b, _) = operand(terms, kinds, b, Some(kind))?;
            #[cfg(verus_keep_ghost)]
            let arguments = verus_exec_expr!(Ghost((a@, b@)));
            let term = Term::eq(a, b);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    typed_sources(input@, terms@, kinds@, environment) implies
                    denotation::term(term, environment) == expression(*op, inputs@, input@, terms@, environment) by {
                    reveal(expression);
                    reveal(at);
                    if let Op::Eq(a, b) = *op {
                        assert(denotation::term(arguments@.0, environment) == denotation::term(terms@[a as int], environment));
                        assert(denotation::term(arguments@.1, environment) == denotation::term(terms@[b as int], environment));
                    }
                    reveal_with_fuel(denotation::term, 1);
                }
            }
            (Kind::Bool, term)
        }
        Op::Lt(a, b) => {
            let (a, _) = operand(terms, kinds, a, Some(Kind::Int))?;
            let (b, _) = operand(terms, kinds, b, Some(Kind::Int))?;
            #[cfg(verus_keep_ghost)]
            let arguments = verus_exec_expr!(Ghost((a@, b@)));
            let term = Term::lt(a, b);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    typed_sources(input@, terms@, kinds@, environment) implies
                    denotation::term(term, environment) == expression(*op, inputs@, input@, terms@, environment) by {
                    reveal(expression);
                    reveal(at);
                    reveal(typed_sources);
                    if let Op::Lt(a, b) = *op {
                        assert(denotation::term(arguments@.0, environment) == denotation::term(terms@[a as int], environment));
                        assert(denotation::term(arguments@.1, environment) == denotation::term(terms@[b as int], environment));
                    }
                    reveal_with_fuel(denotation::term, 1);
                }
            }
            (Kind::Bool, term)
        }
        Op::And(a, b) => {
            let (a, _) = operand(terms, kinds, a, Some(Kind::Bool))?;
            let (b, _) = operand(terms, kinds, b, Some(Kind::Bool))?;
            let items = vec![a, b];
            #[cfg(verus_keep_ghost)]
            let supplied = verus_exec_expr!(Ghost(items@));
            let term = Term::and(items);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    match (denotation::term(supplied@[0], environment), denotation::term(supplied@[1], environment)) {
                        (Some(denotation::Value::Boolean(left)), Some(denotation::Value::Boolean(right))) =>
                            #[trigger] denotation::term(term, environment) == Some(denotation::Value::Boolean(left && right)),
                        _ => true,
                    } by {
                    if let (Some(denotation::Value::Boolean(_)), Some(denotation::Value::Boolean(_))) =
                        (denotation::term(supplied@[0], environment), denotation::term(supplied@[1], environment)) {
                        denotation::conjunction_pair(supplied@, term, environment);
                    }
                }
            }
            (Kind::Bool, term)
        }
        Op::Not(a) => {
            let (a, _) = operand(terms, kinds, a, Some(Kind::Bool))?;
            #[cfg(verus_keep_ghost)]
            let argument = verus_exec_expr!(Ghost(a@));
            let term = Term::not(a);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    typed_sources(input@, terms@, kinds@, environment) implies
                    denotation::term(term, environment) == expression(*op, inputs@, input@, terms@, environment) by {
                    reveal(expression);
                    reveal(at);
                    if let Op::Not(a) = *op {
                        assert(denotation::term(argument@, environment) == denotation::term(terms@[a as int], environment));
                    }
                }
            }
            (Kind::Bool, term)
        }
        Op::Select(condition, a, b) => {
            let (condition, _) = operand(terms, kinds, condition, Some(Kind::Bool))?;
            let (a, kind) = operand(terms, kinds, a, None)?;
            let (b, _) = operand(terms, kinds, b, Some(kind))?;
            #[cfg(verus_keep_ghost)]
            let arguments = verus_exec_expr!(Ghost((condition@, a@, b@)));
            let term = Term::ite(condition, a, b);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>|
                    typed_sources(input@, terms@, kinds@, environment) implies
                    denotation::term(term, environment) == expression(*op, inputs@, input@, terms@, environment) by {
                    reveal(expression);
                    reveal(at);
                    if let Op::Select(c, a, b) = *op {
                        assert(denotation::term(arguments@.0, environment) == denotation::term(terms@[c as int], environment));
                        assert(denotation::term(arguments@.1, environment) == denotation::term(terms@[a as int], environment));
                        assert(denotation::term(arguments@.2, environment) == denotation::term(terms@[b as int], environment));
                    }
                }
            }
            (kind, term)
        }
        _ => return Err(Failure::Instruction),
    };
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|environment: Map<Seq<char>, denotation::Value>|
            typed_sources(input@, terms@, kinds@, environment) implies
            denotation::term(result.1, environment) == expression(*op, inputs@, input@, terms@, environment) by {
            reveal(expression);
            reveal(at);
            reveal(typed_sources);
            reveal_with_fuel(denotation::term, 1);
            if let Op::Input(_) = *op {
                integer_one(environment);
            }
        }
        assert forall|environment: Map<Seq<char>, denotation::Value>|
            typed_sources(input@, terms@, kinds@, environment) implies
            has_kind(denotation::term(result.1, environment), result.0) by {
            reveal(expression);
            reveal(at);
            match *op {
                Op::Input(id) => { assert(id < inputs@.len() && id < input@.len()); },
                Op::Int(_) | Op::Bool(_) => {},
                Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
                    assert(a < terms@.len() && b < terms@.len());
                    assert(has_kind(denotation::term(terms@[a as int], environment), kinds@[a as int]));
                    assert(has_kind(denotation::term(terms@[b as int], environment), kinds@[b as int]));
                },
                Op::Not(a) => { assert(a < terms@.len()); },
                Op::Select(c, a, b) => { assert(c < terms@.len() && a < terms@.len() && b < terms@.len()); },
                Op::Mul(..) | Op::DivFloor(..) | Op::DivCeil(..)
                | Op::MulDivFloor(..) | Op::MulDivCeil(..) | Op::ProductsGe(..) => { assert(false); },
            }
        }
        assert forall|left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>|
            typed_sources(input@, terms@, kinds@, left)
                && typed_sources(input@, terms@, kinds@, right)
                && sources_equal(input@, terms@, left, right) implies
                denotation::term(result.1, left) == denotation::term(result.1, right) by {
            assert(denotation::term(result.1, left) == expression(*op, inputs@, input@, terms@, left));
            assert(denotation::term(result.1, right) == expression(*op, inputs@, input@, terms@, right));
            reveal(expression);
            reveal(at);
            match *op {
                Op::Input(id) => {
                    if id < input@.len() {
                        assert(denotation::term(input@[id as int], left) == denotation::term(input@[id as int], right));
                    }
                },
                Op::Int(_) | Op::Bool(_) => {},
                Op::Add(a, b) | Op::Sub(a, b) | Op::Eq(a, b) | Op::Lt(a, b) | Op::And(a, b) => {
                    if a < terms@.len() {
                        assert(denotation::term(terms@[a as int], left) == denotation::term(terms@[a as int], right));
                    }
                    if b < terms@.len() {
                        assert(denotation::term(terms@[b as int], left) == denotation::term(terms@[b as int], right));
                    }
                },
                Op::Not(a) => {},
                Op::Select(c, a, b) => {},
            Op::Mul(..) | Op::DivFloor(..) | Op::DivCeil(..)
            | Op::MulDivFloor(..) | Op::MulDivCeil(..) | Op::ProductsGe(..) => { assert(false); },
            }
        }
        assert(depends_on(result.1, input@, terms@, kinds@));
        assert forall|environment: Map<Seq<char>, denotation::Value>, raw_input: Seq<i64>, previous: Seq<i64>|
            input_values(inputs@, input@, raw_input, environment)
                && prefix_values(kinds@, terms@, previous, environment) implies
            refines(*op, result.0, result.1, raw_input, previous, environment) by {
            reveal(typed_sources);
            assert(typed_sources(input@, terms@, kinds@, environment));
            expression_refines_runtime(*op, inputs@, input@, terms@, kinds@, result.0,
                result.1, raw_input, previous, environment);
        }
    }
    Ok(result)
}
