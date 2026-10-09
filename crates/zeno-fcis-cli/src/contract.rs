//! V2 application contracts: `project.zeno` declarations and a reviewed rules
//! file, `v2/policy.json`, become the original schema `v2/schema.zcve`, the
//! Rust data the library consumes `src/v2_contract.rs`, and the
//! library-encoded policy `v2/policy.zcve`.
//!
//! Generation is pure: callers pass file contents and receive file contents.
//! The policy bytes come from the library's own encoder, and before they are
//! returned the complete catalog binding that the generated `checked_catalog`
//! performs at run time must accept them with the schema. The generated
//! program is not thereby shown to match the owner's intent; the
//! application's own decision examples and laws check that.
//!
//! A rules file may adopt candidate decision programs. Each adoption names a
//! candidate and the `transform` receipt that compared it with the program it
//! replaces, and binds the policy of the version it superseded. Generation
//! replays every receipt, in order, from the rules-compiled program before it
//! emits the last candidate as the contract's program. It emits each
//! superseded version beside the current one and refuses any edit that would
//! change one, an adoption that leaves the program unchanged, and a lineage
//! that would repeat a version, so an application keeps the Authorities of
//! its whole lineage and every store can follow it.
//!
//! A rules file may also list evolutions: earlier contracts that `zeno-fcis
//! contract evolve` replaced with a reviewed rule change. Each is retained
//! under `v2/evolutions/n/` with its own adoptions and the owner's review,
//! and generation regenerates every one of its versions, numbering the
//! whole lineage from 1, checks the policy the evolution bound, and computes
//! the review again from the two contracts: it must be byte for byte the
//! retained text.

mod adoption;
mod declarations;
mod delivery;
pub(crate) mod diff;
pub(crate) mod draft;
#[cfg(test)]
mod draft_tests;
mod expr;
mod graph;
mod layout;
mod migration;
mod model;
mod policy;
mod render;
pub(crate) mod review;
mod rules;
mod schema;
pub(crate) mod symbolic;
#[cfg(test)]
mod tests;

use std::fmt;

use zeno_fcis_codec::{CommitmentHasher, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{v2_authority as authority, v2_catalog as catalog};
use zeno_fcis_synthesis::finite_runtime::import_program;

use crate::transform::{self, Inconclusive, Rejection, Replay, Replayed};
pub(crate) use adoption::{Adopted, AdoptionPlan, CheckedCandidate};
use declarations::Declarations;
pub(crate) use diff::Path as EvolutionPath;
use model::{CompiledClaim, Contract};
use rules::Rules;
use rules::with_receipt_digests;
pub(crate) use rules::{
    Adoption, Evolution, EvolutionKind, Shortcut, Usage, with_adoption, with_evolution,
    without_evolutions,
};

/// The evolutions a rules file lists, after the rules validate.
///
/// # Errors
/// The rules' refusal.
pub(crate) fn listed_evolutions(rules: &str) -> Result<Vec<Evolution>, ContractError> {
    Ok(Rules::read(rules)?.evolutions)
}

/// The directory of adoption `n`'s retained files, relative to the
/// application: `v2/adoptions/n/program.zcve` and `v2/adoptions/n/receipt.json`.
pub(crate) fn adoption_directory(ordinal: usize) -> String {
    format!("v2/adoptions/{ordinal}")
}

/// The directory of evolution `n`'s retained files, relative to the
/// application: the replaced contract's `project.zeno`, `v2/policy.json` and
/// `v2/adoptions/k/`, and the owner's review, `review.txt`.
pub(crate) fn evolution_directory(ordinal: usize) -> String {
    format!("v2/evolutions/{ordinal}")
}

/// The owner's review of an evolution, inside `evolution_directory`.
pub(crate) const EVOLUTION_REVIEW: &str = "review.txt";

/// An evolution's data migration, inside `evolution_directory`.
pub(crate) const EVOLUTION_MIGRATION: &str = "migration.json";

/// A shortcut migration kept beside an evolution's migration, inside
/// `evolution_directory`: `shortcuts/from-{k}.json`.
pub(crate) fn shortcut_file(from_version: u32) -> String {
    format!("shortcuts/from-{from_version}.json")
}

/// One earlier contract that an evolution replaced, as retained under
/// `v2/evolutions/n/`.
#[derive(Clone, Debug)]
pub(crate) struct EvolutionSources<'a> {
    /// Its `project.zeno`.
    pub(crate) project: &'a str,
    /// Its `v2/policy.json`, with its own adoptions and no evolutions.
    pub(crate) rules: &'a str,
    /// Its adoptions' retained files, in order.
    pub(crate) adoptions: Vec<AdoptionSources<'a>>,
    /// `review.txt`: the plain-language diff from its last version to the
    /// next contract's first version.
    pub(crate) review: &'a [u8],
    /// `migration.json`, for a migration.
    pub(crate) migration: Option<&'a [u8]>,
    /// Each shortcut the rules list for it, in order.
    pub(crate) shortcuts: Vec<&'a [u8]>,
}

impl EvolutionSources<'_> {
    /// Its files as a contract of their own.
    fn sources(&self) -> ContractSources<'_> {
        ContractSources {
            project: self.project,
            rules: self.rules,
            schema_origin: None,
            adoptions: &self.adoptions,
            replayed: &[],
            evolutions: &[],
        }
    }
}

/// One adoption's retained files, in adoption order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct AdoptionSources<'a> {
    /// `v2/adoptions/n/program.zcve`: the candidate's canonical program bytes.
    pub(crate) candidate: &'a [u8],
    /// `v2/adoptions/n/receipt.json`: the receipt comparing it with version `n`.
    pub(crate) receipt: &'a [u8],
}

/// The application files a contract is generated from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContractSources<'a> {
    /// `project.zeno`.
    pub(crate) project: &'a str,
    /// `v2/policy.json`: leaf bindings, variables, cases, genesis, law kinds
    /// and adoptions.
    pub(crate) rules: &'a str,
    /// `v2/schema-origin.json` when the application keeps one; its digests
    /// must describe the generated schema.
    pub(crate) schema_origin: Option<&'a str>,
    /// The retained files of every adoption the rules list, in order.
    pub(crate) adoptions: &'a [AdoptionSources<'a>],
    /// Receipts already replayed in this command over exactly these bytes;
    /// generation replays every other one.
    pub(crate) replayed: &'a [Replayed],
    /// The retained files of every evolution the rules list, in order.
    pub(crate) evolutions: &'a [EvolutionSources<'a>],
}

/// A superseded contract version, emitted beside the current one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreviousContract {
    source: String,
    policy: Vec<u8>,
    /// The version's own schema, when a later migration or rename changed
    /// the schema: `v2/schema_v{k}.zcve`.
    schema: Option<Vec<u8>>,
}

impl PreviousContract {
    /// `v2/schema_v{k}.zcve`, for a version whose schema differs from the
    /// current one; `None` when it shares `v2/schema.zcve`.
    pub(crate) fn schema(&self) -> Option<&[u8]> {
        self.schema.as_deref()
    }

    /// `src/v2_contract_v{k}.rs`.
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// `v2/policy_v{k}.zcve`.
    pub(crate) fn policy(&self) -> &[u8] {
        &self.policy
    }
}

/// The generated files and their sizes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedContract {
    schema: Vec<u8>,
    source: String,
    policy: Vec<u8>,
    previous: Vec<PreviousContract>,
    summary: ContractSummary,
    program: Vec<u8>,
    replayed: Vec<Replayed>,
    /// The inductive claims compiled for a behaviour change or migration into this
    /// contract; computed only when generation needs them.
    claims: Vec<CompiledClaim>,
}

impl GeneratedContract {
    /// `v2/schema.zcve`.
    pub(crate) fn schema(&self) -> &[u8] {
        &self.schema
    }

    /// `src/v2_contract.rs`.
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// `v2/policy.zcve`.
    pub(crate) fn policy(&self) -> &[u8] {
        &self.policy
    }

    /// Superseded versions 1.., oldest first; empty without adoptions.
    pub(crate) fn previous(&self) -> &[PreviousContract] {
        &self.previous
    }

    /// Program, law and budget sizes.
    pub(crate) fn summary(&self) -> &ContractSummary {
        &self.summary
    }

    /// The current decision program's canonical bytes: what the next
    /// adoption's receipt names as its original.
    pub(crate) fn program(&self) -> &[u8] {
        &self.program
    }

    /// Every adoption's replayed receipt, oldest first.
    pub(crate) fn replayed(&self) -> &[Replayed] {
        &self.replayed
    }
}

/// Sizes of a generated contract, for reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContractSummary {
    /// The application the rules name.
    pub(crate) application: String,
    /// Position in the lineage: one more than the adoptions.
    pub(crate) version: u32,
    /// Nodes of the scalar decision program.
    pub(crate) program_nodes: usize,
    /// Outputs of the scalar decision program, the case selection included.
    pub(crate) outputs: usize,
    /// Nodes of all law programs.
    pub(crate) law_nodes: usize,
    /// Law IDs in descriptor order; every one is required.
    pub(crate) law_ids: Vec<u32>,
    /// Declared schema types.
    pub(crate) schema_types: usize,
    /// Read, step and byte limits of the shared meter.
    pub(crate) read_budget: u64,
    /// See `read_budget`.
    pub(crate) step_budget: u64,
    /// See `read_budget`.
    pub(crate) byte_budget: u64,
    /// Each adoption of the current contract as its receipt replayed it,
    /// oldest first.
    pub(crate) adoptions: Vec<AdoptionSummary>,
    /// Each behaviour change of the lineage, oldest first.
    pub(crate) evolutions: Vec<EvolutionSummary>,
}

/// One evolution of a lineage, after generation recomputed its review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EvolutionSummary {
    /// The version it follows, from 1.
    pub(crate) version: u32,
    /// The SHA-256 of the replaced contract's last policy.
    pub(crate) superseded_policy_sha256: String,
    /// The SHA-256 of the owner's review text.
    pub(crate) review_sha256: String,
    /// The inductive claims of the contract it leads to, which a store
    /// upgrade across it checks on the target state; empty for a rename.
    pub(crate) claims: Vec<u32>,
    /// How a store follows it.
    pub(crate) kind: &'static str,
    /// For a migration, what generation's forward simulation compared.
    pub(crate) migration: Option<MigrationSummary>,
}

/// A data migration of a lineage, after generation simulated it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationSummary {
    /// The SHA-256 of `migration.json`.
    pub(crate) sha256: String,
    /// The old version's declared states, those its state laws hold on, and
    /// those its genesis evaluation admits.
    pub(crate) states: [u64; 3],
    /// The input tuples compared.
    pub(crate) tuples: u64,
    /// Each shortcut: the version it starts at and the tuples compared.
    pub(crate) shortcuts: Vec<(u32, u64)>,
}

/// One adoption, after its receipt replayed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdoptionSummary {
    pub(crate) candidate_sha256: String,
    pub(crate) receipt_sha256: String,
    /// The usage the rules claim.
    pub(crate) usage: Usage,
    /// Whether the receipt reports equal Step usage on every input.
    pub(crate) usage_preserved: bool,
    /// The SHA-256 of the superseded version's policy, which generation checked.
    pub(crate) superseded_policy_sha256: String,
    /// The Step bounds behind the premise that neither version's Step limit
    /// binds; see [`StepBound`].
    pub(crate) steps: StepBound,
    /// Nodes of the program it replaced, then of the candidate.
    pub(crate) program_nodes: [usize; 2],
}

/// A program successor's Step premise, from the replayed receipt: each
/// program's largest true Step usage on any input, plus one Step for every
/// law node, is within its version's Step limit. Steps are charged only for
/// program instruction attempts and law nodes, so neither limit refuses a
/// decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StepBound {
    /// Largest Step usage of the superseded program, then of the candidate.
    pub(crate) program: [u64; 2],
    /// One Step for every law node.
    pub(crate) laws: u64,
    /// The Step limit of the superseded version, then of the new one.
    pub(crate) limits: [u64; 2],
}

/// The current version of a generated contract as the generator's model:
/// the declarations, rules and schema it was generated from, and the
/// contract with every adoption's candidate applied.
struct Current<'a> {
    declarations: &'a Declarations,
    rules: &'a Rules,
    contract: &'a Contract<'a>,
    schema: &'a [u8],
    generated: &'a GeneratedContract,
}

/// Rebuilds the generator's model of the current version of `generated`,
/// which `generate_contract(sources)` returned, and runs `use_current` on
/// it. No receipt is replayed again, because generation replayed every one;
/// the rebuilt schema and policy must be the generated ones.
///
/// # Errors
/// Returns a refusal of the sources, a rebuilt contract that differs from
/// `generated`, or `use_current`'s error.
fn with_current<R>(
    sources: ContractSources<'_>,
    generated: &GeneratedContract,
    use_current: impl FnOnce(Current<'_>) -> Result<R, ContractError>,
) -> Result<R, ContractError> {
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let mut contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    for (index, files) in sources.adoptions.iter().enumerate() {
        let candidate = import_program(files.candidate).map_err(|error| {
            ContractError::new(
                format!("v2/policy.json adoptions[{index}]"),
                format!("the library refuses the candidate: {error}"),
            )
        })?;
        contract.adopt(&candidate, &format!("adoptions[{index}]"))?;
    }
    if schema != generated.schema() || policy::encode(&contract, &schema)? != generated.policy() {
        return Err(ContractError::new(
            "v2/policy.zcve",
            "the contract rebuilt from these files differs from the generated one",
        ));
    }
    use_current(Current {
        declarations: &declarations,
        rules: &rules,
        contract: &contract,
        schema: &schema,
        generated,
    })
}

/// Why no contract was generated: the file and entry, and the reason.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContractError {
    place: String,
    reason: String,
}

impl ContractError {
    pub(crate) fn new(place: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            place: place.into(),
            reason: reason.into(),
        }
    }

    /// The file, and entry within it, that the reason is about.
    pub(crate) fn place(&self) -> &str {
        &self.place
    }

    /// What is wrong there.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.place, self.reason)
    }
}

impl std::error::Error for ContractError {}

/// Generates `v2/schema.zcve`, `src/v2_contract.rs`, `v2/policy.zcve` and
/// every superseded version from an application's declarations, rules and
/// adoptions.
///
/// # Errors
/// Returns the first declaration or rule with no contract form, an origin
/// record that disagrees with the schema, an adoption whose files or receipt
/// do not bind, or the library's refusal.
pub(crate) fn generate_contract(
    sources: ContractSources<'_>,
) -> Result<GeneratedContract, ContractError> {
    generate(sources, Receipts::Replay)
}

/// How generation treats each adoption's retained receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Receipts {
    /// It must replay byte for byte, or be a replay this command already did.
    Replay,
    /// It is rechecked and rebound to this checker; see `transform::refreshed`.
    Refresh,
}

/// The receipts a refresh rebound to this checker, the rules naming them,
/// and the lineage generated from them, in which every receipt replayed.
#[derive(Debug)]
pub(crate) struct RefreshedReceipts {
    rules: String,
    receipts: Vec<Vec<u8>>,
    generated: GeneratedContract,
}

impl RefreshedReceipts {
    /// `v2/policy.json` naming every receipt's new digest.
    pub(crate) fn rules(&self) -> &str {
        &self.rules
    }

    /// Every adoption's receipt, oldest first, rebound or unchanged.
    pub(crate) fn receipts(&self) -> &[Vec<u8>] {
        &self.receipts
    }

    /// The lineage generated from the new rules and receipts.
    pub(crate) fn generated(&self) -> &GeneratedContract {
        &self.generated
    }
}

/// Rebinds every adoption's receipt to this checker: each pair is checked
/// again under the receipt's limits, and the new receipt must record what the
/// old one does in every field but the checker identity. The lineage is then
/// generated from the new receipts, each of which must replay. A receipt file
/// that already is the new receipt, left by an interrupted refresh, is
/// accepted although the rules still name the old digest.
///
/// # Errors
/// The first adoption whose pair is no longer equivalent or whose receipt
/// would change otherwise, or any refusal of generation.
pub(crate) fn refresh_receipts(
    sources: ContractSources<'_>,
) -> Result<RefreshedReceipts, ContractError> {
    let rechecked = generate(sources, Receipts::Refresh)?;
    let receipts: Vec<Vec<u8>> = rechecked
        .replayed
        .iter()
        .map(|replayed| replayed.receipt().to_vec())
        .collect();
    let digests: Vec<String> = receipts
        .iter()
        .map(|receipt| transform::sha256_hex(receipt))
        .collect();
    let unchanged = sources
        .adoptions
        .iter()
        .zip(&receipts)
        .all(|(files, receipt)| files.receipt == receipt.as_slice());
    let bound = Rules::read(sources.rules)?
        .adoptions
        .iter()
        .map(|adoption| &adoption.receipt_sha256)
        .eq(&digests);
    if unchanged && bound {
        // Every receipt already is this checker's, and the recheck replayed
        // each one exactly.
        return Ok(RefreshedReceipts {
            rules: sources.rules.to_owned(),
            receipts,
            generated: rechecked,
        });
    }
    let rules = with_receipt_digests(sources.rules, &digests)?;
    let adoptions: Vec<AdoptionSources<'_>> = sources
        .adoptions
        .iter()
        .zip(&receipts)
        .map(|(files, receipt)| AdoptionSources {
            candidate: files.candidate,
            receipt,
        })
        .collect();
    // Every new receipt replays from scratch; the recheck is not reused.
    let generated = generate_contract(ContractSources {
        rules: &rules,
        adoptions: &adoptions,
        replayed: &[],
        ..sources
    })?;
    Ok(RefreshedReceipts {
        rules,
        receipts,
        generated,
    })
}

/// Where one contract's versions sit in a lineage that evolved: the
/// versions of the earlier contracts before it, the adoption receipts among
/// them and the behaviour changes. The default is a contract that never
/// evolved, whose output is exactly as before evolutions existed.
#[derive(Default)]
struct Placement<'a> {
    /// The number of versions before this contract's first one.
    offset: usize,
    /// The rendered versions of earlier contracts, oldest first, whose
    /// policies this contract's versions must not repeat; empty to classify
    /// a change before that check.
    previous: &'a [PreviousContract],
    /// The adoption receipt digests of earlier contracts, oldest first.
    receipts: &'a [String],
    /// The behaviour changes before this contract, with their claims.
    evolutions: &'a [render::RenderedEvolution],
    /// This contract's last version is superseded too: its source is a
    /// leaf, `src/v2_contract_v{k}.rs`.
    superseded: bool,
    /// Compile this contract's inductive claims.
    claims: bool,
    /// The current contract's schema, in a lineage with a migration or
    /// rename: a version whose schema differs keeps its own schema file.
    current_schema: Option<&'a [u8]>,
}

fn generate(
    sources: ContractSources<'_>,
    receipts: Receipts,
) -> Result<GeneratedContract, ContractError> {
    let rules = Rules::read(sources.rules)?;
    if sources.evolutions.len() != rules.evolutions.len() {
        return Err(ContractError::new(
            "v2/policy.json evolutions",
            format!(
                "lists {} evolutions but the retained files of {} were supplied",
                rules.evolutions.len(),
                sources.evolutions.len()
            ),
        ));
    }
    if rules.evolutions.is_empty() {
        return generate_era(sources, receipts, &Placement::default());
    }
    // A lineage that moves the state keeps each earlier schema beside the
    // versions that ran it; one that never did generates as before.
    let moves_state = rules
        .evolutions
        .iter()
        .any(|evolution| evolution.kind != EvolutionKind::BehaviourChange);
    let current_schema = if moves_state {
        Some(schema::encode(&Declarations::read(
            sources.project,
            &rules.leaf_bindings,
        )?)?)
    } else {
        None
    };
    let mut previous: Vec<PreviousContract> = Vec::new();
    let mut adopted: Vec<String> = Vec::new();
    let mut evolutions: Vec<render::RenderedEvolution> = Vec::new();
    let mut summaries = Vec::new();
    // Each earlier contract's last version as generated, for shortcuts.
    let mut eras: Vec<(ContractSources<'_>, GeneratedContract)> = Vec::new();
    for (index, (entry, era)) in rules.evolutions.iter().zip(sources.evolutions).enumerate() {
        let ordinal = index + 1;
        let directory = evolution_directory(ordinal);
        let place = format!("v2/policy.json evolutions[{index}]");
        let within = |error: ContractError| {
            ContractError::new(format!("{directory}/{}", error.place()), error.reason())
        };
        if !Rules::read(era.rules)
            .map_err(within)?
            .evolutions
            .is_empty()
        {
            return Err(ContractError::new(
                format!("{directory}/v2/policy.json evolutions"),
                "an earlier contract keeps no evolutions of its own; only the current rules list them",
            ));
        }
        let old_sources = era.sources();
        // A replaced contract's receipts must replay as they are: only the
        // current contract's receipts are ever rebound.
        let old = generate_era(
            old_sources,
            Receipts::Replay,
            &Placement {
                offset: previous.len(),
                previous: &previous,
                receipts: &adopted,
                superseded: true,
                current_schema: current_schema.as_deref(),
                ..Placement::default()
            },
        )
        .map_err(within)?;
        if transform::sha256_hex(old.policy()) != entry.superseded_policy_sha256 {
            return Err(ContractError::new(
                format!("{place}.superseded_policy_sha256"),
                format!(
                    "differs from the last policy of the contract in {directory}: an edit there \
                     changes versions that existing stores may run"
                ),
            ));
        }
        adopted.extend(
            old.summary
                .adoptions
                .iter()
                .map(|adoption| adoption.receipt_sha256.clone()),
        );
        previous.extend(old.previous.iter().cloned());
        previous.push(PreviousContract {
            source: old.source.clone(),
            policy: old.policy.clone(),
            schema: current_schema
                .as_deref()
                .is_some_and(|current| current != old.schema.as_slice())
                .then(|| old.schema.clone()),
        });
        let (project, next_rules) = match sources.evolutions.get(index + 1) {
            Some(next) => (next.project, next.rules),
            None => (sources.project, sources.rules),
        };
        let first = rules::first_version(next_rules)?;
        let next = ContractSources {
            project,
            rules: &first,
            schema_origin: None,
            adoptions: &[],
            replayed: &[],
            evolutions: &[],
        };
        let review = evolution_review(old_sources, &old, next, &previous, &adopted, &entry.kind)?;
        if transform::sha256_hex(review.text.as_bytes()) != entry.review_sha256 {
            return Err(ContractError::new(
                format!("{place}.review_sha256"),
                "differs from the review these two contracts have: the account of the change \
                 `contract evolve` recorded is recomputed from them, and an edit to either \
                 contract changes it",
            ));
        }
        if era.review != review.text.as_bytes() {
            return Err(ContractError::new(
                format!("{directory}/{EVOLUTION_REVIEW}"),
                "differs from the review these two contracts have; `contract evolve` wrote it, \
                 and it must not be edited",
            ));
        }
        let version = u32::try_from(previous.len())
            .map_err(|_| ContractError::new("v2/policy.json evolutions", "too many versions"))?;
        let (step, migration_summary) = match &entry.kind {
            EvolutionKind::BehaviourChange => (
                render::RenderedStep::BehaviourChange(review.claims.clone()),
                None,
            ),
            EvolutionKind::Rename => (render::RenderedStep::Rename, None),
            EvolutionKind::Migration {
                migration_sha256,
                shortcuts,
            } => {
                let file = format!("{directory}/{EVOLUTION_MIGRATION}");
                let bytes = era.migration.ok_or_else(|| {
                    ContractError::new(&file, "is missing; the rules list a migration")
                })?;
                if transform::sha256_hex(bytes) != *migration_sha256 {
                    return Err(ContractError::new(
                        format!("{place}.migration_sha256"),
                        format!(
                            "differs from {file}: `contract evolve` checked and recorded it, and \
                             it must not be edited"
                        ),
                    ));
                }
                let (compiled, simulated) =
                    checked_migration(old_sources, &old, next, &review.generated, bytes, &file)?;
                if era.shortcuts.len() != shortcuts.len() {
                    return Err(ContractError::new(
                        format!("{place}.shortcuts"),
                        format!(
                            "lists {} shortcuts but {} files were supplied",
                            shortcuts.len(),
                            era.shortcuts.len()
                        ),
                    ));
                }
                let mut checked = Vec::new();
                for (shortcut, bytes) in shortcuts.iter().zip(&era.shortcuts) {
                    let file = format!("{directory}/{}", shortcut_file(shortcut.from_version));
                    if transform::sha256_hex(bytes) != shortcut.sha256 {
                        return Err(ContractError::new(
                            &file,
                            "differs from the SHA-256 the rules bind; it must not be edited",
                        ));
                    }
                    let route: Vec<usize> = summaries
                        .iter()
                        .position(|summary: &EvolutionSummary| {
                            summary.version == shortcut.from_version
                        })
                        .map(|start| (start..index).collect())
                        .ok_or_else(|| {
                            ContractError::new(
                                &file,
                                format!(
                                    "starts at version {}, which no earlier migration or rename \
                                     left: a shortcut starts at the last version of an earlier \
                                     contract this lineage evolved from",
                                    shortcut.from_version
                                ),
                            )
                        })?;
                    let mut steps = Vec::new();
                    for earlier in &route {
                        match &evolutions[*earlier].step {
                            render::RenderedStep::Migration { migration, .. } => {
                                steps.push(Some(migration));
                            }
                            render::RenderedStep::Rename => steps.push(None),
                            render::RenderedStep::BehaviourChange(_) => {
                                return Err(ContractError::new(
                                    &file,
                                    format!(
                                        "crosses evolution {}, a behaviour change, whose route keeps \
                                         no observations; a shortcut crosses only migrations and \
                                         renames",
                                        earlier + 1
                                    ),
                                ));
                            }
                        }
                    }
                    steps.push(Some(&compiled));
                    let contracts: Vec<(ContractSources<'_>, &GeneratedContract)> = route
                        .iter()
                        .map(|earlier| (eras[*earlier].0, &eras[*earlier].1))
                        .chain([(old_sources, &old), (next, &review.generated)])
                        .collect();
                    let tuples = checked_shortcut(&contracts, &steps, bytes, shortcut, &file)?;
                    checked.push((shortcut.from_version, tuples));
                }
                let summary = MigrationSummary {
                    sha256: migration_sha256.clone(),
                    states: [
                        simulated.states(),
                        simulated.admitted(),
                        simulated.genesis(),
                    ],
                    tuples: simulated.tuples(),
                    shortcuts: checked,
                };
                (
                    render::RenderedStep::Migration {
                        migration: compiled,
                        claims: review.claims.clone(),
                    },
                    Some(summary),
                )
            }
        };
        summaries.push(EvolutionSummary {
            version,
            superseded_policy_sha256: entry.superseded_policy_sha256.clone(),
            review_sha256: entry.review_sha256.clone(),
            claims: match &step {
                render::RenderedStep::BehaviourChange(claims)
                | render::RenderedStep::Migration { claims, .. } => {
                    claims.iter().map(|claim| claim.id).collect()
                }
                _ => Vec::new(),
            },
            kind: entry.kind.name(),
            migration: migration_summary,
        });
        evolutions.push(render::RenderedEvolution {
            ordinal,
            version,
            step,
        });
        eras.push((old_sources, old));
    }
    let mut generated = generate_era(
        sources,
        receipts,
        &Placement {
            offset: previous.len(),
            previous: &previous,
            receipts: &adopted,
            evolutions: &evolutions,
            ..Placement::default()
        },
    )?;
    previous.append(&mut generated.previous);
    generated.previous = previous;
    generated.summary.evolutions = summaries;
    Ok(generated)
}

/// A migration file compiled against the last version of `old` and the
/// first of `new`, generated as `old_generated` and `new_generated`, and
/// admitted by the shell's forward simulation; `place` names the file.
///
/// # Errors
/// The file's refusal, or the simulation's, in words.
fn checked_migration(
    old: ContractSources<'_>,
    old_generated: &GeneratedContract,
    new: ContractSources<'_>,
    new_generated: &GeneratedContract,
    bytes: &[u8],
    place: &str,
) -> Result<(migration::Compiled, crate::shell_v2::migration::Simulated), ContractError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| ContractError::new(place, "is not UTF-8 text"))?;
    let file = rules::read_migration(text, place)?;
    if file.from_version.is_some() {
        return Err(ContractError::new(
            format!("{place} from_version"),
            "marks a shortcut; a migration between consecutive versions names no from_version",
        ));
    }
    with_current(old, old_generated, |old| {
        with_current(new, new_generated, |new| {
            let compiled = migration::compile(&file, old.declarations, new.declarations, place)?;
            let names = migration::field_names(old.declarations);
            let simulated = policy::with_catalog(old.contract, old.schema, |_, from| {
                policy::with_catalog(new.contract, new.schema, |_, to| {
                    migration::simulate(from, to, &compiled, &names, place)
                })
            })?;
            Ok((compiled, simulated))
        })
    })
}

/// Runs `use_catalogs` on the checked catalog of the current version of
/// each contract, in order.
///
/// # Errors
/// A contract's refusal, or `use_catalogs`'s.
fn with_catalogs<R>(
    contracts: &[(ContractSources<'_>, &GeneratedContract)],
    use_catalogs: &mut dyn FnMut(&[&catalog::BoundCatalog<'_>]) -> Result<R, ContractError>,
) -> Result<R, ContractError> {
    fn nest<R>(
        rest: &[(ContractSources<'_>, &GeneratedContract)],
        bound: &[&catalog::BoundCatalog<'_>],
        use_catalogs: &mut dyn FnMut(&[&catalog::BoundCatalog<'_>]) -> Result<R, ContractError>,
    ) -> Result<R, ContractError> {
        let Some(((sources, generated), rest)) = rest.split_first() else {
            return use_catalogs(bound);
        };
        with_current(*sources, generated, |current| {
            policy::with_catalog(current.contract, current.schema, |_, checked| {
                let more: Vec<&catalog::BoundCatalog<'_>> =
                    bound.iter().copied().chain([checked]).collect();
                nest(rest, &more, use_catalogs)
            })
        })
    }
    nest(contracts, &[], use_catalogs)
}

/// Checks a shortcut: the migration file `bytes`, from the first of
/// `contracts` directly to the last, against the composed route of `steps`
/// (a migration, or `None` for a rename) between consecutive contracts.
/// Returns the input tuples compared.
///
/// # Errors
/// A shortcut whose `from_version` differs from the rules', a file the
/// compiler refuses, or a shortcut that disagrees with the route, in words.
fn checked_shortcut(
    contracts: &[(ContractSources<'_>, &GeneratedContract)],
    steps: &[Option<&migration::Compiled>],
    bytes: &[u8],
    shortcut: &rules::Shortcut,
    place: &str,
) -> Result<u64, ContractError> {
    use crate::shell_v2::migration as shell;
    let text =
        std::str::from_utf8(bytes).map_err(|_| ContractError::new(place, "is not UTF-8 text"))?;
    let file = rules::read_migration(text, place)?;
    if file.from_version != Some(shortcut.from_version) {
        return Err(ContractError::new(
            format!("{place} from_version"),
            format!(
                "must be {}, the version the rules bind this shortcut to",
                shortcut.from_version
            ),
        ));
    }
    let declarations = |sources: &ContractSources<'_>| {
        let rules = Rules::read(sources.rules)?;
        Declarations::read(sources.project, &rules.leaf_bindings)
    };
    let (Some(first), Some(last)) = (contracts.first(), contracts.last()) else {
        return Err(ContractError::new(place, "has no route"));
    };
    let (old, new) = (declarations(&first.0)?, declarations(&last.0)?);
    let compiled = migration::compile(&file, &old, &new, place)?;
    let names = migration::field_names(&old);
    with_catalogs(contracts, &mut |catalogs| {
        let (Some(from), Some(to)) = (catalogs.first(), catalogs.last()) else {
            return Err(ContractError::new(place, "has no route"));
        };
        let target = authority::bind(to).map_err(|refusal| {
            ContractError::new(
                place,
                format!("the library refuses to bind a contract: {refusal:?}"),
            )
        })?;
        let route = |state: &[u8]| {
            let mut state = state.to_vec();
            for (index, step) in steps.iter().enumerate() {
                let (old, new) = (catalogs.get(index)?, catalogs.get(index + 1)?);
                state = match step {
                    Some(migration) => migration
                        .with_shell(|migration| shell::migrate_state(old, new, migration, &state))
                        .ok()?,
                    None => shell::reframe(old, new, &state)?,
                };
            }
            Some(state)
        };
        compiled
            .with_shell(|shortcut| shell::agrees(from, to, &target, shortcut, route))
            .map_err(|unsimulated| {
                ContractError::new(
                    place,
                    format!(
                        "the shortcut disagrees with the composed route of consecutive migrations: {}",
                        migration::describe(unsimulated, from, &names)
                    ),
                )
            })
    })
}

/// A change's review, recomputed, the contract it leads to as generated,
/// and the target contract's freshly compiled claims for state admission.
pub(crate) struct Review {
    /// The plain-language account of `contract diff`, one line each, each
    /// ending in a newline.
    pub(crate) text: String,
    claims: Vec<CompiledClaim>,
    generated: GeneratedContract,
}

/// The review of an evolution of `kind` from `old`, generated as
/// `old_generated` at its place in the lineage, to `new`, the first version
/// of the contract that replaces it, whose versions follow `previous` (which
/// ends with `old`'s last version). The classifier must put the change on
/// the path the kind records. The new contract's versions are not checked
/// against `previous` here, so that a change of another kind, an identical
/// contract included, is refused with its kind; generating the whole
/// lineage checks them.
///
/// # Errors
/// Either contract's refusal, or a change that the kind's path does not
/// take, naming its kind.
fn evolution_review(
    old: ContractSources<'_>,
    old_generated: &GeneratedContract,
    new: ContractSources<'_>,
    previous: &[PreviousContract],
    adopted: &[String],
    kind: &EvolutionKind,
) -> Result<Review, ContractError> {
    let new_generated = generate_era(
        new,
        Receipts::Replay,
        &Placement {
            offset: previous.len(),
            receipts: adopted,
            superseded: true,
            claims: matches!(
                kind,
                EvolutionKind::BehaviourChange | EvolutionKind::Migration { .. }
            ),
            ..Placement::default()
        },
    )?;
    let change =
        diff::between(old, old_generated, new, &new_generated).map_err(|refused| refused.error)?;
    let with_migration = matches!(kind, EvolutionKind::Migration { .. });
    let path = diff::evolution_path(&change, with_migration)?;
    let expected = match kind {
        EvolutionKind::BehaviourChange => diff::Path::BehaviourChange,
        EvolutionKind::Rename => diff::Path::Rename,
        EvolutionKind::Migration { .. } => diff::Path::Migration,
    };
    if path != expected {
        return Err(ContractError::new(
            "v2/policy.json evolutions",
            format!(
                "an evolution recorded as `{}` is classified `{}`: the two contracts no longer \
                 make the change `contract evolve` recorded",
                kind.name(),
                change.kind().name()
            ),
        ));
    }
    let mut text = String::new();
    for line in change.lines() {
        text.push_str(&line);
        text.push('\n');
    }
    Ok(Review {
        text,
        claims: new_generated.claims.clone(),
        generated: new_generated,
    })
}

/// The review that `zeno-fcis contract evolve` records when the contract of
/// an application, generated from `app` as `app_generated`, is replaced by
/// the contract `new`, which has no adoptions or evolutions of its own, and
/// the path the change takes: the new contract's first version follows
/// every version of the application, and the classifier's kind.
/// `with_migration` says whether a migration file was given.
///
/// # Errors
/// Either contract's refusal, or a change no path takes, naming its kind.
pub(crate) fn evolve_change(
    app: ContractSources<'_>,
    app_generated: &GeneratedContract,
    new: ContractSources<'_>,
    with_migration: bool,
) -> Result<(String, diff::Path, &'static str), ContractError> {
    let adopted: Vec<String> = app_generated
        .summary
        .adoptions
        .iter()
        .map(|adoption| adoption.receipt_sha256.clone())
        .collect();
    let new_generated = generate_era(
        new,
        Receipts::Replay,
        &Placement {
            offset: app_generated.previous.len() + 1,
            receipts: &adopted,
            superseded: true,
            ..Placement::default()
        },
    )?;
    let change =
        diff::between(app, app_generated, new, &new_generated).map_err(|refused| refused.error)?;
    let path = diff::evolution_path(&change, with_migration)?;
    let mut text = String::new();
    for line in change.lines() {
        text.push_str(&line);
        text.push('\n');
    }
    Ok((text, path, change.kind().name()))
}

/// Generates one contract of a lineage, placed after `placement`'s earlier
/// versions; see [`generate`].
fn generate_era(
    sources: ContractSources<'_>,
    receipts: Receipts,
    placement: &Placement<'_>,
) -> Result<GeneratedContract, ContractError> {
    let offset = placement.offset;
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    // A version a migration or rename superseded keeps its own schema.
    let own_schema = placement
        .current_schema
        .is_some_and(|current| current != schema.as_slice());
    let schema_file = |version: u32| {
        if own_schema {
            format!("../v2/schema_v{version}.zcve")
        } else {
            "../v2/schema.zcve".to_owned()
        }
    };
    let commitment = schema_commitment(&schema)?;
    if let Some(origin) = sources.schema_origin {
        check_origin(origin, &schema, &commitment)?;
    }
    let mut contract = Contract::build(&declarations, &rules, commitment)?;
    if sources.adoptions.len() != rules.adoptions.len() {
        return Err(ContractError::new(
            "v2/policy.json adoptions",
            format!(
                "lists {} adoptions but {} retained candidate/receipt pairs were supplied",
                rules.adoptions.len(),
                sources.adoptions.len()
            ),
        ));
    }
    let mut previous = Vec::new();
    let mut adoptions = Vec::new();
    let mut replays = Vec::new();
    for (index, (adoption, files)) in rules.adoptions.iter().zip(sources.adoptions).enumerate() {
        let version = index + 1;
        let place = format!("v2/policy.json adoptions[{index}]");
        let directory = adoption_directory(version);
        if transform::sha256_hex(files.candidate) != adoption.candidate_sha256 {
            return Err(ContractError::new(
                format!("{place}.candidate_sha256"),
                format!("differs from {directory}/program.zcve"),
            ));
        }
        let receipt_bound = transform::sha256_hex(files.receipt) == adoption.receipt_sha256;
        if receipts == Receipts::Replay && !receipt_bound {
            return Err(receipt_differs(&place, &directory));
        }
        // Stores may run the superseded version, so it is retained exactly:
        // its policy, regenerated from these declarations and rules, must be
        // the one this adoption superseded.
        let policy = policy::encode(&contract, &schema)?;
        if transform::sha256_hex(&policy) != adoption.superseded_policy_sha256 {
            return Err(ContractError::new(
                format!("{place}.superseded_policy_sha256"),
                format!(
                    "differs from version {version}'s policy as these declarations and rules \
                     produce it: an edit after adoption {version} changes version {version}, \
                     which existing stores may run. Once a version is superseded, only another \
                     adoption changes the contract"
                ),
            ));
        }
        let current = contract.program()?;
        let current_bytes = program_bytes(&current)?;
        if files.candidate == current_bytes.as_slice() {
            return Err(unchanged(&place, &directory, version));
        }
        let cap = transform::DEFAULT_MAX_INPUT_TUPLES;
        let witness = sources
            .replayed
            .iter()
            .find(|witness| witness.is_of(files.receipt, &current_bytes, files.candidate, cap))
            .cloned();
        let checked = match (witness, receipts) {
            (Some(witness), Receipts::Replay) => Ok(witness),
            (None, Receipts::Replay) => {
                transform::replayed(files.receipt, &current_bytes, files.candidate, cap)
            }
            (_, Receipts::Refresh) => {
                transform::refreshed(files.receipt, &current_bytes, files.candidate, cap)
            }
        };
        let replayed = checked.map_err(|refused| {
            let action = match receipts {
                Receipts::Replay => "replay",
                Receipts::Refresh => "refresh",
            };
            ContractError::new(
                &place,
                format!(
                    "{directory}/receipt.json does not {action} against version {version}'s program and the candidate: {}",
                    replay_reason(&refused)
                ),
            )
        })?;
        // A refresh accepts the receipt it writes, from an interrupted refresh.
        if !receipt_bound && files.receipt != replayed.receipt() {
            return Err(receipt_differs(&place, &directory));
        }
        // The receipt is this checker's own, so its usage report is the checker's.
        let usage_preserved = receipt_usage_preserved(replayed.receipt()).ok_or_else(|| {
            ContractError::new(
                &place,
                format!("{directory}/receipt.json has no usage report"),
            )
        })?;
        if adoption.usage == Usage::Preserved && !usage_preserved {
            return Err(ContractError::new(
                format!("{place}.usage"),
                "the receipt reports that Step usage differs; `new-version` is required",
            ));
        }
        // The premises of a program succession that generation checks: the
        // identical laws include law 991, which pins every decision to the
        // case table; the receipt is an equivalence; and below, neither
        // version's Step limit binds.
        if !contract.pins_decisions() {
            return Err(ContractError::new(
                "v2/policy.json",
                "the contract has no decision-conformance law 991 pinning every decision to the case table",
            ));
        }
        let program_steps = replayed.equivalence().usage().max_steps;
        let law_steps = contract.law_steps();
        let superseded_limit = contract.budgets.step;
        replays.push(replayed);
        let version_number = u32::try_from(offset + version)
            .map_err(|_| ContractError::new("v2/policy.json adoptions", "too many adoptions"))?;
        previous.push(PreviousContract {
            source: render::source(
                &contract,
                render::Options {
                    schema: &schema_file(version_number),
                    policy: &format!("../v2/policy_v{version_number}.zcve"),
                    version: version_number,
                    receipts: &[],
                    evolutions: &[],
                },
            )?,
            policy,
            schema: own_schema.then(|| schema.clone()),
        });
        // Replay admitted the candidate; this import gives its nodes.
        let candidate = import_program(files.candidate).map_err(|error| {
            ContractError::new(
                &place,
                format!("the library refuses the candidate: {error}"),
            )
        })?;
        if program_bytes(&candidate)? == current_bytes {
            return Err(unchanged(&place, &directory, version));
        }
        let before = contract.nodes.len();
        contract.adopt(&candidate, &format!("adoptions[{index}]"))?;
        let steps = StepBound {
            program: program_steps,
            laws: law_steps,
            limits: [superseded_limit, contract.budgets.step],
        };
        if !steps.never_binds() {
            return Err(ContractError::new(
                &place,
                format!(
                    "a Step limit could refuse a decision: version {version}'s limit {} and version \
                     {}'s limit {} must cover the largest program usage the receipt measured, {} \
                     and {}, plus {law_steps} law Steps",
                    steps.limits[0],
                    version + 1,
                    steps.limits[1],
                    steps.program[0],
                    steps.program[1]
                ),
            ));
        }
        adoptions.push(AdoptionSummary {
            candidate_sha256: adoption.candidate_sha256.clone(),
            receipt_sha256: adoption.receipt_sha256.clone(),
            usage: adoption.usage,
            usage_preserved,
            superseded_policy_sha256: adoption.superseded_policy_sha256.clone(),
            steps,
            program_nodes: [before, contract.nodes.len()],
        });
    }
    let version = u32::try_from(offset + adoptions.len() + 1)
        .map_err(|_| ContractError::new("v2/policy.json adoptions", "too many adoptions"))?;
    let receipts: Vec<String> = placement
        .receipts
        .iter()
        .cloned()
        .chain(
            adoptions
                .iter()
                .map(|adoption| adoption.receipt_sha256.clone()),
        )
        .collect();
    let leaf = format!("../v2/policy_v{version}.zcve");
    let leaf_schema = schema_file(version);
    let source = render::source(
        &contract,
        if placement.superseded {
            render::Options {
                schema: &leaf_schema,
                policy: &leaf,
                version,
                receipts: &[],
                evolutions: &[],
            }
        } else {
            render::Options {
                schema: "../v2/schema.zcve",
                policy: "../v2/policy.zcve",
                version,
                receipts: &receipts,
                evolutions: placement.evolutions,
            }
        },
    )?;
    let policy = policy::encode(&contract, &schema)?;
    // An identity binds the policy, so a repeated policy is a repeated
    // identity, which no store could tell apart from the earlier version.
    let policies: Vec<&[u8]> = placement
        .previous
        .iter()
        .chain(&previous)
        .map(PreviousContract::policy)
        .chain(std::iter::once(policy.as_slice()))
        .collect();
    for (later, repeated) in policies.iter().enumerate().skip(1) {
        if let Some(earlier) = policies[..later]
            .iter()
            .position(|policy| policy == repeated)
        {
            let earlier_versions = placement.previous.len();
            let place = match later.checked_sub(earlier_versions + 1) {
                Some(index) => format!("v2/policy.json adoptions[{index}]"),
                None => "v2/policy.json evolutions".to_owned(),
            };
            // Versions this check was not given come first.
            let unchecked = offset - earlier_versions.min(offset);
            return Err(ContractError::new(
                place,
                format!(
                    "version {} would repeat version {}: a contract's identity binds its policy, \
                     so stores could not tell them apart. Adopt a program no earlier version ran",
                    unchecked + later + 1,
                    unchecked + earlier + 1
                ),
            ));
        }
    }
    let program = program_bytes(&contract.program()?)?;
    let claims = if placement.claims {
        model::compiled_claims(&declarations, &rules, &contract.laws)?
    } else {
        Vec::new()
    };
    let summary = ContractSummary {
        application: rules.template.clone(),
        version,
        program_nodes: contract.nodes.len(),
        outputs: contract.output_types.len(),
        law_nodes: contract.laws.iter().map(|law| law.nodes.len()).sum(),
        law_ids: contract.laws.iter().map(|law| law.id).collect(),
        schema_types: declarations.types.len(),
        read_budget: contract.budgets.read,
        step_budget: contract.budgets.step,
        byte_budget: contract.budgets.byte,
        adoptions,
        evolutions: Vec::new(),
    };
    Ok(GeneratedContract {
        schema,
        source,
        policy,
        previous,
        summary,
        program,
        replayed: replays,
        claims,
    })
}

fn receipt_differs(place: &str, directory: &str) -> ContractError {
    ContractError::new(
        format!("{place}.receipt_sha256"),
        format!("differs from {directory}/receipt.json"),
    )
}

impl StepBound {
    /// Each version's largest program usage plus every law Step fits its limit.
    pub(crate) fn never_binds(self) -> bool {
        (0..2).all(|side| {
            u128::from(self.program[side]) + u128::from(self.laws) <= u128::from(self.limits[side])
        })
    }
}

/// The refusal of an adoption whose candidate is the program it would replace.
fn unchanged(place: &str, directory: &str, version: usize) -> ContractError {
    ContractError::new(
        place,
        format!(
            "{directory}/program.zcve is version {version}'s own program; an adoption must change it"
        ),
    )
}

/// The canonical bytes of an admitted program: what receipts name.
fn program_bytes(program: &zeno_fcis_synthesis::finite::Program) -> Result<Vec<u8>, ContractError> {
    use zeno_fcis_codec::CanonicalEncode;
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .map_err(|error| {
            ContractError::new(
                "v2/policy.json",
                format!("the decision program has no canonical encoding: {error:?}"),
            )
        })
}

/// `usage.usage_preserved` of a receipt whose bytes replayed.
fn receipt_usage_preserved(receipt: &[u8]) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_slice(receipt).ok()?;
    value.get("usage")?.get("usage_preserved")?.as_bool()
}

/// One line on why a receipt did not replay.
fn replay_reason(replay: &Replay) -> String {
    match replay {
        Replay::Matched => "replayed".to_owned(),
        Replay::Unreadable => "not a transform receipt with readable bindings".to_owned(),
        Replay::OverCap { size, cap } => format!(
            "the input domain of {} tuples exceeds the replay cap of {cap}",
            size.map_or_else(|| "more than u128::MAX".to_owned(), |size| size.to_string())
        ),
        Replay::Differs(fields) if fields.is_empty() => {
            "the recomputed receipt differs in its encoding".to_owned()
        }
        Replay::Differs(fields) => format!("`{}` differ from the record", fields.join("`, `")),
        Replay::NotEquivalent(Rejection::Counterexample(witness)) => format!(
            "the programs differ on input tuple {} of the enumeration",
            witness.ordinal
        ),
        Replay::NotEquivalent(Rejection::Inconclusive(stop)) => match stop {
            Inconclusive::DomainTooLarge { .. } => "the input domain is too large".to_owned(),
            Inconclusive::BudgetBoundary { minimum_limit, .. } => {
                format!("the receipt's Step limit binds; {minimum_limit} never binds")
            }
            Inconclusive::CoverageMismatch { .. } => {
                "the enumeration did not cover the domain".to_owned()
            }
        },
        Replay::NotEquivalent(Rejection::Refused(refusal)) => format!("refused: {refusal:?}"),
    }
}

/// The schema commitment envelopes carry: the domain-separated SHA-256 of
/// the original schema bytes.
fn schema_commitment(schema: &[u8]) -> Result<[u8; 32], ContractError> {
    commitment::<RustCryptoSha256>(domains::SCHEMA, schema)
        .map(|hash| *hash.as_bytes())
        .map_err(|_| ContractError::new("v2/schema.zcve", "is too large to commit to"))
}

/// The origin record's byte count, SHA-256 and commitment must all describe
/// the schema bytes.
fn check_origin(origin: &str, schema: &[u8], commitment: &[u8; 32]) -> Result<(), ContractError> {
    let invalid = |reason: &str| ContractError::new("v2/schema-origin.json", reason);
    let record: serde_json::Value =
        serde_json::from_str(origin).map_err(|error| invalid(&error.to_string()))?;
    let text = |key: &str| record.get(key).and_then(serde_json::Value::as_str);
    let digest = hex(RustCryptoSha256::hash(schema).as_bytes());
    if record.get("bytes").and_then(serde_json::Value::as_u64) != u64::try_from(schema.len()).ok() {
        return Err(invalid("`bytes` differs from v2/schema.zcve"));
    }
    if text("sha256") != Some(digest.as_str()) {
        return Err(invalid("`sha256` differs from v2/schema.zcve"));
    }
    if text("schema_commitment") != Some(hex(commitment).as_str()) {
        return Err(invalid("`schema_commitment` differs from v2/schema.zcve"));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
