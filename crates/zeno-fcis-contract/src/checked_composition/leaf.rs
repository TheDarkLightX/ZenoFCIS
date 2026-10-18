//! One leaf: its files checked against the manifest's pins, compiled through
//! the ordinary contract route, and checked by its own bound Authority on
//! every input of its complete local domain.
//!
//! The local domain is every state × command × context tuple the leaf's own
//! declarations admit; the checker derives it, nobody supplies it. Each
//! tuple is framed as the envelopes the Authority admits and published by
//! the Authority, and every decision is replayed. The checker reads only
//! the Authority's actual outcomes: class, reason, successor, patch,
//! deliveries, reads and usage. Each local input and each state of the
//! StateFact check is recorded in a table whose SHA-256 the result keeps.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};
use zeno_fcis_codec::CommitmentHasher;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_synthesis::finite::{
    V2Resource,
    v2_authority::{Authority, PublicationOutcome},
    v2_composition::{self as c, ReadAttempt, Selector},
    v2_laws as l,
};

use super::hex;
use super::manifest::{self, GuaranteeKind, ReadPath, Witness};
use super::predicate::{Form, Predicate, RootFields, Truth};
use super::refusal::{AuthorityRefusal, Obligation, Refusal, Stage};
use super::table::{Table, TableKind};
use crate::ContractError;
use crate::internals::declarations::{Declarations, Form as TypeForm, ROOTS, Source};
use crate::internals::graph::Atom;
use crate::internals::model::Contract;
use crate::internals::review::domain::{self, LeafDomain, Position};
use crate::internals::review::evaluate::Framer;
use crate::internals::rules::{Class, Rules};
use crate::internals::{policy, schema, schema_commitment};

/// The most local inputs one leaf may have; a larger domain is refused, not
/// sampled.
pub(crate) const MAX_LOCAL_ROWS: u64 = 1 << 20;

/// A leaf compiled from its pinned files.
#[derive(Debug)]
pub(crate) struct Compiled {
    pub(crate) declarations: Declarations,
    pub(crate) rules: Rules,
    pub(crate) schema: Vec<u8>,
    pub(crate) commitment: [u8; 32],
    pub(crate) positions: Vec<Position>,
    pub(crate) invariant: Predicate,
    pub(crate) assumption: Predicate,
    /// Every guarantee in ID order, with its kind.
    pub(crate) guarantees: Vec<(u32, GuaranteeKind, Predicate)>,
}

/// The domain of one root field.
#[derive(Clone, Debug)]
pub(crate) struct FieldDomain {
    pub(crate) field: u16,
    pub(crate) domain: LeafDomain,
}

/// What the checker established for one leaf: its bindings, its counts and
/// the SHA-256 of each table it ran. Fields are private; only the checker
/// builds one.
///
/// ```compile_fail,E0451
/// use zeno_fcis_contract::checked_composition::LeafCertificate;
/// let forged = LeafCertificate { id: 1 };
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeafCertificate {
    pub(crate) id: u32,
    name: String,
    project_sha256: [u8; 32],
    policy_sha256: [u8; 32],
    program_sha256: [u8; 32],
    schema_sha256: [u8; 32],
    policy_bytes_sha256: [u8; 32],
    genesis_subject_sha256: [u8; 32],
    pub(crate) rows: u64,
    pub(crate) admitted: u64,
    admitted_accept: u64,
    admitted_reject: u64,
    excluded_refused: u64,
    pub(crate) replays: u64,
    relation_checks: u64,
    pub(crate) state_fact_rows: u64,
    max_steps: u64,
    max_reads: u64,
    reads: BTreeSet<ReadPath>,
    changed: BTreeSet<u16>,
    /// StateFacts shown to follow from the invariant on every state.
    state_facts: Vec<u32>,
    local_table_sha256: [u8; 32],
    state_fact_table_sha256: [u8; 32],
    /// Rows of this leaf's interface implication, as the consumer, and their
    /// table's SHA-256.
    pub(crate) interface_rows: u64,
    pub(crate) interface_table_sha256: [u8; 32],
}

impl LeafCertificate {
    /// The leaf's manifest ID.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// The SHA-256 of the bound program's identity.
    #[must_use]
    pub const fn program_sha256(&self) -> [u8; 32] {
        self.program_sha256
    }

    /// The SHA-256 of the table of every local input with the Authority's
    /// outcome.
    #[must_use]
    pub const fn local_table_sha256(&self) -> [u8; 32] {
        self.local_table_sha256
    }

    /// The local inputs the leaf's Authority decided.
    #[must_use]
    pub const fn rows(&self) -> u64 {
        self.rows
    }
}

impl Compiled {
    /// The domains of one root's fields, in field order.
    pub(crate) fn fields(&self, source: Source) -> Vec<FieldDomain> {
        self.positions
            .iter()
            .filter(|position| position.source == source)
            .filter_map(|position| {
                Some(FieldDomain {
                    field: position.field?,
                    domain: position.domain.clone(),
                })
            })
            .collect()
    }

    /// The compiled guarantee with this ID.
    pub(crate) fn guarantee(&self, id: u32) -> Option<(GuaranteeKind, &Predicate)> {
        self.guarantees
            .iter()
            .find(|(guarantee, ..)| *guarantee == id)
            .map(|(_, kind, predicate)| (*kind, predicate))
    }
}

/// A component's files checked against their pins and compiled through the
/// ordinary contract route, in the profile's boundaries: record roots, no
/// channel and no delivery. `/1` leaves and `/3` components are both bound
/// here.
#[derive(Debug)]
pub(crate) struct Bound {
    pub(crate) declarations: Declarations,
    pub(crate) rules: Rules,
    pub(crate) schema: Vec<u8>,
    pub(crate) commitment: [u8; 32],
    pub(crate) positions: Vec<Position>,
}

/// Checks a component's two files against their pins and compiles them as
/// `contract review` and `generate contract` do; `label` names the
/// component in every refusal. `record_roots` also requires record command
/// and context roots, as `/1` does; the state root is always a record.
///
/// # Errors
/// A pin the files differ from, the contract route's refusal, or a profile
/// boundary.
pub(crate) fn bind(
    label: &str,
    record_roots: bool,
    project_sha256: &[u8; 32],
    policy_sha256: &[u8; 32],
    project: &str,
    rules: &str,
) -> Result<Bound, Refusal> {
    let place = |file: &str| format!("{label} {file}");
    for (file, text, pinned) in [
        ("project.zeno", project, project_sha256),
        ("v2/policy.json", rules, policy_sha256),
    ] {
        let actual = sha256(text.as_bytes());
        if actual != *pinned {
            return Err(Refusal::new(
                Stage::Binding,
                Obligation::SourceBinding,
                place(file),
                format!(
                    "SHA-256 is {}, the manifest pins {}",
                    hex(&actual),
                    hex(pinned)
                ),
            ));
        }
    }
    let route = |error: ContractError| {
        Refusal::new(
            Stage::Binding,
            Obligation::OrdinaryContractRoute,
            place(error.place()),
            error.reason(),
        )
    };
    let rules = Rules::read(rules).map_err(route)?;
    if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
        return Err(Refusal::new(
            Stage::Binding,
            Obligation::CheckedProfile,
            place("v2/policy.json"),
            "lists adoptions or evolutions; this profile checks the rules-compiled program only",
        ));
    }
    let declarations = Declarations::read(project, &rules.leaf_bindings).map_err(route)?;
    if crate::internals::quantity::typed(&declarations) {
        return Err(Refusal::new(
            Stage::Binding,
            Obligation::CheckedProfile,
            place("project.zeno"),
            "quantity interfaces require a quantity-aware composition profile",
        ));
    }
    let schema = schema::encode(&declarations).map_err(route)?;
    let commitment = schema_commitment(&schema).map_err(route)?;
    let contract = Contract::build(&declarations, &rules, commitment).map_err(route)?;
    let profile = |reason: &str| {
        Refusal::new(
            Stage::Binding,
            Obligation::CheckedProfile,
            place("project.zeno"),
            reason.to_owned(),
        )
    };
    for (source, root) in ROOTS {
        if (record_roots || source == Source::State)
            && !matches!(
                declarations.get(root).map_err(route)?.form,
                TypeForm::Record(_)
            )
        {
            return Err(profile(
                "the profile needs record state, command and context roots",
            ));
        }
    }
    if !contract.channels.is_empty() || contract.branches.iter().any(|b| !b.outbox.is_empty()) {
        return Err(profile(
            "declares a channel or a delivery; the profile has no effects or outbox",
        ));
    }
    let positions = domain::positions(&declarations).map_err(route)?;
    Ok(Bound {
        declarations,
        rules,
        schema,
        commitment,
        positions,
    })
}

/// Checks a leaf's files against its pins and compiles it as `contract
/// review` and `generate contract` do.
///
/// # Errors
/// A pin the files differ from, the contract route's refusal, a profile
/// boundary, or a formula that does not compile.
pub(crate) fn compile(
    entry: &manifest::Leaf,
    project: &str,
    rules: &str,
) -> Result<Compiled, Refusal> {
    let Bound {
        declarations,
        rules,
        schema,
        commitment,
        positions,
    } = bind(
        &format!("leaf {} `{}`", entry.id, entry.name),
        true,
        &entry.project_sha256,
        &entry.policy_sha256,
        project,
        rules,
    )?;
    let profile = |reason: &str| {
        Refusal::new(
            Stage::Binding,
            Obligation::CheckedProfile,
            format!("leaf {} `{}` project.zeno", entry.id, entry.name),
            reason.to_owned(),
        )
    };
    let ids = |source: Source| -> Vec<u16> {
        positions
            .iter()
            .filter(|position| position.source == source)
            .filter_map(|position| position.field)
            .collect()
    };
    let (state, command, context) = (
        ids(Source::State),
        ids(Source::Command),
        ids(Source::Context),
    );
    if command.iter().any(|field| context.contains(field)) {
        return Err(profile(
            "a command and a context field share an ID, so an assumption could not tell them apart",
        ));
    }
    let roots = RootFields {
        state: &state,
        command: &command,
        context: &context,
    };
    let formula = |text: &str, form, key: &str| {
        Predicate::compile(&declarations, &rules.variables, text, form, &roots).map_err(|reason| {
            Refusal::new(
                Stage::Binding,
                Obligation::DefinedPredicate,
                format!("leaf {} `{}` {key}", entry.id, entry.name),
                reason,
            )
        })
    };
    let invariant = formula(&entry.invariant, Form::State, "invariant")?;
    let assumption = formula(&entry.assumption, Form::Input, "assumption")?;
    let guarantees = entry
        .guarantees
        .iter()
        .map(|guarantee| {
            let form = match guarantee.scope {
                None => Form::State,
                Some(scope) => Form::Transition(scope),
            };
            let key = format!("guarantee {}", guarantee.id);
            Ok((
                guarantee.id,
                guarantee.kind,
                formula(&guarantee.formula, form, &key)?,
            ))
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let ownership = |reason: String| {
        Refusal::new(
            Stage::Ownership,
            Obligation::CompleteFootprint,
            format!("leaf {} `{}`", entry.id, entry.name),
            reason,
        )
    };
    let owned: Vec<u16> = entry.owns.iter().map(|owned| owned.field).collect();
    if owned != state {
        return Err(ownership(format!(
            "owns state fields [{}]; its state declares [{}], and a leaf owns its whole state",
            list(&owned),
            list(&state)
        )));
    }
    if let Some(field) = entry.writes.iter().find(|field| !owned.contains(field)) {
        return Err(ownership(format!(
            "declares a write to field {field}, which it does not own"
        )));
    }
    let inputs = [&state, &command, &context];
    if let Some(read) = entry
        .reads
        .iter()
        .find(|read| !inputs[usize::from(read.source)].contains(&read.field))
    {
        return Err(ownership(format!(
            "declares a read of source {} field {}, which its roots do not declare",
            read.source, read.field
        )));
    }
    if let Some(view) = entry.views.iter().find(|view| !state.contains(&view.field)) {
        return Err(ownership(format!(
            "view {} names field {}, which is not one of its state fields",
            view.id, view.field
        )));
    }
    Ok(Compiled {
        declarations,
        rules,
        schema,
        commitment,
        positions,
        invariant,
        assumption,
        guarantees,
    })
}

/// Checks one leaf with its own bound Authority.
///
/// # Errors
/// The first failed obligation, with the input that shows it.
pub(crate) fn certify(
    entry: &manifest::Leaf,
    compiled: &Compiled,
) -> Result<LeafCertificate, Refusal> {
    let contract = Contract::build(&compiled.declarations, &compiled.rules, compiled.commitment)
        .map_err(|error| {
            Refusal::new(
                Stage::Binding,
                Obligation::OrdinaryContractRoute,
                format!("leaf {} `{}` {}", entry.id, entry.name, error.place()),
                error.reason(),
            )
        })?;
    certify_contract(entry, compiled, &contract)
}

/// [`certify`] on an already built contract; a planted-defect control may
/// pass one whose bound program differs from its files.
pub(crate) fn certify_contract(
    entry: &manifest::Leaf,
    compiled: &Compiled,
    contract: &Contract<'_>,
) -> Result<LeafCertificate, Refusal> {
    let binding = |error: ContractError| {
        Refusal::new(
            Stage::Binding,
            Obligation::OrdinaryContractRoute,
            format!("leaf {} `{}` {}", entry.id, entry.name, error.place()),
            error.reason(),
        )
    };
    let policy_bytes = policy::encode(contract, &compiled.schema).map_err(binding)?;
    policy::with_authority(contract, &compiled.schema, |authority| {
        Check {
            entry,
            compiled,
            contract,
            framer: Framer::new(&compiled.positions, contract),
            authority,
        }
        .run(sha256(&policy_bytes))
    })
    .map_err(binding)?
}

struct Check<'a, 'p> {
    entry: &'a manifest::Leaf,
    compiled: &'a Compiled,
    contract: &'a Contract<'a>,
    framer: Framer,
    authority: &'a Authority<'p>,
}

/// Counts, observed footprint and the table of the local rows.
struct Tally {
    rows: u64,
    admitted: u64,
    admitted_accept: u64,
    admitted_reject: u64,
    excluded_refused: u64,
    replays: u64,
    relation_checks: u64,
    max_steps: u64,
    max_reads: u64,
    reads: BTreeSet<ReadPath>,
    changed: BTreeSet<u16>,
    table: Table,
}

/// A local row's outcome tag in its table record.
const ACCEPT: u8 = 0;
const REJECT: u8 = 1;
const COMMITTED_FAILURE: u8 = 2;
const REFUSED: u8 = 3;

impl Check<'_, '_> {
    fn run(&self, policy_bytes_sha256: [u8; 32]) -> Result<LeafCertificate, Refusal> {
        let entry = self.entry;
        let program = sha256(self.authority.identity());
        if program != entry.program_sha256 {
            return Err(self.refusal(
                Stage::Binding,
                Obligation::ProgramBinding,
                format!(
                    "the bound program identity has SHA-256 {}, the manifest pins {}",
                    hex(&program),
                    hex(&entry.program_sha256)
                ),
                None,
            ));
        }
        let values = self.local_values()?;
        let genesis_subject = self.genesis()?;
        let mut tally = Tally {
            rows: 0,
            admitted: 0,
            admitted_accept: 0,
            admitted_reject: 0,
            excluded_refused: 0,
            replays: 0,
            relation_checks: 0,
            max_steps: 0,
            max_reads: 0,
            reads: BTreeSet::new(),
            changed: BTreeSet::new(),
            table: Table::new(TableKind::Local),
        };
        for_each_tuple(&values, |tuple| self.row(tuple, &mut tally))?;
        if let Some(read) = tally.reads.iter().find(|read| !entry.reads.contains(read)) {
            return Err(self.refusal(
                Stage::Ownership,
                Obligation::CompleteFootprint,
                format!(
                    "the Authority reads source {} field {}, outside the declared reads",
                    read.source, read.field
                ),
                None,
            ));
        }
        for witness in &entry.witnesses {
            let seen = match witness {
                Witness::Accept => tally.admitted_accept,
                Witness::Reject => tally.admitted_reject,
            };
            if seen == 0 {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::RequiredWitness,
                    format!("no admitted input decides {}", witness.name()),
                    None,
                ));
            }
        }
        let (state_fact_rows, state_facts, state_fact_table) = self.state_facts()?;
        Ok(LeafCertificate {
            id: entry.id,
            name: entry.name.clone(),
            project_sha256: entry.project_sha256,
            policy_sha256: entry.policy_sha256,
            program_sha256: program,
            schema_sha256: sha256(&self.compiled.schema),
            policy_bytes_sha256,
            genesis_subject_sha256: genesis_subject,
            rows: tally.rows,
            admitted: tally.admitted,
            admitted_accept: tally.admitted_accept,
            admitted_reject: tally.admitted_reject,
            excluded_refused: tally.excluded_refused,
            replays: tally.replays,
            relation_checks: tally.relation_checks,
            state_fact_rows,
            max_steps: tally.max_steps,
            max_reads: tally.max_reads,
            reads: tally.reads,
            changed: tally.changed,
            state_facts,
            local_table_sha256: tally.table.digest(),
            state_fact_table_sha256: state_fact_table,
            interface_rows: 0,
            interface_table_sha256: [0; 32],
        })
    }

    /// Every value of every input position, after the complete domain is
    /// shown to fit the row limit.
    fn local_values(&self) -> Result<Vec<Vec<i128>>, Refusal> {
        let positions = &self.compiled.positions;
        let fits =
            domain::domain_size(positions).is_some_and(|size| size <= u128::from(MAX_LOCAL_ROWS));
        let values: Option<Vec<Vec<i128>>> = positions
            .iter()
            .map(|position| position.domain.values_within(MAX_LOCAL_ROWS))
            .collect();
        match values {
            Some(values) if fits => Ok(values),
            _ => Err(self.refusal(
                Stage::Local,
                Obligation::CompleteEnumeration,
                format!(
                    "the local domain exceeds {MAX_LOCAL_ROWS} inputs; it is refused, not sampled"
                ),
                None,
            )),
        }
    }

    /// Init ⇒ I, then the Authority's genuine genesis and its exact replay.
    fn genesis(&self) -> Result<[u8; 32], Refusal> {
        let mut tuple: Vec<i128> = Vec::new();
        for position in &self.compiled.positions {
            let declared = match (position.source, position.field) {
                (Source::State, Some(field)) => self
                    .contract
                    .genesis
                    .iter()
                    .find(|(id, _)| *id == field)
                    .and_then(|(_, value)| number(value)),
                _ => position.domain.least(),
            };
            let Some(value) = declared else {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::InitImpliesInvariant,
                    format!("{} has no declared genesis value", position.name),
                    None,
                ));
            };
            tuple.push(value);
        }
        let state = self.framer.fields(&self.framer.roots[0], &tuple);
        let witness = Some(self.witness(&tuple, Some("genesis")));
        if state.is_empty() {
            return Err(self.refusal(
                Stage::Local,
                Obligation::InitImpliesInvariant,
                "genesis is empty".into(),
                witness,
            ));
        }
        if !self.defined(
            self.compiled.invariant.on_state(&state),
            "invariant",
            &witness,
        )? {
            return Err(self.refusal(
                Stage::Local,
                Obligation::InitImpliesInvariant,
                "the genesis state does not satisfy the invariant".into(),
                witness,
            ));
        }
        let frames = self.framer.frame(&tuple);
        let published = match self.authority.publish_genesis(&frames[0]) {
            PublicationOutcome::Commit(published) => published,
            PublicationOutcome::Refused { evaluation, error } => {
                let authority = AuthorityRefusal::of(&error, evaluation.diagnostics());
                return Err(self
                    .refusal(
                        Stage::Local,
                        Obligation::GenuineGenesis,
                        "the Authority refuses the declared genesis".into(),
                        witness,
                    )
                    .with_authority(authority));
            }
            _ => {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::GenuineGenesis,
                    "the Authority does not commit the declared genesis".into(),
                    witness,
                ));
            }
        };
        match self
            .authority
            .replay_genesis_publication(&frames[0], published.subject())
        {
            PublicationOutcome::Commit(replayed) if replayed.subject() == published.subject() => {
                Ok(sha256(published.subject()))
            }
            _ => Err(self.refusal(
                Stage::Local,
                Obligation::ExactReplay,
                "genesis does not replay to the same subject".into(),
                witness,
            )),
        }
    }

    /// One local input: the Authority's actual outcome against the leaf's
    /// obligations, recorded in the local table.
    fn row(&self, tuple: &[i128], tally: &mut Tally) -> Result<(), Refusal> {
        tally.rows += 1;
        let roots = &self.framer.roots;
        let pre = self.framer.fields(&roots[0], tuple);
        let command = self.framer.fields(&roots[1], tuple);
        let context = self.framer.fields(&roots[2], tuple);
        let mut input: Vec<c::Field<'static>> = command.iter().chain(&context).copied().collect();
        input.sort_by_key(|field| field.id);
        let witness = Some(self.witness(tuple, None));
        let pre_holds = self.defined(
            self.compiled.invariant.on_state(&pre),
            "invariant",
            &witness,
        )?;
        let assumed = self.defined(
            self.compiled.assumption.on_input(&input),
            "assumption",
            &witness,
        )?;
        let admitted = pre_holds && assumed;
        let frames = self.framer.frame(tuple);
        let raw = c::Raw {
            state: &frames[0],
            command: &frames[1],
            context: &frames[2],
        };
        let outcome = self.authority.publish(raw);
        let evaluation = outcome.evaluation();
        record_reads(evaluation.reads(), tally);
        let usage = evaluation.usage();
        if admitted {
            tally.admitted += 1;
            tally.max_steps = tally.max_steps.max(usage.used(V2Resource::Step));
            tally.max_reads = tally.max_reads.max(usage.used(V2Resource::Read));
        }
        let record = |tally: &mut Tally, outcome: u8| {
            let row = tally.table.row(tuple);
            row.push(u8::from(pre_holds));
            row.push(u8::from(assumed));
            row.push(outcome);
        };
        let (candidate, subject) = match &outcome {
            PublicationOutcome::Commit(publication) => {
                if !publication.effects().is_empty() || !publication.outbox().is_empty() {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::NoEffectsOrOutbox,
                        "a decision delivers".into(),
                        witness,
                    ));
                }
                (evaluation.result(), publication.subject())
            }
            PublicationOutcome::Reject(rejected) => {
                (rejected.result(), evaluation.subject().unwrap_or_default())
            }
            PublicationOutcome::Refused { error, .. } => {
                let authority = AuthorityRefusal::of(error, evaluation.diagnostics());
                if admitted {
                    return Err(self
                        .refusal(
                            Stage::Local,
                            Obligation::NoTechnicalRefusal,
                            "the Authority refuses an input that satisfies I and A".into(),
                            witness,
                        )
                        .with_authority(authority));
                }
                record(tally, REFUSED);
                authority.encode(tally.table.bytes());
                tally.excluded_refused += 1;
                return Ok(());
            }
            _ => {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::KnownOutcome,
                    "the Authority returned an outcome this checker does not know".into(),
                    witness,
                ));
            }
        };
        let Ok(candidate) = candidate else {
            return Err(self.refusal(
                Stage::Local,
                Obligation::KnownOutcome,
                "a decision without a candidate".into(),
                witness,
            ));
        };
        let tag = match candidate.class() {
            c::Class::Accept => ACCEPT,
            c::Class::Reject => REJECT,
            c::Class::CommittedFailure => COMMITTED_FAILURE,
            _ => {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::KnownOutcome,
                    "a decision class this checker does not know".into(),
                    witness,
                ));
            }
        };
        record(tally, tag);
        tally.table.bytes().extend_from_slice(&sha256(subject));
        for patch in candidate.patch() {
            tally.changed.insert(patch.field);
            if !self.entry.writes.contains(&patch.field) {
                return Err(self.refusal(
                    Stage::Ownership,
                    Obligation::CompleteFootprint,
                    format!(
                        "the decision changes field {}, outside the declared writes [{}]",
                        patch.field,
                        list(&self.entry.writes)
                    ),
                    witness,
                ));
            }
        }
        match self.authority.replay_publication(raw, subject) {
            PublicationOutcome::Commit(replayed) if replayed.subject() == subject => {}
            PublicationOutcome::Reject(replayed)
                if replayed.subject().is_ok_and(|bytes| bytes == subject) => {}
            _ => {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::ExactReplay,
                    "the decision does not replay to the same subject".into(),
                    witness,
                ));
            }
        }
        tally.replays += 1;
        if !admitted {
            return Ok(());
        }
        match tag {
            ACCEPT => {
                tally.admitted_accept += 1;
                let post = candidate.post();
                let framed = post.len() == pre.len()
                    && post.iter().zip(&pre).all(|(after, before)| {
                        after.id == before.id
                            && (self.entry.writes.contains(&after.id)
                                || after.value == before.value)
                    });
                if !framed {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::Frame,
                        "the successor changes a field outside the declared writes".into(),
                        witness,
                    ));
                }
                if !self.defined(
                    self.compiled.invariant.on_state(post),
                    "invariant",
                    &witness,
                )? {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::Preservation,
                        "an admitted Accept leaves the invariant".into(),
                        witness,
                    ));
                }
            }
            REJECT => {
                tally.admitted_reject += 1;
                let business = candidate.reason().is_some_and(|reason| {
                    self.contract
                        .reasons
                        .iter()
                        .any(|(id, class)| *id == reason && *class == Class::Reject)
                });
                if !business || !candidate.post().is_empty() || !candidate.patch().is_empty() {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::UnchangedBusinessReject,
                        match candidate.reason() {
                            Some(reason) => format!(
                                "a Reject with reason {reason} is not an unchanged business rejection"
                            ),
                            None => "a Reject without a reason is not an unchanged business rejection"
                                .to_owned(),
                        },
                        witness,
                    ));
                }
            }
            _ => {
                return Err(self.refusal(
                    Stage::Local,
                    Obligation::CommittedFailureUnreachable,
                    "an admitted input commits a failure".into(),
                    witness,
                ));
            }
        }
        let observed = l::Candidate {
            class: candidate.class(),
            reason: candidate.reason(),
            post: l::RootView::Record(candidate.post()),
            patch: candidate.patch(),
            effects: candidate.effects(),
            outbox: candidate.outbox(),
            reads: evaluation.reads(),
            attempts: evaluation.decision_attempts(),
            usage: evaluation.decision_usage().unwrap_or(usage),
        };
        let frame = l::Frame::Transition {
            pre: l::RootView::Record(candidate.pre()),
            command: l::RootView::Record(&command),
            context: l::RootView::Record(&context),
            candidate: &observed,
        };
        for (id, kind, relation) in &self.compiled.guarantees {
            if *kind != GuaranteeKind::TransitionRelation {
                continue;
            }
            tally.relation_checks += 1;
            match relation.on_transition(&frame) {
                Truth::Holds | Truth::OutOfScope => {}
                Truth::Fails => {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::TransitionRelation,
                        format!("transition relation {id} fails on an admitted decision"),
                        witness,
                    ));
                }
                undefined @ Truth::Undefined(_) => {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::DefinedPredicate,
                        format!("transition relation {id} {}", undefined.describe()),
                        witness,
                    ));
                }
            }
        }
        Ok(())
    }

    /// I ⇒ G for every StateFact, on every state of the complete state
    /// domain, each state recorded in the StateFact table.
    fn state_facts(&self) -> Result<(u64, Vec<u32>, [u8; 32]), Refusal> {
        let state = self.compiled.fields(Source::State);
        let Some(values) = state
            .iter()
            .map(|field| field.domain.values_within(MAX_LOCAL_ROWS))
            .collect::<Option<Vec<Vec<i128>>>>()
        else {
            return Err(self.refusal(
                Stage::Local,
                Obligation::CompleteEnumeration,
                "the state domain exceeds the row limit".into(),
                None,
            ));
        };
        let facts: Vec<(u32, &Predicate)> = self
            .compiled
            .guarantees
            .iter()
            .filter(|(_, kind, _)| *kind == GuaranteeKind::StateFact)
            .map(|(id, _, predicate)| (*id, predicate))
            .collect();
        let mut rows = 0u64;
        let mut table = Table::new(TableKind::StateFact);
        for_each_tuple(&values, |tuple| {
            rows += 1;
            let record: Vec<c::Field<'static>> = state
                .iter()
                .zip(tuple)
                .map(|(field, value)| c::Field {
                    id: field.field,
                    value: atom(&field.domain, *value),
                })
                .collect();
            let witness = Some(Value::Object(
                state
                    .iter()
                    .zip(tuple)
                    .map(|(field, value)| {
                        (
                            format!("post.100.{}", field.field),
                            super::json_integer(*value),
                        )
                    })
                    .collect::<Map<_, _>>(),
            ));
            let holds = self.defined(
                self.compiled.invariant.on_state(&record),
                "invariant",
                &witness,
            )?;
            table.row(tuple).push(u8::from(holds));
            if !holds {
                return Ok(());
            }
            for (id, fact) in &facts {
                if !self.defined(fact.on_state(&record), &format!("StateFact {id}"), &witness)? {
                    return Err(self.refusal(
                        Stage::Local,
                        Obligation::StateFact,
                        format!("StateFact {id} does not follow from the invariant"),
                        witness,
                    ));
                }
            }
            Ok(())
        })?;
        Ok((
            rows,
            facts.iter().map(|(id, _)| *id).collect(),
            table.digest(),
        ))
    }

    fn defined(&self, truth: Truth, what: &str, witness: &Option<Value>) -> Result<bool, Refusal> {
        match truth {
            Truth::Holds => Ok(true),
            Truth::Fails => Ok(false),
            Truth::OutOfScope | Truth::Undefined(_) => Err(self.refusal(
                Stage::Local,
                Obligation::DefinedPredicate,
                format!("the {what} {}", truth.describe()),
                witness.clone(),
            )),
        }
    }

    fn refusal(
        &self,
        stage: Stage,
        obligation: Obligation,
        reason: String,
        witness: Option<Value>,
    ) -> Refusal {
        let refusal = Refusal::new(
            stage,
            obligation,
            format!("leaf {} `{}`", self.entry.id, self.entry.name),
            reason,
        );
        match witness {
            Some(witness) => refusal.with_witness(witness),
            None => refusal,
        }
    }

    fn witness(&self, tuple: &[i128], note: Option<&str>) -> Value {
        let mut row: Map<String, Value> = self
            .compiled
            .positions
            .iter()
            .zip(tuple)
            .map(|(position, value)| (position.name.clone(), super::json_integer(*value)))
            .collect();
        if let Some(note) = note {
            row.insert("input".to_owned(), json!(note));
        }
        Value::Object(row)
    }
}

impl LeafCertificate {
    pub(crate) fn json(&self) -> Value {
        let paths = |reads: &BTreeSet<ReadPath>| {
            reads
                .iter()
                .map(|read| {
                    let root =
                        ["pre.100", "command.101", "context.102"][usize::from(read.source.min(2))];
                    format!("{root}.{}", read.field)
                })
                .collect::<Vec<_>>()
        };
        json!({
            "id": self.id,
            "name": self.name,
            "bindings": {
                "project_sha256": hex(&self.project_sha256),
                "policy_sha256": hex(&self.policy_sha256),
                "program_sha256": hex(&self.program_sha256),
                "schema_sha256": hex(&self.schema_sha256),
                "policy_bytes_sha256": hex(&self.policy_bytes_sha256),
            },
            "genesis_subject_sha256": hex(&self.genesis_subject_sha256),
            "local": {
                "rows": self.rows,
                "admitted": self.admitted,
                "admitted_accept": self.admitted_accept,
                "admitted_reject": self.admitted_reject,
                "excluded": self.rows - self.admitted,
                "excluded_refused": self.excluded_refused,
                "replayed_decisions": self.replays,
                "transition_relation_checks": self.relation_checks,
                "state_fact_rows": self.state_fact_rows,
                "max_admitted_usage": {"step": self.max_steps, "read": self.max_reads},
            },
            "observed_footprint": {
                "reads": paths(&self.reads),
                "writes": self.changed.iter().map(|field| format!("post.100.{field}")).collect::<Vec<_>>(),
            },
            "state_facts": self.state_facts,
            "interface_rows": self.interface_rows,
            "tables": {
                "local_sha256": hex(&self.local_table_sha256),
                "state_fact_sha256": hex(&self.state_fact_table_sha256),
                "interface_sha256": hex(&self.interface_table_sha256),
            },
        })
    }
}

fn record_reads(reads: &[ReadAttempt], tally: &mut Tally) {
    for read in reads {
        if let Selector::Field(field) = read.selector {
            tally.reads.insert(ReadPath {
                source: read.source,
                field,
            });
        }
    }
}

/// Calls `visit` on every tuple of the product of `values`, last position
/// fastest, and stops at its first error.
pub(crate) fn for_each_tuple<E>(
    values: &[Vec<i128>],
    mut visit: impl FnMut(&[i128]) -> Result<(), E>,
) -> Result<(), E> {
    if values.iter().any(Vec::is_empty) {
        return Ok(());
    }
    let mut index = vec![0usize; values.len()];
    let mut tuple: Vec<i128> = values.iter().map(|domain| domain[0]).collect();
    loop {
        visit(&tuple)?;
        let mut position = values.len();
        loop {
            if position == 0 {
                return Ok(());
            }
            position -= 1;
            index[position] += 1;
            if index[position] < values[position].len() {
                tuple[position] = values[position][index[position]];
                break;
            }
            index[position] = 0;
            tuple[position] = values[position][0];
        }
    }
}

/// The atom a domain's value number denotes, as the review frames it: a
/// boolean, an integer, or a sum variant.
pub(crate) fn atom(domain: &LeafDomain, value: i128) -> c::Atom<'static> {
    match domain {
        LeafDomain::Bool => c::Atom::Bool(value != 0),
        LeafDomain::Int { .. } => c::Atom::I128(i128::from(value)),
        LeafDomain::Sum { type_id, .. } => c::Atom::Sum {
            type_id: *type_id,
            variant: u16::try_from(value).unwrap_or(u16::MAX),
        },
    }
}

pub(crate) fn number(atom: &Atom) -> Option<i128> {
    match atom {
        Atom::Bool(value) => Some(i128::from(*value)),
        Atom::I128(value) => i128::try_from(*value).ok(),
        Atom::U128(value) => i128::try_from(*value).ok(),
        Atom::Sum { variant, .. } => Some(i128::from(*variant)),
        Atom::Text(_) => None,
    }
}

/// Field IDs as a reader writes them: `110, 111`.
fn list(fields: &[u16]) -> String {
    fields
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    *RustCryptoSha256::hash(bytes).as_bytes()
}

#[cfg(test)]
#[test]
fn quantity_interfaces_refuse_before_legacy_composition_erasure() {
    let source = include_str!("../../../../examples/arithmetic-fused/project.zeno")
        .replace(
            "Scalar in 0..=9223372036854775807;",
            "Scalar in 0..=9223372036854775807 unit USD scale e0;",
        )
        .replace(
            "Denominator in 1..=9223372036854775807;",
            "Denominator in 1..=9223372036854775807 unit 1 scale e0;",
        );
    let rules = include_str!("../../../../examples/arithmetic-fused/v2/policy.json").replace(
        "template-declarative-policy/2",
        "template-declarative-policy/3",
    );
    let result = bind(
        "quantity-test",
        false,
        &sha256(source.as_bytes()),
        &sha256(rules.as_bytes()),
        &source,
        &rules,
    );
    let Err(error) = result else {
        panic!("quantity interfaces must refuse before composition erasure");
    };
    assert_eq!(error.stage(), Stage::Binding);
    assert_eq!(error.obligation(), Obligation::CheckedProfile);
    assert_eq!(
        error.reason(),
        "quantity interfaces require a quantity-aware composition profile"
    );
}
