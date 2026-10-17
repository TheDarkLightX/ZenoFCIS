//! The actual eager node loop, separated from the caller's native diagnostics.
//! Definitions already appended before a refusal remain in the supplied Script.

use super::node::{self, Kind};
#[cfg(verus_keep_ghost)]
use crate::evaluation::admission::spec as admission;
#[cfg(verus_keep_ghost)]
use crate::script::spec as script_spec;
use crate::symbolic::smt::{Script, Sort, Term, indexed_name};
#[cfg(verus_keep_ghost)]
use crate::term::denotation;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::{Domain, Op};

#[cfg(verus_keep_ghost)]
#[path = "nodes_spec.rs"]
pub(super) mod spec;

#[cfg(verus_keep_ghost)]
#[path = "nodes_realization.rs"]
pub(super) mod realization;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct EncodedNodes {
    pub(super) kinds: Vec<Kind>,
    pub(super) terms: Vec<Term>,
    pub(super) checked: Vec<Term>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct Failure {
    pub(super) index: usize,
    pub(super) cause: node::Failure,
}

/// All executable arguments remain unrestricted. The semantic clauses are
/// conditional on an admitted raw input and the actual definition equations.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    final(script).comments@ == old(script).comments@,
    final(script).constants@ == old(script).constants@,
    final(script).observations@ == old(script).observations@,
    final(script).assertions@ == old(script).assertions@,
    final(script).definitions@.len() >= old(script).definitions@.len(),
    final(script).definitions@.subrange(0, old(script).definitions@.len() as int)
        == old(script).definitions@,
    match result {
        Ok(encoded) => encoded.terms@.len() == nodes@.len()
            && spec::shape(final(script).definitions@, old(script).definitions@.len(),
                prefix@, encoded.kinds@, encoded.terms@)
            && spec::bodies(final(script).definitions@, old(script).definitions@.len(),
                domains@, nodes@, input@, encoded.kinds@, encoded.terms@)
            && spec::guarded(nodes@, encoded.terms@, encoded.checked@)
            && (forall|count: nat| count <= nodes@.len() ==>
                spec::guards(nodes@, count) <= encoded.checked@.len()),
        Err(failure) => failure.index < nodes@.len()
            && (failure.cause == node::Failure::Instruction
                ==> crate::evaluation::spec::new_arithmetic(nodes@[failure.index as int]))
            && final(script).definitions@.len()
                == old(script).definitions@.len() + failure.index
            && exists|prefix_kinds: Seq<Kind>, prefix_terms: Seq<Term>, prefix_checked: Seq<Term>|
                prefix_terms.len() == failure.index
                && spec::shape(final(script).definitions@, old(script).definitions@.len(),
                    prefix@, prefix_kinds, prefix_terms)
                && spec::bodies(final(script).definitions@, old(script).definitions@.len(),
                    domains@, nodes@, input@, prefix_kinds, prefix_terms)
                && spec::guarded(nodes@, prefix_terms, prefix_checked),
    },
    input@.len() == domains@.len() ==> match result {
        Ok(encoded) => admission::prefix(domains@, nodes@, nodes@.len())
            == Ok(node::static_kinds(encoded.kinds@)),
        Err(failure) => admission::prefix(domains@, nodes@, failure.index as nat).is_ok()
            && (failure.cause == node::Failure::Instruction
                || admission::prefix(domains@, nodes@, (failure.index + 1) as nat).is_err()),
    },
    input@.len() == domains@.len()
        && admission::prefix(domains@, nodes@, nodes@.len()).is_ok()
        && (forall|i:int| 0<=i<nodes@.len() ==> !crate::evaluation::spec::new_arithmetic(nodes@[i]))
        ==> result.is_ok(),
    match result {
        Ok(encoded) => forall|environment: Map<Seq<char>, denotation::Value>,
            raw_input: Seq<i128>, count: nat|
            count <= nodes@.len()
                && node::input_values(domains@, input@, raw_input, environment)
                && script_spec::definitions_hold(final(script).definitions@,
                    old(script).definitions@.len(), nodes@.len(), environment) ==>
            spec::meaning(nodes@, raw_input, count, input@,
                encoded.kinds@.subrange(0, count as int),
                encoded.terms@.subrange(0, count as int),
                encoded.checked@.subrange(0, spec::guards(nodes@, count) as int), environment),
        Err(_) => true,
    },
))]
pub(super) fn encode_nodes(
    script: &mut Script,
    prefix: &str,
    domains: &[Domain],
    nodes: &[Op],
    input: &[Term],
) -> Result<EncodedNodes, Failure> {
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(<Term as View>::view);
        broadcast use vstd::seq::group_seq_lemmas;
        reveal_with_fuel(spec::guards, 1);
    }
    #[cfg(verus_keep_ghost)]
    let base = verus_exec_expr!(Ghost(script.definitions@.len()));
    let mut kinds = Vec::with_capacity(nodes.len());
    let mut terms: Vec<Term> = Vec::with_capacity(nodes.len());
    let mut checked = Vec::new();
    let mut index = 0;
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(node::static_kinds(kinds@) =~= Seq::empty());
        assert(admission::prefix(domains@, nodes@, 0) == Ok(Seq::empty()));
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= nodes@.len(),
            kinds@.len() == index, terms@.len() == index,
            script.comments@ == old(script).comments@,
            script.constants@ == old(script).constants@,
            script.observations@ == old(script).observations@,
            script.assertions@ == old(script).assertions@,
            base@ == old(script).definitions@.len(),
            script.definitions@.subrange(0, base@ as int) == old(script).definitions@,
            spec::shape(script.definitions@, base@, prefix@, kinds@, terms@),
            spec::bodies(script.definitions@, base@, domains@, nodes@, input@, kinds@, terms@),
            spec::guarded(nodes@, terms@, checked@),
            input@.len() == domains@.len() ==>
                admission::prefix(domains@, nodes@, index as nat)
                    == Ok(node::static_kinds(kinds@)),
        decreases nodes@.len() - index,
    ))]
    while index < nodes.len() {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        let op = &nodes[index];
        #[cfg(verus_keep_ghost)]
        let previous = verus_exec_expr!(Ghost((kinds@, terms@, checked@, script.definitions@)));
        let (kind, term) = match node::encode(op, domains, input, &terms, &kinds) {
            Ok(encoded) => encoded,
            Err(cause) => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(exists|prefix_kinds: Seq<Kind>, prefix_terms: Seq<Term>, prefix_checked: Seq<Term>|
                        prefix_terms.len() == index
                        && spec::shape(script.definitions@, base@, prefix@, prefix_kinds, prefix_terms)
                        && spec::bodies(script.definitions@, base@, domains@, nodes@, input@,
                            prefix_kinds, prefix_terms)
                        && spec::guarded(nodes@, prefix_terms, prefix_checked));
                    if input@.len() == domains@.len() && cause != node::Failure::Instruction {
                        assert(admission::kind(*op, domains@, node::static_kinds(kinds@)).is_err());
                        match admission::kind(*op, domains@, node::static_kinds(kinds@)) {
                            Err(error) => {
                                assert(admission::prefix(domains@, nodes@, (index + 1) as nat) == Err(error));
                                admission::prefix_failure_persists(domains@, nodes@,
                                    (index + 1) as nat, nodes@.len(), error);
                            },
                            _ => {},
                        }
                    }
                }
                return Err(Failure { index, cause });
            }
        };
        #[cfg(verus_keep_ghost)]
        let body = verus_exec_expr!(Ghost(term@));
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(spec::body_fact(body@, *op, domains@, input@, kind, previous@.0, previous@.1)) by {
                reveal(spec::body_fact);
                assert(body@ == term);
                assert(previous@.0.len() == previous@.1.len());
            }
        }
        let name = indexed_name(prefix, index);
        let sort = match kind {
            Kind::Bool => Sort::Bool,
            Kind::Int => Sort::Int,
        };
        let defined = script.define(&name, sort, term);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(name@ == crate::term::names::generated(prefix@, index as nat));
            assert(script.definitions@.len() == base@ + index + 1);
            assert(script.definitions@[base@ as int + index as int].0@ == name@);
            assert(script.definitions@[base@ as int + index as int].1 == sort);
            assert(sort == if node::boolean(kind) { Sort::Bool } else { Sort::Int });
        }
        if matches!(op, Op::Add(..) | Op::Sub(..)) {
            // Only names the original two temporary constructor results.
            let in_range = Term::within(&defined, i128::MIN, i128::MAX);
            #[cfg(verus_keep_ghost)]
            let range = verus_exec_expr!(Ghost(in_range@));
            let guard = Term::not(in_range);
            #[cfg(verus_keep_ghost)]
            proof! {
                assert forall|environment: Map<Seq<char>, denotation::Value>, value: int|
                    denotation::term(defined, environment)
                        == Some(denotation::Value::Integer(value)) implies
                    denotation::term(guard, environment)
                        == Some(denotation::Value::Boolean(spec::outside(value))) by {
                    assert(denotation::term(range@, environment)
                        == Some(denotation::Value::Boolean(i128::MIN <= value && value <= i128::MAX)));
                    assert(denotation::term(guard, environment)
                        == Some(denotation::Value::Boolean(!(i128::MIN <= value && value <= i128::MAX))));
                }
            }
            checked.push(guard);
        }
        kinds.push(kind);
        terms.push(defined);
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
            reveal(spec::shape);
            spec::guards_step(nodes@, index as nat);
            assert(spec::shape(previous@.3, base@, prefix@, previous@.0, previous@.1));
            assert(node::static_kinds(kinds@)
                =~= node::static_kinds(previous@.0).push(node::boolean(kind)));
            assert forall|j: int| 0 <= j < terms@.len() implies
                #[trigger] script.definitions@[base@ as int + j].0@
                    == crate::term::names::generated(prefix@, j as nat)
                && script.definitions@[base@ as int + j].1
                    == if node::boolean(kinds@[j]) { Sort::Bool } else { Sort::Int }
                && match terms@[j] {
                    Term::Name(name) => name@ == script.definitions@[base@ as int + j].0@,
                    _ => false,
                } by {
                if j < index {
                    assert(script.definitions@[base@ as int + j] == previous@.3[base@ as int + j]);
                    assert(terms@[j] == previous@.1[j]);
                    assert(kinds@[j] == previous@.0[j]);
                    assert(previous@.3[base@ as int + j].0@
                        == crate::term::names::generated(prefix@, j as nat));
                } else {
                    assert(j == index);
                    assert(script.definitions@[base@ as int + j].0@ == name@);
                    assert(terms@[j] == defined);
                    assert(kinds@[j] == kind);
                }
            }
            assert forall|j: int| 0 <= j < terms@.len() implies
                spec::body_fact(script.definitions@[base@ as int + j].2, nodes@[j], domains@,
                    input@, kinds@[j], kinds@.subrange(0, j), terms@.subrange(0, j)) by {
                if j < index {
                    assert(kinds@.subrange(0, j) =~= previous@.0.subrange(0, j));
                    assert(terms@.subrange(0, j) =~= previous@.1.subrange(0, j));
                    assert(script.definitions@[base@ as int + j] == previous@.3[base@ as int + j]);
                } else {
                    assert(j == index);
                    assert(kinds@.subrange(0, j) =~= previous@.0);
                    assert(terms@.subrange(0, j) =~= previous@.1);
                    assert(script.definitions@[base@ as int + j].2 == body@);
                }
            }
            assert forall|j: int| 0 <= j < terms@.len() && spec::arithmetic(nodes@[j]) implies
                spec::guards(nodes@, j as nat) < checked@.len()
                && (forall|environment: Map<Seq<char>, denotation::Value>, value: int|
                    denotation::term(terms@[j], environment)
                        == Some(denotation::Value::Integer(value)) ==>
                    denotation::term(checked@[spec::guards(nodes@, j as nat) as int], environment)
                        == Some(denotation::Value::Boolean(spec::outside(value)))) by {
                if j < index {
                    assert(terms@[j] == previous@.1[j]);
                    assert(checked@.subrange(0, previous@.2.len() as int) == previous@.2);
                } else {
                    assert(j == index);
                    assert(spec::guards(nodes@, index as nat) == previous@.2.len());
                }
            }
            assert(kinds@.len() == terms@.len());
            assert(script.definitions@.len() == base@ + terms@.len());
            assert(spec::shape(script.definitions@, base@, prefix@, kinds@, terms@));
        }
        index += 1;
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert forall|count: nat| count <= nodes@.len() implies
            spec::guards(nodes@, count) <= checked@.len() by {
            spec::guards_monotone(nodes@, count, nodes@.len());
        }
        assert forall|environment: Map<Seq<char>, denotation::Value>, raw_input: Seq<i128>, count: nat|
            count <= nodes@.len()
                && node::input_values(domains@, input@, raw_input, environment)
                && script_spec::definitions_hold(script.definitions@, base@, nodes@.len(), environment)
            implies spec::meaning(nodes@, raw_input, count, input@,
                kinds@.subrange(0, count as int), terms@.subrange(0, count as int),
                checked@.subrange(0, spec::guards(nodes@, count) as int), environment) by {
            spec::guards_monotone(nodes@, count, nodes@.len());
            spec::prefix_refines(script.definitions@, base@, prefix@, domains@, nodes@, input@,
                kinds@, terms@, checked@, raw_input, environment, count);
        }
    }
    Ok(EncodedNodes {
        kinds,
        terms,
        checked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_extension_is_unsupported_without_a_fresh_symbol_or_certificate_term() {
        for op in [
            Op::Mul(0, 1),
            Op::DivFloor(0, 1),
            Op::DivCeil(0, 1),
            Op::MulDivFloor(0, 1, 2),
            Op::MulDivCeil(0, 1, 2),
            Op::ProductsGe([0, 1, 2], [0, 1, 2]),
        ] {
            let mut script = Script::default();
            let result = encode_nodes(
                &mut script,
                "p",
                &[],
                &[Op::Int(5), Op::Int(2), Op::Int(1), op],
                &[],
            );
            assert!(matches!(
                result,
                Err(Failure {
                    index: 3,
                    cause: node::Failure::Instruction
                })
            ));
            assert_eq!(script.definitions.len(), 3);
            assert!(script.constants.is_empty());
        }
    }

    #[test]
    fn retains_prior_definitions_and_stops_at_first_bad_operand() {
        let mut script = Script::default();
        script.define("prior", Sort::Int, Term::Int(7));

        let result = encode_nodes(
            &mut script,
            "p",
            &[],
            &[Op::Int(3), Op::Add(0, 1), Op::Int(9)],
            &[],
        );

        assert!(matches!(
            result,
            Err(Failure {
                index: 1,
                cause: node::Failure::Operand(1)
            })
        ));
        assert_eq!(
            script.definitions,
            vec![
                ("prior".to_owned(), Sort::Int, Term::Int(7)),
                ("p0".to_owned(), Sort::Int, Term::Int(3)),
            ]
        );
    }
}
