//! Pure comparison-query construction, shared by the CLI and its proof subject.
//! Solver execution, runtime replay and receipt publication stay in the caller.

use crate::symbolic::program;
use crate::symbolic::smt::Script;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[path = "queries_spec.rs"]
pub(super) mod spec;

/// Maximum inclusive first-output values handled by separate difference pieces.
#[cfg_attr(verus_keep_ghost, verus_spec)]
pub(crate) const MAX_CASE_PIECES: u128 = 256;

#[path = "queries/base.rs"]
mod construction;
use construction::base;
#[path = "queries/compare.rs"]
mod comparison;
use comparison::same;

#[path = "queries/pieces.rs"]
mod construction_pieces;

/// One query of a check, ready to run.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct Prepared {
    pub(super) label: String,
    pub(super) meaning: String,
    #[cfg(test)]
    pub(super) script: Script,
    pub(super) source: String,
    /// Erased copy of the exact script whose checked printer produced `source`.
    #[cfg(verus_keep_ghost)]
    pub(super) witness: Ghost<Script>,
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Prepared {
    fn new(label: String, meaning: String, script: Script) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|result: Prepared|
            result.source@ == crate::script::spec::source(script)
                && result.witness@ == script
                && result.label@ == label@ && result.meaning@ == meaning@));
        let source = script.source();
        #[cfg(verus_keep_ghost)]
        let witness = verus_exec_expr!(Ghost(script));
        Self {
            label,
            meaning,
            source,
            #[cfg(test)]
            script,
            #[cfg(verus_keep_ghost)]
            witness,
        }
    }
}

/// The queries of one check.
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub(super) struct Queries {
    /// One query per piece of the domain, in order.
    pub(super) pieces: Vec<Prepared>,
    pub(super) control: Prepared,
}

#[cfg(verus_keep_ghost)]
verus! {
    pub(super) open spec fn printed(query: Prepared) -> bool {
        query.source@ == crate::script::spec::source(query.witness@)
    }

    pub(super) open spec fn complete(queries: Queries, original: program::Program<'_>,
        candidate: program::Program<'_>) -> bool {
        printed(queries.control)
            && spec::query(queries.control.witness@, spec::Piece::Control, original, candidate)
            && queries.pieces@.len() == spec::output_piece_count(original.outputs@) + 2
            && forall|i: int| 0 <= i < queries.pieces@.len() ==>
                printed(#[trigger] queries.pieces@[i]) && spec::query(queries.pieces@[i].witness@,
                    spec::partition_piece(original.outputs@, i as nat), original, candidate)
    }

    /// The actual returned partition has a model precisely for runtime differences.
    /// Includes declaration constraints and model existence, not only implication.
    pub(super) proof fn runtime_difference_has_model(queries: Queries,
        original: program::Program<'_>, candidate: program::Program<'_>, raw: Seq<i128>)
        requires complete(queries, original, candidate),
            original.inputs@ == candidate.inputs@, original.outputs@ == candidate.outputs@,
            crate::evaluation::spec::admitted(original.inputs@, raw),
        ensures (spec::execution(original, raw) != spec::execution(candidate, raw)) ==
            (exists|i: int, environment: Map<Seq<char>, crate::term::denotation::Value>|
                0 <= i < queries.pieces@.len()
                && spec::context(queries.pieces@[i].witness@, original, candidate, raw, environment)
                && spec::declarations_hold(queries.pieces@[i].witness@, environment)
                && spec::assertions_hold(queries.pieces@[i].witness@, environment)),
    {
        assert(queries.pieces@.len() >= 3);
        assert(spec::query(queries.pieces@[0].witness@,
            spec::partition_piece(original.outputs@, 0nat), original, candidate));
        assert(spec::represented_result(original, raw));
        spec::partition_covers(original, candidate, raw);
        if spec::execution(original, raw) != spec::execution(candidate, raw) {
            let index = choose|i: nat| i < spec::output_piece_count(original.outputs@) + 2
                && spec::expected(spec::partition_piece(original.outputs@, i), original, candidate, raw);
            let script = queries.pieces@[index as int].witness@;
            let environment = spec::model_for_query(script,
                spec::partition_piece(original.outputs@, index), original, candidate, raw);
            assert(spec::assertions_hold(script, environment));
            assert(exists|i: int, model: Map<Seq<char>, crate::term::denotation::Value>|
                0 <= i < queries.pieces@.len()
                && spec::context(queries.pieces@[i].witness@, original, candidate, raw, model)
                && spec::declarations_hold(queries.pieces@[i].witness@, model)
                && spec::assertions_hold(queries.pieces@[i].witness@, model));
        }
        assert forall|i: int, environment: Map<Seq<char>, crate::term::denotation::Value>|
            0 <= i < queries.pieces@.len()
                && spec::context(queries.pieces@[i].witness@, original, candidate, raw, environment)
                && spec::declarations_hold(queries.pieces@[i].witness@, environment)
                && spec::assertions_hold(queries.pieces@[i].witness@, environment) implies
            spec::execution(original, raw) != spec::execution(candidate, raw) by {
            assert(spec::expected(spec::partition_piece(original.outputs@, i as nat),
                original, candidate, raw));
        }
    }
}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Queries {
    pub(super) fn build(
        original: program::Program<'_>,
        candidate: program::Program<'_>,
    ) -> Result<Self, crate::symbolic::Unsupported> {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(
            |result: Result<Queries, crate::symbolic::Unsupported>| match result {
                Ok(queries) => complete(queries, original, candidate),
                Err(_) => true,
            }
        ));
        let mut pieces = Vec::new();
        // Keep the explicit cases aligned with the checked partition proof.
        #[allow(clippy::manual_map)]
        let first = match original.outputs.first() {
            Some(domain) => Some(domain.bounds()),
            None => None,
        };
        let span = match first {
            Some((min, max)) => max.checked_sub(min),
            None => None,
        };
        match (first, span) {
            // `max` is read only by the Verus invariant below; ordinary builds compile that out.
            #[cfg_attr(not(verus_keep_ghost), allow(unused_variables))]
            (Some((min, max)), Some(span)) if span >= 0 && span < MAX_CASE_PIECES as i128 => {
                let count = span as usize + 1;
                let mut offset = 0usize;
                #[cfg_attr(verus_keep_ghost, verus_spec(
                    invariant offset <= count,
                        min <= max, span as int == max as int - min as int,
                        count as int == span as int + 1,
                        0 < count as int <= MAX_CASE_PIECES as int,
                        spec::split(original.outputs@),
                        spec::bounds(original.outputs@[0]) == (min as int, max as int),
                        pieces@.len() == offset,
                        forall|i: int| 0 <= i < pieces@.len() ==>
                            printed(#[trigger] pieces@[i]) && spec::query(pieces@[i].witness@,
                                spec::partition_piece(original.outputs@, i as nat), original, candidate),
                    decreases count - offset,
                ))]
                while offset < count {
                    // offset <= span, so this sum remains within [min, max].
                    // Only the bounded offset advances after the final piece.
                    let value = min + offset as i128;
                    pieces.push(construction_pieces::outputs(
                        original,
                        candidate,
                        Some(value),
                    )?);
                    offset += 1;
                }
            }
            _ => {
                pieces.push(construction_pieces::outputs(original, candidate, None)?);
            }
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            assert(pieces@.len() == spec::output_piece_count(original.outputs@));
            assert forall|i: int| 0 <= i < pieces@.len() implies
                printed(#[trigger] pieces@[i]) && spec::query(pieces@[i].witness@,
                    spec::partition_piece(original.outputs@, i as nat), original, candidate) by {}
        }
        pieces.push(construction_pieces::failure(original, candidate, true)?);
        pieces.push(construction_pieces::failure(original, candidate, false)?);
        let control = construction_pieces::control(original, candidate)?;
        Ok(Self { pieces, control })
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use zeno_fcis_synthesis::finite::{Domain, Op, Program as OwnedProgram};

    #[test]
    fn partitions_cover_both_i128_endpoints_and_overflowing_spans() {
        for (min, max, count) in [
            (i128::MIN, i128::MIN, 3),
            (i128::MAX, i128::MAX, 3),
            (i128::MIN, i128::MIN + 2, 5),
            (i128::MAX - 2, i128::MAX, 5),
            (i128::MIN, i128::MAX, 3),
        ] {
            let owned = OwnedProgram::try_new(
                vec![Domain::Int {
                    min: i128::MIN,
                    max: i128::MAX,
                }],
                vec![Domain::Int { min, max }],
                vec![Op::Input(0)],
                vec![0],
            )
            .unwrap_or_else(|error| panic!("partition fixture: {error:?}"));
            let borrowed = program::Program::of(&owned);
            let queries = Queries::build(borrowed, borrowed)
                .unwrap_or_else(|error| panic!("partition construction: {error:?}"));
            assert_eq!(queries.pieces.len(), count, "{min}..={max}");
        }
    }
}
