//! The composed-domain obligations, their planner and the result.
//!
//! Three kinds of obligation, each an exhaustive finite check that never
//! enumerates the global product:
//!
//! - `local`, per component: its declared genesis lies in its field
//!   domains, the Authority commits it (its genesis laws hold) and it
//!   satisfies the invariant; and from every state that satisfies the
//!   invariant, on every input that satisfies the assumption, every
//!   committing decision (Accept or CommittedFailure) keeps the invariant.
//!   Decisions the Authority refuses commit nothing: they are counted and the
//!   first is reported, not failures. A read-only component has no genesis
//!   of its own, and every committing decision must leave its state
//!   unchanged. Size: the component's local domain.
//! - `assume`, per component: over every value of the request fields its
//!   participation and bindings read and every state of the components they
//!   read that satisfies those components' invariants, where it
//!   participates, every binding has a value in its field's domain and the
//!   delivered command and context satisfy the assumption; for a read-only
//!   component the delivered state also lies in its domains and satisfies
//!   its invariant. Size: those request fields' domains times those
//!   components' state domains.
//! - `couple`, per coupling over members K: the coupling holds at the
//!   members' genesis and every joint step, simulated over the members in
//!   merge order from member and provider states that satisfy every
//!   invariant and the coupling, keeps it. A component outside K is never
//!   enumerated: one that rejects or refuses changes nothing, and one with a
//!   CommittedFailure case may end the step just before the next member,
//!   which is checked as possible wherever such a component sits between
//!   members, an over-approximation. Size: the states of K and of the
//!   components K's participation and bindings read, times K's request
//!   fields.
//!
//! The planner computes every size exactly before anything is decided, in
//! `u128`, refusing a size that overflows. One obligation over
//! `max_obligation`, or their sum over `max_total`, refuses the whole check
//! before any decision, formula or row is evaluated. The planner also
//! counts the tables the check builds, each component's decisions and each
//! formula's values, and refuses them over the same limits. Within budget
//! every obligation is checked exhaustively; a proved obligation's
//! `evaluated` counts every row of its planned space, including rows a
//! premise makes vacuous, and a violated one stops at its first witness.

use serde_json::{Map, Value, json};

use super::domain::{Composition, MAX_OBLIGATION, MAX_TOTAL};
use super::formula::Delivered;
use super::identity::Identity;
use super::joint::{Decided, Genesis, Local, Tables, Work, gather, local, tables_fit};
use super::leaf::for_each_tuple;
use super::manifest::DOMAIN_SCHEMA;
use super::refusal::Refusal;
use super::{DOMAIN_CLAIM, DOMAIN_NONCLAIMS, DOMAIN_TRUSTED_BASE, DomainSources, hex};

/// The schema of a composed-domain result.
pub const DOMAIN_RESULT_SCHEMA: &str = "zeno-fcis/composed-domain-result/1";

/// A composed-domain verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// Every obligation holds.
    Proved,
    /// An obligation fails; the result carries the first witness.
    Violated,
    /// The planner refused the check before evaluating anything.
    Refused,
}

impl Verdict {
    /// The verdict's name in a result.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Proved => "proved",
            Self::Violated => "violated",
            Self::Refused => "refused",
        }
    }
}

/// The kind of an obligation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Local,
    Assume,
    Couple,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Assume => "assume",
            Self::Couple => "couple",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Planned,
    Proved,
    Violated,
    Refused,
}

impl Status {
    const fn name(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Proved => "proved",
            Self::Violated => "violated",
            Self::Refused => "refused",
        }
    }
}

/// One obligation: what it checks, its planned size and what its check
/// found.
#[derive(Clone, Debug)]
struct Checked {
    kind: Kind,
    subject: usize,
    name: String,
    /// `None` when the size overflows `u128`.
    size: Option<u128>,
    status: Status,
    evaluated: u64,
    admitted: u64,
    refused_decisions: u64,
    first_refusal: Option<Value>,
    witness: Option<Value>,
}

impl Checked {
    fn json(&self) -> Value {
        let mut out = json!({
            "kind": self.kind.name(),
            "subject": self.name,
            "size": self.size.map_or_else(|| "overflow".to_owned(), |size| size.to_string()),
            "status": self.status.name(),
            "evaluated": self.evaluated,
            "admitted": self.admitted,
        });
        if self.kind == Kind::Local {
            out["refused_decisions"] = json!(self.refused_decisions);
            out["first_refusal"] = self.first_refusal.clone().unwrap_or(Value::Null);
        }
        if let Some(witness) = &self.witness {
            out["witness"] = witness.clone();
        }
        out
    }

    fn violated(&mut self, witness: Value) {
        if self.status != Status::Violated {
            self.status = Status::Violated;
            self.witness = Some(witness);
        }
    }
}

/// The plan: every obligation's exact size, and the planner's refusal.
/// What the planner decided: every obligation with its size, the total, the
/// total cells of every table the check would build, and its refusal.
struct Plan {
    obligations: Vec<Checked>,
    total: Option<u128>,
    tables: Option<u128>,
    refusal: Option<Value>,
}

fn plan(composition: &Composition) -> Plan {
    let request_size = |cone: &[usize]| {
        cone.iter().try_fold(1u128, |size, at| {
            size.checked_mul(composition.manifest.request[*at].domain.size())
        })
    };
    let states_size = |components: &[usize]| {
        components.iter().try_fold(1u128, |size, at| {
            size.checked_mul(composition.components[*at].state_domain_size()?)
        })
    };
    let mut obligations = Vec::new();
    let mut add = |kind, subject, name: &str, size| {
        obligations.push(Checked {
            kind,
            subject,
            name: name.to_owned(),
            size,
            status: Status::Planned,
            evaluated: 0,
            admitted: 0,
            refused_decisions: 0,
            first_refusal: None,
            witness: None,
        });
    };
    for (at, component) in composition.components.iter().enumerate() {
        add(
            Kind::Local,
            at,
            &component.name,
            component.local_domain_size(),
        );
        let size = request_size(&component.cone)
            .zip(states_size(&component.providers))
            .and_then(|(requests, states)| requests.checked_mul(states));
        add(Kind::Assume, at, &component.name, size);
    }
    for (at, coupling) in composition.couplings.iter().enumerate() {
        let (cone, states) = coupling_spaces(composition, &coupling.members);
        let size = request_size(&cone)
            .zip(states_size(&states))
            .and_then(|(requests, states)| requests.checked_mul(states));
        add(Kind::Couple, at, &coupling.name, size);
    }
    let limits = composition.manifest.limits;
    let total = obligations.iter().try_fold(0u128, |total, obligation| {
        total.checked_add(obligation.size?)
    });
    let factor = |size: u128, limit: u64| size.div_ceil(u128::from(limit)).to_string();
    let mut refusal = None;
    for obligation in &mut obligations {
        let over = obligation
            .size
            .is_none_or(|size| size > u128::from(limits.max_obligation));
        if !over {
            continue;
        }
        obligation.status = Status::Refused;
        if refusal.is_none() {
            refusal = Some(match obligation.size {
                Some(size) => json!({
                    "reason": "obligation-over-budget",
                    "obligation": {"kind": obligation.kind.name(), "subject": obligation.name, "size": size.to_string()},
                    "limit": limits.max_obligation.to_string(),
                    "factor": factor(size, limits.max_obligation),
                }),
                None => json!({
                    "reason": "obligation-size-overflow",
                    "obligation": {"kind": obligation.kind.name(), "subject": obligation.name, "size": "overflow"},
                    "limit": limits.max_obligation.to_string(),
                    "factor": "overflow",
                }),
            });
        }
    }
    if refusal.is_none() {
        refusal = match total {
            Some(total) if total <= u128::from(limits.max_total) => None,
            Some(total) => Some(json!({
                "reason": "total-over-budget",
                "total": total.to_string(),
                "limit": limits.max_total.to_string(),
                "factor": factor(total, limits.max_total),
            })),
            None => Some(json!({
                "reason": "total-size-overflow",
                "total": "overflow",
                "limit": limits.max_total.to_string(),
                "factor": "overflow",
            })),
        };
    }
    // The tables the check builds are planned too: each component's
    // decisions and each formula's values, one evaluation per cell.
    let tables = match tables_fit(composition, limits.max_obligation, limits.max_total) {
        Ok(cells) => Some(cells),
        Err((place, size, limit)) => {
            if refusal.is_none() {
                refusal = Some(json!({
                    "reason": if place == "every table" { "tables-total-over-budget" } else { "table-over-budget" },
                    "table": place,
                    "size": size.map_or_else(|| "overflow".to_owned(), |size| size.to_string()),
                    "limit": limit.to_string(),
                    "factor": size.map_or_else(|| "overflow".to_owned(), |size| factor(size, limit)),
                }));
            }
            None
        }
    };
    Plan {
        obligations,
        total,
        tables,
        refusal,
    }
}

/// A coupling's request fields and the components whose states it ranges
/// over: its members and the components their participation and bindings
/// read, in merge order.
fn coupling_spaces(composition: &Composition, members: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mut cone = std::collections::BTreeSet::new();
    let mut states: std::collections::BTreeSet<usize> = members.iter().copied().collect();
    for member in members {
        let component = &composition.components[*member];
        cone.extend(component.cone.iter().copied());
        states.extend(component.providers.iter().copied());
    }
    (cone.into_iter().collect(), states.into_iter().collect())
}

/// The result of a composed-domain check. Fields are private; only
/// [`check_domain`] builds one.
#[derive(Debug)]
pub struct DomainResult {
    name: String,
    manifest_sha256: [u8; 32],
    commitment: [u8; 32],
    identity: Identity,
    request: Vec<Value>,
    components: Vec<Value>,
    limits: (u64, u64, bool),
    obligations: Vec<Checked>,
    total: Option<u128>,
    tables: Option<u128>,
    monolithic: [String; 3],
    verdict: Verdict,
    refusal: Option<Value>,
    decisions: u64,
    formula_cells: u64,
}

impl DomainResult {
    /// The verdict.
    #[must_use]
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// The rows every obligation evaluated, in total.
    #[must_use]
    pub fn evaluated(&self) -> u64 {
        self.obligations
            .iter()
            .map(|obligation| obligation.evaluated)
            .sum()
    }

    /// The decisions the bound Authorities made and the formula values the
    /// law evaluator decided; both zero when the planner refused.
    #[must_use]
    pub const fn work(&self) -> (u64, u64) {
        (self.decisions, self.formula_cells)
    }

    /// The commitment of the checked manifest's canonical bytes.
    #[must_use]
    pub const fn manifest_commitment(&self) -> [u8; 32] {
        self.commitment
    }

    /// The result as JSON, the same on repeat.
    #[must_use]
    pub fn json(&self) -> Value {
        let mut checker = self.identity.json();
        if let Some(fields) = checker.as_object_mut() {
            fields.insert("id".to_owned(), json!(super::domain::DOMAIN_CHECKER));
            fields.insert("version".to_owned(), json!(env!("CARGO_PKG_VERSION")));
            fields.insert("evidence_caching".to_owned(), json!("none"));
        }
        let witness = self
            .obligations
            .iter()
            .find(|obligation| obligation.status == Status::Violated)
            .map(|obligation| {
                json!({
                    "obligation": {"kind": obligation.kind.name(), "subject": obligation.name},
                    "witness": obligation.witness,
                })
            });
        json!({
            "schema": DOMAIN_RESULT_SCHEMA,
            "profile": "composed-domain",
            "verdict": self.verdict.name(),
            "manifest": {
                "name": self.name,
                "schema": DOMAIN_SCHEMA,
                "sha256": hex(&self.manifest_sha256),
                "commitment": hex(&self.commitment),
            },
            "checker": checker,
            "request": self.request,
            "components": self.components,
            "limits": {
                "max_obligation": self.limits.0.to_string(),
                "max_total": self.limits.1.to_string(),
                "lowered": self.limits.2,
                "ceilings": {"max_obligation": MAX_OBLIGATION.to_string(), "max_total": MAX_TOTAL.to_string()},
            },
            "obligations": self.obligations.iter().map(Checked::json).collect::<Vec<_>>(),
            "planned_total": self.total.map_or_else(|| "overflow".to_owned(), |total| total.to_string()),
            "planned_table_values": self.tables.map_or_else(|| "over budget".to_owned(), |cells| cells.to_string()),
            "evaluated": self.evaluated(),
            "work": {"authority_decisions": self.decisions, "formula_values": self.formula_cells},
            "monolithic": {
                "states": self.monolithic[0],
                "requests": self.monolithic[1],
                "pairs": self.monolithic[2],
                "enumerated": false,
            },
            "refusal": self.refusal,
            "witness": witness,
            "claim": DOMAIN_CLAIM,
            "trusted_base": DOMAIN_TRUSTED_BASE,
            "nonclaims": DOMAIN_NONCLAIMS,
        })
    }

    /// One line for each count a reader checks first.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "composed-domain composition {}: {}",
            self.name,
            self.verdict.name()
        )];
        for obligation in &self.obligations {
            lines.push(format!(
                "{} {}: size {}, evaluated {}, {}",
                obligation.kind.name(),
                obligation.name,
                obligation
                    .size
                    .map_or_else(|| "overflow".to_owned(), |size| size.to_string()),
                obligation.evaluated,
                obligation.status.name()
            ));
        }
        lines.push(format!(
            "planned {} of at most {}; evaluated {}",
            self.total
                .map_or_else(|| "overflow".to_owned(), |total| total.to_string()),
            self.limits.1,
            self.evaluated()
        ));
        lines.push(format!(
            "monolithic product, not enumerated: {} states x {} requests = {} pairs",
            self.monolithic[0], self.monolithic[1], self.monolithic[2]
        ));
        if let Some(refusal) = &self.refusal {
            lines.push(format!("refused before evaluation: {refusal}"));
        }
        if let Some(obligation) = self
            .obligations
            .iter()
            .find(|obligation| obligation.status == Status::Violated)
        {
            lines.push(format!(
                "first witness, {} {}: {}",
                obligation.kind.name(),
                obligation.name,
                obligation.witness.clone().unwrap_or(Value::Null)
            ));
        }
        lines.push(format!("manifest commitment {}", hex(&self.commitment)));
        lines.push(format!("checker identity {}", hex(&self.identity.digest())));
        lines
    }
}

/// Checks a composed-domain composition: validates and binds it, plans every
/// obligation and, within budget, checks each exhaustively.
///
/// # Errors
/// The manifest's or a component's refusal at validation. A planner
/// refusal is a result with verdict [`Verdict::Refused`], not an error.
pub fn check_domain(sources: &DomainSources<'_>) -> Result<DomainResult, Refusal> {
    run_check(sources, false)
}

/// The check with its tables built before the plan: a planted defect that
/// shows the evaluation counters see evaluation done before a refusal.
#[cfg(test)]
pub(crate) fn check_domain_building_tables_first(
    sources: &DomainSources<'_>,
) -> Result<DomainResult, Refusal> {
    run_check(sources, true)
}

fn run_check(sources: &DomainSources<'_>, tables_first: bool) -> Result<DomainResult, Refusal> {
    let identity = Identity::current();
    let work = Work::default();
    let composition = Composition::bind(sources.manifest, sources.components)?;
    if tables_first {
        Tables::build(&composition, &work)?;
    }
    let Plan {
        mut obligations,
        total,
        tables,
        refusal,
    } = plan(&composition);
    let mut components: Vec<Value> = composition
        .components
        .iter()
        .map(|component| {
            json!({
                "name": component.name,
                "bindings": {
                    "project_sha256": hex(&component.project_sha256),
                    "policy_sha256": hex(&component.policy_sha256),
                    "program_sha256": hex(&component.program_sha256),
                    "schema_sha256": hex(&super::leaf::sha256(&component.bound.schema)),
                },
                "state": if component.owns { "owned" } else { "read-only" },
                "committed_failure_case": component.committed_failure,
                "decisions_sha256": Value::Null,
            })
        })
        .collect();
    let verdict = if refusal.is_some() {
        Verdict::Refused
    } else {
        let built = Tables::build(&composition, &work)?;
        for (report, own) in components.iter_mut().zip(&built.components) {
            report["decisions_sha256"] = json!(hex(&own.decisions.digest));
        }
        for obligation in &mut obligations {
            match obligation.kind {
                Kind::Local => check_local(&composition, &built, obligation),
                Kind::Assume => check_assume(&composition, &built, obligation),
                Kind::Couple => check_couple(&composition, &built, obligation),
            }
            if obligation.status == Status::Planned {
                obligation.status = Status::Proved;
            }
        }
        if obligations
            .iter()
            .all(|obligation| obligation.status == Status::Proved)
        {
            Verdict::Proved
        } else {
            Verdict::Violated
        }
    };
    Ok(DomainResult {
        name: composition.manifest.name.clone(),
        manifest_sha256: composition.manifest_sha256,
        commitment: composition.commitment,
        identity,
        request: composition
            .manifest
            .request
            .iter()
            .map(|field| json!({"name": field.name, "domain": field.domain.json()}))
            .collect(),
        components,
        limits: (
            composition.manifest.limits.max_obligation,
            composition.manifest.limits.max_total,
            composition.manifest.limits.lowered,
        ),
        obligations,
        total,
        tables,
        monolithic: monolithic(&composition),
        verdict,
        refusal,
        decisions: work.decisions.get(),
        formula_cells: work.formula_values.get(),
    })
}

/// The global product's states, requests and pairs, as exact decimals.
fn monolithic(composition: &Composition) -> [String; 3] {
    let states: Vec<u128> = composition
        .components
        .iter()
        .filter(|component| component.owns)
        .flat_map(|component| {
            component
                .state_fields()
                .iter()
                .map(|field| field.domain.size())
        })
        .collect();
    let requests: Vec<u128> = composition
        .manifest
        .request
        .iter()
        .map(|field| field.domain.size())
        .collect();
    let all: Vec<u128> = states.iter().chain(&requests).copied().collect();
    [
        decimal_product(&states),
        decimal_product(&requests),
        decimal_product(&all),
    ]
}

/// The exact product of `factors` in decimal, however large. Each factor is
/// a domain size, at most 2^64, so one limb times a factor and the carry fit
/// in `u128`.
fn decimal_product(factors: &[u128]) -> String {
    // Little-endian limbs of nine decimal digits.
    const BASE: u128 = 1_000_000_000;
    let mut limbs: Vec<u128> = vec![1];
    for factor in factors {
        let mut carry = 0u128;
        for limb in &mut limbs {
            let value = *limb * factor + carry;
            *limb = value % BASE;
            carry = value / BASE;
        }
        while carry > 0 {
            limbs.push(carry % BASE);
            carry /= BASE;
        }
    }
    while limbs.len() > 1 && limbs.last() == Some(&0) {
        limbs.pop();
    }
    let mut text = limbs.last().map_or_else(String::new, ToString::to_string);
    for limb in limbs.iter().rev().skip(1) {
        text.push_str(&format!("{limb:09}"));
    }
    text
}

/// Names and values of `fields` as a JSON object.
fn named<'a>(names: impl Iterator<Item = &'a str>, values: &[i64]) -> Value {
    Value::Object(
        names
            .zip(values)
            .map(|(name, value)| (name.to_owned(), json!(value)))
            .collect::<Map<_, _>>(),
    )
}

/// The states of `components` as `component.field` values.
fn states_json(composition: &Composition, components: &[usize], states: &[Vec<i64>]) -> Value {
    let mut out = Map::new();
    for at in components {
        let component = &composition.components[*at];
        for (field, value) in component.state_fields().iter().zip(&states[*at]) {
            out.insert(format!("{}.{}", component.name, field.name), json!(value));
        }
    }
    Value::Object(out)
}

fn request_json(composition: &Composition, cone: &[usize], request: &[i64]) -> Value {
    named(
        cone.iter()
            .map(|at| composition.manifest.request[*at].name.as_str()),
        &cone.iter().map(|at| request[*at]).collect::<Vec<_>>(),
    )
}

fn check_local(composition: &Composition, tables: &Tables, obligation: &mut Checked) {
    let at = obligation.subject;
    let component = &composition.components[at];
    let own = &tables.components[at];
    let width = component.state_width;
    let state_names = || {
        component
            .state_fields()
            .iter()
            .map(|field| field.name.as_str())
    };
    let input_names = || {
        component
            .input_fields()
            .iter()
            .map(|field| field.name.as_str())
    };
    let invariant = |state: &[i64]| {
        own.invariant
            .holds(&gather(&component.invariant.reads, &[], &[], state))
    };
    if component.owns {
        let genesis = &component.genesis;
        let witness = |failed: &str, extra: Option<Value>| {
            json!({
                "genesis": named(state_names(), genesis),
                "failed": failed,
                "authority": extra,
            })
        };
        if !component
            .state_fields()
            .iter()
            .zip(genesis)
            .all(|(field, value)| field.domain.contains(*value))
        {
            obligation.violated(witness(
                "the declared genesis lies outside the field domains",
                None,
            ));
            return;
        }
        match &own.genesis {
            Genesis::Committed | Genesis::ReadOnly => {}
            Genesis::Refused(authority) => {
                obligation.violated(witness(
                    "the Authority refuses the declared genesis: a genesis law fails",
                    Some(authority.clone()),
                ));
                return;
            }
            Genesis::NotCommitted => {
                obligation.violated(witness(
                    "the Authority does not commit the declared genesis",
                    None,
                ));
                return;
            }
        }
        if !invariant(genesis) {
            obligation.violated(witness(
                "the genesis state does not satisfy the invariant",
                None,
            ));
            return;
        }
    }
    let values: Vec<Vec<i64>> = component
        .fields
        .iter()
        .map(|field| field.domain.values())
        .collect();
    let mut row = 0usize;
    let _ = for_each_tuple(&values, |tuple| {
        let at_row = row;
        row += 1;
        obligation.evaluated += 1;
        let state = &tuple[..width];
        let admitted = invariant(state)
            && own
                .assume
                .holds(&gather(&component.assume.reads, &[], &[], tuple));
        if !admitted {
            return Ok(());
        }
        obligation.admitted += 1;
        let (kind, reason, post) = own.decisions.at(at_row);
        let witness = |class: &str, failed: &str| {
            json!({
                "state": named(state_names(), state),
                "inputs": named(input_names(), &tuple[width..]),
                "class": class,
                "reason": reason,
                "post": named(state_names(), post),
                "failed": failed,
            })
        };
        match kind {
            Decided::Refused(class) => {
                obligation.refused_decisions += 1;
                if obligation.first_refusal.is_none() {
                    obligation.first_refusal = Some(json!({
                        "state": named(state_names(), state),
                        "inputs": named(input_names(), &tuple[width..]),
                        "refusal": class.name(),
                    }));
                }
                Ok(())
            }
            Decided::Reject => Ok(()),
            Decided::Accept | Decided::CommittedFailure => {
                let class = if kind == Decided::Accept {
                    "Accept"
                } else {
                    "CommittedFailure"
                };
                if !component.owns && post != state {
                    obligation.violated(witness(
                        class,
                        "a committing decision of a read-only component changes its state",
                    ));
                    return Err(());
                }
                if !invariant(post) {
                    obligation.violated(witness(
                        class,
                        "the committed successor does not satisfy the invariant: a counterexample to induction",
                    ));
                    return Err(());
                }
                Ok(())
            }
        }
    });
}

/// The product of the state domains of `components`, as one vector per
/// component of the composition, empty outside `components`.
fn for_each_states(
    composition: &Composition,
    components: &[usize],
    mut visit: impl FnMut(&[Vec<i64>]) -> Result<(), ()>,
) -> Result<(), ()> {
    let mut values: Vec<Vec<i64>> = Vec::new();
    let mut owners: Vec<usize> = Vec::new();
    for at in components {
        for field in composition.components[*at].state_fields() {
            values.push(field.domain.values());
            owners.push(*at);
        }
    }
    let mut states: Vec<Vec<i64>> = vec![Vec::new(); composition.components.len()];
    for_each_tuple(&values, |tuple| {
        for state in &mut states {
            state.clear();
        }
        for (owner, value) in owners.iter().zip(tuple) {
            states[*owner].push(*value);
        }
        visit(&states)
    })
}

/// The product of the request fields of `cone`, as a full request vector
/// with zeros outside `cone`.
fn for_each_request(
    composition: &Composition,
    cone: &[usize],
    mut visit: impl FnMut(&[i64]) -> Result<(), ()>,
) -> Result<(), ()> {
    let values: Vec<Vec<i64>> = cone
        .iter()
        .map(|at| composition.manifest.request[*at].domain.values())
        .collect();
    let mut request = vec![0i64; composition.manifest.request.len()];
    for_each_tuple(&values, |tuple| {
        for (at, value) in cone.iter().zip(tuple) {
            request[*at] = *value;
        }
        visit(&request)
    })
}

fn invariants_hold(
    composition: &Composition,
    tables: &Tables,
    components: &[usize],
    states: &[Vec<i64>],
) -> bool {
    components.iter().all(|at| {
        let reads = &composition.components[*at].invariant.reads;
        tables.components[*at]
            .invariant
            .holds(&gather(reads, &[], &[], &states[*at]))
    })
}

fn check_assume(composition: &Composition, tables: &Tables, obligation: &mut Checked) {
    let at = obligation.subject;
    let component = &composition.components[at];
    let own = &tables.components[at];
    let width = component.state_width;
    let cone = &component.cone;
    let providers = &component.providers;
    let _ = for_each_request(composition, cone, |request| {
        for_each_states(composition, providers, |states| {
            obligation.evaluated += 1;
            if !invariants_hold(composition, tables, providers, states)
                || !own.participates.holds(&gather(
                    &component.participates.reads,
                    request,
                    states,
                    &[],
                ))
            {
                return Ok(());
            }
            obligation.admitted += 1;
            let witness = |failed: String, delivered: Option<Value>| {
                json!({
                    "request": request_json(composition, cone, request),
                    "providers": states_json(composition, providers, states),
                    "delivered": delivered,
                    "failed": failed,
                })
            };
            let mut tuple = vec![0i64; width];
            let deliver = |field: usize, value: Delivered| match value {
                Delivered::Value(value) => Ok(value),
                Delivered::Outside(side) => Err(witness(
                    format!(
                        "the binding of `{}` lies {} its field's domain",
                        component.fields[field].name,
                        match side {
                            super::formula::Side::Below => "below",
                            super::formula::Side::Above => "above",
                            super::formula::Side::Unordered => "outside",
                        }
                    ),
                    None,
                )),
                Delivered::Undefined => Err(witness(
                    format!(
                        "the binding of `{}` has no value",
                        component.fields[field].name
                    ),
                    None,
                )),
            };
            if !component.owns {
                for (field, (resolved, table)) in component
                    .state_bindings
                    .iter()
                    .zip(&own.state_bindings)
                    .enumerate()
                {
                    let value = table.value(&gather(&resolved.reads, request, states, &[]));
                    match deliver(field, value) {
                        Ok(value) => tuple[field] = value,
                        Err(witness) => {
                            obligation.violated(witness);
                            return Err(());
                        }
                    }
                }
            }
            for (offset, (resolved, table)) in
                component.bindings.iter().zip(&own.bindings).enumerate()
            {
                let value = table.value(&gather(&resolved.reads, request, states, &[]));
                match deliver(width + offset, value) {
                    Ok(value) => tuple.push(value),
                    Err(witness) => {
                        obligation.violated(witness);
                        return Err(());
                    }
                }
            }
            let delivered = named(
                component.fields.iter().map(|field| field.name.as_str()),
                &tuple,
            );
            if !component.owns
                && !own
                    .invariant
                    .holds(&gather(&component.invariant.reads, &[], &[], &tuple))
            {
                obligation.violated(witness(
                    "the delivered read-only state does not satisfy the invariant".to_owned(),
                    Some(delivered),
                ));
                return Err(());
            }
            if !own
                .assume
                .holds(&gather(&component.assume.reads, &[], &[], &tuple))
            {
                obligation.violated(witness(
                    "the delivered command and context do not satisfy the assumption".to_owned(),
                    Some(delivered),
                ));
                return Err(());
            }
            Ok(())
        })
    });
}

fn check_couple(composition: &Composition, tables: &Tables, obligation: &mut Checked) {
    let coupling = &composition.couplings[obligation.subject];
    let table = &tables.couplings[obligation.subject];
    let members = &coupling.members;
    let (cone, spaces) = coupling_spaces(composition, members);
    let holds =
        |states: &[Vec<i64>]| table.holds(&gather(&coupling.invariant.reads, &[], states, &[]));
    // The components with a CommittedFailure case that run between the
    // previous member and each member: any of them may end the step just
    // before it.
    let mut cutters: Vec<Vec<usize>> = Vec::with_capacity(members.len());
    let mut previous = 0usize;
    for (position, member) in members.iter().enumerate() {
        let from = if position == 0 { 0 } else { previous + 1 };
        cutters.push(
            (from..*member)
                .filter(|at| composition.components[*at].committed_failure)
                .collect(),
        );
        previous = *member;
    }
    let names = |list: &[usize]| {
        list.iter()
            .map(|at| composition.components[*at].name.clone())
            .collect::<Vec<_>>()
    };
    let mut genesis: Vec<Vec<i64>> = vec![Vec::new(); composition.components.len()];
    for member in members {
        genesis[*member] = composition.components[*member].genesis.clone();
    }
    if !holds(&genesis) {
        obligation.violated(json!({
            "genesis": states_json(composition, members, &genesis),
            "failed": "the coupling does not hold at the members' genesis",
        }));
        return;
    }
    let requests: u64 = cone
        .iter()
        .map(|at| composition.manifest.request[*at].domain.size())
        .try_fold(1u64, |size, values| {
            size.checked_mul(u64::try_from(values).ok()?)
        })
        .unwrap_or(u64::MAX);
    let _ = for_each_states(composition, &spaces, |states| {
        if !invariants_hold(composition, tables, &spaces, states) || !holds(states) {
            obligation.evaluated = obligation.evaluated.saturating_add(requests);
            return Ok(());
        }
        for_each_request(composition, &cone, |request| {
            obligation.evaluated += 1;
            obligation.admitted += 1;
            let mut after = states.to_vec();
            let mut vetoed = false;
            let mut failed_at = None;
            for (position, member) in members.iter().enumerate() {
                if !cutters[position].is_empty() && !holds(&after) {
                    obligation.violated(json!({
                        "states": states_json(composition, &spaces, states),
                        "request": request_json(composition, &cone, request),
                        "after": states_json(composition, members, &after),
                        "ended_by_possible_committed_failure_of": names(&cutters[position]),
                        "failed": "a step that a component outside the coupling may end here breaks the coupling",
                    }));
                    return Err(());
                }
                match local(composition, tables, *member, states, request) {
                    Local::Skip => {}
                    Local::Reject(_) | Local::Refused(_) => {
                        vetoed = true;
                        break;
                    }
                    Local::Commit { failure, post, .. } => {
                        after[*member] = post;
                        if failure {
                            failed_at = Some(*member);
                            break;
                        }
                    }
                }
            }
            if !vetoed && !holds(&after) {
                obligation.violated(json!({
                    "states": states_json(composition, &spaces, states),
                    "request": request_json(composition, &cone, request),
                    "after": states_json(composition, members, &after),
                    "committed_failure": failed_at.map(|at| composition.components[at].name.clone()),
                    "failed": "the joint step breaks the coupling",
                }));
                return Err(());
            }
            Ok(())
        })
    });
}
