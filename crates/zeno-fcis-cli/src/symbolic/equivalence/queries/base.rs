//! Common native input declaration and two-program construction.
#[cfg(verus_keep_ghost)]
use super::spec;
#[cfg(verus_keep_ghost)]
use crate::script::spec as script_spec;
use crate::symbolic::program::{self, Encoded};
#[cfg(verus_keep_ghost)]
use crate::symbolic::smt::Values;
use crate::symbolic::smt::{Script, Term};
#[cfg(verus_keep_ghost)]
use crate::term::denotation;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// The common native construction body; all executable arguments stay unrestricted.
#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    match result {
        Ok((script, left, right)) => spec::base(script.definitions@,
            original.inputs@, original.nodes@, candidate.inputs@, candidate.nodes@)
            && spec::constants_prefix(script.constants@, original.inputs@, original.inputs@.len())
            && script.observations@.len() == 0 && script.assertions@.len() == 0
            && left.outputs@.len() == original.outputs@.len()
            && right.outputs@.len() == candidate.outputs@.len()
            && spec::pair_meaning(script.definitions@, original, candidate, left, right)
            && (original.inputs@ == candidate.inputs@ ==>
                spec::runtime_supported(original) && spec::runtime_supported(candidate)),
        Err(_) => true,
    },
))]
pub(super) fn base(
    original: program::Program<'_>,
    candidate: program::Program<'_>,
    purpose: &str,
) -> Result<(Script, Encoded, Encoded), crate::symbolic::Unsupported> {
    let mut script = Script::default();
    #[cfg(verus_keep_ghost)]
    proof! {
        script_spec::empty_fields(script);
        reveal(<Term as View>::view);
        reveal_strlit("x");
        reveal_strlit("p");
        reveal_strlit("q");
        assert("x"@ =~= seq!['x']);
        assert("p"@ =~= seq!['p']);
        assert("q"@ =~= seq!['q']);
    }
    script.comment(&format!("zeno-fcis/transform-symbolic/1 {purpose}"));
    let mut input = Vec::with_capacity(original.inputs.len());
    #[cfg(verus_keep_ghost)]
    proof! { program::bridge::empty_input_names(); }
    let mut index = 0;
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant index <= original.inputs@.len(), input@.len() == index,
            program::bridge::input_names(input@), script.definitions@.len() == 0,
            spec::constants_prefix(script.constants@, original.inputs@, index as nat),
            script.observations@.len() == 0, script.assertions@.len() == 0,
        decreases original.inputs@.len() - index,
    ))]
    while index < original.inputs.len() {
        let domain = &original.inputs[index];
        let name = crate::symbolic::smt::indexed_name("x", index);
        let values = program::input_values(*domain)?;
        #[cfg(verus_keep_ghost)]
        let old_constants = verus_exec_expr!(Ghost(script.constants@));
        script.constant(&name, values);
        let wire = Term::name(&name);
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<Term as View>::view);
            reveal(<Values as View>::view);
            reveal_strlit("x");
            assert("x"@ =~= seq!['x']);
            assert(name@ == crate::term::names::generated(seq!['x'], index as nat));
            program::bridge::input_name_push(input@, wire@);
            spec::constants_push(old_constants@, script.constants@, original.inputs@, index as nat);
        }
        input.push(wire);
        index += 1;
    }
    let left = program::encode(&mut script, "p", original, &input)?;
    #[cfg(verus_keep_ghost)]
    let left_definitions = verus_exec_expr!(Ghost(script.definitions@));
    let right = program::encode(&mut script, "q", candidate, &input)?;
    #[cfg(verus_keep_ghost)]
    proof! {
        spec::record_base(left_definitions@, script.definitions@, original.inputs@,
            original.nodes@, candidate.inputs@, candidate.nodes@, input@);
        program::bridge::block_length(left_definitions@, 0, seq!['p'],
            original.inputs@, original.nodes@, input@);
        program::bridge::block_length(script.definitions@, left_definitions@.len(), seq!['q'],
            candidate.inputs@, candidate.nodes@, input@);
        assert forall|environment: Map<Seq<char>, denotation::Value>, raw: Seq<i64>|
            original.inputs@ == candidate.inputs@
                && crate::evaluation::spec::admitted(original.inputs@, raw)
                && program::bridge::raw_inputs(raw, environment)
                && script_spec::definitions_hold(script.definitions@, 0,
                    script.definitions@.len(), environment) implies
            program::bridge::outcome(original.inputs@, original.outputs@, original.nodes@,
                original.roots@, raw, left.outputs@, left.trap, left.admitted, environment)
            && program::bridge::outcome(candidate.inputs@, candidate.outputs@, candidate.nodes@,
                candidate.roots@, raw, right.outputs@, right.trap, right.admitted, environment) by {
            program::bridge::named_input_values(original.inputs@, input@, raw, environment);
            assert forall|j: int| 0 <= j < left_definitions@.len() implies
                environment.contains_key(left_definitions@[j].0@)
                && denotation::term(left_definitions@[j].2, environment)
                    == Some(environment[left_definitions@[j].0@]) by {
                assert(left_definitions@[j] == script.definitions@[j]);
            }
            assert(script_spec::definitions_hold(left_definitions@, 0,
                original.nodes@.len(), environment));
            assert(script_spec::definitions_hold(script.definitions@, left_definitions@.len(),
                candidate.nodes@.len(), environment));
        }
        assert forall|raw: Seq<i64>|
            original.inputs@ == candidate.inputs@
                && crate::evaluation::spec::admitted(original.inputs@, raw) implies
                spec::represented_result(original, raw) && spec::represented_result(candidate, raw) by {
            let environment = spec::model_for_base(script.definitions@, original.inputs@,
                original.nodes@, candidate.inputs@, candidate.nodes@, raw);
            assert(program::bridge::outcome(original.inputs@, original.outputs@, original.nodes@,
                original.roots@, raw, left.outputs@, left.trap, left.admitted, environment));
            assert(program::bridge::outcome(candidate.inputs@, candidate.outputs@, candidate.nodes@,
                candidate.roots@, raw, right.outputs@, right.trap, right.admitted, environment));
        }
    }
    Ok((script, left, right))
}
