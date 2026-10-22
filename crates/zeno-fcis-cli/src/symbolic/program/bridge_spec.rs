//! Erased caller bridge for actual successful eager-definition blocks.
//! The public signatures hide the private node Kind and retain actual bodies.

use super::{node, nodes};
use crate::evaluation::{Failure as RuntimeFailure, spec as runtime};
use crate::script::spec as script_spec;
use crate::symbolic::smt::{Sort, Term};
use crate::term::{denotation, names};
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::{Domain, Op};

verus! {

/// Exact successful outputs and the two runtime refusals represented by a
/// successfully encoded scalar program. Boolean terms must remain defined even
/// when an earlier eager instruction overflowed.
pub(crate) open spec fn outcome(
    input_domains: Seq<Domain>, output_domains: Seq<Domain>,
    instructions: Seq<Op>, roots: Seq<u16>, raw: Seq<i128>,
    outputs: Seq<Term>, trap: Term, admitted: Term,
    environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    (forall|i: int| 0 <= i < outputs.len() ==>
        match denotation::term(#[trigger] outputs[i], environment) {
            Some(denotation::Value::Integer(_)) => true, _ => false,
        }) && (match denotation::term(trap, environment) {
        Some(denotation::Value::Boolean(_)) => true, _ => false,
    }) && (match denotation::term(admitted, environment) {
        Some(denotation::Value::Boolean(_)) => true, _ => false,
    }) && match runtime::execution(input_domains, output_domains, instructions, roots, raw) {
        Ok(values) => denotation::term(trap, environment) == Some(denotation::Value::Boolean(false))
            && denotation::term(admitted, environment) == Some(denotation::Value::Boolean(true))
            && super::roots::spec::output_values(outputs, values, environment),
        Err(RuntimeFailure::Arithmetic) =>
            denotation::term(trap, environment) == Some(denotation::Value::Boolean(true)),
        Err(RuntimeFailure::OutputDomain) =>
            denotation::term(trap, environment) == Some(denotation::Value::Boolean(false))
            && denotation::term(admitted, environment) == Some(denotation::Value::Boolean(false)),
        _ => false,
    }
}

/// The root owner's exact wire values preserve ordered result equality.
pub(crate) proof fn output_values_equal(
    left: Seq<Term>, right: Seq<Term>, left_values: Seq<i128>, right_values: Seq<i128>,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires left.len() == right.len(),
        super::roots::spec::output_values(left, left_values, environment),
        super::roots::spec::output_values(right, right_values, environment),
    ensures (left_values == right_values) ==
        (forall|i: int| 0 <= i < left.len() ==>
            denotation::term(#[trigger] left[i], environment)
                == denotation::term(right[i], environment)),
{
    if left_values == right_values {
        assert forall|i: int| 0 <= i < left.len() implies
            denotation::term(#[trigger] left[i], environment)
                == denotation::term(right[i], environment) by {
            assert(left_values[i] == right_values[i]);
        }
    } else if forall|i: int| 0 <= i < left.len() ==>
        denotation::term(#[trigger] left[i], environment)
            == denotation::term(right[i], environment) {
        assert forall|i: int| 0 <= i < left_values.len() implies
            #[trigger] left_values[i] == right_values[i] by {
            assert(denotation::term(left[i], environment)
                == Some(denotation::Value::Integer(left_values[i] as int)));
            assert(denotation::term(right[i], environment)
                == Some(denotation::Value::Integer(right_values[i] as int)));
        }
        assert(left_values =~= right_values);
    }
}

pub(crate) closed spec fn block(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, instructions: Seq<Op>, input: Seq<Term>,
) -> bool {
    exists|witness: (Seq<node::Kind>, Seq<Term>)|
        witness.1.len() == instructions.len()
        && nodes::spec::shape(definitions, base, prefix, witness.0, witness.1)
        && nodes::spec::bodies(definitions, base, domains, instructions,
            input, witness.0, witness.1)
}

/// Called with the actual successful helper's vectors before roots consume them.
pub(super) proof fn record_block(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, instructions: Seq<Op>, input: Seq<Term>,
    kinds: Seq<node::Kind>, terms: Seq<Term>,
)
    requires terms.len() == instructions.len(),
        nodes::spec::shape(definitions, base, prefix, kinds, terms),
        nodes::spec::bodies(definitions, base, domains, instructions, input, kinds, terms),
    ensures block(definitions, base, prefix, domains, instructions, input),
{
    reveal(block);
    let witness = (kinds, terms);
    assert(witness.1.len() == instructions.len()
        && nodes::spec::shape(definitions, base, prefix, witness.0, witness.1)
        && nodes::spec::bodies(definitions, base, domains, instructions,
            input, witness.0, witness.1));
    assert(exists|witness: (Seq<node::Kind>, Seq<Term>)|
        witness.1.len() == instructions.len()
        && nodes::spec::shape(definitions, base, prefix, witness.0, witness.1)
        && nodes::spec::bodies(definitions, base, domains, instructions,
            input, witness.0, witness.1));
}

pub(crate) proof fn block_length(
    definitions: Seq<(String, Sort, Term)>, base: nat, prefix: Seq<char>,
    domains: Seq<Domain>, instructions: Seq<Op>, input: Seq<Term>,
)
    requires block(definitions, base, prefix, domains, instructions, input),
    ensures definitions.len() == base + instructions.len(),
{
    reveal(block);
    let witness = choose|witness: (Seq<node::Kind>, Seq<Term>)|
        witness.1.len() == instructions.len()
        && nodes::spec::shape(definitions, base, prefix, witness.0, witness.1)
        && nodes::spec::bodies(definitions, base, domains, instructions,
            input, witness.0, witness.1);
}

pub(crate) closed spec fn input_names(input: Seq<Term>) -> bool {
    nodes::realization::scalar_input_names(input)
}

pub(crate) proof fn empty_input_names()
    ensures input_names(Seq::<Term>::empty()),
{
    reveal(input_names);
}

pub(crate) proof fn input_name_push(input: Seq<Term>, wire: Term)
    requires input_names(input),
        match wire {
            Term::Name(name) => name@ == names::generated(seq!['x'], input.len()),
            _ => false,
        },
    ensures input_names(input.push(wire)),
{
    reveal(input_names);
    assert forall|i: int| 0 <= i < input.push(wire).len() implies
        match #[trigger] input.push(wire)[i] {
            Term::Name(name) => name@ == names::generated(seq!['x'], i as nat),
            _ => false,
        } by {
        if i < input.len() { assert(input.push(wire)[i] == input[i]); }
        else { assert(i == input.len() && input.push(wire)[i] == wire); }
    }
}

pub(crate) open spec fn raw_inputs(
    raw: Seq<i128>, environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    forall|i: int| 0 <= i < raw.len() ==>
        #[trigger] environment.contains_key(names::generated(seq!['x'], i as nat))
        && environment[names::generated(seq!['x'], i as nat)]
            == denotation::Value::Integer(raw[i] as int)
}

/// Generated input names denote the supplied admitted runtime tuple.
pub(crate) proof fn named_input_values(
    domains: Seq<Domain>, input: Seq<Term>, raw: Seq<i128>,
    environment: Map<Seq<char>, denotation::Value>,
)
    requires input_names(input), input.len() == raw.len(),
        runtime::admitted(domains, raw), raw_inputs(raw, environment),
    ensures node::input_values(domains, input, raw, environment),
{
    reveal(input_names);
    assert forall|i: int| 0 <= i < raw.len() implies
        denotation::term(input[i], environment)
            == Some(denotation::Value::Integer(raw[i] as int)) by {
        assert(match input[i] {
            Term::Name(name) => name@ == names::generated(seq!['x'], i as nat),
            _ => false,
        });
        reveal_with_fuel(denotation::term, 1);
    }
}

/// Chooses actual stored-block witnesses and invokes the existing triangular
/// realization proof. No admission or domain/input-length premise is needed.
pub(crate) proof fn model_for_pair(
    left_definitions: Seq<(String, Sort, Term)>, definitions: Seq<(String, Sort, Term)>,
    left_domains: Seq<Domain>, left_instructions: Seq<Op>,
    right_domains: Seq<Domain>, right_instructions: Seq<Op>,
    input: Seq<Term>, raw: Seq<i128>,
) -> (environment: Map<Seq<char>, denotation::Value>)
    requires input_names(input), raw.len() == input.len(),
        block(left_definitions, 0, seq!['p'], left_domains, left_instructions, input),
        block(definitions, left_definitions.len(), seq!['q'],
            right_domains, right_instructions, input),
        definitions.subrange(0, left_definitions.len() as int) == left_definitions,
    ensures script_spec::definitions_hold(definitions, 0, definitions.len(), environment),
        raw_inputs(raw, environment),
{
    reveal(block);
    reveal(input_names);
    let left = choose|witness: (Seq<node::Kind>, Seq<Term>)|
        witness.1.len() == left_instructions.len()
        && nodes::spec::shape(left_definitions, 0, seq!['p'], witness.0, witness.1)
        && nodes::spec::bodies(left_definitions, 0, left_domains, left_instructions,
            input, witness.0, witness.1);
    let right = choose|witness: (Seq<node::Kind>, Seq<Term>)|
        witness.1.len() == right_instructions.len()
        && nodes::spec::shape(definitions, left_definitions.len(), seq!['q'], witness.0, witness.1)
        && nodes::spec::bodies(definitions, left_definitions.len(), right_domains, right_instructions,
            input, witness.0, witness.1);
    let environment = nodes::realization::scalar_pair_model(left_definitions, definitions,
        left_domains, left_instructions, left.0, left.1,
        right_domains, right_instructions, right.0, right.1, input, raw);
    assert forall|i: int| 0 <= i < raw.len() implies
        #[trigger] environment.contains_key(names::generated(seq!['x'], i as nat))
        && environment[names::generated(seq!['x'], i as nat)]
            == denotation::Value::Integer(raw[i] as int) by {
        reveal_with_fuel(denotation::term, 1);
        assert(denotation::term(input[i], environment)
            == Some(denotation::Value::Integer(raw[i] as int)));
    }
    assert(raw_inputs(raw, environment));
    environment
}

}
