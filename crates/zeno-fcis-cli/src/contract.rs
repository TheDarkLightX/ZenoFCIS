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

use declarations::Declarations;
use model::Contract;
use rules::Rules;

/// The application files a contract is generated from.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ContractSources<'a> {
    /// `project.zeno`.
    pub(crate) project: &'a str,
    /// `v2/policy.json`: leaf bindings, variables, cases, genesis and law kinds.
    pub(crate) rules: &'a str,
    /// `v2/schema-origin.json` when the application keeps one; its digests
    /// must describe the generated schema.
    pub(crate) schema_origin: Option<&'a str>,
}

/// The generated files and their sizes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GeneratedContract {
    schema: Vec<u8>,
    source: String,
    policy: Vec<u8>,
    summary: ContractSummary,
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

    /// Program, law and budget sizes.
    pub(crate) fn summary(&self) -> &ContractSummary {
        &self.summary
    }
}

/// Sizes of a generated contract, for reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContractSummary {
    /// The application the rules name.
    pub(crate) application: String,
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

/// Generates `v2/schema.zcve`, `src/v2_contract.rs` and `v2/policy.zcve`
/// from an application's declarations and rules.
///
/// # Errors
/// Returns the first declaration or rule with no contract form, an origin
/// record that disagrees with the schema, or the library's refusal.
pub(crate) fn generate_contract(
    sources: ContractSources<'_>,
) -> Result<GeneratedContract, ContractError> {
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    if let Some(origin) = sources.schema_origin {
        check_origin(origin, &schema, &commitment)?;
    }
    let contract = Contract::build(&declarations, &rules, commitment)?;
    let source = render::source(&contract)?;
    let policy = policy::encode(&contract, &schema)?;
    let summary = ContractSummary {
        application: rules.template.clone(),
        program_nodes: contract.nodes.len(),
        outputs: contract.output_types.len(),
        law_nodes: contract.laws.iter().map(|law| law.nodes.len()).sum(),
        law_ids: contract.laws.iter().map(|law| law.id).collect(),
        schema_types: declarations.types.len(),
        read_budget: contract.budgets.read,
        step_budget: contract.budgets.step,
        byte_budget: contract.budgets.byte,
    };
    Ok(GeneratedContract {
        schema,
        source,
        policy,
        summary,
    })
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
