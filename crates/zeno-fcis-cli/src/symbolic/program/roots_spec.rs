//! Erased observations of the actual ordered root/output loop.
//! Projection and domain membership are the existing runtime specifications.

use super::super::node;
use super::{Domain, Failure, Kind, Term};
use crate::evaluation::spec as runtime;
use crate::term::denotation;
use vstd::prelude::*;

verus! {

pub(in super::super) open spec fn matching(domain: Domain, kind: Kind) -> bool {
    matches!((domain, kind), (Domain::Bool, Kind::Bool) | (Domain::Int { .. }, Kind::Int))
}

pub(in super::super) open spec fn ready(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>, position: int,
) -> bool {
    0 <= position < domains.len() && position < roots.len()
        && roots[position] < terms.len() && roots[position] < kinds.len()
        && matching(domains[position], kinds[roots[position] as int])
}

pub(in super::super) open spec fn ready_prefix(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>, count: nat,
) -> bool {
    count <= domains.len() && count <= roots.len()
        && forall|j: int| 0 <= j < count ==> ready(domains, roots, terms, kinds, j)
}

pub(in super::super) open spec fn acceptable(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>,
) -> bool {
    domains.len() == roots.len() && ready_prefix(domains, roots, terms, kinds, roots.len())
}

/// Exact first CLI refusal, including precedence over later roots.
pub(in super::super) open spec fn refusal(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>, failure: Failure,
) -> bool {
    match failure {
        Failure::Count => domains.len() != roots.len(),
        Failure::Missing { position, root } => domains.len() == roots.len()
            && position < roots.len() && root == roots[position as int] as usize
            && ready_prefix(domains, roots, terms, kinds, position as nat)
            && (root >= terms.len() || root >= kinds.len()),
        Failure::Domain { position } => domains.len() == roots.len()
            && position < roots.len()
            && ready_prefix(domains, roots, terms, kinds, position as nat)
            && roots[position as int] < terms.len() && roots[position as int] < kinds.len()
            && !matching(domains[position as int], kinds[roots[position as int] as int]),
    }
}

pub(in super::super) open spec fn integer(domain: Domain) -> bool {
    matches!(domain, Domain::Int { .. })
}

/// Number of actual within guards; Boolean outputs append no guard.
#[verifier::opaque]
pub(in super::super) open spec fn guards(domains: Seq<Domain>, count: nat) -> nat
    recommends count <= domains.len(),
    decreases count,
{
    if count == 0 || count > domains.len() { 0 }
    else { guards(domains, (count - 1) as nat)
        + if integer(domains[count as int - 1]) { 1nat } else { 0nat } }
}

pub(in super::super) proof fn guards_step(domains: Seq<Domain>, count: nat)
    requires count < domains.len(),
    ensures guards(domains, count + 1) == guards(domains, count)
        + if integer(domains[count as int]) { 1nat } else { 0nat },
{
    reveal_with_fuel(guards, 1);
}

proof fn guards_monotone(domains: Seq<Domain>, left: nat, right: nat)
    requires left <= right <= domains.len(),
    ensures guards(domains, left) <= guards(domains, right),
    decreases right - left,
{
    if right > left {
        guards_monotone(domains, left, (right - 1) as nat);
        guards_step(domains, (right - 1) as nat);
    }
}

/// Int cloning preserves even partial meaning; Boolean conversion requires Bool.
pub(in super::super) open spec fn output_fact(source: Term, output: Term, kind: Kind) -> bool {
    forall|environment: Map<Seq<char>, denotation::Value>|
        denotation::term(output, environment) == if node::boolean(kind) {
            match denotation::term(source, environment) {
                Some(denotation::Value::Boolean(value)) =>
                    Some(denotation::Value::Integer(if value { 1int } else { 0int })),
                _ => None,
            }
        } else { denotation::term(source, environment) }
}

pub(in super::super) open spec fn guard_fact(domain: Domain, source: Term, guard: Term) -> bool {
    match domain {
        Domain::Int { min, max } => forall|environment: Map<Seq<char>, denotation::Value>, value: int|
            denotation::term(source, environment) == Some(denotation::Value::Integer(value)) ==>
                denotation::term(guard, environment)
                    == Some(denotation::Value::Boolean(min <= value && value <= max)),
        _ => false,
    }
}

pub(in super::super) open spec fn facts(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>, count: nat,
    outputs: Seq<Term>, admitted: Seq<Term>,
) -> bool {
    ready_prefix(domains, roots, terms, kinds, count)
        && outputs.len() == count && admitted.len() == guards(domains, count)
        && (forall|j: int| 0 <= j < count ==>
            output_fact(terms[roots[j] as int], outputs[j], kinds[roots[j] as int]))
        && (forall|j: int| 0 <= j < count && integer(domains[j]) ==>
            guards(domains, j as nat) < admitted.len()
            && guard_fact(domains[j], terms[roots[j] as int], admitted[guards(domains, j as nat) as int]))
}

pub(in super::super) open spec fn booleans(
    items: Seq<Term>, count: nat, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    count <= items.len() && forall|j: int| 0 <= j < count ==>
        match denotation::term(#[trigger] items[j], environment) {
            Some(denotation::Value::Boolean(_)) => true, _ => false,
        }
}

pub(in super::super) open spec fn typed(
    outputs: Seq<Term>, admitted: Seq<Term>, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    (forall|j: int| 0 <= j < outputs.len() ==>
        match denotation::term(#[trigger] outputs[j], environment) {
            Some(denotation::Value::Integer(_)) => true, _ => false,
        }) && booleans(admitted, admitted.len(), environment)
}

pub(in super::super) open spec fn projected(values: Seq<i64>, roots: Seq<u16>, count: nat) -> Seq<i64> {
    Seq::new(count, |j: int| values[roots[j] as int])
}

pub(crate) open spec fn output_values(
    outputs: Seq<Term>, values: Seq<i64>, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    outputs.len() == values.len() && forall|j: int| 0 <= j < values.len() ==>
        denotation::term(outputs[j], environment) == Some(denotation::Value::Integer(values[j] as int))
}

pub(in super::super) open spec fn meaning(
    domains: Seq<Domain>, roots: Seq<u16>, outputs: Seq<Term>, admitted: Seq<Term>,
    values: Seq<i64>, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    domains.len() == roots.len()
        && runtime::projection(values, roots, roots.len()) == Ok(projected(values, roots, roots.len()))
        && output_values(outputs, projected(values, roots, roots.len()), environment)
        && booleans(admitted, admitted.len(), environment)
        && denotation::junction(admitted, admitted.len(), environment, true)
            == Some(denotation::Value::Boolean(runtime::admitted(domains, projected(values, roots, roots.len()))))
}

/// Includes ordered/repeated roots, with no set conversion or uniqueness premise.
proof fn projection_ordered(values: Seq<i64>, roots: Seq<u16>, count: nat)
    requires count <= roots.len(), forall|j: int| 0 <= j < count ==> roots[j] < values.len(),
    ensures runtime::projection(values, roots, count) == Ok(projected(values, roots, count)),
    decreases count,
{
    broadcast use vstd::seq::group_seq_lemmas;
    reveal_with_fuel(runtime::projection, 1);
    if count == 0 {
        assert(projected(values, roots, count) =~= Seq::empty());
    } else {
        let j = (count - 1) as nat;
        projection_ordered(values, roots, j);
        assert(projected(values, roots, count)
            =~= projected(values, roots, j).push(values[roots[j as int] as int]));
    }
}

proof fn admitted_step(domains: Seq<Domain>, values: Seq<i64>, domain: Domain, value: i64)
    requires domains.len() == values.len(),
    ensures runtime::admitted(domains.push(domain), values.push(value))
        == (runtime::admitted(domains, values) && runtime::contains(domain, value)),
{
    broadcast use vstd::seq::group_seq_lemmas;
    if runtime::admitted(domains.push(domain), values.push(value)) {
        assert forall|j: int| 0 <= j < domains.len() implies
            runtime::contains(domains[j], values[j]) by {
            assert(domains.push(domain)[j] == domains[j]);
            assert(values.push(value)[j] == values[j]);
        }
        assert(runtime::contains(domains.push(domain)[domains.len() as int],
            values.push(value)[values.len() as int]));
        assert(runtime::contains(domain, value));
    }
    if runtime::admitted(domains, values) && runtime::contains(domain, value) {
        assert forall|j: int| 0 <= j < domains.len() + 1 implies
            runtime::contains(domains.push(domain)[j], values.push(value)[j]) by {
            if j < domains.len() {} else { assert(j == domains.len()); }
        }
    }
}

/// No canonical i64 premise is needed for typed totality after an eager overflow.
pub(in super::super) proof fn typed_refines(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>,
    outputs: Seq<Term>, admitted: Seq<Term>, environment: Map<Seq<char>, denotation::Value>,
)
    requires domains.len() == roots.len(),
        facts(domains, roots, terms, kinds, roots.len(), outputs, admitted),
        node::typed_sources(Seq::empty(), terms, kinds, environment),
    ensures typed(outputs, admitted, environment),
{
    assert forall|j: int| 0 <= j < outputs.len() implies
        match denotation::term(#[trigger] outputs[j], environment) {
            Some(denotation::Value::Integer(_)) => true, _ => false,
        } by {
        assert(ready(domains, roots, terms, kinds, j));
        let root = roots[j] as int;
        assert(node::has_kind(denotation::term(terms[root], environment), kinds[root]));
        assert(output_fact(terms[root], outputs[j], kinds[root]));
    }
    typed_guards(domains, roots, terms, kinds, admitted, environment, roots.len());
}

proof fn typed_guards(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>,
    admitted: Seq<Term>, environment: Map<Seq<char>, denotation::Value>, count: nat,
)
    requires domains.len() == roots.len(), count <= roots.len(),
        ready_prefix(domains, roots, terms, kinds, roots.len()),
        admitted.len() == guards(domains, roots.len()),
        forall|j: int| 0 <= j < roots.len() && integer(domains[j]) ==>
            ready(domains, roots, terms, kinds, j)
            && guards(domains, j as nat) < admitted.len()
            && guard_fact(domains[j], terms[roots[j] as int], admitted[guards(domains, j as nat) as int]),
        node::typed_sources(Seq::empty(), terms, kinds, environment),
    ensures booleans(admitted, guards(domains, count), environment),
    decreases count,
{
    guards_monotone(domains, count, roots.len());
    if count == 0 {
        reveal_with_fuel(guards, 1);
    } else {
        let j = (count - 1) as nat;
        typed_guards(domains, roots, terms, kinds, admitted, environment, j);
        guards_step(domains, j);
        if integer(domains[j as int]) {
            assert(ready(domains, roots, terms, kinds, j as int));
            let root = roots[j as int] as int;
            assert(!node::boolean(kinds[root]));
            let value = match denotation::term(terms[root], environment) {
                Some(denotation::Value::Integer(value)) => value,
                _ => { assert(false); 0int },
            };
            assert(denotation::term(terms[root], environment)
                == Some(denotation::Value::Integer(value)));
            assert(guard_fact(domains[j as int], terms[root], admitted[guards(domains, j) as int]));
            match domains[j as int] {
                Domain::Int { min, max } => {
                    assert(denotation::term(admitted[guards(domains, j) as int], environment)
                        == Some(denotation::Value::Boolean(min <= value && value <= max)));
                },
                _ => {},
            }
            assert forall|i: int| 0 <= i < guards(domains, count) implies
                match denotation::term(#[trigger] admitted[i], environment) {
                    Some(denotation::Value::Boolean(_)) => true, _ => false,
                } by {
                if i < guards(domains, j) {} else { assert(i == guards(domains, j)); }
            }
        }
    }
}

/// Induction over the existing runtime projection and actual emitted guards.
/// The fixed environment and canonical prefix values are supplied, not created.
pub(in super::super) proof fn canonical_prefix(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>,
    outputs: Seq<Term>, admitted: Seq<Term>, values: Seq<i64>,
    environment: Map<Seq<char>, denotation::Value>, count: nat,
)
    requires domains.len() == roots.len(), count <= roots.len(),
        facts(domains, roots, terms, kinds, roots.len(), outputs, admitted),
        node::prefix_values(kinds, terms, values, environment),
    ensures runtime::projection(values, roots, count) == Ok(projected(values, roots, count)),
        output_values(outputs.subrange(0, count as int), projected(values, roots, count), environment),
        booleans(admitted, guards(domains, count), environment),
        denotation::junction(admitted, guards(domains, count), environment, true)
            == Some(denotation::Value::Boolean(runtime::admitted(
                domains.subrange(0, count as int), projected(values, roots, count)))),
    decreases count,
{
    broadcast use vstd::seq::group_seq_lemmas;
    guards_monotone(domains, count, roots.len());
    assert forall|j: int| 0 <= j < count implies roots[j] < values.len() by {
        assert(ready(domains, roots, terms, kinds, j));
    }
    projection_ordered(values, roots, count);
    if count == 0 {
        reveal_with_fuel(guards, 1);
        reveal_with_fuel(denotation::junction, 1);
        assert(domains.subrange(0, 0) =~= Seq::empty());
        assert(projected(values, roots, 0) =~= Seq::empty());
        assert(outputs.subrange(0, 0) =~= Seq::empty());
    } else {
        let j = (count - 1) as nat;
        canonical_prefix(domains, roots, terms, kinds, outputs, admitted, values, environment, j);
        guards_step(domains, j);
        assert(ready(domains, roots, terms, kinds, j as int));
        let root = roots[j as int] as int;
        let value = values[root];
        assert(output_fact(terms[root], outputs[j as int], kinds[root]));
        assert(denotation::term(terms[root], environment) == Some(node::wire_value(kinds[root], value)));
        assert(domains.subrange(0, count as int) =~= domains.subrange(0, j as int).push(domains[j as int]));
        assert(projected(values, roots, count) =~= projected(values, roots, j).push(value));
        admitted_step(domains.subrange(0, j as int), projected(values, roots, j), domains[j as int], value);
        match domains[j as int] {
            Domain::Bool => {
                assert(node::boolean(kinds[root]));
                assert(0 <= value <= 1);
                assert((if value == 1 { 1int } else { 0int }) == value as int);
                assert(runtime::contains(domains[j as int], value));
            },
            Domain::Int { min, max } => {
                assert(!node::boolean(kinds[root]));
                assert(guard_fact(domains[j as int], terms[root], admitted[guards(domains, j) as int]));
                assert(denotation::term(admitted[guards(domains, j) as int], environment)
                    == Some(denotation::Value::Boolean(runtime::contains(domains[j as int], value))));
                reveal_with_fuel(denotation::junction, 1);
                assert forall|i: int| 0 <= i < guards(domains, count) implies
                    match denotation::term(#[trigger] admitted[i], environment) {
                        Some(denotation::Value::Boolean(_)) => true, _ => false,
                    } by {
                    if i < guards(domains, j) {} else { assert(i == guards(domains, j)); }
                }
            },
        }
        assert(denotation::term(outputs[j as int], environment)
            == Some(denotation::Value::Integer(value as int)));
        assert forall|i: int| 0 <= i < count implies
            denotation::term(outputs.subrange(0, count as int)[i], environment)
                == Some(denotation::Value::Integer(projected(values, roots, count)[i] as int)) by {
            if i < j { assert(outputs.subrange(0, count as int)[i] == outputs.subrange(0, j as int)[i]); }
            else { assert(i == j); }
        }
    }
}

/// A value returned by the actual Term::and constructor, via its owner contract.
pub(in super::super) proof fn folded_refines(
    domains: Seq<Domain>, roots: Seq<u16>, terms: Seq<Term>, kinds: Seq<Kind>,
    outputs: Seq<Term>, guards: Seq<Term>, admitted: Term, values: Seq<i64>,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires domains.len() == roots.len(),
        facts(domains, roots, terms, kinds, roots.len(), outputs, guards),
        node::prefix_values(kinds, terms, values, environment),
        denotation::conjunction_result(guards, admitted),
    ensures runtime::projection(values, roots, roots.len()) == Ok(projected(values, roots, roots.len())),
        output_values(outputs, projected(values, roots, roots.len()), environment),
        denotation::term(admitted, environment) == Some(denotation::Value::Boolean(
            runtime::admitted(domains, projected(values, roots, roots.len())))),
{
    canonical_prefix(domains, roots, terms, kinds, outputs, guards, values, environment, roots.len());
    broadcast use vstd::seq::group_seq_lemmas;
    assert(domains.subrange(0, domains.len() as int) =~= domains);
    assert(outputs.subrange(0, outputs.len() as int) =~= outputs);
    denotation::conjunction_meaning(guards, admitted, environment);
}

/// No-root raw programs are accepted by this helper even though library program
/// admission separately requires a nonempty output list. Eager nodes still run.
pub(in super::super) proof fn empty_folded_projection(
    outputs: Seq<Term>, guards: Seq<Term>, admitted: Term, values: Seq<i64>,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires outputs.len() == 0, guards.len() == 0,
        denotation::conjunction_result(guards, admitted),
    ensures runtime::projection(values, Seq::empty(), 0) == Ok(Seq::<i64>::empty()),
        runtime::admitted(Seq::empty(), Seq::empty()),
        output_values(outputs, Seq::empty(), environment),
        denotation::term(admitted, environment) == Some(denotation::Value::Boolean(true)),
{
    broadcast use vstd::seq::group_seq_lemmas;
    assert(outputs =~= Seq::empty());
    assert(guards =~= Seq::empty());
    reveal_with_fuel(runtime::projection, 1);
    reveal_with_fuel(denotation::junction, 1);
    denotation::conjunction_meaning(guards, admitted, environment);
}

/// OutputDomain belongs after a successful actual eager prefix. Earlier Arithmetic
/// is owned by the prefix proof; it is not inferred from these output guards.
pub(in super::super) proof fn scalar_execution_after_prefix(
    input_domains: Seq<Domain>, domains: Seq<Domain>, nodes: Seq<crate::evaluation::Op>,
    roots: Seq<u16>, input: Seq<i64>, values: Seq<i64>, terms: Seq<Term>, kinds: Seq<Kind>,
    outputs: Seq<Term>, guards: Seq<Term>, admitted: Term,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires runtime::admitted(input_domains, input),
        runtime::prefix(nodes, input, nodes.len()) == Ok(values),
        domains.len() == roots.len(),
        facts(domains, roots, terms, kinds, roots.len(), outputs, guards),
        node::prefix_values(kinds, terms, values, environment),
        denotation::conjunction_result(guards, admitted),
    ensures output_values(outputs, projected(values, roots, roots.len()), environment),
        runtime::execution(input_domains, domains, nodes, roots, input) ==
            match denotation::term(admitted, environment) {
                Some(denotation::Value::Boolean(true)) => Ok(projected(values, roots, roots.len())),
                _ => Err(crate::evaluation::Failure::OutputDomain),
            },
        match denotation::term(admitted, environment) {
            Some(denotation::Value::Boolean(_)) => true, _ => false,
        },
{
    folded_refines(domains, roots, terms, kinds, outputs, guards, admitted, values, environment);
}

}
