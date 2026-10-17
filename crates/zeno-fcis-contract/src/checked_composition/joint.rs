//! The composed-domain joint step, and its public API.
//!
//! One global step runs the components in merge order on the same
//! pre-state. A component whose `participates` formula does not hold is
//! skipped. A participating component's command and context fields are its
//! bindings' values, and a read-only component's state is its state
//! bindings' values, each read from the request and from earlier
//! components' pre-state. Its bound Authority decides that local input:
//!
//! - an Accept successor stays provisional;
//! - a Reject, or any refusal, ends the step and nothing changes. A refusal
//!   is the Authority's (an input or successor outside its field domains, a
//!   violated law, an arithmetic or meter refusal), a binding that has no
//!   value or lies outside its field's domain, or a committing decision of a
//!   read-only component that changes its state;
//! - the first CommittedFailure commits the provisional successors up to and
//!   including its own, then ends the step;
//! - when every participant accepts, every successor commits.
//!
//! This follows the composed-domain oracle retained in
//! `verification/kernel-laws/src/oracle/domain` in merge order, provisional
//! Accepts, a Reject that changes nothing, and a CommittedFailure that
//! commits its prefix. It differs in three ways: that oracle has no
//! participation formula; it reports a refusal as an error, where this step
//! ends with no change; and it routes one component's outputs to later
//! components of the same step through ports, where a binding here reads
//! earlier components' pre-state.
//!
//! [`JointComposition`] binds every component once, from in-memory sources,
//! and decides every input of each component's complete local domain with
//! its bound Authority when it is built; [`JointComposition::joint_step`]
//! looks those decisions up, and is pure and deterministic.

use std::cell::Cell as Counter;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::DomainSources;
use super::domain::{Component, Composition, FieldDomain, Read};
use super::formula::{Axis, Delivered, Index, TruthTable, ValueTable, product};
use super::leaf::{for_each_tuple, sha256};
use super::refusal::{AuthorityRefusal, ExecutionRefusal, LawRefusal, Obligation, Refusal, Stage};
use crate::internals::policy;
use crate::internals::review::evaluate::Framer;
use zeno_fcis_synthesis::finite::v2_authority::PublicationOutcome;
use zeno_fcis_synthesis::finite::v2_composition as c;

/// Why a component refused a step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RefusedClass {
    /// A binding has no value.
    BindingUndefined,
    /// A binding's value lies outside its field's domain.
    BindingOutsideDomain,
    /// A committing decision of a read-only component changes its state.
    ReadOnlyStateChanged,
    /// An applicable law refused the decision.
    Law,
    /// A value left its declared domain.
    Domain,
    /// Checked arithmetic overflowed.
    Arithmetic,
    /// The shared meter refused.
    Meter,
    /// The input was not admitted.
    Input,
    /// Any other refusal of the Authority.
    Other,
}

impl RefusedClass {
    /// The class's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::BindingUndefined => "binding-undefined",
            Self::BindingOutsideDomain => "binding-outside-domain",
            Self::ReadOnlyStateChanged => "read-only-state-changed",
            Self::Law => "law",
            Self::Domain => "domain",
            Self::Arithmetic => "arithmetic",
            Self::Meter => "meter",
            Self::Input => "input",
            Self::Other => "other",
        }
    }

    /// The class of an Authority refusal, read from its variants.
    const fn of(refusal: AuthorityRefusal) -> Self {
        match refusal {
            AuthorityRefusal::Law {
                refusal: LawRefusal::Meter(_),
                ..
            }
            | AuthorityRefusal::Execution(ExecutionRefusal::Meter(_)) => Self::Meter,
            AuthorityRefusal::Law { .. } => Self::Law,
            AuthorityRefusal::Execution(
                ExecutionRefusal::InputDomain | ExecutionRefusal::OutputDomain,
            )
            | AuthorityRefusal::Schema
            | AuthorityRefusal::Output
            | AuthorityRefusal::Decision => Self::Domain,
            AuthorityRefusal::Execution(ExecutionRefusal::Arithmetic) => Self::Arithmetic,
            AuthorityRefusal::Ingress(_)
            | AuthorityRefusal::Frame(_)
            | AuthorityRefusal::Binding => Self::Input,
            _ => Self::Other,
        }
    }

    const fn code(self) -> u8 {
        match self {
            Self::BindingUndefined => 0,
            Self::BindingOutsideDomain => 1,
            Self::ReadOnlyStateChanged => 2,
            Self::Law => 3,
            Self::Domain => 4,
            Self::Arithmetic => 5,
            Self::Meter => 6,
            Self::Input => 7,
            Self::Other => 8,
        }
    }
}

/// How a joint step ended. Components are named as the manifest names them.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OutcomeKind {
    /// Every participant accepted; every successor committed.
    Accept,
    /// A component rejected; nothing changed.
    Reject {
        /// The rejecting component.
        component: String,
        /// Its reason ID.
        reason: Option<u32>,
    },
    /// A component refused; nothing changed.
    Refused {
        /// The refusing component.
        component: String,
        /// Why.
        class: RefusedClass,
    },
    /// A component committed a failure: the successors up to and including
    /// its own committed.
    CommittedFailure {
        /// The failing component.
        component: String,
        /// Its reason ID.
        reason: Option<u32>,
    },
}

/// The outcome of one joint step and the committed global successor, which
/// equals the pre-state after a Reject or a refusal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JointOutcome {
    /// How the step ended.
    pub kind: OutcomeKind,
    /// The committed global state, in the layout of
    /// [`JointComposition::state_layout`].
    pub successor: Vec<i128>,
}

impl JointOutcome {
    /// The outcome as JSON.
    #[must_use]
    pub fn json(&self) -> Value {
        let kind = match &self.kind {
            OutcomeKind::Accept => json!({"kind": "Accept"}),
            OutcomeKind::Reject { component, reason } => {
                json!({"kind": "Reject", "component": component, "reason": reason})
            }
            OutcomeKind::Refused { component, class } => {
                json!({"kind": "Refused", "component": component, "class": class.name()})
            }
            OutcomeKind::CommittedFailure { component, reason } => {
                json!({"kind": "CommittedFailure", "component": component, "reason": reason})
            }
        };
        json!({"outcome": kind, "successor": self.successor})
    }
}

/// One reachable (global state, request) pair and its joint step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JointRow {
    /// The global pre-state.
    pub state: Vec<i128>,
    /// The request.
    pub request: Vec<i128>,
    /// The step.
    pub outcome: JointOutcome,
}

/// One slot of the global state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSlot {
    /// The owning component.
    pub component: String,
    /// The state field's name.
    pub field: String,
    /// Its values.
    pub domain: FieldDomain,
}

/// One request field.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestSlot {
    /// The field's name.
    pub name: String,
    /// Its values.
    pub domain: FieldDomain,
}

/// A global state or request that does not fit the layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JointInputError {
    /// What does not fit.
    pub reason: String,
}

impl std::fmt::Display for JointInputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for JointInputError {}

/// A component's decision on one local input, as its bound Authority made
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Decided {
    Accept,
    Reject,
    CommittedFailure,
    Refused(RefusedClass),
}

/// Every decision of a component on its complete local domain, in
/// enumeration order, the last position fastest.
#[derive(Debug)]
pub(crate) struct Decisions {
    index: Index,
    kinds: Vec<Decided>,
    reasons: Vec<Option<u32>>,
    /// The successor of each committing row, `width` values each; zeros
    /// elsewhere.
    posts: Vec<i128>,
    width: usize,
    /// The SHA-256 chained over every row.
    pub(crate) digest: [u8; 32],
}

impl Decisions {
    /// The decision on a local tuple: its kind, reason and successor.
    pub(crate) fn get(&self, tuple: &[i128]) -> Option<(Decided, Option<u32>, &[i128])> {
        let at = self.index.of(tuple)?;
        Some((
            *self.kinds.get(at)?,
            self.reasons[at],
            &self.posts[at * self.width..(at + 1) * self.width],
        ))
    }

    /// The decision at enumeration position `at`.
    pub(crate) fn at(&self, at: usize) -> (Decided, Option<u32>, &[i128]) {
        (
            self.kinds[at],
            self.reasons[at],
            &self.posts[at * self.width..(at + 1) * self.width],
        )
    }
}

/// The Authority's verdict on a component's declared genesis.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Genesis {
    /// A read-only component's state is its bindings', not its genesis.
    ReadOnly,
    Committed,
    Refused(Value),
    NotCommitted,
}

/// Decides every input of a component's complete local domain, and its
/// genesis, with its bound Authority; each decision is counted in
/// `decisions`. The domain's size is checked before anything is built.
pub(crate) fn decide_all(
    component: &Component,
    decisions_made: &Counter<u64>,
) -> Result<(Decisions, Genesis), Refusal> {
    if component
        .local_domain_size()
        .is_none_or(|rows| rows > u128::from(super::domain::MAX_OBLIGATION))
    {
        return Err(Refusal::new(
            Stage::Local,
            Obligation::CompleteEnumeration,
            format!("component `{}`", component.name),
            "the local domain exceeds the obligation ceiling; it is refused, not sampled",
        ));
    }
    let contract = component.contract()?;
    let values: Vec<Vec<i128>> = component
        .fields
        .iter()
        .map(|field| field.domain.values())
        .collect();
    let axes: Vec<Axis> = values
        .iter()
        .map(|values| Axis {
            values: values.clone(),
        })
        .collect();
    let rows = product(&axes);
    let width = component.state_width;
    let state_ids: Vec<Option<u16>> = component
        .bound
        .positions
        .iter()
        .take(width)
        .map(|position| position.field)
        .collect();
    policy::with_authority(&contract, &component.bound.schema, |authority| {
        let framer = Framer::new(&component.bound.positions, &contract);
        let genesis = if component.owns {
            let mut tuple = component.genesis.clone();
            tuple.extend(values[width..].iter().map(|values| values[0]));
            let frames = framer.frame(&tuple);
            match authority.publish_genesis(&frames[0]) {
                PublicationOutcome::Commit(_) => Genesis::Committed,
                PublicationOutcome::Refused { evaluation, error } => {
                    Genesis::Refused(AuthorityRefusal::of(&error, evaluation.diagnostics()).json())
                }
                _ => Genesis::NotCommitted,
            }
        } else {
            Genesis::ReadOnly
        };
        let mut kinds = Vec::with_capacity(rows);
        let mut reasons = Vec::with_capacity(rows);
        let mut posts = Vec::with_capacity(rows * width);
        let mut digest = sha256(b"ZFCIS-COMPOSED-DOMAIN-DECISIONS\0");
        let mut record = Vec::new();
        for_each_tuple::<()>(&values, |tuple| {
            decisions_made.set(decisions_made.get() + 1);
            let frames = framer.frame(tuple);
            let evaluation = authority.evaluate(c::Raw {
                state: &frames[0],
                command: &frames[1],
                context: &frames[2],
            });
            let (kind, reason) = match evaluation.result() {
                Ok(candidate) => {
                    let kind = match candidate.class() {
                        c::Class::Accept => Decided::Accept,
                        c::Class::Reject => Decided::Reject,
                        c::Class::CommittedFailure => Decided::CommittedFailure,
                        _ => Decided::Refused(RefusedClass::Other),
                    };
                    if matches!(kind, Decided::Accept | Decided::CommittedFailure) {
                        let by_id: BTreeMap<u16, i128> = candidate
                            .post()
                            .iter()
                            .filter_map(|field| Some((field.id, number(field.value)?)))
                            .collect();
                        for (slot, id) in state_ids.iter().enumerate() {
                            let value = id.and_then(|id| by_id.get(&id).copied());
                            posts.push(value.unwrap_or(tuple[slot]));
                        }
                        (kind, candidate.reason())
                    } else {
                        posts.extend(std::iter::repeat_n(0, width));
                        (kind, candidate.reason())
                    }
                }
                Err(refusal) => {
                    posts.extend(std::iter::repeat_n(0, width));
                    let kind = AuthorityRefusal::of(&refusal, evaluation.diagnostics());
                    (Decided::Refused(RefusedClass::of(kind)), None)
                }
            };
            record.clear();
            record.extend_from_slice(&digest);
            for value in tuple {
                record.extend_from_slice(&value.to_be_bytes());
            }
            record.push(match kind {
                Decided::Accept => 0,
                Decided::Reject => 1,
                Decided::CommittedFailure => 2,
                Decided::Refused(class) => 3 + class.code(),
            });
            record.extend_from_slice(&reason.unwrap_or(u32::MAX).to_be_bytes());
            for value in &posts[posts.len() - width..] {
                record.extend_from_slice(&value.to_be_bytes());
            }
            digest = sha256(&record);
            kinds.push(kind);
            reasons.push(reason);
            Ok(())
        })
        .ok();
        (
            Decisions {
                index: Index::new(axes),
                kinds,
                reasons,
                posts,
                width,
                digest,
            },
            genesis,
        )
    })
    .map_err(|error| {
        Refusal::new(
            Stage::Binding,
            Obligation::OrdinaryContractRoute,
            format!("component `{}` {}", component.name, error.place()),
            error.reason(),
        )
    })
}

/// The number a successor's value denotes: 0 or 1 for a boolean, the
/// integer, or the variant ID.
fn number(atom: c::Atom<'_>) -> Option<i128> {
    match atom {
        c::Atom::Bool(value) => Some(i128::from(value)),
        c::Atom::I128(value) => i128::try_from(value).ok(),
        c::Atom::U128(value) => i128::try_from(value).ok(),
        c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => Some(i128::from(variant)),
        _ => None,
    }
}

/// Every formula table and decision table of one component.
#[derive(Debug)]
pub(crate) struct ComponentTables {
    pub(crate) participates: TruthTable,
    pub(crate) bindings: Vec<ValueTable>,
    pub(crate) state_bindings: Vec<ValueTable>,
    pub(crate) assume: TruthTable,
    pub(crate) invariant: TruthTable,
    pub(crate) decisions: Decisions,
    pub(crate) genesis: Genesis,
}

/// Every table of a composition.
#[derive(Debug)]
pub(crate) struct Tables {
    pub(crate) components: Vec<ComponentTables>,
    pub(crate) couplings: Vec<TruthTable>,
}

/// The evaluations a check made: decisions by the bound Authorities and
/// formula values by the library's evaluators, each counted as it happens.
#[derive(Debug, Default)]
pub(crate) struct Work {
    pub(crate) decisions: Counter<u64>,
    pub(crate) formula_values: Counter<u64>,
}

/// Every table a check or the joint step builds, with its exact number of
/// cells: each component's decisions over its local domain and each
/// formula over the product of the domains it reads. `None` where the
/// size overflows `u128`.
pub(crate) fn table_sizes(composition: &Composition) -> Vec<(String, Option<u128>)> {
    let mut out = Vec::new();
    for component in &composition.components {
        let name = &component.name;
        out.push((
            format!("decisions of `{name}`"),
            component.local_domain_size(),
        ));
        out.push((
            format!("participation of `{name}`"),
            composition.formula_size(component, &component.participates),
        ));
        let width = component.state_width;
        for (at, resolved) in component.bindings.iter().enumerate() {
            out.push((
                format!("binding of `{name}.{}`", component.fields[width + at].name),
                composition.formula_size(component, resolved),
            ));
        }
        for (at, resolved) in component.state_bindings.iter().enumerate() {
            out.push((
                format!("binding of `{name}.{}`", component.fields[at].name),
                composition.formula_size(component, resolved),
            ));
        }
        out.push((
            format!("assumption of `{name}`"),
            composition.formula_size(component, &component.assume),
        ));
        out.push((
            format!("invariant of `{name}`"),
            composition.formula_size(component, &component.invariant),
        ));
    }
    for coupling in &composition.couplings {
        out.push((
            format!("coupling `{}`", coupling.name),
            composition.formula_size(
                &composition.components[coupling.members[0]],
                &coupling.invariant,
            ),
        ));
    }
    out
}

/// The first table over `max_one` cells, or the total over `max_total`:
/// `Ok` with the total when every table fits.
pub(crate) fn tables_fit(
    composition: &Composition,
    max_one: u64,
    max_total: u64,
) -> Result<u128, (String, Option<u128>, u64)> {
    let mut total = 0u128;
    for (place, size) in table_sizes(composition) {
        match size {
            Some(size) if size <= u128::from(max_one) => {
                total = total.saturating_add(size);
            }
            other => return Err((place, other, max_one)),
        }
    }
    if total > u128::from(max_total) {
        return Err(("every table".to_owned(), Some(total), max_total));
    }
    Ok(total)
}

impl Tables {
    /// Builds every table: each formula over the complete product of the
    /// domains it reads, each component's decisions over its complete local
    /// domain. Every table's size is checked before it is built, and every
    /// evaluation is counted in `work`.
    ///
    /// # Errors
    /// A table over the obligation ceiling, or a compiler or binding
    /// refusal.
    pub(crate) fn build(composition: &Composition, work: &Work) -> Result<Self, Refusal> {
        let too_large = |owner: &str| {
            Refusal::new(
                Stage::Local,
                Obligation::CompleteEnumeration,
                owner.to_owned(),
                "a formula's table exceeds the obligation ceiling; it is refused, not sampled",
            )
        };
        let compile = |owner: &str, reason: String| {
            Refusal::new(
                Stage::Manifest,
                Obligation::DefinedPredicate,
                owner.to_owned(),
                reason,
            )
        };
        let fits = |owner: &Component, resolved: &super::domain::Resolved| {
            composition
                .formula_size(owner, resolved)
                .is_some_and(|size| size <= u128::from(super::domain::MAX_OBLIGATION))
        };
        let mut components = Vec::with_capacity(composition.components.len());
        for component in &composition.components {
            let owner = format!("component `{}`", component.name);
            let declarations = &component.bound.declarations;
            let axes = |resolved: &super::domain::Resolved| {
                if !fits(component, resolved) {
                    return Err(too_large(&owner));
                }
                Ok(composition.axes(component, resolved))
            };
            let truth = |resolved: &super::domain::Resolved| {
                resolved
                    .formula
                    .truth_table(declarations, axes(resolved)?, &work.formula_values)
                    .map_err(|reason| compile(&owner, reason))
            };
            let value = |resolved: &super::domain::Resolved, field: usize| {
                resolved
                    .formula
                    .value_table(
                        axes(resolved)?,
                        &component.fields[field].domain,
                        &work.formula_values,
                    )
                    .map_err(|reason| compile(&owner, reason))
            };
            let width = component.state_width;
            let participates = truth(&component.participates)?;
            let bindings = component
                .bindings
                .iter()
                .enumerate()
                .map(|(at, resolved)| value(resolved, width + at))
                .collect::<Result<Vec<_>, _>>()?;
            let state_bindings = component
                .state_bindings
                .iter()
                .enumerate()
                .map(|(at, resolved)| value(resolved, at))
                .collect::<Result<Vec<_>, _>>()?;
            let assume = truth(&component.assume)?;
            let invariant = truth(&component.invariant)?;
            let (decisions, genesis) = decide_all(component, &work.decisions)?;
            components.push(ComponentTables {
                participates,
                bindings,
                state_bindings,
                assume,
                invariant,
                decisions,
                genesis,
            });
        }
        let mut couplings = Vec::with_capacity(composition.couplings.len());
        for coupling in &composition.couplings {
            let owner = &composition.components[coupling.members[0]];
            let place = format!("coupling `{}`", coupling.name);
            if !fits(owner, &coupling.invariant) {
                return Err(too_large(&place));
            }
            let axes = composition.axes(owner, &coupling.invariant);
            let table = coupling
                .invariant
                .formula
                .truth_table(&owner.bound.declarations, axes, &work.formula_values)
                .map_err(|reason| compile(&place, reason))?;
            couplings.push(table);
        }
        Ok(Self {
            components,
            couplings,
        })
    }
}

/// The values a formula reads, from the request, the global pre-state (one
/// vector per component, empty for a read-only one) and a local tuple.
pub(crate) fn gather(
    reads: &[Read],
    request: &[i128],
    states: &[Vec<i128>],
    local: &[i128],
) -> Vec<i128> {
    reads
        .iter()
        .map(|read| match read {
            Read::Request(at) => request[*at],
            Read::State { component, field } => states[*component][*field],
            Read::Local(at) => local[*at],
        })
        .collect()
}

/// What one component does in a step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Local {
    Skip,
    Reject(Option<u32>),
    Refused(RefusedClass),
    /// Accept or CommittedFailure, with its successor; a read-only
    /// component's is its unchanged state.
    Commit {
        failure: bool,
        reason: Option<u32>,
        post: Vec<i128>,
    },
}

/// The delivered state and inputs of a participating component: its local
/// tuple, or the refusal of a binding.
pub(crate) fn delivered(
    component: &Component,
    tables: &ComponentTables,
    request: &[i128],
    pre: &[Vec<i128>],
    own: &[i128],
) -> Result<Vec<i128>, RefusedClass> {
    let mut tuple = Vec::with_capacity(component.fields.len());
    let deliver = |resolved: &super::domain::Resolved, table: &ValueTable| match table
        .value(&gather(&resolved.reads, request, pre, &[]))
    {
        Delivered::Value(value) => Ok(value),
        Delivered::Outside(_) => Err(RefusedClass::BindingOutsideDomain),
        Delivered::Undefined => Err(RefusedClass::BindingUndefined),
    };
    if component.owns {
        tuple.extend_from_slice(own);
    } else {
        for (resolved, table) in component.state_bindings.iter().zip(&tables.state_bindings) {
            tuple.push(deliver(resolved, table)?);
        }
    }
    for (resolved, table) in component.bindings.iter().zip(&tables.bindings) {
        tuple.push(deliver(resolved, table)?);
    }
    Ok(tuple)
}

/// Component `at`'s part of a step from the global pre-state `pre`.
pub(crate) fn local(
    composition: &Composition,
    tables: &Tables,
    at: usize,
    pre: &[Vec<i128>],
    request: &[i128],
) -> Local {
    let component = &composition.components[at];
    let own = &tables.components[at];
    if !own
        .participates
        .holds(&gather(&component.participates.reads, request, pre, &[]))
    {
        return Local::Skip;
    }
    let tuple = match delivered(component, own, request, pre, &pre[at]) {
        Ok(tuple) => tuple,
        Err(class) => return Local::Refused(class),
    };
    let Some((kind, reason, post)) = own.decisions.get(&tuple) else {
        return Local::Refused(RefusedClass::BindingOutsideDomain);
    };
    match kind {
        Decided::Refused(class) => Local::Refused(class),
        Decided::Reject => Local::Reject(reason),
        Decided::Accept | Decided::CommittedFailure => {
            let state = &tuple[..component.state_width];
            if !component.owns && post != state {
                return Local::Refused(RefusedClass::ReadOnlyStateChanged);
            }
            Local::Commit {
                failure: kind == Decided::CommittedFailure,
                reason,
                post: post.to_vec(),
            }
        }
    }
}

/// How a step ended, by component position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Ended {
    Accept,
    Reject(usize, Option<u32>),
    Refused(usize, RefusedClass),
    CommittedFailure(usize, Option<u32>),
}

/// One joint step from the global pre-state `pre`, one state vector per
/// component, empty for a read-only one.
pub(crate) fn step(
    composition: &Composition,
    tables: &Tables,
    pre: &[Vec<i128>],
    request: &[i128],
) -> (Ended, Vec<Vec<i128>>) {
    let mut after = pre.to_vec();
    for at in 0..composition.components.len() {
        match local(composition, tables, at, pre, request) {
            Local::Skip => {}
            Local::Reject(reason) => return (Ended::Reject(at, reason), pre.to_vec()),
            Local::Refused(class) => return (Ended::Refused(at, class), pre.to_vec()),
            Local::Commit {
                failure,
                reason,
                post,
            } => {
                if composition.components[at].owns {
                    after[at] = post;
                }
                if failure {
                    return (Ended::CommittedFailure(at, reason), after);
                }
            }
        }
    }
    (Ended::Accept, after)
}

/// Every request in enumeration order, the last field fastest, one at a
/// time: a request domain is never collected.
struct Odometer<'a> {
    slots: &'a [RequestSlot],
    offsets: Vec<u128>,
    done: bool,
}

impl<'a> Odometer<'a> {
    fn new(slots: &'a [RequestSlot]) -> Self {
        Self {
            slots,
            offsets: vec![0; slots.len()],
            done: false,
        }
    }

    fn next_tuple(&mut self) -> Option<Vec<i128>> {
        if self.done {
            return None;
        }
        let tuple = self
            .slots
            .iter()
            .zip(&self.offsets)
            .map(|(slot, offset)| slot.domain.nth(*offset))
            .collect();
        self.done = true;
        for (slot, offset) in self.slots.iter().zip(&mut self.offsets).rev() {
            *offset += 1;
            if *offset < slot.domain.size() {
                self.done = false;
                break;
            }
            *offset = 0;
        }
        Some(tuple)
    }
}

/// A validated composed-domain composition with every decision its
/// components' bound Authorities make, ready to step.
///
/// Built from in-memory sources: the `zeno-fcis/checked-composition/3`
/// manifest and each component's `project.zeno` and `v2/policy.json`, in
/// merge order. It checks no obligation; [`super::check_domain`] does.
///
/// ```
/// use zeno_fcis_contract::checked_composition::{
///     DomainSources, JointComposition, LeafSources, OutcomeKind,
/// };
/// macro_rules! fixture {
///     ($path:literal) => {
///         include_str!(concat!(
///             env!("CARGO_MANIFEST_DIR"),
///             "/tests/fixtures/checked-composition-3/guarded/",
///             $path
///         ))
///         .to_owned()
///     };
/// }
/// // In merge order: the guard decides first.
/// let components = [
///     LeafSources {
///         project: fixture!("guard/project.zeno"),
///         rules: fixture!("guard/v2/policy.json"),
///     },
///     LeafSources {
///         project: fixture!("budget/project.zeno"),
///         rules: fixture!("budget/v2/policy.json"),
///     },
/// ];
/// let manifest = fixture!("composition.json");
/// let joint = JointComposition::from_sources(&DomainSources {
///     manifest: &manifest,
///     components: &components,
/// })
/// .unwrap();
/// // The guard owns no state: the global state is the budget's alone.
/// assert_eq!(joint.state_layout().len(), 1);
/// let genesis = joint.genesis().to_vec();
/// // Spend 1 with authorization: both accept and the budget goes down.
/// let step = joint.joint_step(&genesis, &[1, 1]).unwrap();
/// assert_eq!(step.kind, OutcomeKind::Accept);
/// assert_eq!(step.successor, vec![genesis[0] - 1]);
/// // Without authorization the guard rejects and nothing changes.
/// let step = joint.joint_step(&genesis, &[1, 0]).unwrap();
/// assert!(matches!(step.kind, OutcomeKind::Reject { ref component, .. } if component == "guard"));
/// assert_eq!(step.successor, genesis);
/// ```
#[derive(Debug)]
pub struct JointComposition {
    composition: Composition,
    tables: Tables,
    layout: Vec<StateSlot>,
    request: Vec<RequestSlot>,
    genesis: Vec<i128>,
}

impl JointComposition {
    /// Validates the manifest, binds every component's Authority once and
    /// decides every input of each component's local domain with it. Every
    /// table's size is planned first: a table over 2^22 cells, or all of them
    /// over 2^25, is refused before anything is built.
    ///
    /// # Errors
    /// The manifest's or a component's refusal, or a table over a ceiling.
    pub fn from_sources(sources: &DomainSources<'_>) -> Result<Self, Refusal> {
        let composition = Composition::bind(sources.manifest, sources.components)?;
        // Every table's size is planned before anything is built.
        if let Err((place, size, limit)) = tables_fit(
            &composition,
            super::domain::MAX_OBLIGATION,
            super::domain::MAX_TOTAL,
        ) {
            return Err(Refusal::new(
                Stage::Local,
                Obligation::CompleteEnumeration,
                place,
                format!(
                    "{} cells exceed the limit of {limit}; refused before anything is built",
                    size.map_or_else(|| "more than 2^128".to_owned(), |size| size.to_string())
                ),
            ));
        }
        let tables = Tables::build(&composition, &Work::default())?;
        let mut layout = Vec::new();
        let mut genesis = Vec::new();
        for component in composition
            .components
            .iter()
            .filter(|component| component.owns)
        {
            for (field, value) in component.state_fields().iter().zip(&component.genesis) {
                layout.push(StateSlot {
                    component: component.name.clone(),
                    field: field.name.clone(),
                    domain: field.domain.clone(),
                });
                genesis.push(*value);
            }
        }
        let request = composition
            .manifest
            .request
            .iter()
            .map(|field| RequestSlot {
                name: field.name.clone(),
                domain: field.domain.clone(),
            })
            .collect();
        Ok(Self {
            composition,
            tables,
            layout,
            request,
            genesis,
        })
    }

    /// The components' names, in merge order.
    #[must_use]
    pub fn components(&self) -> Vec<&str> {
        self.composition
            .components
            .iter()
            .map(|component| component.name.as_str())
            .collect()
    }

    /// The global state's slots: each owning component's state fields, in
    /// merge order, then field order. A read-only component has none.
    #[must_use]
    pub fn state_layout(&self) -> &[StateSlot] {
        &self.layout
    }

    /// The request's fields, in manifest order.
    #[must_use]
    pub fn request_layout(&self) -> &[RequestSlot] {
        &self.request
    }

    /// The global genesis state: each owning component's declared genesis.
    #[must_use]
    pub fn genesis(&self) -> &[i128] {
        &self.genesis
    }

    /// One joint step. Pure and deterministic.
    ///
    /// # Errors
    /// A global state or request of the wrong length, or with a value
    /// outside its slot's domain.
    pub fn joint_step(
        &self,
        global_state: &[i128],
        request: &[i128],
    ) -> Result<JointOutcome, JointInputError> {
        let pre = self.split(global_state, request)?;
        let (ended, after) = step(&self.composition, &self.tables, &pre, request);
        Ok(self.outcome(&ended, &after))
    }

    /// Every reachable (global state, request) pair from genesis, breadth
    /// first, with its joint step: states in discovery order, requests in
    /// enumeration order, the last field fastest.
    ///
    /// # Errors
    /// More than `max_pairs` pairs: the exploration is refused, not cut.
    pub fn reachable(&self, max_pairs: u64) -> Result<Vec<JointRow>, u64> {
        let mut seen: BTreeSet<Vec<i128>> = BTreeSet::new();
        seen.insert(self.genesis.clone());
        let mut frontier = vec![self.genesis.clone()];
        let mut rows = Vec::new();
        let mut pairs = 0u64;
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for state in &frontier {
                // Requests are enumerated as they are used, never collected.
                let mut requests = Odometer::new(&self.request);
                while let Some(request) = requests.next_tuple() {
                    pairs += 1;
                    if pairs > max_pairs {
                        return Err(max_pairs);
                    }
                    let Ok(outcome) = self.joint_step(state, &request) else {
                        return Err(pairs);
                    };
                    if seen.insert(outcome.successor.clone()) {
                        next.push(outcome.successor.clone());
                    }
                    rows.push(JointRow {
                        state: state.clone(),
                        request,
                        outcome,
                    });
                }
            }
            frontier = next;
        }
        Ok(rows)
    }

    fn split(
        &self,
        global_state: &[i128],
        request: &[i128],
    ) -> Result<Vec<Vec<i128>>, JointInputError> {
        let error = |reason: String| JointInputError { reason };
        if global_state.len() != self.layout.len() {
            return Err(error(format!(
                "the global state has {} values; the layout has {} slots",
                global_state.len(),
                self.layout.len()
            )));
        }
        if request.len() != self.request.len() {
            return Err(error(format!(
                "the request has {} values; it has {} fields",
                request.len(),
                self.request.len()
            )));
        }
        for (slot, value) in self.layout.iter().zip(global_state) {
            if !slot.domain.contains(*value) {
                return Err(error(format!(
                    "{}.{} = {value} lies outside its domain",
                    slot.component, slot.field
                )));
            }
        }
        for (slot, value) in self.request.iter().zip(request) {
            if !slot.domain.contains(*value) {
                return Err(error(format!(
                    "request field {} = {value} lies outside its domain",
                    slot.name
                )));
            }
        }
        let mut pre = Vec::with_capacity(self.composition.components.len());
        let mut rest = global_state;
        for component in &self.composition.components {
            let width = if component.owns {
                component.state_width
            } else {
                0
            };
            let (own, tail) = rest.split_at(width);
            pre.push(own.to_vec());
            rest = tail;
        }
        Ok(pre)
    }

    fn outcome(&self, ended: &Ended, after: &[Vec<i128>]) -> JointOutcome {
        let name = |at: usize| self.composition.components[at].name.clone();
        let kind = match *ended {
            Ended::Accept => OutcomeKind::Accept,
            Ended::Reject(at, reason) => OutcomeKind::Reject {
                component: name(at),
                reason,
            },
            Ended::Refused(at, class) => OutcomeKind::Refused {
                component: name(at),
                class,
            },
            Ended::CommittedFailure(at, reason) => OutcomeKind::CommittedFailure {
                component: name(at),
                reason,
            },
        };
        JointOutcome {
            kind,
            successor: after.iter().flatten().copied().collect(),
        }
    }
}
