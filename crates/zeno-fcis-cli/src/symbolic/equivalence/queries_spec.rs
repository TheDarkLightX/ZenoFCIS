//! Erased witness retained by the actual common query-base constructor.
//! Only its actual definitions and generated input names are covered here.

use crate::script::spec as script_spec;
use crate::symbolic::program::{Encoded, Program, bridge};
use crate::symbolic::smt::{Sort, Term, Values};
use crate::term::{denotation, names};
use vstd::prelude::*;
use zeno_fcis_synthesis::finite::{Domain, Op};

verus! {

/// The actual domain partition, erased from the shipped query builder.
pub(crate) enum Piece {
    Outputs(Option<int>),
    Arithmetic,
    OutputDomain,
    Control,
}

pub(crate) open spec fn assertions_hold(script: crate::symbolic::smt::Script,
    environment: Map<Seq<char>, denotation::Value>) -> bool {
    forall|i: int| 0 <= i < script.assertions@.len() ==>
        denotation::term(#[trigger] script.assertions@[i], environment)
            == Some(denotation::Value::Boolean(true))
}

pub(crate) open spec fn execution(program: Program<'_>, raw: Seq<i128>)
    -> Result<Seq<i128>, crate::evaluation::Failure> {
    crate::evaluation::spec::execution(program.inputs@, program.outputs@,
        program.nodes@, program.roots@, raw)
}

pub(crate) open spec fn represented_result(program: Program<'_>, raw: Seq<i128>) -> bool {
    match execution(program, raw) {
        Ok(_) | Err(crate::evaluation::Failure::Arithmetic)
            | Err(crate::evaluation::Failure::OutputDomain) => true,
        _ => false,
    }
}

/// Successful construction represents every admitted tuple, including failures.
pub(crate) open spec fn runtime_supported(program: Program<'_>) -> bool {
    forall|raw: Seq<i128>| crate::evaluation::spec::admitted(program.inputs@, raw) ==>
        represented_result(program, raw)
}

pub(crate) open spec fn expected(piece: Piece, original: Program<'_>,
    candidate: Program<'_>, raw: Seq<i128>) -> bool {
    let left = execution(original, raw);
    let right = execution(candidate, raw);
    match piece {
        Piece::Outputs(first) => match left {
            Ok(values) => (match first {
                None => true,
                Some(value) => values.len() > 0 && values[0] as int == value,
            }) && right != left,
            Err(_) => false,
        },
        Piece::Arithmetic => left == Err(crate::evaluation::Failure::Arithmetic)
            && right != left,
        Piece::OutputDomain => left == Err(crate::evaluation::Failure::OutputDomain)
            && right != left,
        Piece::Control => match (left, right) {
            (Ok(a), Ok(b)) => if a.len() > 0 && b.len() > 0 {
                b[0] as int + 1 != a[0] as int
            } else { true },
            _ => false,
        },
    }
}

/// Conditions supplied by the checked caller and actual common-base witness.
/// Model existence is established separately by model_for_base.
pub(crate) open spec fn context(script: crate::symbolic::smt::Script,
    original: Program<'_>, candidate: Program<'_>, raw: Seq<i128>,
    environment: Map<Seq<char>, denotation::Value>) -> bool {
    original.inputs@ == candidate.inputs@ && original.outputs@ == candidate.outputs@
        && crate::evaluation::spec::admitted(original.inputs@, raw)
        && bridge::raw_inputs(raw, environment)
        && script_spec::definitions_hold(script.definitions@, 0, script.definitions@.len(), environment)
}

pub(crate) open spec fn query(script: crate::symbolic::smt::Script,
    piece: Piece, original: Program<'_>, candidate: Program<'_>) -> bool {
    base(script.definitions@, original.inputs@, original.nodes@, candidate.inputs@, candidate.nodes@)
        && constants_prefix(script.constants@, original.inputs@, original.inputs@.len())
        && (original.inputs@ == candidate.inputs@ ==>
            runtime_supported(original) && runtime_supported(candidate))
        && forall|environment: Map<Seq<char>, denotation::Value>, raw: Seq<i128>|
            context(script, original, candidate, raw, environment) ==>
                assertions_hold(script, environment) == expected(piece, original, candidate, raw)
}

/// Logical constraints printed for the actual input declarations.
pub(crate) open spec fn declarations_hold(script: crate::symbolic::smt::Script,
    environment: Map<Seq<char>, denotation::Value>) -> bool {
    forall|i: int| 0 <= i < script.constants@.len() ==>
        environment.contains_key((#[trigger] script.constants@[i]).0@)
        && match (script.constants@[i].1, environment[script.constants@[i].0@]) {
            (Values::Range { min, max }, denotation::Value::Integer(value)) =>
                min as int <= value <= max as int,
            _ => false,
        }
}

/// A concrete model exists for each admitted input; query meaning is nonvacuous.
pub(crate) proof fn model_for_query(script: crate::symbolic::smt::Script,
    piece: Piece, original: Program<'_>, candidate: Program<'_>, raw: Seq<i128>)
    -> (environment: Map<Seq<char>, denotation::Value>)
    requires query(script, piece, original, candidate),
        original.inputs@ == candidate.inputs@, original.outputs@ == candidate.outputs@,
        crate::evaluation::spec::admitted(original.inputs@, raw),
    ensures context(script, original, candidate, raw, environment),
        declarations_hold(script, environment),
        assertions_hold(script, environment) == expected(piece, original, candidate, raw),
{
    let environment = model_for_base(script.definitions@, original.inputs@, original.nodes@,
        candidate.inputs@, candidate.nodes@, raw);
    assert forall|i: int| 0 <= i < script.constants@.len() implies
        environment.contains_key((#[trigger] script.constants@[i]).0@)
        && match (script.constants@[i].1, environment[script.constants@[i].0@]) {
            (Values::Range { min, max }, denotation::Value::Integer(value)) =>
                min as int <= value <= max as int,
            _ => false,
        } by {
        assert(0 <= i < raw.len());
        assert(script.constants@[i].0@ == names::generated(seq!['x'], i as nat));
        assert(environment.contains_key(names::generated(seq!['x'], i as nat)));
        assert(environment[names::generated(seq!['x'], i as nat)]
            == denotation::Value::Integer(raw[i] as int));
        assert(crate::evaluation::spec::contains(original.inputs@[i], raw[i]));
    }
    environment
}

pub(crate) open spec fn bounds(domain: Domain) -> (int, int) {
    match domain { Domain::Bool => (0, 1), Domain::Int { min, max } => (min as int, max as int) }
}

pub(crate) open spec fn split(domains: Seq<Domain>) -> bool {
    domains.len() > 0 && bounds(domains[0]).0 <= bounds(domains[0]).1
        && bounds(domains[0]).1 - bounds(domains[0]).0 < super::MAX_CASE_PIECES as int
}

pub(crate) open spec fn output_piece_count(domains: Seq<Domain>) -> nat {
    if split(domains) { (bounds(domains[0]).1 - bounds(domains[0]).0 + 1) as nat }
    else { 1nat }
}

pub(crate) open spec fn partition_piece(domains: Seq<Domain>, index: nat) -> Piece {
    let count = output_piece_count(domains);
    if index < count {
        Piece::Outputs(if split(domains) { Some(bounds(domains[0]).0 + index as int) } else { None })
    } else if index == count { Piece::Arithmetic }
    else { Piece::OutputDomain }
}

/// A successful runtime output belongs to the declared first-output range.
/// Arithmetic and output-domain failures need no output value to select a piece.
pub(crate) proof fn partition_covers(original: Program<'_>, candidate: Program<'_>, raw: Seq<i128>)
    requires match execution(original, raw) {
        Ok(_) | Err(crate::evaluation::Failure::Arithmetic)
            | Err(crate::evaluation::Failure::OutputDomain) => true,
        _ => false,
    },
    ensures (execution(original, raw) != execution(candidate, raw)) ==
        (exists|i: nat| i < output_piece_count(original.outputs@) + 2
            && expected(partition_piece(original.outputs@, i), original, candidate, raw)),
{
    let count = output_piece_count(original.outputs@);
    match execution(original, raw) {
        Ok(values) => {
            assert(crate::evaluation::spec::admitted(original.outputs@, values));
            if split(original.outputs@) {
                assert(values.len() > 0);
                assert(bounds(original.outputs@[0]).0 <= values[0] as int <= bounds(original.outputs@[0]).1);
                let index = (values[0] as int - bounds(original.outputs@[0]).0) as nat;
                assert(index < count);
                if execution(original, raw) != execution(candidate, raw) {
                    assert(expected(partition_piece(original.outputs@, index), original, candidate, raw));
                }
            } else if execution(original, raw) != execution(candidate, raw) {
                assert(expected(partition_piece(original.outputs@, 0nat), original, candidate, raw));
            }
        },
        Err(crate::evaluation::Failure::Arithmetic) => {
            if execution(original, raw) != execution(candidate, raw) {
                assert(expected(partition_piece(original.outputs@, count), original, candidate, raw));
            }
        },
        Err(crate::evaluation::Failure::OutputDomain) => {
            if execution(original, raw) != execution(candidate, raw) {
                assert(expected(partition_piece(original.outputs@, count + 1), original, candidate, raw));
            }
        },
        _ => {},
    }
    assert forall|i: nat| i < count + 2 &&
        expected(partition_piece(original.outputs@, i), original, candidate, raw) implies
            execution(original, raw) != execution(candidate, raw) by {}
}

pub(crate) open spec fn domain_values(domain: Domain, values: Values) -> bool {
    match (domain, values) {
        (Domain::Bool, Values::Range { min, max }) => min == 0 && max == 1,
        (Domain::Int { min, max }, Values::Range { min: lo, max: hi }) =>
            lo == min as int && hi == max as int,
        _ => false,
    }
}

pub(crate) open spec fn constants_prefix(
    constants: Seq<(String, Values)>, domains: Seq<Domain>, count: nat,
) -> bool {
    constants.len() == count && count <= domains.len()
        && forall|i: int| 0 <= i < count ==>
            (#[trigger] constants[i]).0@ == names::generated(seq!['x'], i as nat)
            && domain_values(domains[i], constants[i].1)
}

pub(crate) proof fn constants_push(
    before: Seq<(String, Values)>, after: Seq<(String, Values)>,
    domains: Seq<Domain>, count: nat,
)
    requires constants_prefix(before, domains, count), count < domains.len(),
        after.len() == count + 1, after.subrange(0, count as int) == before,
        after[count as int].0@ == names::generated(seq!['x'], count),
        domain_values(domains[count as int], after[count as int].1),
    ensures constants_prefix(after, domains, count + 1),
{
    assert forall|i: int| 0 <= i < count + 1 implies
        (#[trigger] after[i]).0@ == names::generated(seq!['x'], i as nat)
        && domain_values(domains[i], after[i].1) by {
        if i < count {
            assert(after[i] == before[i]);
            assert(before[i].0@ == names::generated(seq!['x'], i as nat));
            assert(domain_values(domains[i], before[i].1));
        } else {
            assert(i == count);
            assert(after[i].0@ == names::generated(seq!['x'], count));
            assert(domain_values(domains[i], after[i].1));
        }
    }
    assert(after.len() == count + 1 && count + 1 <= domains.len());
}

/// Both actual returned encodings have their respective runtime meanings under
/// the shared declared input ABI. Different ABIs are refused by the caller.
pub(crate) open spec fn pair_meaning(
    definitions: Seq<(String, Sort, Term)>,
    original: Program<'_>, candidate: Program<'_>, left: Encoded, right: Encoded,
) -> bool {
    forall|environment: Map<Seq<char>, denotation::Value>, raw: Seq<i128>|
        original.inputs@ == candidate.inputs@
            && crate::evaluation::spec::admitted(original.inputs@, raw)
            && bridge::raw_inputs(raw, environment)
            && script_spec::definitions_hold(definitions, 0, definitions.len(), environment) ==>
        bridge::outcome(original.inputs@, original.outputs@, original.nodes@,
            original.roots@, raw, left.outputs@, left.trap, left.admitted, environment)
        && bridge::outcome(candidate.inputs@, candidate.outputs@, candidate.nodes@,
            candidate.roots@, raw, right.outputs@, right.trap, right.admitted, environment)
}

pub(crate) closed spec fn base(
    definitions: Seq<(String, Sort, Term)>,
    left_domains: Seq<Domain>, left_instructions: Seq<Op>,
    right_domains: Seq<Domain>, right_instructions: Seq<Op>,
) -> bool {
    definitions.len() == left_instructions.len() + right_instructions.len()
        && exists|input: Seq<Term>|
            input.len() == left_domains.len() && bridge::input_names(input)
            && bridge::block(definitions.subrange(0, left_instructions.len() as int),
                0, seq!['p'], left_domains, left_instructions, input)
            && bridge::block(definitions, left_instructions.len(), seq!['q'],
                right_domains, right_instructions, input)
}

pub(super) proof fn record_base(
    left_definitions: Seq<(String, Sort, Term)>, definitions: Seq<(String, Sort, Term)>,
    left_domains: Seq<Domain>, left_instructions: Seq<Op>,
    right_domains: Seq<Domain>, right_instructions: Seq<Op>, input: Seq<Term>,
)
    requires input.len() == left_domains.len(), bridge::input_names(input),
        bridge::block(left_definitions, 0, seq!['p'], left_domains, left_instructions, input),
        bridge::block(definitions, left_definitions.len(), seq!['q'],
            right_domains, right_instructions, input),
        definitions.subrange(0, left_definitions.len() as int) == left_definitions,
    ensures base(definitions, left_domains, left_instructions, right_domains, right_instructions),
{
    bridge::block_length(left_definitions, 0, seq!['p'], left_domains, left_instructions, input);
    bridge::block_length(definitions, left_definitions.len(), seq!['q'],
        right_domains, right_instructions, input);
    reveal(base);
    assert(exists|supplied: Seq<Term>|
        supplied.len() == left_domains.len() && bridge::input_names(supplied)
        && bridge::block(definitions.subrange(0, left_instructions.len() as int),
            0, seq!['p'], left_domains, left_instructions, supplied)
        && bridge::block(definitions, left_instructions.len(), seq!['q'],
            right_domains, right_instructions, supplied));
}

/// Direct consumer: existence follows from the actual two block witnesses and
/// generated-name input witness, never from a conditional fixed-map theorem.
pub(crate) proof fn model_for_base(
    definitions: Seq<(String, Sort, Term)>,
    left_domains: Seq<Domain>, left_instructions: Seq<Op>,
    right_domains: Seq<Domain>, right_instructions: Seq<Op>, raw: Seq<i128>,
) -> (environment: Map<Seq<char>, denotation::Value>)
    requires base(definitions, left_domains, left_instructions, right_domains, right_instructions),
        raw.len() == left_domains.len(),
    ensures script_spec::definitions_hold(definitions, 0, definitions.len(), environment),
        bridge::raw_inputs(raw, environment),
{
    reveal(base);
    let input = choose|input: Seq<Term>|
        input.len() == left_domains.len() && bridge::input_names(input)
        && bridge::block(definitions.subrange(0, left_instructions.len() as int),
            0, seq!['p'], left_domains, left_instructions, input)
        && bridge::block(definitions, left_instructions.len(), seq!['q'],
            right_domains, right_instructions, input);
    let left_definitions = definitions.subrange(0, left_instructions.len() as int);
    assert(left_definitions.len() == left_instructions.len());
    bridge::model_for_pair(left_definitions, definitions, left_domains, left_instructions,
        right_domains, right_instructions, input, raw)
}

}
