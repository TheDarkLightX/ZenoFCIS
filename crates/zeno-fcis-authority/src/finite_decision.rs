//! Opt-in library interpretation of a closed finite decision contract.
//!
//! This first profile covers flat record inputs, accepting updates, ordinary
//! rejections, and outbox entries. Unsupported shapes refuse construction or
//! execution. It is not the mandatory V2 authority gate: the 1.x authority
//! still exposes its legacy project-program path and law engine.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::marker::PhantomData;

use zeno_fcis_catalog::{CatalogError, ProjectCatalog};
use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_compose::{AccessPath, ContractError, PathAtom};
use zeno_fcis_core::{Budget, BudgetExceeded, BudgetLimits, DecisionKind, Resource};
use zeno_fcis_crypto::ApprovedCommitmentProvider;
use zeno_fcis_patch::{PatchError, PathSegment, ValuePath, value_at};
use zeno_fcis_plan::OutboxEntry;
use zeno_fcis_project::SemanticId;
use zeno_fcis_schema::{TypeId, TypeKind, ValidationLimits};
use zeno_fcis_synthesis::finite::{Domain as FiniteDomain, Error as FiniteError, Program};
use zeno_fcis_synthesis::finite_runtime::{evaluator_hash, import_program};
use zeno_fcis_transition::{CataloguedTransitionBuilder, TransitionDecision, TransitionError};
use zeno_fcis_value::{Field, Value, ValueError};

use crate::{CatalogTransitionProgram, ReviewedTransitionInput};

const MAX_ROWS: u64 = 1_000_000;
const RESOURCES: [Resource; 7] = [
    Resource::Read,
    Resource::Write,
    Resource::Candidate,
    Resource::Effect,
    Resource::Byte,
    Resource::WitnessByte,
    Resource::Depth,
];

/// The admitted invocation part containing one direct record field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSource {
    /// Pre-state record.
    State,
    /// Command record.
    Command,
    /// Authenticated context record.
    Context,
}

impl InputSource {
    const fn tag(self) -> u128 {
        match self {
            Self::State => 0,
            Self::Command => 1,
            Self::Context => 2,
        }
    }
}

/// Checked scalar projection from a schema-admitted field into finite IR.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputScalar {
    /// Exact signed integer within the declared finite domain.
    I128,
    /// Boolean represented as zero or one.
    Bool,
    /// Closed sum with no payload, mapped by stable variant identifiers.
    SumVariants {
        /// Stable sum type identifier.
        type_id: u32,
        /// Ascending variant identifiers and their finite integer codes.
        variants: Vec<(u16, i64)>,
    },
}

/// One input position in the finite program ABI.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputBinding {
    /// Invocation part containing the field.
    pub source: InputSource,
    /// Stable field identifier.
    pub field: u16,
    /// Library-interpreted scalar projection.
    pub scalar: InputScalar,
}

/// One finite branch code, either an ordinary reject or an accept.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionBranch {
    /// Exact first-output code.
    pub code: i64,
    /// None accepts; Some names one catalogued ordinary rejection.
    pub rejection: Option<SemanticId>,
}

/// One direct state-field assignment on the accepting branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateAssignment {
    /// Stable direct field identifier.
    pub field: u16,
    /// Zero-based finite result position holding an integer.
    pub output: u16,
}

/// One outbox obligation on the accepting branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConditionalOutbox {
    /// Stable entry ordinal.
    pub ordinal: u32,
    /// Catalogued channel identifier.
    pub channel: SemanticId,
    /// Boolean output position that determines whether to enqueue.
    pub when_output: u16,
    /// Fixed destination data admitted by the catalog on sealing.
    pub destination: Value,
    /// Ascending payload-record field identifiers copied from raw input fields.
    pub payload_fields: Vec<(u16, u16)>,
}

/// Closed, total finite program plus the complete supported decision plan.
#[derive(Clone, Debug)]
pub struct FiniteDecisionContract {
    source_hash: Hash32,
    program_bytes: Box<[u8]>,
    program: Program,
    expected_inputs: Vec<FiniteDomain>,
    expected_outputs: Vec<FiniteDomain>,
    inputs: Vec<InputBinding>,
    branches: Vec<DecisionBranch>,
    assignments: Vec<StateAssignment>,
    outbox: Vec<ConditionalOutbox>,
    limits: BudgetLimits,
    max_steps: u16,
}

/// Failure to admit, evaluate, or seal one finite decision.
#[derive(Debug)]
pub enum FiniteDecisionError {
    /// A closed contract or invocation condition is not met.
    Invalid(&'static str),
    /// The finite program trapped or failed its declared domain.
    Finite(FiniteError),
    /// Canonical identity construction failed.
    Encode(EncodeError),
    /// Catalog admission failed.
    Catalog(CatalogError),
    /// A value path failed to resolve.
    Patch(PatchError),
    /// The library meter refused work.
    Budget(BudgetExceeded),
    /// A semantic footprint could not be formed.
    Compose(ContractError),
    /// A canonical payload record could not be formed.
    Value(ValueError),
    /// The transition builder refused the complete decision.
    Transition(TransitionError),
}

impl FiniteDecisionContract {
    /// Admits exact canonical IR, validates the complete plan shape, and
    /// enumerates every finite input to refuse traps or missing branch codes.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        source_hash: Hash32,
        program_bytes: &[u8],
        expected_inputs: Vec<FiniteDomain>,
        expected_outputs: Vec<FiniteDomain>,
        inputs: Vec<InputBinding>,
        branches: Vec<DecisionBranch>,
        assignments: Vec<StateAssignment>,
        outbox: Vec<ConditionalOutbox>,
        limits: BudgetLimits,
        max_steps: u16,
    ) -> Result<Self, FiniteDecisionError> {
        if source_hash == Hash32::ZERO || max_steps == 0 {
            return Err(FiniteDecisionError::Invalid("source-or-step-limit"));
        }
        let program = import_program(program_bytes).map_err(FiniteDecisionError::Finite)?;
        if program.inputs() != expected_inputs || program.outputs() != expected_outputs {
            return Err(FiniteDecisionError::Invalid("program-abi"));
        }
        if program.nodes().len() > usize::from(max_steps)
            || inputs.is_empty()
            || inputs.len() != program.inputs().len()
        {
            return Err(FiniteDecisionError::Invalid("input-or-step-shape"));
        }
        validate_plan(&program, &inputs, &branches, &assignments, &outbox)?;
        verify_total(&program, &branches)?;
        Ok(Self {
            source_hash,
            program_bytes: program_bytes.into(),
            program,
            expected_inputs,
            expected_outputs,
            inputs,
            branches,
            assignments,
            outbox,
            limits,
            max_steps,
        })
    }

    /// Domain-separated identity computed from the complete canonical contract
    /// and the exact library evaluator source.
    pub fn identity<H: ApprovedCommitmentProvider>(&self) -> Result<Hash32, FiniteDecisionError> {
        let finite_evaluator =
            evaluator_hash().map_err(|_| FiniteDecisionError::Invalid("evaluator-identity"))?;
        let gate_evaluator = commitment::<H>(
            Domain::new("zeno-fcis/finite-decision-evaluator", 1)
                .map_err(FiniteDecisionError::Encode)?,
            include_bytes!("finite_decision.rs"),
        )
        .map_err(FiniteDecisionError::Encode)?;
        let value = Value::tuple(vec![
            Value::Text("zeno-fcis/finite-decision-contract/1".into()),
            hash_value(self.source_hash),
            hash_value(finite_evaluator),
            hash_value(gate_evaluator),
            Value::Bytes(self.program_bytes.clone()),
            Value::tuple(
                self.expected_inputs
                    .iter()
                    .copied()
                    .map(domain_value)
                    .collect(),
            ),
            Value::tuple(
                self.expected_outputs
                    .iter()
                    .copied()
                    .map(domain_value)
                    .collect(),
            ),
            Value::tuple(self.inputs.iter().map(InputBinding::value).collect()),
            Value::tuple(
                self.branches
                    .iter()
                    .copied()
                    .map(DecisionBranch::value)
                    .collect(),
            ),
            Value::tuple(
                self.assignments
                    .iter()
                    .copied()
                    .map(StateAssignment::value)
                    .collect(),
            ),
            Value::tuple(self.outbox.iter().map(ConditionalOutbox::value).collect()),
            Value::tuple(
                RESOURCES
                    .iter()
                    .copied()
                    .map(|resource| Value::U128(self.limits.limit(resource).into()))
                    .collect(),
            ),
            Value::U128(self.max_steps.into()),
            Value::tuple(Vec::new()), // explicit empty effect plan
            Value::tuple(Vec::new()), // explicit empty committed-failure plan
        ]);
        let bytes = value
            .canonical_bytes()
            .map_err(FiniteDecisionError::Encode)?;
        commitment::<H>(
            Domain::new("zeno-fcis/finite-decision-contract", 1)
                .map_err(FiniteDecisionError::Encode)?,
            &bytes,
        )
        .map_err(FiniteDecisionError::Encode)
    }
}

fn hash_value(hash: Hash32) -> Value {
    Value::Bytes(hash.as_bytes().to_vec().into_boxed_slice())
}

fn domain_value(domain: FiniteDomain) -> Value {
    match domain {
        FiniteDomain::Bool => Value::tuple(vec![Value::Bool(true), Value::I128(0), Value::I128(1)]),
        FiniteDomain::Int { min, max } => Value::tuple(vec![
            Value::Bool(false),
            Value::I128(min.into()),
            Value::I128(max.into()),
        ]),
    }
}

impl InputBinding {
    fn value(&self) -> Value {
        let scalar = match &self.scalar {
            InputScalar::I128 => Value::tuple(vec![Value::U128(0)]),
            InputScalar::Bool => Value::tuple(vec![Value::U128(1)]),
            InputScalar::SumVariants { type_id, variants } => Value::tuple(vec![
                Value::U128(2),
                Value::U128((*type_id).into()),
                Value::tuple(
                    variants
                        .iter()
                        .map(|(id, code)| {
                            Value::tuple(vec![
                                Value::U128((*id).into()),
                                Value::I128((*code).into()),
                            ])
                        })
                        .collect(),
                ),
            ]),
        };
        Value::tuple(vec![
            Value::U128(self.source.tag()),
            Value::U128(self.field.into()),
            scalar,
        ])
    }
}

impl DecisionBranch {
    fn value(self) -> Value {
        Value::tuple(vec![
            Value::I128(self.code.into()),
            Value::Bool(self.rejection.is_some()),
            Value::U128(self.rejection.map_or(0, SemanticId::get).into()),
        ])
    }
}

impl StateAssignment {
    fn value(self) -> Value {
        Value::tuple(vec![
            Value::U128(self.field.into()),
            Value::U128(self.output.into()),
        ])
    }
}

impl ConditionalOutbox {
    fn value(&self) -> Value {
        Value::tuple(vec![
            Value::U128(self.ordinal.into()),
            Value::U128(self.channel.get().into()),
            Value::U128(self.when_output.into()),
            self.destination.clone(),
            Value::tuple(
                self.payload_fields
                    .iter()
                    .map(|(field, input)| {
                        Value::tuple(vec![
                            Value::U128((*field).into()),
                            Value::U128((*input).into()),
                        ])
                    })
                    .collect(),
            ),
        ])
    }
}

fn validate_plan(
    program: &Program,
    inputs: &[InputBinding],
    branches: &[DecisionBranch],
    assignments: &[StateAssignment],
    outbox: &[ConditionalOutbox],
) -> Result<(), FiniteDecisionError> {
    let Some(FiniteDomain::Int { min, max }) = program.outputs().first().copied() else {
        return Err(FiniteDecisionError::Invalid("decision-code-domain"));
    };
    let branch_count = i128::from(max) - i128::from(min) + 1;
    if !(1..=32).contains(&branch_count) || branches.len() != branch_count as usize {
        return Err(FiniteDecisionError::Invalid("branch-count"));
    }
    for (index, branch) in branches.iter().enumerate() {
        if i128::from(branch.code) != i128::from(min) + index as i128 {
            return Err(FiniteDecisionError::Invalid("branch-order"));
        }
    }
    for (domain, binding) in program.inputs().iter().copied().zip(inputs) {
        if binding.field == 0 {
            return Err(FiniteDecisionError::Invalid("zero-input-field"));
        }
        match (&binding.scalar, domain) {
            (InputScalar::I128, FiniteDomain::Int { .. })
            | (InputScalar::Bool, FiniteDomain::Bool) => {}
            (InputScalar::SumVariants { type_id, variants }, FiniteDomain::Int { .. }) => {
                if *type_id == 0
                    || variants.is_empty()
                    || variants.windows(2).any(|pair| pair[0].0 >= pair[1].0)
                    || variants.iter().enumerate().any(|(index, (_, code))| {
                        variants[..index].iter().any(|(_, earlier)| earlier == code)
                    })
                    || variants
                        .iter()
                        .any(|(variant, code)| *variant == 0 || !domain.contains(*code))
                {
                    return Err(FiniteDecisionError::Invalid("sum-projection"));
                }
            }
            _ => return Err(FiniteDecisionError::Invalid("scalar-domain")),
        }
    }
    if inputs.iter().enumerate().any(|(index, binding)| {
        inputs[..index]
            .iter()
            .any(|earlier| earlier.source == binding.source && earlier.field == binding.field)
    }) {
        return Err(FiniteDecisionError::Invalid("duplicate-input-field"));
    }
    if assignments
        .windows(2)
        .any(|pair| pair[0].field >= pair[1].field)
    {
        return Err(FiniteDecisionError::Invalid("assignment-order"));
    }
    for assignment in assignments {
        if assignment.field == 0
            || assignment.output == 0
            || !matches!(
                program.outputs().get(usize::from(assignment.output)),
                Some(FiniteDomain::Int { .. })
            )
        {
            return Err(FiniteDecisionError::Invalid("assignment-shape"));
        }
    }
    for (index, entry) in outbox.iter().enumerate() {
        if entry.ordinal != index as u32
            || entry.channel.get() == 0
            || !matches!(
                program.outputs().get(usize::from(entry.when_output)),
                Some(FiniteDomain::Bool)
            )
            || entry
                .payload_fields
                .windows(2)
                .any(|pair| pair[0].0 >= pair[1].0)
            || entry
                .payload_fields
                .iter()
                .any(|(field, input)| *field == 0 || usize::from(*input) >= inputs.len())
        {
            return Err(FiniteDecisionError::Invalid("outbox-shape"));
        }
    }
    let mut used_outputs = vec![false; program.outputs().len()];
    used_outputs[0] = true;
    for assignment in assignments {
        used_outputs[usize::from(assignment.output)] = true;
    }
    for entry in outbox {
        used_outputs[usize::from(entry.when_output)] = true;
    }
    if used_outputs.contains(&false) {
        return Err(FiniteDecisionError::Invalid("unused-output"));
    }
    Ok(())
}

fn verify_total(program: &Program, branches: &[DecisionBranch]) -> Result<(), FiniteDecisionError> {
    let mut rows = 1_u64;
    for domain in program.inputs() {
        let (min, max) = domain.bounds();
        let width = u64::try_from(i128::from(max) - i128::from(min) + 1)
            .map_err(|_| FiniteDecisionError::Invalid("input-domain-width"))?;
        rows = rows
            .checked_mul(width)
            .ok_or(FiniteDecisionError::Invalid("input-product"))?;
        if rows > MAX_ROWS {
            return Err(FiniteDecisionError::Invalid("input-product"));
        }
    }
    let mut row: Vec<i64> = program
        .inputs()
        .iter()
        .map(|domain| domain.bounds().0)
        .collect();
    for _ in 0..rows {
        let output = program
            .evaluate(&row)
            .map_err(FiniteDecisionError::Finite)?;
        if !branches.iter().any(|branch| branch.code == output[0]) {
            return Err(FiniteDecisionError::Invalid("missing-branch"));
        }
        for (value, domain) in row.iter_mut().zip(program.inputs()) {
            let (min, max) = domain.bounds();
            if *value < max {
                *value += 1;
                break;
            }
            *value = min;
        }
    }
    Ok(())
}

/// One library-owned interpreter for a bound, closed finite decision contract.
pub struct FiniteDecisionProgram<H: ApprovedCommitmentProvider> {
    contract: FiniteDecisionContract,
    catalog_hash: Hash32,
    identity: Hash32,
    marker: PhantomData<H>,
}

impl<H: ApprovedCommitmentProvider> FiniteDecisionProgram<H> {
    /// Refuses a contract whose identity, reason codes, or channels differ from
    /// the reviewed catalog.
    pub fn try_new(
        catalog: &ProjectCatalog,
        contract: FiniteDecisionContract,
    ) -> Result<Self, FiniteDecisionError> {
        let identity = contract.identity::<H>()?;
        if catalog.profile().bindings().algorithm_hash != identity {
            return Err(FiniteDecisionError::Invalid("catalog-algorithm-binding"));
        }
        validate_schema_binding(catalog, &contract)?;
        for branch in &contract.branches {
            if let Some(reason) = branch.rejection {
                catalog
                    .validate_reason(reason.get(), DecisionKind::Reject)
                    .map_err(FiniteDecisionError::Catalog)?;
            }
        }
        for entry in &contract.outbox {
            if catalog.manifest().channel(entry.channel).is_none() {
                return Err(FiniteDecisionError::Invalid("unknown-outbox-channel"));
            }
        }
        let catalog_hash = catalog
            .commitment::<H>()
            .map_err(FiniteDecisionError::Catalog)?;
        Ok(Self {
            contract,
            catalog_hash,
            identity,
            marker: PhantomData,
        })
    }
}

fn input_field_type(
    catalog: &ProjectCatalog,
    source: InputSource,
    field: u16,
) -> Result<TypeId, FiniteDecisionError> {
    let profile = catalog.profile();
    let root = match source {
        InputSource::State => profile.state_type(),
        InputSource::Command => profile.command_type(),
        InputSource::Context => profile.context_type(),
    };
    let Some(definition) = catalog.schema().type_by_id(TypeId::new(root.get())) else {
        return Err(FiniteDecisionError::Invalid("missing-root-type"));
    };
    let TypeKind::Record { fields } = definition.kind() else {
        return Err(FiniteDecisionError::Invalid("non-record-root"));
    };
    fields
        .iter()
        .find(|candidate| candidate.id().get() == field)
        .map(|candidate| candidate.type_id())
        .ok_or(FiniteDecisionError::Invalid("unknown-input-field"))
}

fn validate_schema_binding(
    catalog: &ProjectCatalog,
    contract: &FiniteDecisionContract,
) -> Result<(), FiniteDecisionError> {
    let schema = catalog.schema();
    for source in [
        InputSource::State,
        InputSource::Command,
        InputSource::Context,
    ] {
        let root = match source {
            InputSource::State => catalog.profile().state_type(),
            InputSource::Command => catalog.profile().command_type(),
            InputSource::Context => catalog.profile().context_type(),
        };
        let Some(definition) = schema.type_by_id(TypeId::new(root.get())) else {
            return Err(FiniteDecisionError::Invalid("missing-root-type"));
        };
        let TypeKind::Record { fields } = definition.kind() else {
            return Err(FiniteDecisionError::Invalid("non-record-root"));
        };
        if fields.len()
            != contract
                .inputs
                .iter()
                .filter(|binding| binding.source == source)
                .count()
        {
            return Err(FiniteDecisionError::Invalid("incomplete-input-projection"));
        }
    }
    for (index, binding) in contract.inputs.iter().enumerate() {
        let type_id = input_field_type(catalog, binding.source, binding.field)?;
        let Some(definition) = schema.type_by_id(type_id) else {
            return Err(FiniteDecisionError::Invalid("missing-input-type"));
        };
        let domain = contract.expected_inputs[index];
        match (definition.kind(), &binding.scalar, domain) {
            (
                TypeKind::I128 { min, max },
                InputScalar::I128,
                FiniteDomain::Int {
                    min: dmin,
                    max: dmax,
                },
            ) if *min == i128::from(dmin) && *max == i128::from(dmax) => {}
            (TypeKind::Bool, InputScalar::Bool, FiniteDomain::Bool) => {}
            (
                TypeKind::Sum { variants: admitted },
                InputScalar::SumVariants {
                    type_id: declared,
                    variants,
                },
                FiniteDomain::Int { min, max },
            ) if type_id.get() == *declared
                && admitted.len() == variants.len()
                && i128::from(max) - i128::from(min) + 1 == variants.len() as i128
                && admitted.iter().zip(variants).all(|(actual, (id, _))| {
                    actual.id().get() == *id && actual.payload().is_none()
                }) => {}
            (
                TypeKind::Enum { variants: admitted },
                InputScalar::SumVariants {
                    type_id: declared,
                    variants,
                },
                FiniteDomain::Int { min, max },
            ) if type_id.get() == *declared
                && admitted.len() == variants.len()
                && i128::from(max) - i128::from(min) + 1 == variants.len() as i128
                && admitted
                    .iter()
                    .zip(variants)
                    .all(|(actual, (id, _))| actual.id().get() == *id) => {}
            _ => return Err(FiniteDecisionError::Invalid("input-schema-domain")),
        }
    }
    for assignment in &contract.assignments {
        let type_id = input_field_type(catalog, InputSource::State, assignment.field)?;
        let Some(definition) = schema.type_by_id(type_id) else {
            return Err(FiniteDecisionError::Invalid("missing-output-type"));
        };
        let FiniteDomain::Int { min, max } =
            contract.expected_outputs[usize::from(assignment.output)]
        else {
            return Err(FiniteDecisionError::Invalid("output-domain"));
        };
        if !matches!(definition.kind(), TypeKind::I128 { min: actual_min, max: actual_max }
            if *actual_min == i128::from(min) && *actual_max == i128::from(max))
        {
            return Err(FiniteDecisionError::Invalid("output-schema-domain"));
        }
    }
    let Some(state_type) = schema.type_by_id(TypeId::new(catalog.profile().state_type().get()))
    else {
        return Err(FiniteDecisionError::Invalid("missing-state-type"));
    };
    let TypeKind::Record {
        fields: state_fields,
    } = state_type.kind()
    else {
        return Err(FiniteDecisionError::Invalid("non-record-state"));
    };
    if state_fields.len() != contract.assignments.len()
        || state_fields
            .iter()
            .zip(&contract.assignments)
            .any(|(field, assignment)| field.id().get() != assignment.field)
    {
        return Err(FiniteDecisionError::Invalid("incomplete-successor"));
    }
    for entry in &contract.outbox {
        let Some(channel) = catalog.manifest().channel(entry.channel) else {
            return Err(FiniteDecisionError::Invalid("unknown-outbox-channel"));
        };
        schema
            .validate_value(
                channel.destination_type(),
                &entry.destination,
                ValidationLimits::default(),
            )
            .map_err(|_| FiniteDecisionError::Invalid("outbox-destination-type"))?;
        let Some(payload_type) = schema.type_by_id(channel.payload_type()) else {
            return Err(FiniteDecisionError::Invalid("missing-payload-type"));
        };
        let TypeKind::Record { fields } = payload_type.kind() else {
            return Err(FiniteDecisionError::Invalid("non-record-payload"));
        };
        if fields.len() != entry.payload_fields.len()
            || fields
                .iter()
                .zip(&entry.payload_fields)
                .any(|(field, (id, index))| {
                    field.id().get() != *id
                        || input_field_type(
                            catalog,
                            contract.inputs[usize::from(*index)].source,
                            contract.inputs[usize::from(*index)].field,
                        )
                        .map(|source_type| source_type != field.type_id())
                        .unwrap_or(true)
                })
        {
            return Err(FiniteDecisionError::Invalid("outbox-payload-type"));
        }
    }
    Ok(())
}

impl<H: ApprovedCommitmentProvider> CatalogTransitionProgram<H> for FiniteDecisionProgram<H> {
    type Error = FiniteDecisionError;

    fn transition_build_hash(&self) -> Hash32 {
        self.identity
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        if input
            .catalog()
            .commitment::<H>()
            .map_err(FiniteDecisionError::Catalog)?
            != self.catalog_hash
        {
            return Err(FiniteDecisionError::Invalid("catalog-mismatch"));
        }
        let state = input.pre_state().value().value();
        let command = input.command().value().value();
        let context = input.context().value().value();
        let mut budget = Budget::new(self.contract.limits);
        budget
            .charge(Resource::Candidate, 1)
            .map_err(FiniteDecisionError::Budget)?;
        let mut finite_input = Vec::with_capacity(self.contract.inputs.len());
        let mut raw_inputs = Vec::with_capacity(self.contract.inputs.len());
        for binding in &self.contract.inputs {
            let root = match binding.source {
                InputSource::State => state,
                InputSource::Command => command,
                InputSource::Context => context,
            };
            let raw = value_at(
                root,
                &ValuePath::new(vec![PathSegment::Field(binding.field)]),
            )
            .map_err(FiniteDecisionError::Patch)?;
            budget
                .charge(Resource::Read, 1)
                .map_err(FiniteDecisionError::Budget)?;
            let scalar = project_input(raw, &binding.scalar)?;
            finite_input.push(scalar);
            raw_inputs.push(raw);
        }
        let output = self
            .contract
            .program
            .evaluate(&finite_input)
            .map_err(FiniteDecisionError::Finite)?;
        let branch = self
            .contract
            .branches
            .iter()
            .find(|branch| branch.code == output[0])
            .ok_or(FiniteDecisionError::Invalid("missing-branch"))?;
        if branch.rejection.is_none() {
            for _assignment in &self.contract.assignments {
                budget
                    .charge(Resource::Read, 1)
                    .map_err(FiniteDecisionError::Budget)?;
                budget
                    .charge(Resource::Write, 1)
                    .map_err(FiniteDecisionError::Budget)?;
            }
            for entry in &self.contract.outbox {
                if output[usize::from(entry.when_output)] == 1 {
                    budget
                        .charge(Resource::Effect, 1)
                        .map_err(FiniteDecisionError::Budget)?;
                }
            }
        }
        let mut builder = CataloguedTransitionBuilder::<H>::try_new(
            input.catalog(),
            state,
            input.state_domain(),
            input.expected_bindings().command_hash(),
            input.expected_bindings().context_hash(),
            budget.used(),
            input.limits(),
        )
        .map_err(FiniteDecisionError::Transition)?;
        for binding in &self.contract.inputs {
            match binding.source {
                InputSource::State => {
                    builder
                        .read(ValuePath::new(vec![PathSegment::Field(binding.field)]))
                        .map_err(FiniteDecisionError::Transition)?;
                }
                InputSource::Context => {
                    builder
                        .observe_context(
                            AccessPath::try_new(
                                input.catalog().profile().context_type().get(),
                                vec![PathAtom::Field(binding.field)],
                            )
                            .map_err(FiniteDecisionError::Compose)?,
                        )
                        .map_err(FiniteDecisionError::Transition)?;
                }
                InputSource::Command => {}
            }
        }
        if let Some(reason) = branch.rejection {
            builder
                .require(false, reason)
                .map_err(FiniteDecisionError::Transition)?;
        } else {
            for assignment in &self.contract.assignments {
                builder
                    .update(
                        ValuePath::new(vec![PathSegment::Field(assignment.field)]),
                        Value::I128(output[usize::from(assignment.output)].into()),
                    )
                    .map_err(FiniteDecisionError::Transition)?;
            }
            for entry in &self.contract.outbox {
                if output[usize::from(entry.when_output)] == 1 {
                    let fields = entry
                        .payload_fields
                        .iter()
                        .map(|(field, index)| {
                            Field::new(*field, (*raw_inputs[usize::from(*index)]).clone())
                        })
                        .collect();
                    let payload =
                        Value::record_canonical(fields).map_err(FiniteDecisionError::Value)?;
                    builder
                        .enqueue(OutboxEntry::new(
                            entry.ordinal,
                            entry.channel.get(),
                            entry.destination.clone(),
                            payload,
                        ))
                        .map_err(FiniteDecisionError::Transition)?;
                }
            }
        }
        builder.seal().map_err(FiniteDecisionError::Transition)
    }
}

fn project_input(raw: &Value, scalar: &InputScalar) -> Result<i64, FiniteDecisionError> {
    match (raw, scalar) {
        (Value::I128(value), InputScalar::I128) => {
            i64::try_from(*value).map_err(|_| FiniteDecisionError::Invalid("integer-out-of-range"))
        }
        (Value::Bool(value), InputScalar::Bool) => Ok(i64::from(*value)),
        (
            Value::Sum {
                type_id,
                variant,
                payload: None,
            },
            InputScalar::SumVariants {
                type_id: expected,
                variants,
            },
        ) if type_id == expected => variants
            .iter()
            .find(|(id, _)| id == variant)
            .map(|(_, code)| *code)
            .ok_or(FiniteDecisionError::Invalid("unknown-variant")),
        (
            Value::Enum { type_id, variant },
            InputScalar::SumVariants {
                type_id: expected,
                variants,
            },
        ) if type_id == expected => variants
            .iter()
            .find(|(id, _)| id == variant)
            .map(|(_, code)| *code)
            .ok_or(FiniteDecisionError::Invalid("unknown-variant")),
        _ => Err(FiniteDecisionError::Invalid("input-scalar-shape")),
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use zeno_fcis_synthesis::finite::Op;

    fn bytes(program: &Program) -> Vec<u8> {
        program.value().canonical_bytes().expect("finite encoding")
    }

    #[test]
    fn closed_plan_admits_total_program_and_rejects_wrong_abi() {
        let program = Program::try_new(
            vec![FiniteDomain::Bool],
            vec![FiniteDomain::Int { min: 0, max: 1 }],
            vec![Op::Input(0), Op::Int(0), Op::Int(1), Op::Select(0, 1, 2)],
            vec![3],
        )
        .expect("typed program");
        let admit = |inputs| {
            FiniteDecisionContract::try_new(
                Hash32::new([1; 32]),
                &bytes(&program),
                inputs,
                vec![FiniteDomain::Int { min: 0, max: 1 }],
                vec![InputBinding {
                    source: InputSource::State,
                    field: 1,
                    scalar: InputScalar::Bool,
                }],
                vec![
                    DecisionBranch {
                        code: 0,
                        rejection: None,
                    },
                    DecisionBranch {
                        code: 1,
                        rejection: Some(SemanticId::try_new(1).expect("id")),
                    },
                ],
                vec![],
                vec![],
                BudgetLimits::zero(),
                4,
            )
        };
        assert!(admit(vec![FiniteDomain::Bool]).is_ok());
        assert!(matches!(
            admit(vec![FiniteDomain::Int { min: 0, max: 1 }]),
            Err(FiniteDecisionError::Invalid("program-abi"))
        ));
    }

    #[test]
    fn eager_trap_refuses_contract_before_authority() {
        let program = Program::try_new(
            vec![FiniteDomain::Int {
                min: i64::MAX,
                max: i64::MAX,
            }],
            vec![FiniteDomain::Int { min: 0, max: 1 }],
            vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
            vec![2],
        )
        .expect("typed program");
        let result = FiniteDecisionContract::try_new(
            Hash32::new([1; 32]),
            &bytes(&program),
            program.inputs().to_vec(),
            program.outputs().to_vec(),
            vec![InputBinding {
                source: InputSource::State,
                field: 1,
                scalar: InputScalar::I128,
            }],
            vec![
                DecisionBranch {
                    code: 0,
                    rejection: None,
                },
                DecisionBranch {
                    code: 1,
                    rejection: None,
                },
            ],
            vec![],
            vec![],
            BudgetLimits::zero(),
            3,
        );
        assert!(matches!(result, Err(FiniteDecisionError::Finite(_))));
    }
}
