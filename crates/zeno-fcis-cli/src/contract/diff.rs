//! `zeno-fcis contract diff`: the kind of change from one contract to
//! another and a plain-language account of every changed item, as a pure
//! function of both contracts' files.
//!
//! Each side is the current version that `generate contract` generates from
//! a contract directory, so every receipt of its lineage has replayed.
//! Exactly one kind is decided, by comparing the canonical policy and schema
//! bytes and the structure the generator built them from: the first kind in
//! [`Kind::PRECEDENCE`] whose condition holds.
//!
//! - **Identical:** the canonical policies are byte-identical.
//! - **Program successor:** the new policy, with its decision program's
//!   instructions and roots and its Step limit replaced by the old one's, is
//!   byte for byte the old policy. This is premise 1 of the F6.1 upgrade,
//!   computed as the SQLite shell computes it.
//! - **Rename:** both schemas declare the same types, fields and variants,
//!   with the same IDs and forms; some name differs; and the new policy, with
//!   the old schema in place of its own, is byte for byte the old policy.
//! - **Layout change:** the state layout differs, while every other type and
//!   every channel is unchanged.
//! - **Rule change:** the schemas are byte-identical and so are the channels.
//! - **Unrelated:** anything else.
//!
//! The classifier decides only the structural kind; it runs no decision.
//! Whether a rule change preserves decisions is for its admission path to
//! check.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};
use zeno_fcis_spec::{ClaimDecl, LawScope};
use zeno_fcis_synthesis::finite::{
    V2Resource, V2ScalarProgram, v2_authority as authority,
    v2_catalog::BoundCatalog,
    v2_composition::{Descriptor, FrameBinding, Framing},
};

use super::declarations::{Declarations, Form, Leaf, ROOTS};
use super::expr::{self, Ast};
use super::model::{Law, ScalarDomain};
use super::rules::{Case, Class, Constant, LawKind};
use super::{
    ContractError, ContractSources, Current, GeneratedContract, generate_contract, policy,
    with_current,
};
use crate::transform::sha256_hex;

/// The versioned schema of the document `contract diff --format json` prints.
pub(crate) const DIFF_SCHEMA: &str = "zeno-fcis/contract-diff/1";

const STATE: u32 = ROOTS[0].1;
const COMMAND: u32 = ROOTS[1].1;
const CONTEXT: u32 = ROOTS[2].1;

/// The kind of a change from one contract to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// The canonical policies are byte-identical, so the identities are equal.
    Identical,
    /// Only the decision program's instructions and roots and the Step limit differ.
    ProgramSuccessor,
    /// Only profile, type, field or variant names differ.
    Rename,
    /// The state layout differs; every other type and every channel does not.
    LayoutChange,
    /// The schema and the channels are unchanged, and more than the program is not.
    RuleChange,
    /// Anything else.
    Unrelated,
}

impl Kind {
    /// Every kind in the fixed order its condition is tried in; the first
    /// that holds is the kind of the change.
    pub(crate) const PRECEDENCE: [Self; 6] = [
        Self::Identical,
        Self::ProgramSuccessor,
        Self::Rename,
        Self::LayoutChange,
        Self::RuleChange,
        Self::Unrelated,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Identical => "identical",
            Self::ProgramSuccessor => "program-successor",
            Self::Rename => "rename",
            Self::LayoutChange => "layout-change",
            Self::RuleChange => "rule-change",
            Self::Unrelated => "unrelated",
        }
    }

    /// Whether this kind's own condition holds, before precedence.
    fn holds(self, facts: &Facts) -> bool {
        match self {
            Self::Identical => facts.same_policy,
            Self::ProgramSuccessor => facts.successor,
            Self::Rename => facts.same_shapes && !facts.same_names && facts.same_but_names,
            Self::LayoutChange => facts.state_differs && facts.same_rest && facts.same_channels,
            Self::RuleChange => facts.same_schema && facts.same_channels,
            Self::Unrelated => true,
        }
    }
}

/// What the comparison established, from which the kind is decided.
#[derive(Clone, Copy, Debug)]
struct Facts {
    /// The canonical policies are byte-identical.
    same_policy: bool,
    /// The F6.1 premise-1 comparison holds; see [`program_successor`].
    successor: bool,
    /// The new policy, with the old schema in place of its own, is the old
    /// policy; see [`same_but_names`].
    same_but_names: bool,
    /// The same types, fields and variants, with the same IDs and forms.
    same_shapes: bool,
    /// The same profile, type, field and variant names.
    same_names: bool,
    /// The schemas are byte-identical.
    same_schema: bool,
    /// The same channels, each with the same destination and payload type.
    same_channels: bool,
    /// The state record, or a type one of its fields has, differs.
    state_differs: bool,
    /// Every type other than those only the state has is unchanged.
    same_rest: bool,
}

/// Which contract a refusal is about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Side {
    Old,
    New,
}

impl Side {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }
}

/// A contract that could not be compared, and which one it is.
#[derive(Debug)]
pub(crate) struct Refused {
    pub(crate) side: Side,
    pub(crate) error: ContractError,
}

/// Classifies the change from the contract `old` to the contract `new` and
/// accounts for it. Each side is generated exactly as `generate contract`
/// generates it, every adoption receipt replayed.
///
/// # Errors
/// The side whose contract generation refuses, with the refusal.
pub(crate) fn diff(old: ContractSources<'_>, new: ContractSources<'_>) -> Result<Diff, Refused> {
    let refused = |side| move |error| Refused { side, error };
    let old_generated = generate_contract(old).map_err(refused(Side::Old))?;
    let new_generated = generate_contract(new).map_err(refused(Side::New))?;
    between(old, &old_generated, new, &new_generated)
}

/// [`diff`] of two contracts already generated from these sources.
///
/// # Errors
/// The side whose model cannot be rebuilt from its sources, with the reason.
pub(super) fn between(
    old: ContractSources<'_>,
    old_generated: &GeneratedContract,
    new: ContractSources<'_>,
    new_generated: &GeneratedContract,
) -> Result<Diff, Refused> {
    let nested = with_current(old, old_generated, |old| {
        Ok(with_current(new, new_generated, |new| {
            Ok(compare(&old, &new))
        }))
    });
    match nested {
        Ok(Ok(Ok(diff))) => Ok(diff),
        Err(error) => Err(Refused {
            side: Side::Old,
            error,
        }),
        Ok(Err(error) | Ok(Err(error))) => Err(Refused {
            side: Side::New,
            error,
        }),
    }
}

/// The refusal of an adoption that would not make a program successor of
/// the version it supersedes: `contract adopt` admits nothing else.
///
/// # Errors
/// Unless `diff` is a program succession.
pub(super) fn require_successor(
    diff: &Diff,
    place: &str,
    version: usize,
) -> Result<(), ContractError> {
    if diff.kind() == Kind::ProgramSuccessor {
        return Ok(());
    }
    let parts: Vec<&str> = diff.parts.iter().map(|part| part.name()).collect();
    Err(ContractError::new(
        place,
        format!(
            "the adoption would make a {} of version {version}, not a program successor (changed: \
             {}); `contract adopt` admits only a program successor, whose policy differs from the \
             superseded one only in its decision program and Step limit",
            diff.kind().name(),
            if parts.is_empty() {
                "nothing".to_owned()
            } else {
                parts.join(", ")
            }
        ),
    ))
}

/// How `contract evolve` takes a change: the admission path a store then
/// follows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Path {
    /// A rule change without a migration: the behaviour-change upgrade.
    BehaviourChange,
    /// A rename: the exact rename tier.
    Rename,
    /// A layout or rule change with a declared migration: forward
    /// simulation.
    Migration,
}

/// The path `contract evolve` takes for a change of `diff`'s kind, with a
/// migration file or without one: a rule change without a migration is a
/// behaviour change; a rename takes the rename tier and no migration; a
/// layout change needs a migration, and a rule change may declare one, which
/// forward simulation then checks.
///
/// # Errors
/// A change of another kind, or a migration where none applies, naming the
/// kind and the path that does take it.
pub(super) fn evolution_path(diff: &Diff, with_migration: bool) -> Result<Path, ContractError> {
    let path = match (diff.kind(), with_migration) {
        (Kind::RuleChange, false) => return Ok(Path::BehaviourChange),
        (Kind::Rename, false) => return Ok(Path::Rename),
        (Kind::LayoutChange | Kind::RuleChange, true) => return Ok(Path::Migration),
        (Kind::Identical, _) => "nothing changes, so there is nothing to evolve",
        (Kind::ProgramSuccessor, _) => {
            "adopt the new decision program with `contract adopt`; a store then upgrades as a \
             program successor"
        }
        (Kind::Rename, true) => {
            "a rename changes only names and takes the exact rename tier: run `contract evolve` \
             without --migration"
        }
        (Kind::LayoutChange, false) => {
            "a layout change needs a data migration: declare one in a migration file and pass it \
             with --migration"
        }
        (Kind::Unrelated, _) => "no admission path takes this change; a store cannot follow it",
    };
    let parts: Vec<&str> = diff.parts.iter().map(|part| part.name()).collect();
    let admitted = if with_migration {
        "with --migration, `contract evolve` admits a layout change or a rule change"
    } else {
        "without --migration, `contract evolve` admits a rule change or a rename"
    };
    Err(ContractError::new(
        "contract evolve",
        format!(
            "the change is classified `{}` (changed: {}): {admitted}; {path}",
            diff.kind().name(),
            if parts.is_empty() {
                "nothing".to_owned()
            } else {
                parts.join(", ")
            }
        ),
    ))
}

/// A part of a contract a change touched.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Part {
    /// A name of an item on both sides.
    Names,
    /// The state record or a type only the state has.
    State,
    /// Any other type: the command, the context, a payload, a destination, a
    /// type they share with the state, or a type nothing has.
    Interface,
    /// A channel's ID, destination type or payload type.
    Channels,
    /// A declared reason or its class.
    Reasons,
    /// The genesis state.
    Genesis,
    /// A compiled law, declared or generated.
    Laws,
    /// The case table.
    Cases,
    /// The decision program.
    Program,
    /// The Step limit.
    StepLimit,
    /// The read, write, byte or effect limit.
    Limits,
}

impl Part {
    fn name(self) -> &'static str {
        match self {
            Self::Names => "names",
            Self::State => "state",
            Self::Interface => "interface",
            Self::Channels => "channels",
            Self::Reasons => "reasons",
            Self::Genesis => "genesis",
            Self::Laws => "laws",
            Self::Cases => "cases",
            Self::Program => "program",
            Self::StepLimit => "step-limit",
            Self::Limits => "limits",
        }
    }
}

/// The read, write, byte, effect and Step limits, in this order.
const LIMITS: [&str; 5] = ["read", "write", "byte", "effect", "Step"];

/// One side's version, as reports name it.
#[derive(Clone, Debug)]
struct Version {
    application: String,
    version: u32,
    policy_sha256: String,
    schema_sha256: String,
    program_sha256: String,
    program_nodes: usize,
    /// See [`LIMITS`].
    limits: [u64; 5],
}

impl Version {
    fn of(current: &Current<'_>) -> Self {
        let (generated, budgets) = (current.generated, current.contract.budgets);
        Self {
            application: current.rules.template.clone(),
            version: generated.summary().version,
            policy_sha256: sha256_hex(generated.policy()),
            schema_sha256: sha256_hex(generated.schema()),
            program_sha256: sha256_hex(generated.program()),
            program_nodes: current.contract.nodes.len(),
            limits: [
                budgets.read,
                budgets.write,
                budgets.byte,
                budgets.effect,
                budgets.step,
            ],
        }
    }

    fn json(&self) -> Value {
        let limits: Map<String, Value> = LIMITS
            .iter()
            .zip(self.limits)
            .map(|(name, limit)| (name.to_lowercase(), json!(limit)))
            .collect();
        json!({
            "application": self.application, "version": self.version,
            "policy_sha256": self.policy_sha256, "schema_sha256": self.schema_sha256,
            "program": {"sha256": self.program_sha256, "nodes": self.program_nodes},
            "limits": limits
        })
    }
}

/// One changed item, or one difference outside the contract: what it is,
/// how it changed and one plain sentence, with its values before and after
/// and, for a changed case, law or program, which aspects changed.
#[derive(Clone, Debug)]
struct Entry {
    item: &'static str,
    id: String,
    change: &'static str,
    text: String,
    old: Option<Value>,
    new: Option<Value>,
    aspects: Vec<&'static str>,
}

impl Entry {
    fn new(item: &'static str, id: impl ToString, change: &'static str, text: String) -> Self {
        Self {
            item,
            id: id.to_string(),
            change,
            text,
            old: None,
            new: None,
            aspects: Vec::new(),
        }
    }

    fn values(self, old: Value, new: Value) -> Self {
        Self {
            old: Some(old),
            new: Some(new),
            ..self
        }
    }

    fn was(self, old: Value) -> Self {
        Self {
            old: Some(old),
            ..self
        }
    }

    fn is(self, new: Value) -> Self {
        Self {
            new: Some(new),
            ..self
        }
    }

    fn json(&self) -> Value {
        let mut fields = Map::new();
        fields.insert("item".to_owned(), json!(self.item));
        fields.insert("id".to_owned(), json!(self.id));
        fields.insert("change".to_owned(), json!(self.change));
        fields.insert("text".to_owned(), json!(self.text));
        for (key, value) in [("old", &self.old), ("new", &self.new)] {
            if let Some(value) = value {
                fields.insert(key.to_owned(), value.clone());
            }
        }
        if !self.aspects.is_empty() {
            fields.insert("aspects".to_owned(), json!(self.aspects));
        }
        Value::Object(fields)
    }
}

/// A decided change kind, with the account of every changed item.
#[derive(Clone, Debug)]
pub(crate) struct Diff {
    kind: Kind,
    parts: Vec<Part>,
    old: Version,
    new: Version,
    /// The version of the new contract's lineage that is the old contract,
    /// and the receipt digests of the adoptions after it.
    lineage: Option<(u32, Vec<String>)>,
    /// The version of the old contract's lineage that is the new contract,
    /// when it is an earlier one.
    rollback: Option<u32>,
    /// Changed items of the contract.
    changes: Vec<Entry>,
    /// Differences that are not part of the contract.
    notes: Vec<Entry>,
}

impl Diff {
    pub(crate) fn kind(&self) -> Kind {
        self.kind
    }

    /// The whole document, schema [`DIFF_SCHEMA`]: deterministic for the
    /// same two contracts, wherever their directories are.
    pub(crate) fn json(&self) -> Value {
        json!({
            "schema": DIFF_SCHEMA,
            "status": "classified",
            "authority": "none",
            "kind": self.kind.name(),
            "precedence": Kind::PRECEDENCE.map(Kind::name),
            "parts": self.parts.iter().map(|part| part.name()).collect::<Vec<_>>(),
            "old": self.old.json(),
            "new": self.new.json(),
            "lineage": {
                "old_in_new": self.lineage.as_ref().map(|(version, _)| version),
                "receipts": self.lineage.as_ref().map_or(&[][..], |(_, receipts)| receipts),
                "new_in_old": self.rollback,
            },
            "admission": self.admission(),
            "changes": self.changes.iter().map(Entry::json).collect::<Vec<_>>(),
            "notes": self.notes.iter().map(Entry::json).collect::<Vec<_>>(),
            "summary": self.lines(),
        })
    }

    /// The plain-language account, one line each.
    pub(crate) fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "{}: {} version {} -> {} version {}",
                self.kind.name(),
                self.old.application,
                self.old.version,
                self.new.application,
                self.new.version
            ),
            self.description().to_owned(),
            format!("admission: {}", self.admission_text()),
        ];
        if self.changes.is_empty() {
            lines.push("changes: none".to_owned());
        } else {
            lines.push("changes:".to_owned());
            lines.extend(self.changes.iter().map(|entry| format!("- {}", entry.text)));
        }
        if !self.notes.is_empty() {
            lines.push("not part of the contract:".to_owned());
            lines.extend(self.notes.iter().map(|entry| format!("- {}", entry.text)));
        }
        lines
    }

    /// What the kind means for these two contracts.
    fn description(&self) -> &'static str {
        match self.kind {
            Kind::Identical => {
                "The canonical policies are byte-identical: the two contracts are one contract, \
                 with one identity."
            }
            Kind::ProgramSuccessor => "Only the decision program and its Step limit differ.",
            Kind::Rename => {
                "Only names differ: the same types, fields and variants, with the same IDs and \
                 forms."
            }
            Kind::LayoutChange => {
                "The state's fields differ; the command, the context, the channels and every \
                 other type do not."
            }
            Kind::RuleChange => {
                "The laws, cases, reasons or genesis state differ; the schema and the channels do \
                 not."
            }
            Kind::Unrelated if self.mixed_rename() => {
                "Names changed together with other parts of the contract, which no single \
                 admission path takes."
            }
            Kind::Unrelated => {
                "The command, the context, a channel, or a type beside those only the state has, \
                 differs."
            }
        }
    }

    /// An unrelated change in which names changed along with other parts,
    /// while the state layout, every other type and the channels did not.
    fn mixed_rename(&self) -> bool {
        self.kind == Kind::Unrelated
            && self.parts.contains(&Part::Names)
            && !self
                .parts
                .iter()
                .any(|part| matches!(part, Part::State | Part::Interface | Part::Channels))
    }

    /// The new contract's lineage holds the old contract below its current
    /// version, with the receipts of the adoptions after it.
    fn lineage_holds_old(&self) -> bool {
        self.lineage
            .as_ref()
            .is_some_and(|(version, _)| *version < self.new.version)
    }

    fn admission_text(&self) -> String {
        match self.kind {
            Kind::Identical => "none: nothing changes, so nothing needs to be admitted.".to_owned(),
            Kind::ProgramSuccessor => {
                let path = "an F3 equivalence receipt and the F6.1 program-successor upgrade, \
                    which exists today and admits the new version at any state.";
                match (&self.lineage, self.rollback) {
                    (Some((version, receipts)), _) if self.lineage_holds_old() => {
                        let adoptions = match receipts.len() {
                            1 => "the receipt of the adoption".to_owned(),
                            count => format!("the receipts of the {count} adoptions"),
                        };
                        format!(
                            "{path} The new contract's lineage holds the old contract as version \
                             {version}, with {adoptions} after it, so a store at the old contract \
                             upgrades with the application's `--upgrade`."
                        )
                    }
                    (_, Some(version)) => format!(
                        "{path} But the new contract is version {version} of the old contract's \
                         own lineage, and a store never returns to an earlier version."
                    ),
                    _ => format!(
                        "{path} The new contract's lineage does not hold the old contract yet: \
                         adopt the new contract's decision program into the old contract with \
                         `contract adopt`, which needs an F3 receipt comparing the two programs."
                    ),
                }
            }
            Kind::Rename => "G2's rename tier, which exists today and admits the new contract \
                at any state when it is the old one with the names substituted: `contract \
                evolve` records this account with the change, and a store upgrades with the \
                application's `--upgrade`, its state framed again under the new names."
                .to_owned(),
            Kind::LayoutChange => "a G2 data migration, which exists today: `contract evolve \
                --migration` takes a declarative migration from the old state to the new one, \
                and it is admitted only by forward simulation over the old contract's whole \
                declared input domain, at most 2^20 tuples: on the migrated state the new \
                contract must make every old decision, send every old delivery and reach the \
                migrated successor. A store then upgrades with the application's `--upgrade`."
                .to_owned(),
            Kind::RuleChange => "G14.1's behaviour-change upgrade, which exists today: \
                `contract evolve` records this account with the change, and a store upgrades \
                with the application's `--upgrade` when every state law of the new contract and \
                every inductive claim it declares hold on the store's state. Genesis exactness, \
                law 990, applies only to new stores and is not checked there. G2's forward \
                simulation also exists today, for a rule change that preserves every decision: \
                `contract evolve --migration` admits it only when simulation over the whole \
                declared input domain finds no difference."
                .to_owned(),
            Kind::Unrelated if self.mixed_rename() => "refused: no admission path takes this \
                change. Make the renaming one version and the other change the next."
                .to_owned(),
            Kind::Unrelated => "refused: no admission path takes this change; a store cannot \
                follow it and must start again under the new contract."
                .to_owned(),
        }
    }

    fn admission(&self) -> Value {
        let path = |id: &str, feature: &str, exists: bool, when: &str| json!({"id": id, "feature": feature, "exists_today": exists, "when": when});
        let paths = match self.kind {
            Kind::Identical | Kind::Unrelated => Vec::new(),
            Kind::ProgramSuccessor => vec![path(
                "f6.1-program-successor",
                "F6.1",
                true,
                "an F3 equivalence receipt compares the two programs",
            )],
            Kind::Rename => vec![path(
                "g2-rename",
                "G2",
                true,
                "the new contract is the old one with the names substituted",
            )],
            Kind::LayoutChange => vec![path(
                "g2-migration",
                "G2",
                true,
                "a migration maps the old state to the new one and forward simulation holds",
            )],
            Kind::RuleChange => vec![
                path(
                    "g2-forward-simulation",
                    "G2",
                    true,
                    "the decisions are preserved",
                ),
                path(
                    "g14.1-behaviour-change",
                    "G14.1",
                    true,
                    "every state law and declared inductive claim of the new contract holds on the store's state",
                ),
            ],
        };
        json!({
            "needed": self.kind != Kind::Identical,
            "refused": self.kind == Kind::Unrelated,
            "paths": paths,
            "ready": self.kind == Kind::ProgramSuccessor && self.lineage_holds_old(),
            "text": self.admission_text(),
        })
    }
}

/// Whether `to` succeeds `from` by its decision program alone: `to`'s
/// complete canonical policy, with its program's instructions and roots and
/// its Step limit replaced by `from`'s, is byte for byte `from`'s policy.
///
/// This mirrors `zeno_fcis_shell_sqlite::v2::upgrade::program_successor`,
/// the shell's premise 1 of an F6.1 Tier A upgrade, line for line. The
/// tests compile that function from the shell's own source and check that
/// it accepts exactly the pairs this classifier calls identical or program
/// successors, and that every pair the shell's Tier A admission admits is one; Tier A
/// also needs premises this classifier does not check.
fn program_successor(
    from: &BoundCatalog<'_>,
    to: &BoundCatalog<'_>,
) -> Result<bool, ContractError> {
    let (old, new) = (from.descriptor(), to.descriptor());
    let substituted = Descriptor {
        state: new.state,
        command: new.command,
        context: new.context,
        program: V2ScalarProgram {
            inputs: new.program.inputs,
            outputs: new.program.outputs,
            nodes: old.program.nodes,
            roots: old.program.roots,
        },
        bindings: new.bindings,
        output_types: new.output_types,
        decision_output: new.decision_output,
        branches: new.branches,
        reasons: new.reasons,
        channels: new.channels,
        laws: new.laws,
        required: new.required,
        limits: new
            .limits
            .with_limit(V2Resource::Step, old.limits.limit(V2Resource::Step)),
    };
    let policy = authority::policy_bytes(
        &substituted,
        to.original_schema(),
        to.framing(),
        to.channel_roots(),
    )
    .ok_or_else(overflow)?;
    Ok(policy == from.original_contract())
}

/// Whether `to` differs from `from` in its original schema alone: `to`'s
/// complete canonical policy, with `from`'s original schema and schema
/// commitment in place of its own, is byte for byte `from`'s policy. Every
/// descriptor field, the channel links and the framing's roots and size
/// limits are then `from`'s.
fn same_but_names(from: &BoundCatalog<'_>, to: &BoundCatalog<'_>) -> Result<bool, ContractError> {
    let (old, new) = (from.framing(), to.framing());
    let frame = |binding: FrameBinding, commitment: FrameBinding| FrameBinding {
        schema: commitment.schema,
        ..binding
    };
    let framing = Framing {
        state: frame(new.state, old.state),
        command: frame(new.command, old.command),
        context: frame(new.context, old.context),
    };
    let policy = authority::policy_bytes(
        to.descriptor(),
        from.original_schema(),
        &framing,
        to.channel_roots(),
    )
    .ok_or_else(overflow)?;
    Ok(policy == from.original_contract())
}

fn overflow() -> ContractError {
    ContractError::new(
        "contract diff",
        "the library policy encoding overflowed while comparing the contracts",
    )
}

/// A declared type's form with every name erased.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Shape {
    Leaf(Leaf),
    /// Each field's ID and type, in order.
    Record(Vec<(u16, u32)>),
    /// Each variant's ID, in order.
    Sum(Vec<u16>),
}

impl Shape {
    fn of(form: &Form) -> Self {
        match form {
            Form::Leaf(leaf) => Self::Leaf(*leaf),
            Form::Record(fields) => Self::Record(
                fields
                    .iter()
                    .map(|field| (field.id, field.type_id))
                    .collect(),
            ),
            Form::Sum(variants) => Self::Sum(variants.iter().map(|variant| variant.id).collect()),
        }
    }

    /// The same form, a record or a sum on both sides counting as one
    /// whatever their fields or variants, which are compared one by one.
    fn same_form(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Record(_), Self::Record(_)) | (Self::Sum(_), Self::Sum(_)) => true,
            _ => self == other,
        }
    }

    fn text(&self) -> String {
        match self {
            Self::Leaf(Leaf::Bool) => "a boolean".to_owned(),
            Self::Leaf(Leaf::I128 { min, max }) => format!("an integer from {min} to {max}"),
            Self::Leaf(Leaf::Text { min, max }) => format!("text of {min} to {max} bytes"),
            Self::Record(fields) => format!("a record of {} fields", fields.len()),
            Self::Sum(variants) => format!("a sum of {} variants", variants.len()),
        }
    }
}

/// Every name the schema holds.
#[derive(Debug, Eq, PartialEq)]
struct Names {
    profile: String,
    types: BTreeMap<u32, String>,
    /// By owning type and field ID.
    fields: BTreeMap<(u32, u16), String>,
    /// By owning type and variant ID.
    variants: BTreeMap<(u32, u16), String>,
}

/// A case's content with every variable replaced by its definition, as
/// generation compiles it; its `rule` note is left out.
#[derive(Debug, Eq, PartialEq)]
struct CaseContent {
    when: Ast,
    class: Class,
    reason: Option<u32>,
    post: BTreeMap<u16, Ast>,
    outbox: Vec<DeliveryContent>,
}

#[derive(Debug, Eq, PartialEq)]
struct DeliveryContent {
    ordinal: u32,
    channel: u32,
    destination: String,
    payload: BTreeMap<u16, Ast>,
    idempotency: u128,
}

/// One side, with what the comparison reads from it.
struct View<'a> {
    current: &'a Current<'a>,
    shapes: BTreeMap<u32, Shape>,
    names: Names,
    links: Vec<(u32, u32, u32)>,
    /// The state root and every type its fields have.
    state_types: BTreeSet<u32>,
    /// The command and context roots, the channels' destination and payload
    /// types, and every type their records' fields have.
    interface_types: BTreeSet<u32>,
    cases: Vec<CaseContent>,
}

impl<'a> View<'a> {
    fn of(current: &'a Current<'a>) -> Result<Self, ContractError> {
        let declarations = current.declarations;
        let shapes: BTreeMap<u32, Shape> = declarations
            .types
            .iter()
            .map(|(id, declared)| (*id, Shape::of(&declared.form)))
            .collect();
        let mut names = Names {
            profile: declarations.profile.clone(),
            types: BTreeMap::new(),
            fields: BTreeMap::new(),
            variants: BTreeMap::new(),
        };
        for (id, declared) in &declarations.types {
            names.types.insert(*id, declared.name.clone());
            match &declared.form {
                Form::Record(fields) => names.fields.extend(
                    fields
                        .iter()
                        .map(|field| ((*id, field.id), field.name.clone())),
                ),
                Form::Sum(variants) => names.variants.extend(
                    variants
                        .iter()
                        .map(|variant| ((*id, variant.id), variant.name.clone())),
                ),
                Form::Leaf(_) => {}
            }
        }
        let channels = &declarations.channels;
        let state_types = reachable(&shapes, [STATE]);
        let interface_types = reachable(
            &shapes,
            [COMMAND, CONTEXT].into_iter().chain(
                channels
                    .iter()
                    .flat_map(|channel| [channel.destination, channel.payload]),
            ),
        );
        let variables = &current.rules.variables;
        Ok(Self {
            current,
            shapes,
            names,
            links: channels
                .iter()
                .map(|channel| (channel.id, channel.destination, channel.payload))
                .collect(),
            state_types,
            interface_types,
            cases: current
                .rules
                .cases
                .iter()
                .map(|case| content(case, variables))
                .collect::<Result<_, _>>()?,
        })
    }

    fn declarations(&self) -> &'a Declarations {
        self.current.declarations
    }
}

/// `roots` and every type a record among them, or among those, has a field of.
fn reachable(shapes: &BTreeMap<u32, Shape>, roots: impl IntoIterator<Item = u32>) -> BTreeSet<u32> {
    let mut found = BTreeSet::new();
    let mut pending: Vec<u32> = roots.into_iter().collect();
    while let Some(id) = pending.pop() {
        if found.insert(id)
            && let Some(Shape::Record(fields)) = shapes.get(&id)
        {
            pending.extend(fields.iter().map(|(_, type_id)| *type_id));
        }
    }
    found
}

fn content(case: &Case, variables: &BTreeMap<String, Ast>) -> Result<CaseContent, ContractError> {
    let expand = |ast: &Ast| {
        expr::expand(ast, variables)
            .map_err(|reason| ContractError::new("v2/policy.json cases", reason))
    };
    let values = |values: &BTreeMap<u16, Ast>| {
        values
            .iter()
            .map(|(field, value)| Ok((*field, expand(value)?)))
            .collect::<Result<BTreeMap<_, _>, ContractError>>()
    };
    Ok(CaseContent {
        when: expand(&case.when)?,
        class: case.class,
        reason: case.reason,
        post: values(&case.post)?,
        outbox: case
            .outbox
            .iter()
            .map(|delivery| {
                Ok(DeliveryContent {
                    ordinal: delivery.ordinal,
                    channel: delivery.channel,
                    destination: delivery.destination.clone(),
                    payload: values(&delivery.payload)?,
                    idempotency: delivery.idempotency,
                })
            })
            .collect::<Result<_, ContractError>>()?,
    })
}

fn compare(old: &Current<'_>, new: &Current<'_>) -> Result<Diff, ContractError> {
    let (successor, same_but_names) = policy::with_catalog(old.contract, old.schema, |_, from| {
        policy::with_catalog(new.contract, new.schema, |_, to| {
            Ok((program_successor(from, to)?, same_but_names(from, to)?))
        })
    })?;
    let (old, new) = (View::of(old)?, View::of(new)?);
    let types = union(old.shapes.keys(), new.shapes.keys());
    let state_types = union(&old.state_types, &new.state_types);
    let shared = |id: &u32| old.interface_types.contains(id) || new.interface_types.contains(id);
    let facts = Facts {
        same_policy: old.current.generated.policy() == new.current.generated.policy(),
        successor,
        same_but_names,
        same_shapes: old.shapes == new.shapes,
        same_names: old.names == new.names,
        same_schema: old.current.schema == new.current.schema,
        same_channels: old.links == new.links,
        state_differs: state_types
            .iter()
            .any(|id| old.shapes.get(id) != new.shapes.get(id)),
        same_rest: types
            .iter()
            .filter(|id| !state_types.contains(id) || shared(id))
            .all(|id| old.shapes.get(id) == new.shapes.get(id)),
    };
    // The schema is the profile name and every type's name and form, and
    // both policy comparisons embed the channel links, the first also the
    // schema: a part of either this model misses fails closed here, before
    // any kind is decided.
    if facts.same_schema != (facts.same_shapes && facts.same_names)
        || (facts.successor && !(facts.same_schema && facts.same_channels))
        || (facts.same_but_names && !facts.same_channels)
    {
        return Err(ContractError::new(
            "contract diff",
            "the structural comparison disagrees with the canonical bytes",
        ));
    }
    let kind = Kind::PRECEDENCE
        .into_iter()
        .find(|kind| kind.holds(&facts))
        .unwrap_or(Kind::Unrelated);
    let mut account = Account {
        old: &old,
        new: &new,
        parts: BTreeSet::new(),
        changes: Vec::new(),
        labels: Vec::new(),
    };
    account.schema();
    account.channels();
    account.reasons();
    account.genesis();
    let cases_changed = account.cases();
    account.laws(cases_changed);
    account.program();
    let Account {
        mut parts,
        changes,
        labels,
        ..
    } = account;
    if facts.state_differs {
        parts.insert(Part::State);
    }
    if !facts.same_rest {
        parts.insert(Part::Interface);
    }
    let (old_generated, new_generated) = (old.current.generated, new.current.generated);
    Ok(Diff {
        kind,
        parts: parts.into_iter().collect(),
        old: Version::of(old.current),
        new: Version::of(new.current),
        lineage: lineage(old_generated, new_generated),
        rollback: lineage(new_generated, old_generated)
            .map(|(version, _)| version)
            .filter(|version| *version < old_generated.summary().version),
        changes,
        notes: notes(&old, &new, labels),
    })
}

/// The version of `lineage`'s lineage whose policy is `of`'s current one,
/// with the receipt digests of the adoptions after it.
fn lineage(of: &GeneratedContract, lineage: &GeneratedContract) -> Option<(u32, Vec<String>)> {
    let position = lineage
        .previous()
        .iter()
        .map(|previous| previous.policy())
        .chain(std::iter::once(lineage.policy()))
        .position(|policy| policy == of.policy())?;
    // The current contract's adoptions are the lineage's last steps; an
    // earlier contract's, behind an evolution, are not listed.
    let adoptions = &lineage.summary().adoptions;
    let first = lineage.previous().len() - adoptions.len();
    let receipts = adoptions
        .get(position.saturating_sub(first)..)
        .unwrap_or_default()
        .iter()
        .map(|adoption| adoption.receipt_sha256.clone())
        .collect();
    Some((u32::try_from(position + 1).ok()?, receipts))
}

fn union<'i, T: Copy + Ord + 'i>(
    left: impl IntoIterator<Item = &'i T>,
    right: impl IntoIterator<Item = &'i T>,
) -> BTreeSet<T> {
    left.into_iter().chain(right).copied().collect()
}

/// The changed items found so far and the parts they touch.
struct Account<'v, 'a> {
    old: &'v View<'a>,
    new: &'v View<'a>,
    parts: BTreeSet<Part>,
    changes: Vec<Entry>,
    /// Aligned cases whose `rule` note changed, for the notes.
    labels: Vec<Entry>,
}

impl<'a> Account<'_, 'a> {
    fn push(&mut self, part: Part, entry: Entry) {
        self.parts.insert(part);
        self.changes.push(entry);
    }

    /// The part a change to type `id` touches: the state when only the
    /// state has it, on either side; otherwise the rest of the schema.
    fn type_part(&self, id: u32) -> Part {
        let (old, new) = (self.old, self.new);
        let state = old.state_types.contains(&id) || new.state_types.contains(&id);
        let shared = old.interface_types.contains(&id) || new.interface_types.contains(&id);
        if state && !shared {
            Part::State
        } else {
            Part::Interface
        }
    }

    /// The profile, then every type with its fields and variants, in ID order.
    fn schema(&mut self) {
        let (old, new) = (self.old.declarations(), self.new.declarations());
        if old.profile != new.profile {
            let text = format!(
                "the project was renamed from `{}` to `{}`",
                old.profile, new.profile
            );
            let entry = Entry::new("profile", "", "renamed", text);
            self.push(
                Part::Names,
                entry.values(json!(old.profile), json!(new.profile)),
            );
        }
        for id in union(old.types.keys(), new.types.keys()) {
            let part = self.type_part(id);
            match (old.types.get(&id), new.types.get(&id)) {
                (Some(was), None) => {
                    let text = format!(
                        "type {id} `{}`, {}{}, was removed",
                        was.name,
                        Shape::of(&was.form).text(),
                        uses(old, id)
                    );
                    let entry = Entry::new("type", id, "removed", text);
                    self.push(part, entry.was(json!(was.name)));
                }
                (None, Some(is)) => {
                    let text = format!(
                        "type {id} `{}`, {}{}, was added",
                        is.name,
                        Shape::of(&is.form).text(),
                        uses(new, id)
                    );
                    let entry = Entry::new("type", id, "added", text);
                    self.push(part, entry.is(json!(is.name)));
                }
                (Some(was), Some(is)) => {
                    if was.name != is.name {
                        let text = format!("type {id} `{}` was renamed `{}`", was.name, is.name);
                        let entry = Entry::new("type", id, "renamed", text);
                        self.push(Part::Names, entry.values(json!(was.name), json!(is.name)));
                    }
                    let (before, after) = (Shape::of(&was.form), Shape::of(&is.form));
                    if !before.same_form(&after) {
                        let text = format!(
                            "type {id} `{}` changed from {} to {}{}",
                            is.name,
                            before.text(),
                            after.text(),
                            uses(new, id)
                        );
                        let entry = Entry::new("type", id, "changed", text);
                        self.push(
                            part,
                            entry.values(json!(before.text()), json!(after.text())),
                        );
                    }
                    match (&was.form, &is.form) {
                        (Form::Record(_), Form::Record(_)) => self.fields(id, part),
                        (Form::Sum(_), Form::Sum(_)) => self.variants(id, part),
                        _ => {}
                    }
                }
                (None, None) => {}
            }
        }
    }

    fn fields(&mut self, owner: u32, part: Part) {
        let (old, new) = (self.old.declarations(), self.new.declarations());
        let fields = |declarations: &Declarations| -> BTreeMap<u16, (String, u32)> {
            declarations.fields(owner).map_or_else(
                |_| BTreeMap::new(),
                |fields| {
                    fields
                        .iter()
                        .map(|field| (field.id, (field.name.clone(), field.type_id)))
                        .collect()
                },
            )
        };
        let (before, after) = (fields(old), fields(new));
        let label = field_label(new, owner);
        for id in union(before.keys(), after.keys()) {
            let key = format!("{owner}.{id}");
            match (before.get(&id), after.get(&id)) {
                (Some((name, _)), None) => {
                    let text = format!("{label} {id} `{name}` was removed");
                    self.push(
                        part,
                        Entry::new("field", key, "removed", text).was(json!(name)),
                    );
                }
                (None, Some((name, type_id))) => {
                    let start = (owner == STATE)
                        .then(|| self.new.current.rules.genesis.get(&id))
                        .flatten()
                        .map(|value| format!("; it starts at {}", constant(new, *type_id, *value)))
                        .unwrap_or_default();
                    let text = format!(
                        "{label} {id} `{name}` of type {} was added{start}",
                        type_name(new, *type_id)
                    );
                    let entry = Entry::new("field", key, "added", text);
                    self.push(part, entry.is(json!({"name": name, "type": type_id})));
                }
                (Some((was_name, was_type)), Some((is_name, is_type))) => {
                    if was_name != is_name {
                        let text = format!("{label} {id} `{was_name}` was renamed `{is_name}`");
                        let entry = Entry::new("field", &key, "renamed", text);
                        self.push(Part::Names, entry.values(json!(was_name), json!(is_name)));
                    }
                    if was_type != is_type {
                        let text = format!(
                            "{label} {id} `{is_name}` changed its type from {} to {}",
                            type_name(old, *was_type),
                            type_name(new, *is_type)
                        );
                        let entry = Entry::new("field", key, "changed", text);
                        self.push(part, entry.values(json!(was_type), json!(is_type)));
                    }
                }
                (None, None) => {}
            }
        }
    }

    fn variants(&mut self, owner: u32, part: Part) {
        let (old, new) = (self.old.declarations(), self.new.declarations());
        let variants = |declarations: &Declarations| -> BTreeMap<u16, String> {
            match declarations
                .types
                .get(&owner)
                .map(|declared| &declared.form)
            {
                Some(Form::Sum(variants)) => variants
                    .iter()
                    .map(|variant| (variant.id, variant.name.clone()))
                    .collect(),
                _ => BTreeMap::new(),
            }
        };
        let (before, after) = (variants(old), variants(new));
        let of = format!("of type {}{}", type_name(new, owner), uses(new, owner));
        for id in union(before.keys(), after.keys()) {
            let key = format!("{owner}.{id}");
            match (before.get(&id), after.get(&id)) {
                (Some(name), None) => {
                    let text = format!("variant {id} `{name}` {of} was removed");
                    self.push(
                        part,
                        Entry::new("variant", key, "removed", text).was(json!(name)),
                    );
                }
                (None, Some(name)) => {
                    let text = format!("variant {id} `{name}` {of} was added");
                    self.push(
                        part,
                        Entry::new("variant", key, "added", text).is(json!(name)),
                    );
                }
                (Some(was), Some(is)) if was != is => {
                    let text = format!("variant {id} `{was}` {of} was renamed `{is}`");
                    let entry = Entry::new("variant", key, "renamed", text);
                    self.push(Part::Names, entry.values(json!(was), json!(is)));
                }
                _ => {}
            }
        }
    }

    fn channels(&mut self) {
        let (old, new) = (self.old.declarations(), self.new.declarations());
        let links = |declarations: &Declarations| -> BTreeMap<u32, (u32, u32)> {
            declarations
                .channels
                .iter()
                .map(|channel| (channel.id, (channel.destination, channel.payload)))
                .collect()
        };
        let (before, after) = (links(old), links(new));
        let types = |link: &(u32, u32)| json!({"destination": link.0, "payload": link.1});
        for id in union(before.keys(), after.keys()) {
            match (before.get(&id), after.get(&id)) {
                (Some(was), None) => {
                    let text = format!("{} was removed", channel_label(old, id));
                    let entry = Entry::new("channel", id, "removed", text);
                    self.push(Part::Channels, entry.was(types(was)));
                }
                (None, Some(is)) => {
                    let text = format!(
                        "{} was added: it delivers payload type {} to destination type {}",
                        channel_label(new, id),
                        type_name(new, is.1),
                        type_name(new, is.0)
                    );
                    let entry = Entry::new("channel", id, "added", text);
                    self.push(Part::Channels, entry.is(types(is)));
                }
                (Some(was), Some(is)) if was != is => {
                    let mut aspects = Vec::new();
                    for (aspect, was_type, is_type) in
                        [("destination", was.0, is.0), ("payload", was.1, is.1)]
                    {
                        if was_type != is_type {
                            aspects.push(format!(
                                "its {aspect} type {} became {}",
                                type_name(old, was_type),
                                type_name(new, is_type)
                            ));
                        }
                    }
                    let text =
                        format!("{} changed: {}", channel_label(new, id), aspects.join("; "));
                    let entry = Entry::new("channel", id, "changed", text);
                    self.push(Part::Channels, entry.values(types(was), types(is)));
                }
                _ => {}
            }
        }
    }

    /// Declared reasons, each with the one class every case using it has.
    fn reasons(&mut self) {
        let classes = |view: &View<'_>| -> BTreeMap<u32, Class> {
            view.current.contract.reasons.iter().copied().collect()
        };
        let (before, after) = (classes(self.old), classes(self.new));
        let (old, new) = (self.old.declarations(), self.new.declarations());
        for id in union(before.keys(), after.keys()) {
            let entry = match (before.get(&id), after.get(&id)) {
                (Some(class), None) => {
                    let text = format!(
                        "{} was removed; it was for a {}",
                        reason_label(old, id),
                        class.name()
                    );
                    Entry::new("reason", id, "removed", text).was(json!(class.name()))
                }
                (None, Some(class)) => {
                    let text = format!(
                        "{} was added, for a {}",
                        reason_label(new, id),
                        class.name()
                    );
                    Entry::new("reason", id, "added", text).is(json!(class.name()))
                }
                (Some(was), Some(is)) if was != is => {
                    let text = format!(
                        "{} changed from a {} to a {}",
                        reason_label(new, id),
                        was.name(),
                        is.name()
                    );
                    let entry = Entry::new("reason", id, "changed", text);
                    entry.values(json!(was.name()), json!(is.name()))
                }
                _ => continue,
            };
            self.push(Part::Reasons, entry);
        }
    }

    /// The genesis value of every state field on both sides.
    fn genesis(&mut self) {
        let (old, new) = (self.old.declarations(), self.new.declarations());
        let before = &self.old.current.rules.genesis;
        let after = &self.new.current.rules.genesis;
        for (field, was) in before {
            let (Some(is), Some((_, was_type)), Some((name, is_type))) = (
                after.get(field),
                state_field(old, *field),
                state_field(new, *field),
            ) else {
                continue;
            };
            if was == is {
                continue;
            }
            let (was, is) = (constant(old, was_type, *was), constant(new, is_type, *is));
            let text = format!(
                "the genesis value of state field {field} `{name}` changed from {was} to {is}"
            );
            let entry = Entry::new("genesis", field, "changed", text);
            self.push(Part::Genesis, entry.values(json!(was), json!(is)));
        }
    }

    /// Cases aligned in order by their content; returns whether any changed.
    fn cases(&mut self) -> bool {
        let (old, new) = (self.old, self.new);
        let (old_cases, new_cases) = (&old.current.rules.cases, &new.current.rules.cases);
        let mut changed = false;
        for step in align(&old.cases, &new.cases) {
            let entry = match step {
                Aligned::Same(before, after) => {
                    let (was, is) = (&old_cases[before].rule, &new_cases[after].rule);
                    if was != is {
                        let text = format!(
                            "case {after} is now labelled {} (was {})",
                            rule_text(is.as_deref()),
                            rule_text(was.as_deref())
                        );
                        let entry = Entry::new("case-rule", before, "changed", text);
                        self.labels.push(entry.values(json!(was), json!(is)));
                    }
                    continue;
                }
                Aligned::Removed(index) => {
                    let case = &old_cases[index];
                    let text = format!(
                        "{} was removed; it decided {} when `{}`",
                        case_label(index, case),
                        decision(old.declarations(), case),
                        expr::render(&case.when)
                    );
                    Entry::new("case", index, "removed", text).was(case_json(index, case))
                }
                Aligned::Added(index) => {
                    let case = &new_cases[index];
                    let text = format!(
                        "{} was added: it decides {} when `{}`",
                        case_label(index, case),
                        decision(new.declarations(), case),
                        expr::render(&case.when)
                    );
                    Entry::new("case", index, "added", text).is(case_json(index, case))
                }
                Aligned::Changed(before, after) => self.changed_case(before, after),
            };
            changed = true;
            self.push(Part::Cases, entry);
        }
        changed
    }

    fn changed_case(&self, before: usize, after: usize) -> Entry {
        let (old, new) = (self.old, self.new);
        let was = &old.current.rules.cases[before];
        let is = &new.current.rules.cases[after];
        let (was_content, is_content) = (&old.cases[before], &new.cases[after]);
        let mut texts = Vec::new();
        let mut aspects = Vec::new();
        if was_content.when != is_content.when {
            aspects.push("when");
            let (written_was, written_is) = (expr::render(&was.when), expr::render(&is.when));
            texts.push(if written_was == written_is {
                format!("its condition `{written_is}` reads a variable whose definition changed")
            } else {
                format!("its condition `{written_was}` became `{written_is}`")
            });
        }
        if was.class != is.class {
            aspects.push("class");
            texts.push(format!(
                "its class {} became {}",
                was.class.name(),
                is.class.name()
            ));
        }
        if was.reason != is.reason {
            aspects.push("reason");
            let reason = |declarations: &Declarations, reason: Option<u32>| {
                reason.map_or("none".to_owned(), |id| reason_label(declarations, id))
            };
            texts.push(format!(
                "its reason {} became {}",
                reason(old.declarations(), was.reason),
                reason(new.declarations(), is.reason)
            ));
        }
        let fields: Vec<String> = union(was_content.post.keys(), is_content.post.keys())
            .into_iter()
            .filter(|field| was_content.post.get(field) != is_content.post.get(field))
            .map(|field| {
                let name = state_field(new.declarations(), field).map_or("", |(name, _)| name);
                format!("{field} `{name}`")
            })
            .collect();
        if !fields.is_empty() {
            aspects.push("post");
            texts.push(format!(
                "its successor value of state field{} {} changed",
                if fields.len() == 1 { "" } else { "s" },
                fields.join(", ")
            ));
        }
        if was_content.outbox != is_content.outbox {
            aspects.push("outbox");
            texts.push(deliveries(&was_content.outbox, &is_content.outbox));
        }
        let label = if before == after {
            case_label(before, was)
        } else {
            format!("{} (now case {after})", case_label(before, was))
        };
        let text = format!("{label} changed: {}", texts.join("; "));
        Entry {
            aspects,
            ..Entry::new("case", before, "changed", text)
                .values(case_json(before, was), case_json(after, is))
        }
    }

    /// Every compiled law, declared or generated, in ID order.
    fn laws(&mut self, cases_changed: bool) {
        let (old, new) = (self.old, self.new);
        let laws = |view: &View<'a>| -> BTreeMap<u32, &'a Law> {
            view.current
                .contract
                .laws
                .iter()
                .map(|law| (law.id, law))
                .collect()
        };
        let (before, after) = (laws(old), laws(new));
        let genesis_changed = old.current.rules.genesis != new.current.rules.genesis;
        for id in union(before.keys(), after.keys()) {
            let entry = match (before.get(&id), after.get(&id)) {
                (Some(law), None) => {
                    let text = format!("{} was removed", law_label(old.declarations(), law));
                    Entry::new("law", id, "removed", text).was(law_json(old.declarations(), law))
                }
                (None, Some(law)) => {
                    let text = format!(
                        "{} was added, {}",
                        law_label(new.declarations(), law),
                        scope_text(law.scope, law.genesis)
                    );
                    Entry::new("law", id, "added", text).is(law_json(new.declarations(), law))
                }
                (Some(was), Some(is))
                    if (was.kind, was.scope, was.genesis, &was.nodes, was.root)
                        != (is.kind, is.scope, is.genesis, &is.nodes, is.root) =>
                {
                    let (old_formula, new_formula) = (
                        declared_formula(old.declarations(), id),
                        declared_formula(new.declarations(), id),
                    );
                    let mut texts = Vec::new();
                    let mut aspects = Vec::new();
                    if let (Some(was_formula), Some(is_formula)) = (old_formula, new_formula)
                        && was_formula != is_formula
                    {
                        aspects.push("formula");
                        texts.push(format!(
                            "its formula `{}` became `{}`",
                            expr::render(was_formula),
                            expr::render(is_formula)
                        ));
                    }
                    if was.kind != is.kind {
                        aspects.push("kind");
                        texts.push(format!(
                            "its kind {} became {}",
                            was.kind.name(),
                            is.kind.name()
                        ));
                    }
                    if (was.scope, was.genesis) != (is.scope, is.genesis) {
                        aspects.push("scope");
                        texts.push(format!(
                            "its scope `{}` became `{}`",
                            scope_text(was.scope, was.genesis),
                            scope_text(is.scope, is.genesis)
                        ));
                    }
                    let label = law_label(new.declarations(), is);
                    let text = if texts.is_empty() {
                        aspects.push("program");
                        let reason = match is.kind {
                            LawKind::DecisionConformance if cases_changed => "with the case table",
                            LawKind::DecisionConformance | LawKind::InitialCondition
                                if genesis_changed =>
                            {
                                "with the genesis state"
                            }
                            _ => "because a type or value it reads changed",
                        };
                        format!("{label} changed {reason}")
                    } else {
                        format!("{label} changed: {}", texts.join("; "))
                    };
                    Entry {
                        aspects,
                        ..Entry::new("law", id, "changed", text).values(
                            law_json(old.declarations(), was),
                            law_json(new.declarations(), is),
                        )
                    }
                }
                _ => continue,
            };
            self.push(Part::Laws, entry);
        }
    }

    /// The decision program and the limits.
    fn program(&mut self) {
        let (old, new) = (self.old.current, self.new.current);
        if old.generated.program() != new.generated.program() {
            let (was, is) = (old.contract, new.contract);
            let domains = |domains: &[ScalarDomain]| {
                domains
                    .iter()
                    .map(policy::scalar_domain)
                    .collect::<Vec<_>>()
            };
            let mut texts = Vec::new();
            let mut aspects = Vec::new();
            if (&was.nodes, &was.program_roots) != (&is.nodes, &is.program_roots) {
                aspects.push("instructions");
                texts.push(format!(
                    "its instructions ({} nodes before, {} after)",
                    was.nodes.len(),
                    is.nodes.len()
                ));
            }
            for (aspect, text, was_domains, is_domains) in [
                (
                    "inputs",
                    "its input domains",
                    &was.input_domains,
                    &is.input_domains,
                ),
                (
                    "outputs",
                    "its output domains",
                    &was.output_domains,
                    &is.output_domains,
                ),
            ] {
                if domains(was_domains) != domains(is_domains) {
                    aspects.push(aspect);
                    texts.push(text.to_owned());
                }
            }
            let text = format!("the decision program changed: {}", texts.join("; "));
            let entry = Entry::new("program", "", "changed", text)
                .values(json!(was.nodes.len()), json!(is.nodes.len()));
            self.push(Part::Program, Entry { aspects, ..entry });
        }
        let (was, is) = (Version::of(old).limits, Version::of(new).limits);
        for (index, name) in LIMITS.iter().enumerate() {
            if was[index] != is[index] {
                let part = if *name == "Step" {
                    Part::StepLimit
                } else {
                    Part::Limits
                };
                let text = format!(
                    "the {name} limit changed from {} to {}",
                    was[index], is[index]
                );
                let entry = Entry::new("limit", name.to_lowercase(), "changed", text);
                self.push(part, entry.values(json!(was[index]), json!(is[index])));
            }
        }
    }
}

/// Differences outside the contract, in this order: the application name,
/// law, reason and channel names, reason precedences, the `rule` notes of
/// aligned cases, rule variables and claims.
fn notes(old: &View<'_>, new: &View<'_>, labels: Vec<Entry>) -> Vec<Entry> {
    let (was, is) = (old.current, new.current);
    let mut notes = Vec::new();
    if was.rules.template != is.rules.template {
        let text = format!(
            "the application name changed from `{}` to `{}`",
            was.rules.template, is.rules.template
        );
        let entry = Entry::new("application", "", "changed", text);
        notes.push(entry.values(json!(was.rules.template), json!(is.rules.template)));
    }
    let law_names = |declarations: &Declarations| -> BTreeMap<u32, String> {
        declarations
            .laws
            .iter()
            .map(|law| (law.id, law.name.clone()))
            .collect()
    };
    let reason_names = |declarations: &Declarations| -> BTreeMap<u32, String> {
        declarations
            .notes
            .reasons
            .iter()
            .map(|(id, (name, _))| (*id, name.clone()))
            .collect()
    };
    let (old_notes, new_notes) = (&was.declarations.notes, &is.declarations.notes);
    for (item, label, before, after) in [
        (
            "law-name",
            "law",
            law_names(was.declarations),
            law_names(is.declarations),
        ),
        (
            "reason-name",
            "reason",
            reason_names(was.declarations),
            reason_names(is.declarations),
        ),
        (
            "channel-name",
            "channel",
            old_notes.channels.clone(),
            new_notes.channels.clone(),
        ),
    ] {
        for (id, was_name) in &before {
            if let Some(is_name) = after.get(id)
                && was_name != is_name
            {
                let text = format!("{label} {id} is now named `{is_name}` (was `{was_name}`)");
                let entry = Entry::new(item, id, "renamed", text);
                notes.push(entry.values(json!(was_name), json!(is_name)));
            }
        }
    }
    for (id, (_, was_precedence)) in &old_notes.reasons {
        if let Some((_, is_precedence)) = new_notes.reasons.get(id)
            && was_precedence != is_precedence
        {
            let text =
                format!("reason {id} now has precedence {is_precedence} (was {was_precedence})");
            let entry = Entry::new("reason-precedence", id, "changed", text);
            notes.push(entry.values(json!(was_precedence), json!(is_precedence)));
        }
    }
    notes.extend(labels);
    notes.extend(variables(&was.rules.variables, &is.rules.variables));
    notes.extend(claims(&old_notes.claims, &new_notes.claims));
    notes
}

/// Rule variables by name; a removed and an added variable with one
/// definition are one renamed variable.
fn variables(before: &BTreeMap<String, Ast>, after: &BTreeMap<String, Ast>) -> Vec<Entry> {
    let mut notes = Vec::new();
    let mut added: Vec<(&String, &Ast)> = after
        .iter()
        .filter(|(name, _)| !before.contains_key(*name))
        .collect();
    for (name, definition) in before {
        match after.get(name) {
            Some(is) if is != definition => {
                let (was, is) = (expr::render(definition), expr::render(is));
                let text = format!("variable `{name}` changed from `{was}` to `{is}`");
                notes.push(
                    Entry::new("variable", name, "changed", text).values(json!(was), json!(is)),
                );
            }
            Some(_) => {}
            None => match added.iter().position(|(_, other)| *other == definition) {
                Some(position) => {
                    let (renamed, _) = added.remove(position);
                    let text = format!("variable `{name}` was renamed `{renamed}`");
                    let entry = Entry::new("variable", name, "renamed", text);
                    notes.push(entry.values(json!(name), json!(renamed)));
                }
                None => {
                    let text = format!("variable `{name}` was removed");
                    let entry = Entry::new("variable", name, "removed", text);
                    notes.push(entry.was(json!(expr::render(definition))));
                }
            },
        }
    }
    for (name, definition) in added {
        let definition = expr::render(definition);
        let text = format!("variable `{name}` was added: `{definition}`");
        notes.push(Entry::new("variable", name, "added", text).is(json!(definition)));
    }
    notes
}

fn claims(before: &[ClaimDecl], after: &[ClaimDecl]) -> Vec<Entry> {
    let by_id = |claims: &[ClaimDecl]| -> BTreeMap<u32, ClaimDecl> {
        claims
            .iter()
            .map(|claim| (claim.id().get(), claim.clone()))
            .collect()
    };
    let (before, after) = (by_id(before), by_id(after));
    union(before.keys(), after.keys())
        .into_iter()
        .filter_map(|id| {
            let (change, claim) = match (before.get(&id), after.get(&id)) {
                (Some(claim), None) => ("removed", claim),
                (None, Some(claim)) => ("added", claim),
                (Some(was), Some(is)) if was != is => ("changed", is),
                _ => return None,
            };
            let text = format!("claim {id} `{}` was {change}", claim.name().as_str());
            Some(Entry::new("claim", id, change, text))
        })
        .collect()
}

/// The steps of aligning two sequences in order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Aligned {
    /// Equal items, at their old and new positions.
    Same(usize, usize),
    /// Unequal items paired within one run of unaligned items.
    Changed(usize, usize),
    Removed(usize),
    Added(usize),
}

/// Aligns `old` with `new` along a longest common subsequence of equal
/// items. Within each run of items between two aligned ones, the i-th old
/// item pairs with the i-th new one as changed, and the rest are removed or
/// added. Deterministic: a tie removes an old item before adding a new one.
fn align<T: PartialEq>(old: &[T], new: &[T]) -> Vec<Aligned> {
    let (rows, columns) = (old.len(), new.len());
    // longest[i][j]: the longest common subsequence of old[i..] and new[j..].
    let mut longest = vec![vec![0_usize; columns + 1]; rows + 1];
    for i in (0..rows).rev() {
        for j in (0..columns).rev() {
            longest[i][j] = if old[i] == new[j] {
                longest[i + 1][j + 1] + 1
            } else {
                longest[i + 1][j].max(longest[i][j + 1])
            };
        }
    }
    let mut steps = Vec::new();
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    let flush = |steps: &mut Vec<Aligned>, removed: &mut Vec<usize>, added: &mut Vec<usize>| {
        let pairs = removed.len().min(added.len());
        steps.extend(
            removed
                .iter()
                .zip(added.iter())
                .map(|(old, new)| Aligned::Changed(*old, *new)),
        );
        steps.extend(removed[pairs..].iter().map(|old| Aligned::Removed(*old)));
        steps.extend(added[pairs..].iter().map(|new| Aligned::Added(*new)));
        removed.clear();
        added.clear();
    };
    let (mut i, mut j) = (0, 0);
    while i < rows || j < columns {
        if i < rows && j < columns && old[i] == new[j] {
            flush(&mut steps, &mut removed, &mut added);
            steps.push(Aligned::Same(i, j));
            i += 1;
            j += 1;
        } else if j == columns || (i < rows && longest[i + 1][j] >= longest[i][j + 1]) {
            removed.push(i);
            i += 1;
        } else {
            added.push(j);
            j += 1;
        }
    }
    flush(&mut steps, &mut removed, &mut added);
    steps
}

/// What changed in a case's deliveries.
fn deliveries(was: &[DeliveryContent], is: &[DeliveryContent]) -> String {
    if was.len() != is.len() {
        return format!("its deliveries changed from {} to {}", was.len(), is.len());
    }
    let changed: Vec<String> = was
        .iter()
        .zip(is)
        .enumerate()
        .filter(|(_, (was, is))| was != is)
        .map(|(index, (was, is))| {
            let fields: Vec<&str> = [
                ("ordinal", was.ordinal != is.ordinal),
                ("channel", was.channel != is.channel),
                ("destination", was.destination != is.destination),
                ("payload", was.payload != is.payload),
                ("idempotency ordinal", was.idempotency != is.idempotency),
            ]
            .into_iter()
            .filter_map(|(field, differs)| differs.then_some(field))
            .collect();
            format!("delivery {index} changed its {}", fields.join(", "))
        })
        .collect();
    format!("its {}", changed.join("; its "))
}

/// `case 12 "execute: tier 1 and above need the CFO"`.
fn case_label(index: usize, case: &Case) -> String {
    match &case.rule {
        Some(rule) => format!("case {index} {}", rule_text(Some(rule))),
        None => format!("case {index}"),
    }
}

fn rule_text(rule: Option<&str>) -> String {
    rule.map_or_else(
        || "without a label".to_owned(),
        |rule| json!(rule).to_string(),
    )
}

/// `a Reject with reason 201 `wrong_status``, or `an Accept`.
fn decision(declarations: &Declarations, case: &Case) -> String {
    match case.reason {
        Some(reason) => format!(
            "a {} with {}",
            case.class.name(),
            reason_label(declarations, reason)
        ),
        None => format!("an {}", case.class.name()),
    }
}

fn case_json(index: usize, case: &Case) -> Value {
    json!({
        "index": index, "rule": case.rule, "when": expr::render(&case.when),
        "class": case.class.name(), "reason": case.reason
    })
}

fn law_json(declarations: &Declarations, law: &Law) -> Value {
    json!({
        "kind": law.kind.name(), "scope": scope_text(law.scope, law.genesis),
        "formula": declared_formula(declarations, law.id).map(expr::render)
    })
}

/// `reason 208 `over_daily_limit``.
fn reason_label(declarations: &Declarations, id: u32) -> String {
    match declarations.notes.reasons.get(&id) {
        Some((name, _)) => format!("reason {id} `{name}`"),
        None => format!("reason {id}"),
    }
}

/// `channel 301 `alert``.
fn channel_label(declarations: &Declarations, id: u32) -> String {
    match declarations.notes.channels.get(&id) {
        Some(name) => format!("channel {id} `{name}`"),
        None => format!("channel {id}"),
    }
}

/// A declared law by its name, a generated one by what it holds.
fn law_label(declarations: &Declarations, law: &Law) -> String {
    if let Some(declared) = declarations
        .laws
        .iter()
        .find(|declared| declared.id == law.id)
    {
        return format!("law {} `{}`", law.id, declared.name);
    }
    let id = law.id;
    match law.kind {
        LawKind::DecisionConformance => {
            format!("generated law {id} (every decision follows the case table)")
        }
        LawKind::InitialCondition => format!("generated law {id} (the genesis state)"),
        LawKind::CommittedFailureEffects => {
            format!("framework law {id} (no committed failure is lawful)")
        }
        _ => format!("framework law {id} (a reject changes nothing)"),
    }
}

fn declared_formula(declarations: &Declarations, id: u32) -> Option<&Ast> {
    declarations
        .laws
        .iter()
        .find(|law| law.id == id)
        .map(|law| &law.formula)
}

/// The scope as `project.zeno` writes it, such as `on commit, genesis`.
fn scope_text(scope: LawScope, genesis: bool) -> String {
    let scope = match scope {
        LawScope::Always => "on any",
        LawScope::Accept => "on accept",
        LawScope::Reject => "on reject",
        LawScope::CommittedFailure => "on failure",
        LawScope::Committing => "on commit",
    };
    if genesis {
        format!("{scope}, genesis")
    } else {
        scope.to_owned()
    }
}

/// `105 `Tier``.
fn type_name(declarations: &Declarations, id: u32) -> String {
    match declarations.types.get(&id) {
        Some(declared) => format!("{id} `{}`", declared.name),
        None => id.to_string(),
    }
}

/// `state field`, `command field`, `context field`, or a field of a payload
/// or other record type.
fn field_label(declarations: &Declarations, owner: u32) -> String {
    match owner {
        STATE => "state field".to_owned(),
        COMMAND => "command field".to_owned(),
        CONTEXT => "context field".to_owned(),
        _ if declarations
            .channels
            .iter()
            .any(|channel| channel.payload == owner) =>
        {
            format!("payload field of type {}", type_name(declarations, owner))
        }
        _ => format!("field of type {}", type_name(declarations, owner)),
    }
}

/// What has type `id`, as a parenthesised list, or nothing when nothing does.
fn uses(declarations: &Declarations, id: u32) -> String {
    let mut uses = Vec::new();
    match id {
        STATE => uses.push("the state".to_owned()),
        COMMAND => uses.push("the command".to_owned()),
        CONTEXT => uses.push("the context".to_owned()),
        _ => {}
    }
    for (owner, declared) in &declarations.types {
        if let Form::Record(fields) = &declared.form {
            for field in fields.iter().filter(|field| field.type_id == id) {
                uses.push(format!(
                    "{} {} `{}`",
                    field_label(declarations, *owner),
                    field.id,
                    field.name
                ));
            }
        }
    }
    for channel in &declarations.channels {
        for (role, type_id) in [
            ("destination", channel.destination),
            ("payload", channel.payload),
        ] {
            if type_id == id {
                uses.push(format!(
                    "the {role} of {}",
                    channel_label(declarations, channel.id)
                ));
            }
        }
    }
    if uses.is_empty() {
        String::new()
    } else {
        format!(" ({})", uses.join(", "))
    }
}

/// A state field's name and type.
fn state_field(declarations: &Declarations, field: u16) -> Option<(&str, u32)> {
    declarations
        .state_fields()
        .ok()?
        .iter()
        .find(|declared| declared.id == field)
        .map(|declared| (declared.name.as_str(), declared.type_id))
}

/// A genesis value as written, a sum's variant with its name.
fn constant(declarations: &Declarations, type_id: u32, value: Constant) -> String {
    let variant = |value: i128| match declarations.types.get(&type_id).map(|t| &t.form) {
        Some(Form::Sum(variants)) => variants
            .iter()
            .find(|variant| i128::from(variant.id) == value)
            .map(|variant| format!("{value} `{}`", variant.name)),
        _ => None,
    };
    match value {
        Constant::Bool(value) => value.to_string(),
        Constant::Int(value) => variant(value).unwrap_or_else(|| value.to_string()),
    }
}
