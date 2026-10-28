//! Checked assume-guarantee composition, profile 1: separately bound leaf
//! contracts, one selected leaf per command, immutable peer views read from
//! the same pre-state, disjoint writes, no effects or outbox.
//!
//! A manifest (`zeno-fcis/checked-composition/1`) names each leaf's
//! `project.zeno` and `v2/policy.json` by SHA-256, the identity of the
//! program they bind to, the leaf's invariant I, its assumption A over its
//! interface observation and command, its guarantees, the state it owns and
//! its complete footprint, and the interface edges that feed one leaf's
//! interface field from a peer's view.
//!
//! The checker owns every verdict and takes only source bytes: the manifest
//! and each leaf's two files, which it compiles itself. For each leaf it
//! derives the complete local domain from the leaf's own declarations and
//! runs the leaf's bound Authority on every input: genuine genesis and
//! publication, exact replay. It requires Init ⇒ I; on every input
//! satisfying I and A, an Accept that keeps I or an unchanged business
//! Reject, no technical refusal, effect, delivery or committed failure, and
//! every transition relation; I ⇒ each StateFact on every state; the
//! required witnesses. Across leaves, `zeno-fcis-compose` judges the
//! composition specification the manifest denotes: disjoint ownership as
//! the absence of a write-write conflict, and a provider guarantee for each
//! assumption. The checker adds what compose cannot express: complete
//! ownership of slots `0..n`, exactly one provider for each interface field
//! through a typed view, discharge by StateFacts only, and, over every view
//! value and command, that the provider StateFacts imply the consumer
//! assumption. A cycle of edges is accepted: every fact on it is derived
//! from its own leaf's invariant, which that leaf's own genesis and
//! preservation checks establish, never from another leaf's assumption.
//!
//! A `zeno-fcis/checked-composition/2` manifest also nests the leaves in
//! groups, with the two checks of equation 2.8 of Capucci and Myers,
//! "Compositionality of Lyapunov functions via assume-guarantee reasoning"
//! (arXiv:2604.03017). Each group has an outer assumption over its
//! environment inputs (the interface fields no edge from inside it feeds)
//! and the leaves' command fields, and an outer guarantee over the views it
//! exports. For each group the checker shows, over the complete domains of
//! the fields each formula reads, that the provider leaves' StateFacts on
//! the inputs fed inside the group and the outer assumption imply every
//! child's assumption, and that the children's guarantees imply the outer
//! guarantee. A child assumption that reads only environment inputs and
//! commands and is not discharged rises into the group's outer assumption;
//! at the root, the outer assumption is the condition of the claim. The
//! claim is unconditional when the root assumes nothing, unconditional under
//! named external assumptions when it reads only environment inputs, and
//! conditional when it reads a command field;
//! [`CheckedComposition::claim_kind`] and [`CheckedComposition::conditions`]
//! state it.
//!
//! This is not the strict discharge rule of equation 2.8: assumptions are
//! discharged by the provider leaves' StateFacts, which hold because every
//! leaf's invariant holds jointly, and the outer guarantees are checked and
//! reported but discharge nothing. Outer assumptions read environment
//! inputs and command fields only, not the route: which leaf a command
//! selects. A command field is one variable shared by ID, so every leaf
//! that declares an ID must give it the same type.
//!
//! [`aggregate`] generates the composition's aggregate: one ordinary
//! contract whose state is every leaf's slots and whose command selects one
//! leaf. With the aggregate in the sources, [`check`] validates it after
//! rule SUBST of the paper, structurally and without enumerating the joint
//! state: state projection, command routing, views from the same pre-state,
//! the selected leaf's decision on each route, every unselected slot
//! unchanged, no eager node that can trap, a meter that covers the eager
//! work, and laws implied by the leaves' invariants.
//!
//! An aggregate has at most 64 inputs, 64 outputs and 4,096 program nodes;
//! a register ring of one state field per leaf fits at most 61 leaves.
//!
//! The repository's Verus harness `verification/verus/checked_assembly.rs`
//! proves, with the tables, the validated step equation and genesis as
//! premises, that every history of the aggregate keeps every leaf's
//! invariant at every state, and that the validator's route specialization
//! preserves the evaluator specification's values. [`TRUSTED_BASE`] lists
//! what is tested, not proved, including the link from the bound
//! Authorities' actual decisions to the theorem's abstract decision
//! functions: the application-level safety statement is machine-checked
//! over the evaluator specification, with the validator implementation
//! tested, not proved.
//!
//! The result is a [`CheckedComposition`], which only [`check`] builds. It
//! records the checker's source-bound identity and the SHA-256 of every
//! table it ran; [`CheckedComposition::receipt`] is its canonical receipt
//! and [`replay`] runs every check again and requires the same receipt.
//!
//! A `zeno-fcis/checked-composition/3` manifest selects the composed-domain
//! profile instead, which [`check_domain`] checks: components step together
//! in merge order on one pre-state, each reading the request and earlier
//! components' pre-state through bindings; a Reject or any refusal ends the
//! step with no change, the first CommittedFailure commits the successors
//! up to and including its own, and otherwise every successor commits
//! ([`joint`]). The checker plans three kinds of obligation, `local`,
//! `assume` and `couple`, each an exhaustive finite check that never
//! enumerates the global product, computes every size before evaluating
//! anything, refuses a plan over budget with nothing evaluated, and checks a
//! plan within budget exhaustively ([`obligations`]). Its verdict is
//! `proved`, `violated` with the first witness, or `refused`. A component may
//! own no state: its state fields are then read-only bindings of earlier
//! components' pre-state, which no committing decision may change. The
//! claim, [`DOMAIN_CLAIM`], rests on exhaustive finite checks and an
//! induction argument stated in `docs/COMPOSED_DOMAIN_COMPOSITION.md`, not
//! on a machine-checked proof; [`DOMAIN_TRUSTED_BASE`] lists what it rests
//! on. [`JointComposition`] exposes the joint step itself, from in-memory
//! sources. This profile's code is LLM-generated, as is the rest of this
//! crate, and checked by its tests and by an independent reference.

mod aggregate;
mod assembly;
#[cfg(test)]
mod assembly_differential_tests;
#[cfg(test)]
mod assembly_tests;
mod compose;
#[cfg(test)]
mod compose_tests;
mod coupling_cuts;
pub mod domain;
#[cfg(test)]
mod domain_tests;
mod formula;
mod hierarchy;
#[cfg(test)]
mod hierarchy_tests;
pub(crate) mod identity;
#[cfg(test)]
mod identity_tests;
pub mod joint;
#[cfg(test)]
mod joint_tests;
#[cfg(test)]
mod journal_tests;
mod leaf;
mod manifest;
pub mod obligations;
#[cfg(test)]
mod obligations_tests;
mod predicate;
mod receipt;
#[cfg(test)]
mod receipt_tests;
mod refusal;
mod resolve;
pub mod service;
#[cfg(test)]
mod service_tests;
mod structure;
#[cfg(test)]
mod structure_tests;
mod table;
mod term;
#[cfg(test)]
mod term_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde_json::{Value, json};
use zeno_fcis_compose::{CompositionClaim, CompositionSpec};
use zeno_fcis_synthesis::finite::v2_composition as c;

use crate::internals::declarations::Source;
pub use aggregate::AggregateSources;
use assembly::Assembly;
pub use compose::{CheckedAssumeGuarantee, ComposeRefusal, require_checked};
pub use domain::{DOMAIN_CHECKER, FieldDomain, MAX_OBLIGATION, MAX_TOTAL, domain_paths};
pub use hierarchy::{ClaimKind, Condition};
pub(crate) use identity::Identity;
pub use joint::{
    JointComposition, JointInputError, JointOutcome, JointRow, OutcomeKind, RefusedClass,
    RequestSlot, StateSlot,
};
pub use leaf::LeafCertificate;
use leaf::{Compiled, sha256};
use manifest::Manifest;
pub use manifest::{CHECKER, DOMAIN_SCHEMA, HIERARCHY_SCHEMA, MANIFEST_SCHEMA, leaf_paths};
pub use obligations::{DOMAIN_RESULT_SCHEMA, DomainResult, Verdict, check_domain};
use predicate::Truth;
pub use receipt::{
    DOMAIN_RECEIPT_SCHEMA, RECEIPT_SCHEMA, ReceiptForm, ReplayRefusal, replay, replay_domain,
};
pub use refusal::{
    AuthorityRefusal, ComposeFinding, ExecutionRefusal, LawRefusal, MeterRefusal, Obligation,
    Refusal, Stage,
};
use structure::Resolved;
pub use structure::SPEC_VERSION;
use table::{Table, TableKind};

/// The schema of a checked composition's result.
pub const RESULT_SCHEMA: &str = "zeno-fcis/checked-composition-result/2";

// Report preparation only: the same safe-number-or-decimal-string/1 rule
// as CLI door/judge reports, without changing canonical scalar input formats.
const INTEGER_ENCODING: &str = "safe-number-or-decimal-string/1";
fn json_integer(value: i128) -> Value {
    const SAFE: i128 = (1i128 << 53) - 1;
    if (-SAFE..=SAFE).contains(&value) {
        Value::from(value as i64)
    } else {
        Value::from(value.to_string())
    }
}
fn json_integers(values: &[i128]) -> Value {
    Value::Array(values.iter().copied().map(json_integer).collect())
}

/// What an application-level claim rests on that is tested, not proved.
/// The repository's Verus harness `verification/verus/checked_assembly.rs`
/// proves the rest over the evaluator specification. The result, its
/// receipt, the Verus gate's receipt and the documentation state this list.
pub const TRUSTED_BASE: [&str; 6] = [
    "The checker's exhaustive enumeration of each leaf's local, StateFact and interface tables with the leaf's bound Authority.",
    "The group checks of equation 2.8: their tables and the discharge and rise of each child's assumption.",
    "The assembly validator's canonical terms, route specialization and case pairing, compared with the library evaluator on the register ring and chain and constructor by constructor in unit tests.",
    "The assembly validator's comparison of classes, reasons, assignments and frames, its field, genesis and routing checks, and its law-implication tables.",
    "The interval analysis showing that no eager node can trap, and the meter check with its cost model of one Step per program or law node.",
    "The link from the bound Authorities' actual decisions (case selection, assignments, law and meter refusals) to the abstract decision functions of the Verus theorem.",
];

const NONCLAIMS: [&str; 10] = [
    "Every item of the trusted base is tested, not proved.",
    "No runtime authority, publication, store or replay of a composed application is granted.",
    "States outside a history from checked genesis keep only the conditional local statements.",
    "A conditional claim, and a claim under named external assumptions, covers only histories whose commands and context satisfy the root's outer assumption; neither the checker nor the aggregate checks those conditions when a command runs.",
    "Outer assumptions read environment inputs and command fields only, not the route: which leaf a command selects.",
    "Assumptions are discharged by the provider leaves' StateFacts; outer guarantees are checked and reported but discharge nothing, so this is not the strict discharge rule of equation 2.8.",
    "No liveness, fairness, trace, continuous-dynamics, delivery, asynchronous or shared-write coupling theorem.",
    "One selected leaf per command, with peer views from the same pre-state; no other coupling.",
    "Finite declared domains only; a table with more than 1048576 rows is refused, not sampled.",
    "An aggregate has at most 64 inputs, 64 outputs and 4,096 program nodes; a register ring fits at most 61 leaves.",
];

/// The claim a `proved` composed-domain result makes, at its true strength.
pub const DOMAIN_CLAIM: &str = "If every obligation holds, every reachable global state satisfies every component invariant and coupling invariant, and every input delivered to a component, with a read-only component's delivered state, is inside its domains and satisfies its assumption and invariant. This rests on an induction argument over executions from genesis, stated in docs/COMPOSED_DOMAIN_COMPOSITION.md, and on exhaustive finite checks of each obligation; it is not machine-checked.";

/// What a composed-domain claim rests on that is tested, not proved.
pub const DOMAIN_TRUSTED_BASE: [&str; 6] = [
    "The induction argument from the three obligations to the claim, stated in docs/COMPOSED_DOMAIN_COMPOSITION.md; it is not machine-checked.",
    "The planner's exact sizes and the checker's exhaustive enumeration of every obligation's planned space.",
    "Each component's decisions, made once by its bound Authority on every input of its complete local domain and looked up by the checks and the joint step.",
    "Each formula's table over the complete product of the domains it reads: a predicate decided by the library's law evaluator, with no value read as false, and a binding computed by the library's program evaluator from its compiled rule expression.",
    "The joint step's implementation, compared with an independent reference; it follows the retained composed-domain oracle's merge order, provisional Accepts, Rejects and CommittedFailure prefix, with participation, no-change refusals and pre-state bindings in place of ports, and is not proved equal to it.",
    "The coupling check's treatment of a component outside the coupling: a Reject or refusal changes nothing, and one with a CommittedFailure case may end the step before the next member.",
];

/// What a composed-domain result does not claim.
pub const DOMAIN_NONCLAIMS: [&str; 7] = [
    "Nothing here is machine-checked; every item of the trusted base is tested, not proved.",
    "A plan over budget is refused before anything is evaluated, never sampled.",
    "No contextual refinement, adoption path or aggregate contract.",
    "Nothing about storage atomicity, concurrency, delivery, effects or outbox.",
    "No runtime authority, publication or store of a composed application is granted.",
    "Finite declared domains only; states not reachable from genesis keep only each obligation's conditional statement.",
    "No liveness, fairness or trace property.",
];

/// The composed-domain manifest and each component's files, in merge order.
#[derive(Clone, Copy, Debug)]
pub struct DomainSources<'a> {
    /// The manifest's bytes.
    pub manifest: &'a str,
    /// Each component's files, in merge order.
    pub components: &'a [LeafSources],
}

/// The manifest, every leaf's files in manifest leaf order and, to check an
/// assembly, the aggregate contract's files.
#[derive(Clone, Copy, Debug)]
pub struct CompositionSources<'a> {
    /// The manifest's bytes.
    pub manifest: &'a str,
    /// Each leaf's files, in the manifest's leaf order.
    pub leaves: &'a [LeafSources],
    /// The aggregate contract to validate against the composition, if any.
    pub aggregate: Option<&'a AggregateSources>,
}

/// One leaf's `project.zeno` and `v2/policy.json`.
#[derive(Clone, Debug)]
pub struct LeafSources {
    /// `project.zeno`.
    pub project: String,
    /// `v2/policy.json`.
    pub rules: String,
}

/// A composition every obligation of the profile holds for. Fields are
/// private; only [`check`] builds one, after every check has passed. A
/// generic evidence verifier's acceptance is a different, attested result
/// and cannot produce this one.
///
/// It has no public constructor, no `Default` and no `Deserialize`:
///
/// ```compile_fail,E0451
/// use zeno_fcis_contract::checked_composition::CheckedComposition;
/// let forged = CheckedComposition { name: String::new() };
/// ```
///
/// ```compile_fail,E0599
/// use zeno_fcis_contract::checked_composition::CheckedComposition;
/// let forged = CheckedComposition::default();
/// ```
///
/// ```compile_fail,E0277
/// use zeno_fcis_contract::checked_composition::CheckedComposition;
/// let forged: CheckedComposition = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Debug)]
pub struct CheckedComposition {
    name: String,
    schema: &'static str,
    manifest_sha256: [u8; 32],
    manifest_commitment: [u8; 32],
    leaves: Vec<LeafCertificate>,
    slots: usize,
    edges: Vec<EdgeResult>,
    cycles: Vec<Vec<u32>>,
    spec: CompositionSpec,
    spec_hash: [u8; 32],
    claims: Vec<CompositionClaim>,
    identity: Identity,
    hierarchy: Option<hierarchy::Checked>,
    assembly: Option<Assembly>,
}

#[derive(Debug)]
struct EdgeResult {
    consumer: u32,
    interface: u16,
    provider: u32,
    view: u32,
    facts: Vec<u32>,
}

impl CheckedComposition {
    /// The result as JSON, the same on repeat.
    #[must_use]
    pub fn json(&self) -> Value {
        let local_rows: u64 = self.leaves.iter().map(|leaf| leaf.rows).sum();
        let admitted: u64 = self.leaves.iter().map(|leaf| leaf.admitted).sum();
        let state_fact_rows: u64 = self.leaves.iter().map(|leaf| leaf.state_fact_rows).sum();
        let replays: u64 = self.leaves.iter().map(|leaf| leaf.replays).sum();
        let mut checker = self.identity.json();
        if let Some(fields) = checker.as_object_mut() {
            fields.insert("id".to_owned(), json!(manifest::CHECKER));
            fields.insert("version".to_owned(), json!(env!("CARGO_PKG_VERSION")));
            fields.insert("evidence_caching".to_owned(), json!("none"));
        }
        json!({
            "schema": RESULT_SCHEMA,
            "integer_encoding": INTEGER_ENCODING,
            "evidence": "checked",
            "profile": "one selected leaf per command; immutable peer views from the same pre-state; disjoint writes; no effects or outbox",
            "manifest": {
                "name": self.name,
                "schema": self.schema,
                "sha256": hex(&self.manifest_sha256),
                "commitment": hex(&self.manifest_commitment),
            },
            "checker": checker,
            "composition_spec": {
                "version": SPEC_VERSION,
                "commitment": hex(&self.spec_hash),
                "claims": self.claims.len(),
            },
            "leaves": self.leaves.iter().map(LeafCertificate::json).collect::<Vec<_>>(),
            "slots": self.slots,
            "edges": self.edges.iter().map(|edge| json!({
                "consumer": edge.consumer,
                "interface": format!("context.102.{}", edge.interface),
                "provider": edge.provider,
                "view": edge.view,
                "facts": edge.facts,
            })).collect::<Vec<_>>(),
            "cycles": self.cycles,
            "hierarchy": self.hierarchy.as_ref().map(hierarchy::Checked::json),
            "assembly": self.assembly.as_ref().map(Assembly::json),
            "totals": {
                "leaves": self.leaves.len(),
                "local_rows": local_rows,
                "admitted_rows": admitted,
                "replayed_decisions": replays,
                "state_fact_rows": state_fact_rows,
                "edges": self.edges.len(),
                "interface_rows": self.interface_rows(),
                "group_assumption_rows": self.hierarchy.as_ref().map_or(0, |checked| checked.rows().0),
                "group_guarantee_rows": self.hierarchy.as_ref().map_or(0, |checked| checked.rows().1),
                "assembly_law_rows": self.assembly.as_ref().map_or(0, Assembly::law_rows),
            },
            "trusted_base": TRUSTED_BASE,
            "nonclaims": NONCLAIMS,
        })
    }

    fn interface_rows(&self) -> u64 {
        self.leaves.iter().map(|leaf| leaf.interface_rows).sum()
    }

    /// One line for each count a reader checks first.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let local_rows: u64 = self.leaves.iter().map(|leaf| leaf.rows).sum();
        let mut lines = vec![
            format!("checked composition {}", self.name),
            format!(
                "leaves {}: {local_rows} local inputs run by their bound Authorities",
                self.leaves.len()
            ),
            format!(
                "edges {}: {} interface implications checked",
                self.edges.len(),
                self.interface_rows()
            ),
            format!("cycles {}", self.cycles.len()),
            format!("manifest commitment {}", hex(&self.manifest_commitment)),
            format!("checker identity {}", hex(&self.identity.digest())),
        ];
        if let Some(checked) = &self.hierarchy {
            lines.extend(checked.lines());
        }
        if let Some(assembly) = &self.assembly {
            lines.push(format!(
                "assembly checked: aggregate program {}",
                hex(&assembly.program_sha256())
            ));
        }
        lines
    }

    /// Each leaf's certificate, in manifest order.
    #[must_use]
    pub fn leaves(&self) -> &[LeafCertificate] {
        &self.leaves
    }

    /// The commitment of the checked manifest's canonical bytes.
    #[must_use]
    pub const fn manifest_commitment(&self) -> [u8; 32] {
        self.manifest_commitment
    }

    /// The source-bound identity of the checker that built this result.
    #[must_use]
    pub const fn checker_source_digest(&self) -> [u8; 32] {
        self.identity.digest()
    }

    /// The kind of the application claim: what the root group's outer
    /// assumption leaves as its condition. A flat manifest's claim is
    /// unconditional.
    #[must_use]
    pub fn claim_kind(&self) -> ClaimKind {
        self.hierarchy
            .as_ref()
            .map_or(ClaimKind::Unconditional, |checked| checked.claim)
    }

    /// The conditions of the application claim, empty when it is
    /// unconditional.
    #[must_use]
    pub fn conditions(&self) -> &[Condition] {
        self.hierarchy
            .as_ref()
            .map_or(&[], |checked| checked.conditions.as_slice())
    }

    /// The `zeno-fcis-compose` specification the manifest denotes, which
    /// compose judged.
    #[must_use]
    pub const fn composition_spec(&self) -> &CompositionSpec {
        &self.spec
    }
}

/// Checks a composition from its source bytes, and, when the sources carry
/// one, validates the aggregate contract against it. Every verdict is the
/// checker's own or compose's.
///
/// # Errors
/// The first failed obligation or boundary refusal, with its witness.
pub fn check(sources: CompositionSources<'_>) -> Result<CheckedComposition, Refusal> {
    let identity = Identity::current();
    let run = run(sources)?;
    let assembly = sources
        .aggregate
        .map(|aggregate| {
            assembly::validate(&run.manifest, &run.compiled, &run.judged.edges, aggregate)
        })
        .transpose()?;
    Ok(result(sources, identity, run, assembly))
}

/// Checks a composition once, generates its aggregate contract and
/// validates it: [`check`] with the generated aggregate in the sources, and
/// the aggregate. Nothing is returned unless every obligation holds.
///
/// # Errors
/// The check's refusal, a leaf outside the assembly profile, or the
/// validator's refusal of the generated aggregate.
pub fn check_generated(
    sources: CompositionSources<'_>,
) -> Result<(CheckedComposition, AggregateSources), Refusal> {
    let identity = Identity::current();
    let run = run(sources)?;
    let generated = aggregate::generate(&run.manifest, &run.compiled, &run.judged.edges)?;
    let assembly = assembly::validate(&run.manifest, &run.compiled, &run.judged.edges, &generated)?;
    Ok((result(sources, identity, run, Some(assembly)), generated))
}

/// The result of a run that every obligation held for.
fn result(
    sources: CompositionSources<'_>,
    identity: Identity,
    run: Run,
    assembly: Option<Assembly>,
) -> CheckedComposition {
    let Run {
        manifest,
        manifest_commitment,
        compiled: _,
        judged,
        leaves,
        hierarchy,
    } = run;
    CheckedComposition {
        name: manifest.name.clone(),
        schema: manifest.schema(),
        manifest_sha256: sha256(sources.manifest.as_bytes()),
        manifest_commitment,
        leaves,
        slots: judged.slots,
        edges: manifest
            .edges
            .iter()
            .map(|edge| EdgeResult {
                consumer: edge.consumer,
                interface: edge.interface,
                provider: edge.provider,
                view: edge.view,
                facts: edge.facts.clone(),
            })
            .collect(),
        cycles: cycles(&manifest),
        spec: judged.spec,
        spec_hash: judged.spec_hash,
        claims: judged.claims,
        identity,
        hierarchy,
        assembly,
    }
}

/// Generates the aggregate contract of a composition, after checking it. The
/// aggregate is an ordinary contract for `generate contract`; only the
/// validator, through [`check`] with the aggregate in the sources, decides
/// whether an aggregate is a machine map onto the composition.
///
/// # Errors
/// The check's refusal, or a leaf outside the assembly profile.
pub fn aggregate(sources: CompositionSources<'_>) -> Result<AggregateSources, Refusal> {
    let run = run(sources)?;
    aggregate::generate(&run.manifest, &run.compiled, &run.judged.edges)
}

/// The checked composition before its result is assembled.
struct Run {
    manifest: Manifest,
    manifest_commitment: [u8; 32],
    compiled: Vec<Compiled>,
    judged: structure::Judged,
    leaves: Vec<LeafCertificate>,
    hierarchy: Option<hierarchy::Checked>,
}

fn run(sources: CompositionSources<'_>) -> Result<Run, Refusal> {
    let manifest = Manifest::read(sources.manifest)?;
    let manifest_commitment = manifest.commitment();
    if sources.leaves.len() != manifest.leaves.len() {
        return Err(Refusal::new(
            Stage::Manifest,
            Obligation::ClosedManifestForm,
            "leaves",
            "the files of every leaf, and only those, must be supplied",
        ));
    }
    let compiled = manifest
        .leaves
        .iter()
        .zip(sources.leaves)
        .map(|(entry, files)| leaf::compile(entry, &files.project, &files.rules))
        .collect::<Result<Vec<_>, _>>()?;
    let judged = structure::judge(&manifest, &compiled)?;
    let mut leaves = manifest
        .leaves
        .iter()
        .zip(&compiled)
        .map(|(entry, leaf)| leaf::certify(entry, leaf))
        .collect::<Result<Vec<_>, _>>()?;
    // A flat manifest checks each leaf's assumption at the root; a
    // hierarchy checks it at its parent group, with the groups' checks.
    let hierarchy = match &judged.tree {
        Some(tree) => {
            let checked = hierarchy::check(&manifest, &compiled, tree)?;
            for (certificate, (rows, table)) in leaves.iter_mut().zip(&checked.leaf_rows) {
                certificate.interface_rows = *rows;
                certificate.interface_table_sha256 = *table;
            }
            Some(checked)
        }
        None => {
            for (consumer, certificate) in leaves.iter_mut().enumerate() {
                let (rows, table) = interface(
                    &manifest.leaves[consumer],
                    &compiled,
                    consumer,
                    &judged.edges,
                )?;
                certificate.interface_rows = rows;
                certificate.interface_table_sha256 = table;
            }
            None
        }
    };
    Ok(Run {
        manifest,
        manifest_commitment,
        compiled,
        judged,
        leaves,
        hierarchy,
    })
}

/// Over every value of the consumer's interface fields and every command,
/// the providers' facts on those values imply the consumer's assumption.
/// Returns the number of rows and the SHA-256 of their table.
fn interface(
    entry: &manifest::Leaf,
    compiled: &[Compiled],
    consumer: usize,
    edges: &[Resolved],
) -> Result<(u64, [u8; 32]), Refusal> {
    let leaf = &compiled[consumer];
    let place = format!("leaf {} `{}` assumption", entry.id, entry.name);
    let refuse = |obligation: Obligation, reason: String| {
        Refusal::new(Stage::Interface, obligation, &place, reason)
    };
    let too_large = || {
        refuse(
            Obligation::CompleteEnumeration,
            "the interface table exceeds the row limit".into(),
        )
    };
    // Each interface field with its values and, for each value, whether the
    // provider facts hold on it; then each command field with its values.
    let mut columns: Vec<(leaf::FieldDomain, Vec<i128>, Option<Vec<bool>>)> = Vec::new();
    for field in leaf.fields(Source::Context) {
        let Some(edge) = edges
            .iter()
            .find(|edge| edge.consumer == consumer && edge.interface == field.field)
        else {
            return Err(refuse(
                Obligation::ProviderBinding,
                "an interface field has no edge".into(),
            ));
        };
        let values = field
            .domain
            .values_within(leaf::MAX_LOCAL_ROWS)
            .ok_or_else(too_large)?;
        let mut holds = Vec::with_capacity(values.len());
        for value in &values {
            let record = [c::Field {
                id: edge.view_field,
                value: leaf::atom(&field.domain, *value),
            }];
            let mut all = true;
            for fact in &edge.facts {
                let Some((_, predicate)) = compiled[edge.provider].guarantee(*fact) else {
                    return Err(refuse(
                        Obligation::ProviderClaim,
                        format!("fact {fact} is not declared"),
                    ));
                };
                match predicate.on_state(&record) {
                    Truth::Holds => {}
                    Truth::Fails => all = false,
                    other => {
                        return Err(refuse(
                            Obligation::DefinedPredicate,
                            format!("fact {fact} {} on view value {value}", other.describe()),
                        ));
                    }
                }
            }
            holds.push(all);
        }
        columns.push((field, values, Some(holds)));
    }
    for field in leaf.fields(Source::Command) {
        let values = field
            .domain
            .values_within(leaf::MAX_LOCAL_ROWS)
            .ok_or_else(too_large)?;
        columns.push((field, values, None));
    }
    let values: Vec<Vec<i128>> = columns
        .iter()
        .map(|(_, values, _)| values.clone())
        .collect();
    values
        .iter()
        .try_fold(1u64, |size, values| size.checked_mul(values.len() as u64))
        .filter(|size| *size <= leaf::MAX_LOCAL_ROWS)
        .ok_or_else(too_large)?;
    let mut rows = 0u64;
    let mut table = Table::new(TableKind::Interface);
    leaf::for_each_tuple(&values, |tuple| {
        rows += 1;
        let facts_hold = columns
            .iter()
            .zip(tuple)
            .all(|((_, values, holds), value)| {
                holds.as_ref().is_none_or(|holds| {
                    values
                        .iter()
                        .position(|candidate| candidate == value)
                        .is_some_and(|at| holds[at])
                })
            });
        let record = table.row(tuple);
        record.push(u8::from(facts_hold));
        if !facts_hold {
            return Ok(());
        }
        let mut input: Vec<c::Field<'static>> = columns
            .iter()
            .zip(tuple)
            .map(|((field, ..), value)| c::Field {
                id: field.field,
                value: leaf::atom(&field.domain, *value),
            })
            .collect();
        input.sort_by_key(|field| field.id);
        let witness = || {
            Value::Object(
                columns
                    .iter()
                    .zip(tuple)
                    .map(|((field, _, holds), value)| {
                        let root = if holds.is_some() {
                            "context.102"
                        } else {
                            "command.101"
                        };
                        (format!("{root}.{}", field.field), json_integer(*value))
                    })
                    .collect(),
            )
        };
        match leaf.assumption.on_input(&input) {
            Truth::Holds => {
                record.push(1);
                Ok(())
            }
            Truth::Fails => Err(refuse(
                Obligation::InterfaceImplication,
                "the providers' StateFacts hold and the assumption fails".into(),
            )
            .with_witness(witness())),
            other => Err(refuse(
                Obligation::DefinedPredicate,
                format!("the assumption {}", other.describe()),
            )
            .with_witness(witness())),
        }
    })?;
    Ok((rows, table.digest()))
}

/// The strongly connected groups of two or more leaves in the provider
/// graph, each in increasing ID order, ordered by their least ID.
fn cycles(manifest: &Manifest) -> Vec<Vec<u32>> {
    let count = manifest.leaves.len();
    let index: BTreeMap<u32, usize> = manifest
        .leaves
        .iter()
        .enumerate()
        .map(|(at, leaf)| (leaf.id, at))
        .collect();
    let mut forward = vec![Vec::new(); count];
    let mut backward = vec![Vec::new(); count];
    for edge in &manifest.edges {
        if let (Some(&from), Some(&to)) = (index.get(&edge.consumer), index.get(&edge.provider)) {
            forward[from].push(to);
            backward[to].push(from);
        }
    }
    // First pass: finishing order of a depth-first search over `forward`.
    let mut finished = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    for start in 0..count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((node, next)) = stack.pop() {
            if let Some(&child) = forward[node].get(next) {
                stack.push((node, next + 1));
                if !seen[child] {
                    seen[child] = true;
                    stack.push((child, 0));
                }
            } else {
                finished.push(node);
            }
        }
    }
    // Second pass: components of `backward` in reverse finishing order.
    let mut component = vec![usize::MAX; count];
    let mut groups: Vec<Vec<u32>> = Vec::new();
    for &start in finished.iter().rev() {
        if component[start] != usize::MAX {
            continue;
        }
        let label = groups.len();
        component[start] = label;
        let mut members = vec![manifest.leaves[start].id];
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for &parent in &backward[node] {
                if component[parent] == usize::MAX {
                    component[parent] = label;
                    members.push(manifest.leaves[parent].id);
                    stack.push(parent);
                }
            }
        }
        members.sort_unstable();
        groups.push(members);
    }
    groups.retain(|group| group.len() > 1);
    groups.sort();
    groups
}

/// Lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
