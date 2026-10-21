//! Invocation-owned compilation, finite domains, and Authority observations.
// SPDX-License-Identifier: MIT OR Apache-2.0
// LLM-generated implementation; checked by the repository's qualification gates.

use zeno_fcis_synthesis::finite::{
    V2Resource, V2Usage, canonical_v2::output, v2_authority::Authority, v2_composition as c,
};

use crate::ContractError;
use crate::checked_composition::{AuthorityRefusal, ExecutionRefusal, LawRefusal};
use crate::internals::declarations::{Declarations, Form, Source};
use crate::internals::graph::Atom;
use crate::internals::model::Contract;
use crate::internals::review::domain::{self, Position};
use crate::internals::review::evaluate::Framer;
use crate::internals::rules::Rules;
use crate::internals::{schema, schema_commitment};

use super::{MAX_INPUTS, MAX_STATES};

pub(super) struct Loaded {
    pub(super) declarations: Declarations,
    pub(super) rules: Rules,
    pub(super) schema: Vec<u8>,
    pub(super) commitment: [u8; 32],
    pub(super) positions: Vec<Position>,
    pub(super) width: usize,
}

impl Loaded {
    pub(super) fn read(project: &str, policy: &str) -> Result<Self, ContractError> {
        let rules = Rules::read(policy)?;
        if !rules.adoptions.is_empty() || !rules.evolutions.is_empty() {
            return Err(ContractError::new(
                "refinement",
                "adoptions and evolutions are outside this profile",
            ));
        }
        let declarations = Declarations::read(project, &rules.leaf_bindings)?;
        if !matches!(declarations.get(100)?.form, Form::Record(_)) {
            return Err(ContractError::new(
                "refinement",
                "the state root must be a record",
            ));
        }
        let schema = schema::encode(&declarations)?;
        let commitment = schema_commitment(&schema)?;
        let positions = domain::positions(&declarations)?;
        let width = positions
            .iter()
            .take_while(|p| p.source == Source::State)
            .count();
        Ok(Self {
            declarations,
            rules,
            schema,
            commitment,
            positions,
            width,
        })
    }

    pub(super) fn same_inputs(&self, other: &Self) -> bool {
        let left = &self.positions[self.width..];
        let right = &other.positions[other.width..];
        left.len() == right.len()
            && left.iter().zip(right).all(|(a, b)| {
                a.source == b.source
                    && a.field == b.field
                    && a.type_id == b.type_id
                    && a.domain == b.domain
            })
    }
}

pub(super) struct Space {
    values: Vec<Vec<i128>>,
    fields: Vec<u16>,
    pub(super) width: usize,
    pub(super) states: usize,
    pub(super) inputs: usize,
}

impl Space {
    /// Plan each product before allocating any axis. The empty product has one tuple.
    pub(super) fn new(loaded: &Loaded) -> Option<Self> {
        let states = domain::domain_size(&loaded.positions[..loaded.width])?;
        let inputs = domain::domain_size(&loaded.positions[loaded.width..])?;
        if states == 0 || inputs == 0 || states > MAX_STATES as u128 || inputs > MAX_INPUTS as u128
        {
            return None;
        }
        let values = loaded
            .positions
            .iter()
            .map(|p| p.domain.values_within(MAX_STATES as u64))
            .collect::<Option<Vec<_>>>()?;
        let fields = loaded.positions[..loaded.width]
            .iter()
            .map(|p| p.field)
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            values,
            fields,
            width: loaded.width,
            states: states as usize,
            inputs: inputs as usize,
        })
    }

    pub(super) fn tuple(&self, state: usize, input: usize) -> Vec<i128> {
        let mut tuple = element(&self.values[..self.width], state);
        tuple.extend(element(&self.values[self.width..], input));
        tuple
    }

    pub(super) fn ordinal(&self, values: &[i128]) -> Option<usize> {
        if values.len() != self.width {
            return None;
        }
        self.values[..self.width]
            .iter()
            .zip(values)
            .try_fold(0usize, |index, (axis, value)| {
                index
                    .checked_mul(axis.len())?
                    .checked_add(axis.iter().position(|x| x == value)?)
            })
    }

    pub(super) fn initial(&self, contract: &Contract<'_>, positions: &[Position]) -> Option<usize> {
        let fields = positions[..self.width]
            .iter()
            .map(|position| {
                let field = position.field?;
                let atom = &contract.genesis.iter().find(|(id, _)| *id == field)?.1;
                match atom {
                    Atom::Bool(v) => Some(i128::from(*v)),
                    Atom::I128(v) => Some(*v),
                    Atom::U128(v) => i128::try_from(*v).ok(),
                    Atom::Sum { variant, .. } => Some(i128::from(*variant)),
                    Atom::Text(_) => None,
                }
            })
            .collect::<Option<Vec<_>>>()?;
        self.ordinal(&fields)
    }
}

fn element(axes: &[Vec<i128>], mut ordinal: usize) -> Vec<i128> {
    let mut values = vec![0; axes.len()];
    for (value, axis) in values.iter_mut().zip(axes).rev() {
        *value = axis[ordinal % axis.len()];
        ordinal /= axis.len();
    }
    values
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    Accept,
    Reject,
    CommittedFailure,
    Refused(AuthorityRefusal),
}

pub(super) struct Step {
    pub(super) kind: Kind,
    pub(super) successor: Option<usize>,
    pub(super) reason: Option<u32>,
    pub(super) effects: Vec<u8>,
    pub(super) outbox: Vec<u8>,
    pub(super) unchanged: bool,
}

impl Step {
    pub(super) fn meter(&self) -> bool {
        matches!(
            self.kind,
            Kind::Refused(
                AuthorityRefusal::Execution(ExecutionRefusal::Meter(_))
                    | AuthorityRefusal::Law {
                        refusal: LawRefusal::Meter(_),
                        ..
                    }
            )
        )
    }

    pub(super) fn unknown(&self) -> bool {
        matches!(
            self.kind,
            Kind::Refused(
                AuthorityRefusal::Other
                    | AuthorityRefusal::Execution(ExecutionRefusal::Other)
                    | AuthorityRefusal::Law {
                        refusal: LawRefusal::Other,
                        ..
                    }
            )
        )
    }
}

/// Only this invocation mutates the tally, from measured evaluations, including refusals.
/// u128 holds the sum of at most 2 * MAX_CHECKS + 2 u64 charges per resource.
#[derive(Default)]
pub(super) struct Work {
    pub(super) evaluations: u64,
    pub(super) usage: [u128; 8],
}

impl Work {
    pub(super) fn record(&mut self, usage: V2Usage) {
        self.evaluations += 1;
        for (total, resource) in self.usage.iter_mut().zip([
            V2Resource::Read,
            V2Resource::Write,
            V2Resource::Candidate,
            V2Resource::Effect,
            V2Resource::Byte,
            V2Resource::WitnessByte,
            V2Resource::Depth,
            V2Resource::Step,
        ]) {
            *total += u128::from(usage.used(resource));
        }
    }
}

pub(super) fn genesis(
    authority: &Authority<'_>,
    framer: &Framer,
    tuple: &[i128],
    work: &mut Work,
) -> Option<AuthorityRefusal> {
    let frames = framer.frame(tuple);
    let evaluation = authority.genesis(&frames[0]);
    work.record(evaluation.usage());
    evaluation
        .result()
        .err()
        .map(|e| AuthorityRefusal::of(&e, evaluation.diagnostics()))
}

pub(super) fn step(
    authority: &Authority<'_>,
    framer: &Framer,
    space: &Space,
    state: usize,
    input: usize,
    work: &mut Work,
) -> Step {
    let tuple = space.tuple(state, input);
    let frames = framer.frame(&tuple);
    let evaluation = authority.evaluate(c::Raw {
        state: &frames[0],
        command: &frames[1],
        context: &frames[2],
    });
    work.record(evaluation.usage());
    let candidate = match evaluation.result() {
        Ok(candidate) => candidate,
        Err(error) => {
            let refusal = AuthorityRefusal::of(&error, evaluation.diagnostics());
            return Step {
                kind: Kind::Refused(refusal),
                successor: Some(state),
                reason: None,
                effects: Vec::new(),
                outbox: Vec::new(),
                unchanged: true,
            };
        }
    };
    let kind = match candidate.class() {
        c::Class::Accept => Kind::Accept,
        c::Class::Reject => Kind::Reject,
        c::Class::CommittedFailure => Kind::CommittedFailure,
        _ => {
            return Step {
                kind: Kind::Refused(AuthorityRefusal::Other),
                successor: None,
                reason: None,
                effects: Vec::new(),
                outbox: Vec::new(),
                unchanged: false,
            };
        }
    };
    let mut observations = [Vec::new(), Vec::new()];
    // Canonical bytes, lengths and order; no digest or debug-rendering equality.
    for (observation, deliveries) in observations
        .iter_mut()
        .zip([candidate.effects(), candidate.outbox()])
    {
        append_number(observation, deliveries.len());
        for delivery in deliveries {
            observation.extend_from_slice(&delivery.ordinal.to_be_bytes());
            observation.extend_from_slice(&delivery.channel.to_be_bytes());
            for encoded in [
                output::encode_atom(delivery.destination, super::MAX_SOURCE_BYTES),
                output::encode_record(&delivery.payload, super::MAX_SOURCE_BYTES),
                output::encode_atom(delivery.idempotency, super::MAX_SOURCE_BYTES),
            ] {
                let Ok(bytes) = encoded else {
                    return Step {
                        kind: Kind::Refused(AuthorityRefusal::Other),
                        successor: None,
                        reason: None,
                        effects: Vec::new(),
                        outbox: Vec::new(),
                        unchanged: false,
                    };
                };
                append_number(observation, bytes.len());
                observation.extend_from_slice(&bytes);
            }
        }
    }
    let rejected = kind == Kind::Reject;
    let unchanged = !rejected
        || (candidate.post().is_empty()
            && candidate.patch().is_empty()
            && candidate.effects().is_empty()
            && candidate.outbox().is_empty());
    let successor = if rejected {
        Some(state)
    } else {
        let values = space
            .fields
            .iter()
            .map(|id| {
                let field = candidate.post().iter().find(|field| field.id == *id)?;
                match field.value {
                    c::Atom::Bool(v) => Some(i128::from(v)),
                    c::Atom::I128(v) => Some(v),
                    c::Atom::U128(v) => i128::try_from(v).ok(),
                    c::Atom::Sum { variant, .. } | c::Atom::Enum { variant, .. } => {
                        Some(i128::from(variant))
                    }
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>();
        values.and_then(|values| space.ordinal(&values))
    };
    let [effects, outbox] = observations;
    Step {
        kind,
        successor,
        reason: candidate.reason(),
        effects,
        outbox,
        unchanged,
    }
}

fn append_number(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&(n as u64).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::policy;

    #[test]
    fn authority_meter_refusals_retain_measured_work_on_each_attempt() {
        let loaded = Loaded::read(
            include_str!(
                "../../../../examples/contract-refinement/reservation/abstract/project.zeno"
            ),
            include_str!(
                "../../../../examples/contract-refinement/reservation/abstract/v2/policy.json"
            ),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let space = Space::new(&loaded).unwrap_or_else(|| panic!("finite space"));
        let mut contract = Contract::build(&loaded.declarations, &loaded.rules, loaded.commitment)
            .unwrap_or_else(|e| panic!("{e}"));
        contract.budgets.step = 1;
        let framer = Framer::new(&loaded.positions, &contract);
        policy::with_authority(&contract, &loaded.schema, |authority| {
            let mut work = Work::default();
            let first = step(authority, &framer, &space, 0, 0, &mut work);
            assert!(first.meter());
            assert_eq!(work.evaluations, 1);
            let usage = work.usage;
            assert!(usage.iter().any(|v| *v > 0));
            let second = step(authority, &framer, &space, 0, 0, &mut work);
            assert!(second.meter());
            assert_eq!(second.kind, first.kind);
            assert_eq!(work.evaluations, 2);
            assert_eq!(work.usage, usage.map(|v| v * 2));
        })
        .unwrap_or_else(|e| panic!("{e}"));
    }
}
