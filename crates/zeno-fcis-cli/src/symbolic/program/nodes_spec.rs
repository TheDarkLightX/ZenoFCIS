//! Erased relations on the actual eager helper's definitions and three vectors.
//! Instruction semantics are the existing evaluation::spec::node/prefix.

use super::super::node;
use super::{Domain, Kind, Op, Sort, Term};
use crate::evaluation::admission::spec as admission;
use crate::evaluation::{Failure as RuntimeFailure, spec as runtime};
use crate::script::spec as script_spec;
use crate::term::{denotation, names};
use vstd::prelude::*;

verus! {

pub(in super::super) open spec fn arithmetic(op: Op) -> bool {
    matches!(op, Op::Add(..) | Op::Sub(..))
}

/// Counts guards only; this does not evaluate instructions or retain values.
#[verifier::opaque]
pub(in super::super) open spec fn guards(nodes: Seq<Op>, count: nat) -> nat
    recommends count <= nodes.len(),
    decreases count,
{
    if count == 0 || count > nodes.len() { 0 }
    else { guards(nodes, (count - 1) as nat)
        + if arithmetic(nodes[count as int - 1]) { 1nat } else { 0nat } }
}

pub(in super::super) proof fn guards_step(nodes: Seq<Op>, count: nat)
    requires count < nodes.len(),
    ensures guards(nodes, count + 1) == guards(nodes, count)
        + if arithmetic(nodes[count as int]) { 1nat } else { 0nat },
{
    reveal_with_fuel(guards, 1);
}

pub(in super::super) proof fn guards_monotone(nodes: Seq<Op>, left: nat, right: nat)
    requires left <= right <= nodes.len(),
    ensures guards(nodes, left) <= guards(nodes, right),
    decreases right - left,
{
    if right > left {
        guards_monotone(nodes, left, (right - 1) as nat);
        guards_step(nodes, (right - 1) as nat);
    }
}

pub(in super::super) open spec fn shape(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    kinds: Seq<Kind>, terms: Seq<Term>,
) -> bool {
    kinds.len() == terms.len() && definitions.len() == base + terms.len()
        && forall|j: int| 0 <= j < terms.len() ==>
            definitions[base as int + j].0@ == names::generated(prefix, j as nat)
            && definitions[base as int + j].1
                == if node::boolean(kinds[j]) { Sort::Bool } else { Sort::Int }
            && match terms[j] {
                Term::Name(name) => name@ == definitions[base as int + j].0@,
                _ => false,
            }
}

/// The recorded body depends only on the meanings of its earlier typed sources.
#[verifier::opaque]
pub(in super::super) open spec fn body_fact(
    body: Term, op: Op, domains: Seq<Domain>, input: Seq<Term>, kind: Kind,
    previous_kinds: Seq<Kind>, previous_terms: Seq<Term>,
) -> bool {
    node::depends_on(body, input, previous_terms, previous_kinds)
        && (input.len() == domains.len() ==> admission::kind(
            op, domains, node::static_kinds(previous_kinds)) == Ok(node::boolean(kind)))
        && (forall|environment: Map<Seq<char>, denotation::Value>|
            node::typed_sources(input, previous_terms, previous_kinds, environment) ==>
                node::has_kind(denotation::term(body, environment), kind))
        && (forall|environment: Map<Seq<char>, denotation::Value>,
            raw_input: Seq<i64>, previous: Seq<i64>|
            node::input_values(domains, input, raw_input, environment)
                && node::prefix_values(previous_kinds, previous_terms, previous, environment) ==>
                node::refines(op, kind, body, raw_input, previous, environment))
}

/// Records exactly the actual node's erased postconditions for each stored body.
pub(in super::super) open spec fn bodies(
    definitions: Seq<(String, Sort, Term)>, base: nat,
    domains: Seq<Domain>, nodes: Seq<Op>, input: Seq<Term>,
    kinds: Seq<Kind>, terms: Seq<Term>,
) -> bool {
    kinds.len() == terms.len() && terms.len() <= nodes.len()
        && base + terms.len() == definitions.len()
        && forall|j: int| 0 <= j < terms.len() ==>
            body_fact(definitions[base as int + j].2, nodes[j], domains, input, kinds[j],
                kinds.subrange(0, j), terms.subrange(0, j))
}

pub(in super::super) open spec fn outside(value: int) -> bool {
    value < i64::MIN || value > i64::MAX
}

pub(in super::super) open spec fn guarded(
    nodes: Seq<Op>, terms: Seq<Term>, checked: Seq<Term>,
) -> bool {
    terms.len() <= nodes.len() && checked.len() == guards(nodes, terms.len())
        && forall|j: int| 0 <= j < terms.len() && arithmetic(nodes[j]) ==>
            guards(nodes, j as nat) < checked.len()
            && (forall|environment: Map<Seq<char>, denotation::Value>, value: int|
                denotation::term(terms[j], environment)
                    == Some(denotation::Value::Integer(value)) ==>
                denotation::term(checked[guards(nodes, j as nat) as int], environment)
                    == Some(denotation::Value::Boolean(outside(value))))
}

pub(in super::super) open spec fn booleans(
    terms: Seq<Term>, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    forall|j: int| 0 <= j < terms.len() ==>
        match denotation::term(#[trigger] terms[j], environment) {
            Some(denotation::Value::Boolean(_)) => true, _ => false,
        }
}

pub(in super::super) open spec fn meaning(
    nodes: Seq<Op>, raw_input: Seq<i64>, count: nat,
    input: Seq<Term>, kinds: Seq<Kind>, terms: Seq<Term>, checked: Seq<Term>,
    environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    kinds.len() == count && terms.len() == count
        && node::typed_sources(input, terms, kinds, environment)
        && booleans(checked, environment)
        && match runtime::prefix(nodes, raw_input, count) {
            Ok(values) => node::prefix_values(kinds, terms, values, environment)
                && denotation::junction(checked, checked.len(), environment, false)
                    == Some(denotation::Value::Boolean(false)),
            Err(RuntimeFailure::Arithmetic) =>
                denotation::junction(checked, checked.len(), environment, false)
                    == Some(denotation::Value::Boolean(true)),
            _ => false,
        }
}

/// The induction is over existing recorded definitions and prefix semantics.
/// The environment is fixed; no extension or existence claim occurs here.
pub(in super::super) proof fn prefix_refines(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, nodes: Seq<Op>, input: Seq<Term>,
    kinds: Seq<Kind>, terms: Seq<Term>, checked: Seq<Term>,
    raw_input: Seq<i64>, environment: Map<Seq<char>, denotation::Value>, count: nat,
)
    requires shape(definitions, base, prefix, kinds, terms),
        bodies(definitions, base, domains, nodes, input, kinds, terms),
        guarded(nodes, terms, checked),
        count <= terms.len(),
        node::input_values(domains, input, raw_input, environment),
        script_spec::definitions_hold(definitions, base, terms.len(), environment),
    ensures meaning(nodes, raw_input, count, input,
        kinds.subrange(0, count as int), terms.subrange(0, count as int),
        checked.subrange(0, guards(nodes, count) as int), environment),
    decreases count,
{
    broadcast use vstd::seq::group_seq_lemmas;
    guards_monotone(nodes, count, terms.len());
    if count == 0 {
        reveal_with_fuel(guards, 1);
        reveal_with_fuel(denotation::junction, 1);
        reveal_with_fuel(runtime::prefix, 1);
        assert(node::typed_sources(input, Seq::empty(), Seq::empty(), environment));
    } else {
        let j = (count - 1) as nat;
        prefix_refines(definitions, base, prefix, domains, nodes, input,
            kinds, terms, checked, raw_input, environment, j);
        guards_step(nodes, j);
        let previous_terms = terms.subrange(0, j as int);
        let previous_kinds = kinds.subrange(0, j as int);
        let previous_checked = checked.subrange(0, guards(nodes, j) as int);
        let next_terms = terms.subrange(0, count as int);
        let next_kinds = kinds.subrange(0, count as int);
        let next_checked = checked.subrange(0, guards(nodes, count) as int);
        let body = definitions[base as int + j as int].2;
        let wire = terms[j as int];
        let kind = kinds[j as int];
        assert(body_fact(body, nodes[j as int], domains, input, kind,
            previous_kinds, previous_terms));
        assert(next_terms =~= previous_terms.push(wire));
        assert(next_kinds =~= previous_kinds.push(kind));
        script_spec::definition_meaning(definitions, base, terms.len(), j, wire, body, environment);
        assert(node::typed_sources(input, previous_terms, previous_kinds, environment));
        assert(node::has_kind(denotation::term(body, environment), kind)) by {
            reveal(body_fact);
        }
        assert(denotation::term(wire, environment) == denotation::term(body, environment));
        assert forall|i: int| 0 <= i < next_terms.len() implies
            node::has_kind(denotation::term(next_terms[i], environment), next_kinds[i]) by {
            if i < j { assert(next_terms[i] == previous_terms[i]); }
            else { assert(i == j); }
        }
        assert(node::typed_sources(input, next_terms, next_kinds, environment));

        if arithmetic(nodes[j as int]) {
            assert(input.len() == domains.len());
            assert(admission::kind(nodes[j as int], domains,
                node::static_kinds(previous_kinds)) == Ok(node::boolean(kind))) by {
                reveal(body_fact);
            }
            assert(!node::boolean(kind));
            let value = match denotation::term(wire, environment) {
                Some(denotation::Value::Integer(value)) => value,
                _ => { assert(false); 0int },
            };
            assert(denotation::term(wire, environment)
                == Some(denotation::Value::Integer(value)));
            let guard = checked[guards(nodes, j) as int];
            assert(denotation::term(guard, environment)
                == Some(denotation::Value::Boolean(outside(value))));
            assert(next_checked =~= previous_checked.push(guard));
            assert forall|i: int| 0 <= i < next_checked.len() implies
                match denotation::term(#[trigger] next_checked[i], environment) {
                    Some(denotation::Value::Boolean(_)) => true, _ => false,
                } by {
                if i < previous_checked.len() { assert(next_checked[i] == previous_checked[i]); }
                else { assert(i == previous_checked.len()); }
            }
            denotation::disjunction_push(previous_checked, guard, environment);
        } else {
            assert(next_checked =~= previous_checked);
        }
        assert(booleans(next_checked, environment));

        // This case split is on the actual checked-i64 prefix, never a new evaluator.
        reveal_with_fuel(runtime::prefix, 1);
        match runtime::prefix(nodes, raw_input, j) {
            Ok(previous) => {
                assert(node::prefix_values(previous_kinds, previous_terms, previous, environment));
                assert(node::refines(nodes[j as int], kind, body, raw_input, previous, environment)) by {
                    reveal(body_fact);
                }
                match runtime::node(nodes[j as int], raw_input, previous) {
                    Ok(value) => {
                        assert(denotation::term(wire, environment)
                            == Some(node::wire_value(kind, value)));
                        assert forall|i: int| 0 <= i < previous.len() + 1 implies
                            (!node::boolean(next_kinds[i]) || 0 <= previous.push(value)[i] <= 1)
                            && denotation::term(next_terms[i], environment)
                                == Some(node::wire_value(next_kinds[i], previous.push(value)[i])) by {
                            if i < previous.len() { assert(i < j); }
                            else { assert(i == j); }
                        }
                        assert(node::prefix_values(next_kinds, next_terms, previous.push(value), environment));
                        if arithmetic(nodes[j as int]) {
                            assert(!outside(value as int));
                            assert(denotation::term(checked[guards(nodes, j) as int], environment)
                                == Some(denotation::Value::Boolean(false)));
                        }
                    },
                    Err(RuntimeFailure::Arithmetic) => {
                        assert(arithmetic(nodes[j as int]));
                        match nodes[j as int] {
                            Op::Add(a, b) | Op::Sub(a, b) => {
                                let value = match nodes[j as int] {
                                    Op::Add(..) => previous[a as int] as int + previous[b as int] as int,
                                    _ => previous[a as int] as int - previous[b as int] as int,
                                };
                                assert(outside(value));
                                assert(denotation::term(wire, environment)
                                    == Some(denotation::Value::Integer(value)));
                                assert(denotation::term(checked[guards(nodes, j) as int], environment)
                                    == Some(denotation::Value::Boolean(true)));
                            },
                            _ => {},
                        }
                    },
                    Err(_) => { assert(false); },
                }
            },
            Err(RuntimeFailure::Arithmetic) => {
                runtime::prefix_failure_persists(nodes, raw_input, j, count, RuntimeFailure::Arithmetic);
                assert(denotation::junction(previous_checked, previous_checked.len(), environment, false)
                    == Some(denotation::Value::Boolean(true)));
            },
            Err(_) => { assert(false); },
        }
    }
}

/// Gives the earliest actual failing attempt without retaining a failure counter.
pub(in super::super) proof fn earliest_arithmetic(nodes: Seq<Op>, input: Seq<i64>, count: nat)
    requires count <= nodes.len(), runtime::prefix(nodes, input, count) == Err(RuntimeFailure::Arithmetic),
    ensures exists|failed: nat, previous: Seq<i64>| failed < count
        && runtime::prefix(nodes, input, failed) == Ok(previous)
        && runtime::node(nodes[failed as int], input, previous) == Err(RuntimeFailure::Arithmetic),
    decreases count,
{
    reveal_with_fuel(runtime::prefix, 1);
    assert(count > 0);
    let j = (count - 1) as nat;
    match runtime::prefix(nodes, input, j) {
        Ok(previous) => {
            assert(runtime::node(nodes[j as int], input, previous) == Err(RuntimeFailure::Arithmetic));
            assert(exists|failed: nat, values: Seq<i64>| failed < count
                && runtime::prefix(nodes, input, failed) == Ok(values)
                && runtime::node(nodes[failed as int], input, values) == Err(RuntimeFailure::Arithmetic));
        },
        Err(RuntimeFailure::Arithmetic) => { earliest_arithmetic(nodes, input, j); },
        Err(_) => { assert(false); },
    }
}

}
