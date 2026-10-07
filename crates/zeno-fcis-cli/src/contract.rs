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

mod adoption;
mod declarations;
mod expr;
mod graph;
mod layout;
mod model;
mod policy;
mod render;
pub(crate) mod review;
mod rules;
mod schema;
#[cfg(test)]
mod tests;

use std::fmt;

use zeno_fcis_codec::{CommitmentHasher, commitment, domains};
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite_runtime::import_program;

use crate::transform::{self, Inconclusive, Rejection, Replay, Replayed};
pub(crate) use adoption::{Adopted, AdoptionPlan, CheckedCandidate};
use declarations::Declarations;
use model::Contract;
use rules::Rules;
use rules::with_receipt_digests;
pub(crate) use rules::{Adoption, Usage, with_adoption};

/// The directory of adoption `n`'s retained files, relative to the
/// application: `v2/adoptions/n/program.zcve` and `v2/adoptions/n/receipt.json`.
pub(crate) fn adoption_directory(ordinal: usize) -> String {
    format!("v2/adoptions/{ordinal}")
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
}

/// A superseded contract version, emitted beside the current one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreviousContract {
    source: String,
    policy: Vec<u8>,
}

impl PreviousContract {
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
    /// Each adoption as its receipt replayed it, oldest first.
    pub(crate) adoptions: Vec<AdoptionSummary>,
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

fn generate(
    sources: ContractSources<'_>,
    receipts: Receipts,
) -> Result<GeneratedContract, ContractError> {
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
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
        let version_number = u32::try_from(version)
            .map_err(|_| ContractError::new("v2/policy.json adoptions", "too many adoptions"))?;
        previous.push(PreviousContract {
            source: render::source(
                &contract,
                render::Options {
                    policy: &format!("../v2/policy_v{version}.zcve"),
                    version: version_number,
                    receipts: &[],
                },
            )?,
            policy,
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
    let version = u32::try_from(adoptions.len() + 1)
        .map_err(|_| ContractError::new("v2/policy.json adoptions", "too many adoptions"))?;
    let receipts: Vec<String> = adoptions
        .iter()
        .map(|adoption| adoption.receipt_sha256.clone())
        .collect();
    let source = render::source(
        &contract,
        render::Options {
            policy: "../v2/policy.zcve",
            version,
            receipts: &receipts,
        },
    )?;
    let policy = policy::encode(&contract, &schema)?;
    // An identity binds the policy, so a repeated policy is a repeated
    // identity, which no store could tell apart from the earlier version.
    let policies: Vec<&[u8]> = previous
        .iter()
        .map(PreviousContract::policy)
        .chain(std::iter::once(policy.as_slice()))
        .collect();
    for (later, repeated) in policies.iter().enumerate().skip(1) {
        if let Some(earlier) = policies[..later]
            .iter()
            .position(|policy| policy == repeated)
        {
            return Err(ContractError::new(
                format!("v2/policy.json adoptions[{}]", later - 1),
                format!(
                    "version {} would repeat version {}: a contract's identity binds its policy, \
                     so stores could not tell them apart. Adopt a program no earlier version ran",
                    later + 1,
                    earlier + 1
                ),
            ));
        }
    }
    let program = program_bytes(&contract.program()?)?;
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
    };
    Ok(GeneratedContract {
        schema,
        source,
        policy,
        previous,
        summary,
        program,
        replayed: replays,
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
