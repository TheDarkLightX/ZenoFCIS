//! Seed programs (G12): the decision programs of the mutants a review found
//! equivalent over the full domain, offered to `optimize --with-candidate`
//! and `loop run --seed` as proposals. A seed is never evidence. The review
//! compares decisions, while a decision program's output 0 is the selected
//! case's index, so a decision-equivalent mutant (two disjoint cases swapped,
//! say) can still be a different program. Each seed therefore carries the
//! transform checker's own verdict against the contract's program, and the
//! optimizer and the loop judge it again before using it.

use std::collections::BTreeSet;

use serde_json::Value;

use super::super::declarations::Declarations;
use super::super::model::Contract;
use super::super::rules::Rules;
use super::super::{ContractError, program_bytes, schema, schema_commitment};
use super::ReviewSources;
use super::mutants;
use crate::transform::{self, Limits};

/// One equivalent mutant's decision program.
#[derive(Debug)]
pub(crate) struct Seed {
    /// The mutant's catalog id, `m0001`, ...
    pub(crate) id: String,
    /// Its canonical decision program.
    pub(crate) program: Vec<u8>,
    /// Whether the transform checker finds it equal to the contract's own
    /// decision program on every input of the declared domain.
    pub(crate) program_equivalent: bool,
}

/// The decision programs, in catalog order, of the mutants that `packet`,
/// the review packet of `sources`, classifies `equivalent-over-full-domain`.
pub(crate) fn equivalent_mutant_programs(
    sources: ReviewSources<'_>,
    packet: &str,
    max_tuples: u64,
) -> Result<Vec<Seed>, ContractError> {
    let packet: Value = serde_json::from_str(packet)
        .map_err(|error| ContractError::new("review packet", error.to_string()))?;
    let equivalent: BTreeSet<&str> = packet["mutants"]["list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|mutant| mutant["classification"] == "equivalent-over-full-domain")
        .filter_map(|mutant| mutant["id"].as_str())
        .collect();
    let rules = Rules::read_for_project(sources.project, sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let original = program_bytes(&Contract::build(&declarations, &rules, commitment)?.program()?)?;
    let limits = Limits {
        steps: transform::DEFAULT_STEP_LIMIT,
        input_tuples: max_tuples,
    };
    let mut seeds = Vec::new();
    for mutant in mutants::catalog(&rules, &declarations) {
        if !equivalent.contains(mutant.id.as_str()) {
            continue;
        }
        let contract = Contract::build(&declarations, &mutant.rules, commitment)?;
        let program = program_bytes(&contract.program()?)?;
        let program_equivalent = transform::check(&original, &program, limits).is_ok();
        seeds.push(Seed {
            id: mutant.id,
            program,
            program_equivalent,
        });
    }
    Ok(seeds)
}
