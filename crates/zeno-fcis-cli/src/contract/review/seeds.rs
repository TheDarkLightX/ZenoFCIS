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
use super::super::{ContractError, Current, program_bytes, schema, schema_commitment};
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
    /// The current reference actually checked, when a retained lineage exists.
    pub(crate) reference_sha256: Option<String>,
}

/// The decision programs, in catalog order, of the mutants that `packet`,
/// the review packet of `sources`, classifies `equivalent-over-full-domain`.
pub(crate) fn equivalent_mutant_programs(
    sources: ReviewSources<'_>,
    packet: &str,
    max_tuples: u64,
) -> Result<Vec<Seed>, ContractError> {
    equivalent_mutant_programs_with_current(sources, packet, max_tuples, None)
}

/// Check mutant seed programs against the actual replayed current program.
pub(crate) fn equivalent_mutant_programs_current(
    current: &Current<'_>,
    examples: Option<&str>,
    packet: &str,
    max_tuples: u64,
) -> Result<Vec<Seed>, ContractError> {
    equivalent_mutant_programs_with_current(
        ReviewSources {
            project: current.project,
            rules: current.rules_source,
            examples,
        },
        packet,
        max_tuples,
        Some(current),
    )
}

fn equivalent_mutant_programs_with_current(
    sources: ReviewSources<'_>,
    packet: &str,
    max_tuples: u64,
    current: Option<&Current<'_>>,
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
    let original = match current {
        Some(current) => current
            .generated
            .program()
            .ok_or_else(|| {
                ContractError::new("lineage binding", "current scalar program is unavailable")
            })?
            .to_vec(),
        None => program_bytes(&Contract::build(&declarations, &rules, commitment)?.program()?)?,
    };
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
        let program_equivalent = match transform::check(&original, &program, limits) {
            Ok(_) => true,
            Err(transform::Rejection::Counterexample(_)) => false,
            Err(transform::Rejection::Refused(
                transform::Refusal::InputAbi { .. } | transform::Refusal::OutputAbi { .. },
            )) => false,
            Err(error) if current.is_some() => {
                return Err(ContractError::new(
                    "mutant program seed",
                    format!("current program comparison did not decide: {error:?}"),
                ));
            }
            Err(_) => false,
        };
        seeds.push(Seed {
            id: mutant.id,
            program,
            program_equivalent,
            reference_sha256: current.map(|_| transform::sha256_hex(&original)),
        });
    }
    Ok(seeds)
}
