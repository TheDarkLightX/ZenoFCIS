//! Erased realization of actual eager definition windows. No node evaluator.
//! Each inserted value is the denotation of an actual stored definition body.

use super::super::node;
use super::spec;
use super::{Domain, Kind, Op, Sort, Term};
use crate::script::spec as script_spec;
use crate::term::{denotation, names};
use vstd::prelude::*;

verus! {

pub(in super::super) open spec fn extends(
    left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>,
) -> bool {
    forall|name: Seq<char>| #[trigger] left.contains_key(name) ==>
        right.contains_key(name) && right[name] == left[name]
}

pub(in super::super) open spec fn added_only(
    initial: Map<Seq<char>, denotation::Value>, result: Map<Seq<char>, denotation::Value>,
    prefix: Seq<char>, count: nat,
) -> bool {
    forall|name: Seq<char>| #[trigger] result.contains_key(name) ==>
        initial.contains_key(name)
        || exists|j: int| 0 <= j < count && name == #[trigger] names::generated(prefix, j as nat)
}

pub(in super::super) open spec fn named(terms: Seq<Term>) -> bool {
    forall|i: int| 0 <= i < terms.len() ==>
        match #[trigger] terms[i] { Term::Name(_) => true, _ => false }
}

pub(in super::super) open spec fn scalar_input_names(input: Seq<Term>) -> bool {
    forall|i: int| 0 <= i < input.len() ==>
        match #[trigger] input[i] {
            Term::Name(name) => name@ == names::generated(seq!['x'], i as nat),
            _ => false,
        }
}

proof fn name_frame(
    wire: Term, left: Map<Seq<char>, denotation::Value>,
    right: Map<Seq<char>, denotation::Value>,
)
    requires match wire { Term::Name(_) => true, _ => false },
        denotation::term(wire, left).is_some(), extends(left, right),
    ensures denotation::term(wire, left) == denotation::term(wire, right),
{
    match wire {
        Term::Name(name) => {
            reveal_with_fuel(denotation::term, 1);
            assert(left.contains_key(name@));
            assert(right.contains_key(name@) && right[name@] == left[name@]);
        },
        _ => { assert(false); },
    }
}

proof fn named_sources_frame(
    input: Seq<Term>, terms: Seq<Term>, kinds: Seq<Kind>,
    left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>,
)
    requires named(input), named(terms), node::typed_sources(input, terms, kinds, left),
        extends(left, right),
    ensures node::typed_sources(input, terms, kinds, right),
        node::sources_equal(input, terms, left, right),
{
    assert forall|i: int| 0 <= i < input.len() implies
        denotation::term(input[i], left) == denotation::term(input[i], right) by {
        assert(denotation::term(input[i], left).is_some());
        name_frame(input[i], left, right);
    }
    assert forall|i: int| 0 <= i < terms.len() implies
        denotation::term(terms[i], left) == denotation::term(terms[i], right) by {
        assert(denotation::term(terms[i], left).is_some());
        name_frame(terms[i], left, right);
    }
    assert(node::sources_equal(input, terms, left, right));
    assert(node::typed_sources(input, terms, kinds, right));
}

proof fn typed_prefix(
    input: Seq<Term>, terms: Seq<Term>, kinds: Seq<Kind>,
    environment: Map<Seq<char>, denotation::Value>, count: nat,
)
    requires node::typed_sources(input, terms, kinds, environment), count <= terms.len(),
    ensures node::typed_sources(input, terms.subrange(0, count as int),
        kinds.subrange(0, count as int), environment),
{
    assert forall|i: int| 0 <= i < count implies
        terms.subrange(0, count as int)[i] == terms[i]
        && kinds.subrange(0, count as int)[i] == kinds[i] by {}
}

/// Unfold only this body's opaque record, not all recorded quantified clauses.
proof fn body_total(
    body: Term, op: Op, domains: Seq<Domain>, input: Seq<Term>, kind: Kind,
    previous_kinds: Seq<Kind>, previous_terms: Seq<Term>,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires spec::body_fact(body, op, domains, input, kind, previous_kinds, previous_terms),
        node::typed_sources(input, previous_terms, previous_kinds, environment),
    ensures node::has_kind(denotation::term(body, environment), kind),
{
    reveal(spec::body_fact);
}

proof fn body_frame(
    body: Term, op: Op, domains: Seq<Domain>, input: Seq<Term>, kind: Kind,
    previous_kinds: Seq<Kind>, previous_terms: Seq<Term>,
    left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>,
)
    requires spec::body_fact(body, op, domains, input, kind, previous_kinds, previous_terms),
        node::typed_sources(input, previous_terms, previous_kinds, left),
        node::typed_sources(input, previous_terms, previous_kinds, right),
        node::sources_equal(input, previous_terms, left, right),
    ensures denotation::term(body, left) == denotation::term(body, right),
{
    reveal(spec::body_fact);
    assert(node::depends_on(body, input, previous_terms, previous_kinds));
}

proof fn shape_named(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    kinds: Seq<Kind>, terms: Seq<Term>,
)
    requires spec::shape(definitions, base, prefix, kinds, terms),
    ensures named(terms),
{
    reveal(spec::shape);
    assert forall|i: int| 0 <= i < terms.len() implies
        match #[trigger] terms[i] { Term::Name(_) => true, _ => false } by {
        assert(definitions[base as int + i].1
            == if node::boolean(kinds[i]) { Sort::Bool } else { Sort::Int });
        assert(definitions[base as int + i].0@ == names::generated(prefix, i as nat));
        assert(match terms[i] {
            Term::Name(name) => name@ == definitions[base as int + i].0@,
            _ => false,
        });
    }
}

/// Construct a witness for any actual successful definition prefix, including
/// the prefix retained on refusal. Premises constrain only erased realization.
pub(in super::super) proof fn realize(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, nodes: Seq<Op>, input: Seq<Term>,
    kinds: Seq<Kind>, terms: Seq<Term>,
    initial: Map<Seq<char>, denotation::Value>, count: nat,
) -> (result: Map<Seq<char>, denotation::Value>)
    requires spec::shape(definitions, base, prefix, kinds, terms),
        spec::bodies(definitions, base, domains, nodes, input, kinds, terms),
        count <= terms.len(), named(input),
        node::typed_sources(input, Seq::empty(), Seq::empty(), initial),
        forall|j: int| 0 <= j < count ==>
            !initial.contains_key(#[trigger] names::generated(prefix, j as nat)),
    ensures extends(initial, result), added_only(initial, result, prefix, count),
        node::typed_sources(input, terms.subrange(0, count as int),
            kinds.subrange(0, count as int), result),
        script_spec::definitions_hold(definitions, base, count, result),
    decreases count,
{
    broadcast use vstd::seq::group_seq_lemmas;
    shape_named(definitions, base, prefix, kinds, terms);
    if count == 0 {
        assert(terms.subrange(0, 0) =~= Seq::empty());
        assert(kinds.subrange(0, 0) =~= Seq::empty());
        initial
    } else {
        let j = (count - 1) as nat;
        let left = realize(definitions, base, prefix, domains, nodes, input,
            kinds, terms, initial, j);
        let previous_terms = terms.subrange(0, j as int);
        let previous_kinds = kinds.subrange(0, j as int);
        let body = definitions[base as int + j as int].2;
        let kind = kinds[j as int];
        body_total(body, nodes[j as int], domains, input, kind,
            previous_kinds, previous_terms, left);
        let value = match denotation::term(body, left) {
            Some(value) => value,
            None => { assert(false); denotation::Value::Integer(0) },
        };
        assert(denotation::term(body, left) == Some(value));
        assert(node::has_kind(Some(value), kind));
        let name = names::generated(prefix, j);
        assert(definitions[base as int + j as int].0@ == name);
        assert forall|i: int| 0 <= i < j implies
            #[trigger] names::generated(prefix, i as nat) != name by {
            names::fresh_index(prefix, i as nat, j);
        }
        if left.contains_key(name) {
            assert(!initial.contains_key(name));
            assert(exists|i: int| 0 <= i < j && name == #[trigger] names::generated(prefix, i as nat));
            let i = choose|i: int| 0 <= i < j && name == #[trigger] names::generated(prefix, i as nat);
            names::fresh_index(prefix, i as nat, j);
            assert(false);
        }
        let right = left.insert(name, value);
        assert forall|key: Seq<char>| left.contains_key(key) implies
            right.contains_key(key) && right[key] == left[key] by {
            assert(key != name);
        }
        assert(extends(left, right));
        assert forall|key: Seq<char>| initial.contains_key(key) implies
            right.contains_key(key) && right[key] == initial[key] by {
            assert(left.contains_key(key) && left[key] == initial[key]);
        }
        assert(extends(initial, right));
        assert forall|key: Seq<char>| right.contains_key(key) implies
            initial.contains_key(key)
            || exists|i: int| 0 <= i < count && key == #[trigger] names::generated(prefix, i as nat) by {
            if key == name {
                assert(exists|i: int| 0 <= i < count && key == #[trigger] names::generated(prefix, i as nat));
            } else {
                assert(left.contains_key(key));
                if !initial.contains_key(key) {
                    assert(exists|i: int| 0 <= i < j && key == #[trigger] names::generated(prefix, i as nat));
                    let i = choose|i: int| 0 <= i < j && key == #[trigger] names::generated(prefix, i as nat);
                    assert(exists|i: int| 0 <= i < count && key == #[trigger] names::generated(prefix, i as nat));
                }
            }
        }
        assert(added_only(initial, right, prefix, count));
        assert(named(previous_terms));
        named_sources_frame(input, previous_terms, previous_kinds, left, right);
        body_frame(body, nodes[j as int], domains, input, kind,
            previous_kinds, previous_terms, left, right);
        assert(denotation::term(body, right) == Some(value));
        assert(right.contains_key(name) && right[name] == value);

        assert forall|i: int| base <= i < base + count implies
            right.contains_key(definitions[i].0@)
            && denotation::term(definitions[i].2, right) == Some(right[definitions[i].0@]) by {
            if i < base + j {
                let offset = (i - base) as nat;
                let old_terms = terms.subrange(0, offset as int);
                let old_kinds = kinds.subrange(0, offset as int);
                typed_prefix(input, previous_terms, previous_kinds, left, offset);
                typed_prefix(input, previous_terms, previous_kinds, right, offset);
                assert(previous_terms.subrange(0, offset as int) =~= old_terms);
                assert(previous_kinds.subrange(0, offset as int) =~= old_kinds);
                assert(named(old_terms));
                named_sources_frame(input, old_terms, old_kinds, left, right);
                body_frame(definitions[i].2, nodes[offset as int], domains, input,
                    kinds[offset as int], old_kinds, old_terms, left, right);
                assert(left.contains_key(definitions[i].0@));
                assert(right[definitions[i].0@] == left[definitions[i].0@]);
            } else {
                assert(i == base + j);
            }
        }
        assert(script_spec::definitions_hold(definitions, base, count, right));
        let next_terms = terms.subrange(0, count as int);
        let next_kinds = kinds.subrange(0, count as int);
        assert(next_terms.len() == count && next_kinds.len() == count);
        assert forall|i: int| 0 <= i < count implies
            node::has_kind(denotation::term(next_terms[i], right), next_kinds[i]) by {
            if i < j {
                assert(next_terms[i] == previous_terms[i] && next_kinds[i] == previous_kinds[i]);
            } else {
                assert(i == j && next_terms[i] == terms[j as int]);
                reveal_with_fuel(denotation::term, 1);
                assert(denotation::term(next_terms[i], right) == Some(value));
            }
        }
        assert forall|i: int| 0 <= i < next_terms.len() implies
            match #[trigger] denotation::term(next_terms[i], right) {
                Some(denotation::Value::Boolean(_)) => node::boolean(next_kinds[i]),
                Some(denotation::Value::Integer(_)) => !node::boolean(next_kinds[i]),
                None => false,
            } by {
            assert(node::has_kind(denotation::term(next_terms[i], right), next_kinds[i]));
        }
        assert(node::typed_sources(input, next_terms, next_kinds, right));
        right
    }
}

/// Existing block equations survive a later extension, because their actual
/// bodies depend only on these retained named inputs and earlier named wires.
pub(in super::super) proof fn preserve_block(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, nodes: Seq<Op>, input: Seq<Term>,
    kinds: Seq<Kind>, terms: Seq<Term>,
    left: Map<Seq<char>, denotation::Value>, right: Map<Seq<char>, denotation::Value>,
)
    requires spec::shape(definitions, base, prefix, kinds, terms),
        spec::bodies(definitions, base, domains, nodes, input, kinds, terms),
        named(input), node::typed_sources(input, terms, kinds, left),
        script_spec::definitions_hold(definitions, base, terms.len(), left), extends(left, right),
    ensures node::typed_sources(input, terms, kinds, right),
        script_spec::definitions_hold(definitions, base, terms.len(), right),
{
    shape_named(definitions, base, prefix, kinds, terms);
    named_sources_frame(input, terms, kinds, left, right);
    assert forall|j: int| base <= j < base + terms.len() implies
        right.contains_key(definitions[j].0@)
        && denotation::term(definitions[j].2, right) == Some(right[definitions[j].0@]) by {
        let offset = (j - base) as nat;
        let previous_terms = terms.subrange(0, offset as int);
        let previous_kinds = kinds.subrange(0, offset as int);
        typed_prefix(input, terms, kinds, left, offset);
        typed_prefix(input, terms, kinds, right, offset);
        assert(named(previous_terms));
        named_sources_frame(input, previous_terms, previous_kinds, left, right);
        body_frame(definitions[j].2, nodes[offset as int], domains, input,
            kinds[offset as int], previous_kinds, previous_terms, left, right);
        assert(left.contains_key(definitions[j].0@));
        assert(right[definitions[j].0@] == left[definitions[j].0@]);
    }
}

/// Direct input-map witness. This maps supplied raw data, not instructions.
pub(in super::super) proof fn input_environment(input: Seq<Term>, raw: Seq<i128>)
    -> (environment: Map<Seq<char>, denotation::Value>)
    requires scalar_input_names(input), input.len() == raw.len(),
    ensures node::typed_sources(input, Seq::empty(), Seq::empty(), environment),
        forall|i: int| 0 <= i < input.len() ==>
            denotation::term(input[i], environment) == Some(denotation::Value::Integer(raw[i] as int)),
        forall|name: Seq<char>| environment.contains_key(name) <==>
            exists|i: int| 0 <= i < input.len() && name == #[trigger] names::generated(seq!['x'], i as nat),
    decreases input.len(),
{
    broadcast use vstd::seq::group_seq_lemmas;
    if input.len() == 0 {
        Map::empty()
    } else {
        let last = input.len() - 1;
        let prefix_input = input.subrange(0, last);
        let prefix_raw = raw.subrange(0, last);
        assert(scalar_input_names(prefix_input));
        let before = input_environment(prefix_input, prefix_raw);
        let name = names::generated(seq!['x'], last as nat);
        assert(!before.contains_key(name)) by {
            if before.contains_key(name) {
                let j = choose|j: int| 0 <= j < last
                    && name == #[trigger] names::generated(seq!['x'], j as nat);
                names::fresh_index(seq!['x'], j as nat, last as nat);
            }
        }
        let environment = before.insert(name, denotation::Value::Integer(raw[last] as int));
        assert forall|key: Seq<char>| before.contains_key(key) implies
            environment.contains_key(key) && environment[key] == before[key] by {
            assert(key != name);
        }
        assert(extends(before, environment));
        assert forall|i: int| 0 <= i < input.len() implies
            denotation::term(input[i], environment)
                == Some(denotation::Value::Integer(raw[i] as int)) by {
            if i < last {
                assert(input[i] == prefix_input[i] && raw[i] == prefix_raw[i]);
                name_frame(input[i], before, environment);
            } else {
                assert(i == last);
                reveal_with_fuel(denotation::term, 1);
            }
        }
        assert forall|key: Seq<char>| #[trigger] environment.contains_key(key) <==>
            (exists|i: int| 0 <= i < input.len()
                && key == #[trigger] names::generated(seq!['x'], i as nat)) by {
            if environment.contains_key(key) {
                if key == name {
                    assert(exists|i: int| 0 <= i < input.len()
                        && key == #[trigger] names::generated(seq!['x'], i as nat));
                } else {
                    assert(before.contains_key(key));
                    let i = choose|i: int| 0 <= i < last
                        && key == #[trigger] names::generated(seq!['x'], i as nat);
                    assert(exists|j: int| 0 <= j < input.len()
                        && key == #[trigger] names::generated(seq!['x'], j as nat));
                }
            }
            if exists|i: int| 0 <= i < input.len()
                && key == #[trigger] names::generated(seq!['x'], i as nat) {
                let i = choose|i: int| 0 <= i < input.len()
                    && key == #[trigger] names::generated(seq!['x'], i as nat);
                if i < last {
                    assert(before.contains_key(key));
                } else { assert(i == last && key == name); }
            }
        }
        environment
    }
}

proof fn scalar_fresh(
    input: Seq<Term>, initial: Map<Seq<char>, denotation::Value>, namespace: char, count: nat,
)
    requires scalar_input_names(input), namespace == 'p' || namespace == 'q',
        forall|name: Seq<char>| initial.contains_key(name) ==>
            exists|i: int| 0 <= i < input.len() && name == #[trigger] names::generated(seq!['x'], i as nat),
    ensures named(input),
        forall|j: int| 0 <= j < count ==>
            !initial.contains_key(#[trigger] names::generated(seq![namespace], j as nat)),
{
    assert forall|j: int| 0 <= j < count implies
        !initial.contains_key(#[trigger] names::generated(seq![namespace], j as nat)) by {
        let name = names::generated(seq![namespace], j as nat);
        if initial.contains_key(name) {
            assert(exists|i: int| 0 <= i < input.len() && name == #[trigger] names::generated(seq!['x'], i as nat));
            let i = choose|i: int| 0 <= i < input.len() && name == #[trigger] names::generated(seq!['x'], i as nat);
            names::disjoint_namespaces('x', namespace, i as nat, j as nat);
            assert(false);
        }
    }
}

/// Actual query-builder arrangement: Name(x_i), all p definitions, then all q
/// definitions. The returned map is a concrete joint model of both windows.
pub(in super::super) proof fn scalar_pair_model(
    left_definitions: Seq<(String, Sort, Term)>, definitions: Seq<(String, Sort, Term)>,
    left_domains: Seq<Domain>, left_nodes: Seq<Op>, left_kinds: Seq<Kind>, left_terms: Seq<Term>,
    right_domains: Seq<Domain>, right_nodes: Seq<Op>, right_kinds: Seq<Kind>, right_terms: Seq<Term>,
    input: Seq<Term>, raw: Seq<i128>,
) -> (environment: Map<Seq<char>, denotation::Value>)
    requires scalar_input_names(input), input.len() == raw.len(),
        spec::shape(left_definitions, 0, seq!['p'], left_kinds, left_terms),
        spec::bodies(left_definitions, 0, left_domains, left_nodes, input, left_kinds, left_terms),
        spec::shape(definitions, left_definitions.len(), seq!['q'], right_kinds, right_terms),
        spec::bodies(definitions, left_definitions.len(), right_domains, right_nodes,
            input, right_kinds, right_terms),
        definitions.subrange(0, left_definitions.len() as int) == left_definitions,
    ensures node::typed_sources(input, left_terms, left_kinds, environment),
        node::typed_sources(input, right_terms, right_kinds, environment),
        script_spec::definitions_hold(definitions, 0, definitions.len(), environment),
        forall|i: int| 0 <= i < input.len() ==>
            denotation::term(input[i], environment) == Some(denotation::Value::Integer(raw[i] as int)),
{
    broadcast use vstd::seq::group_seq_lemmas;
    let initial = input_environment(input, raw);
    scalar_fresh(input, initial, 'p', left_terms.len());
    let left = realize(left_definitions, 0, seq!['p'], left_domains, left_nodes,
        input, left_kinds, left_terms, initial, left_terms.len());
    assert(left_terms.subrange(0, left_terms.len() as int) =~= left_terms);
    assert(left_kinds.subrange(0, left_kinds.len() as int) =~= left_kinds);
    scalar_fresh(input, initial, 'q', right_terms.len());
    assert forall|j: int| 0 <= j < right_terms.len() implies
        !left.contains_key(#[trigger] names::generated(seq!['q'], j as nat)) by {
        let name = names::generated(seq!['q'], j as nat);
        if left.contains_key(name) {
            assert(!initial.contains_key(name));
            assert(exists|i: int| 0 <= i < left_terms.len() && name == #[trigger] names::generated(seq!['p'], i as nat));
            let i = choose|i: int| 0 <= i < left_terms.len() && name == #[trigger] names::generated(seq!['p'], i as nat);
            names::disjoint_namespaces('p', 'q', i as nat, j as nat);
            assert(false);
        }
    }
    typed_prefix(input, left_terms, left_kinds, left, 0);
    assert(left_terms.subrange(0, 0) =~= Seq::empty());
    assert(left_kinds.subrange(0, 0) =~= Seq::empty());
    let environment = realize(definitions, left_definitions.len(), seq!['q'], right_domains,
        right_nodes, input, right_kinds, right_terms, left, right_terms.len());
    assert(right_terms.subrange(0, right_terms.len() as int) =~= right_terms);
    assert(right_kinds.subrange(0, right_kinds.len() as int) =~= right_kinds);
    preserve_block(left_definitions, 0, seq!['p'], left_domains, left_nodes,
        input, left_kinds, left_terms, left, environment);
    assert forall|j: int| 0 <= j < definitions.len() implies
        environment.contains_key(definitions[j].0@)
        && denotation::term(definitions[j].2, environment) == Some(environment[definitions[j].0@]) by {
        if j < left_definitions.len() { assert(definitions[j] == left_definitions[j]); }
        else { assert(left_definitions.len() <= j); }
    }
    assert(script_spec::definitions_hold(definitions, 0, definitions.len(), environment));
    assert forall|i: int| 0 <= i < input.len() implies
        denotation::term(input[i], environment) == Some(denotation::Value::Integer(raw[i] as int)) by {
        name_frame(input[i], initial, left);
        name_frame(input[i], left, environment);
    }
    environment
}

}
