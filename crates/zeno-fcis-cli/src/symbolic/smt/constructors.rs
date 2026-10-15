//! Production primitive term construction, with exact AST postconditions.

use super::Term;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
verus! {
    impl View for Term {
        type V = Term;
        #[verifier::opaque]
        open spec fn view(&self) -> Term { *self }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Term {
    pub(crate) fn name(name: &str) -> Term {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Term| match result { Self::Name(n) => spec_eq(n@, name@), _ => false }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        let mut owned = String::new();
        owned.push_str(name);
        Self::Name(owned)
    }

    pub(crate) fn add(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Term| (match result {
            Self::Add(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false,
        }) && forall|environment: Map<Seq<char>, super::denotation::Value>|
            super::denotation::term(result, environment) ==
                match (#[trigger] super::denotation::term(left@, environment),
                    super::denotation::term(right@, environment)) {
                    (Some(super::denotation::Value::Integer(a)), Some(super::denotation::Value::Integer(b))) =>
                        Some(super::denotation::Value::Integer(a + b)),
                    _ => None,
                }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost((left@, right@)));
        let result = Self::Add(Box::new(left), Box::new(right));
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                super::denotation::term(result, environment) ==
                    match (#[trigger] super::denotation::term(original@.0, environment),
                        super::denotation::term(original@.1, environment)) {
                        (Some(super::denotation::Value::Integer(a)), Some(super::denotation::Value::Integer(b))) =>
                            Some(super::denotation::Value::Integer(a + b)),
                        _ => None,
                    } by {
                reveal_with_fuel(super::denotation::term, 2);
            }
        }
        result
    }

    pub(crate) fn sub(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Term| match result { Self::Sub(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        Self::Sub(Box::new(left), Box::new(right))
    }

    pub(crate) fn mul(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Term| match result { Self::Mul(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        Self::Mul(Box::new(left), Box::new(right))
    }

    pub(crate) fn eq(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Term| (match result {
            Self::Eq(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false,
        }) && forall|environment: Map<Seq<char>, super::denotation::Value>|
            super::denotation::term(result, environment) ==
                match (#[trigger] super::denotation::term(left@, environment),
                    super::denotation::term(right@, environment)) {
                    (Some(super::denotation::Value::Integer(a)), Some(super::denotation::Value::Integer(b))) =>
                        Some(super::denotation::Value::Boolean(a == b)),
                    (Some(super::denotation::Value::Boolean(a)), Some(super::denotation::Value::Boolean(b))) =>
                        Some(super::denotation::Value::Boolean(a == b)),
                    _ => None,
                }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost((left@, right@)));
        let result = Self::Eq(Box::new(left), Box::new(right));
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                super::denotation::term(result, environment) ==
                    match (#[trigger] super::denotation::term(original@.0, environment),
                        super::denotation::term(original@.1, environment)) {
                        (Some(super::denotation::Value::Integer(a)), Some(super::denotation::Value::Integer(b))) =>
                            Some(super::denotation::Value::Boolean(a == b)),
                        (Some(super::denotation::Value::Boolean(a)), Some(super::denotation::Value::Boolean(b))) =>
                            Some(super::denotation::Value::Boolean(a == b)),
                        _ => None,
                    } by {
                reveal_with_fuel(super::denotation::term, 2);
            }
        }
        result
    }

    pub(crate) fn lt(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Term| match result { Self::Lt(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        Self::Lt(Box::new(left), Box::new(right))
    }

    pub(crate) fn le(left: Self, right: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Term| match result { Self::Le(a, b) => spec_eq(*a, left@) && spec_eq(*b, right@), _ => false }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        Self::Le(Box::new(left), Box::new(right))
    }

    /// `min <= term <= max`.
    pub(crate) fn within(term: &Self, min: i128, max: i128) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|output: Term|
            forall|environment: Map<Seq<char>, super::denotation::Value>, value: int|
                super::denotation::term(*term, environment) == Some(super::denotation::Value::Integer(value)) ==>
                super::denotation::term(output, environment) ==
                    Some(super::denotation::Value::Boolean(min as int <= value && value <= max as int))));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        let items = vec![
            Self::le(Self::Int(min), term.clone()),
            Self::le(term.clone(), Self::Int(max)),
        ];
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost(items@));
        let output = Self::and(items);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>, value: int|
                super::denotation::term(*term, environment) == Some(super::denotation::Value::Integer(value)) implies
                super::denotation::term(output, environment) ==
                    Some(super::denotation::Value::Boolean(min as int <= value && value <= max as int)) by {
                reveal_with_fuel(super::denotation::term, 2);
                reveal_with_fuel(super::denotation::junction, 1);
                broadcast use vstd::seq::group_seq_lemmas;
                assert(super::folding::booleans(original@, environment));
                assert(original@ =~= seq![original@[0], original@[1]]);
                super::folding::junction_push(Seq::empty(), original@[0], environment, true);
                let first = Seq::empty().push(original@[0]);
                super::folding::junction_push(first, original@[1], environment, true);
                assert(original@ =~= first.push(original@[1]));
                assert(super::denotation::term(original@[0], environment) ==
                    Some(super::denotation::Value::Boolean(min as int <= value)));
                assert(super::denotation::term(original@[1], environment) ==
                    Some(super::denotation::Value::Boolean(value <= max as int)));
                assert(super::denotation::junction(original@, 2, environment, true) ==
                    Some(super::denotation::Value::Boolean(min as int <= value && value <= max as int)));
                assert(super::denotation::term(output, environment) ==
                    super::denotation::junction(original@, original@.len(), environment, true));
            }
        }
        output
    }

    pub(crate) fn not(inner: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Term| (match inner@ {
            Self::Bool(value) => match result { Self::Bool(b) => b == !value, _ => false },
            Self::Not(arg) => spec_eq(result, *arg),
            _ => match result { Self::Not(a) => spec_eq(*a, inner@), _ => false },
        }) && forall|environment: Map<Seq<char>, super::denotation::Value>|
            match #[trigger] super::denotation::term(inner@, environment) {
                Some(super::denotation::Value::Boolean(value)) =>
                    super::denotation::term(result, environment) == Some(super::denotation::Value::Boolean(!value)),
                _ => true,
            }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost(inner@));
        let result = match inner {
            Self::Bool(value) => Self::Bool(!value),
            Self::Not(inner) => *inner,
            inner => Self::Not(Box::new(inner)),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                match #[trigger] super::denotation::term(original@, environment) {
                    Some(super::denotation::Value::Boolean(value)) =>
                        super::denotation::term(result, environment) == Some(super::denotation::Value::Boolean(!value)),
                    _ => true,
                } by {
                reveal_with_fuel(super::denotation::term, 2);
            }
        }
        result
    }

    pub(crate) fn ite(condition: Self, then: Self, otherwise: Self) -> Term {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Term| (match condition@ {
            Self::Bool(true) => spec_eq(result, then@),
            Self::Bool(false) => spec_eq(result, otherwise@),
            _ => match result { Self::Ite(c, a, b) => spec_eq(*c, condition@) && spec_eq(*a, then@) && spec_eq(*b, otherwise@), _ => false },
        }) && forall|environment: Map<Seq<char>, super::denotation::Value>|
            super::denotation::term(result, environment) == match #[trigger] super::denotation::term(condition@, environment) {
                Some(super::denotation::Value::Boolean(true)) => super::denotation::term(then@, environment),
                Some(super::denotation::Value::Boolean(false)) => super::denotation::term(otherwise@, environment),
                _ => None,
            }));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost((condition@, then@, otherwise@)));
        let result = match condition {
            Self::Bool(true) => then,
            Self::Bool(false) => otherwise,
            condition => Self::Ite(Box::new(condition), Box::new(then), Box::new(otherwise)),
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                super::denotation::term(result, environment) == match #[trigger] super::denotation::term(original@.0, environment) {
                    Some(super::denotation::Value::Boolean(true)) => super::denotation::term(original@.1, environment),
                    Some(super::denotation::Value::Boolean(false)) => super::denotation::term(original@.2, environment),
                    _ => None,
                } by {
                reveal_with_fuel(super::denotation::term, 2);
            }
        }
        result
    }
}
