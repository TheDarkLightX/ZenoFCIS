//! Authority-owned execution of fixed composed domain machines.
//!
//! A [`ComposedDomainProgram`] is one concrete
//! [`CatalogTransitionProgram`].
//! It derives every local state, command, and context envelope from the exact
//! root invocation supplied by the production authority. It then executes one
//! reviewed [`ExecutableComposition`], projects every successor state cell back
//! to its root path, and accounts for every output as either an internal route,
//! one catalogued effect, or one catalogued outbox obligation.
//!
//! The crate adds no commit witness and no shell. The existing catalog authority
//! remains the only constructor of production commit authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::array;
use core::fmt;
use core::marker::PhantomData;

use zeno_fcis_authority::{CatalogTransitionProgram, ReviewedTransitionInput};
use zeno_fcis_catalog::{CatalogError, ProjectCatalog};
use zeno_fcis_codec::{CanonicalEncode, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_core::{
    Budget, BudgetExceeded, BudgetLimits, BudgetUsed, Decision, DecisionKind, Resource,
};
use zeno_fcis_crypto::ApprovedCommitmentProvider;
use zeno_fcis_domain::{
    DomainError, DomainMachine, ExecutableComposition, MachineInterface, SystemCandidate,
};
use zeno_fcis_patch::{PatchError, PathSegment, ValuePath, value_at};
use zeno_fcis_plan::{Effect, OutboxEntry};
use zeno_fcis_project::SemanticId;
use zeno_fcis_schema::{
    SchemaAdmittedTypeEnvelope, SchemaEnvelopeError, ValidationLimits, ValueValidationError,
};
use zeno_fcis_transition::{
    CataloguedTransitionBuilder, TransitionDecision, TransitionError, canonical_access_path,
};
use zeno_fcis_value::Value;

/// Canonical format version for composed-program configuration values.
pub const COMPOSED_PROGRAM_FORMAT_VERSION: u16 = 1;

const RESOURCE_ORDER: [Resource; 7] = [
    Resource::Read,
    Resource::Write,
    Resource::Candidate,
    Resource::Effect,
    Resource::Byte,
    Resource::WitnessByte,
    Resource::Depth,
];

/// Required treatment of one fixed output position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalOutput {
    /// The interface position is inactive.
    Inactive,
    /// The output is consumed only by the exact internal route matrix.
    Internal,
    /// Every emitted value becomes one catalogued authoritative effect.
    Effect {
        /// Catalogued operation identifier.
        operation: SemanticId,
        /// Fixed authority-domain commitment.
        authority: Hash32,
        /// Fixed subject commitment.
        subject: Hash32,
    },
    /// Every emitted value becomes one catalogued outbox obligation.
    Outbox {
        /// Catalogued channel identifier.
        channel: SemanticId,
        /// Fixed schema-admitted destination value.
        destination: Value,
    },
}

impl CanonicalEncode for ExternalOutput {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::Inactive => output.push(0),
            Self::Internal => output.push(1),
            Self::Effect {
                operation,
                authority,
                subject,
            } => {
                output.push(2);
                operation.encode_to(output)?;
                output.extend_from_slice(authority.as_bytes());
                output.extend_from_slice(subject.as_bytes());
            }
            Self::Outbox {
                channel,
                destination,
            } => {
                output.push(3);
                channel.encode_to(output)?;
                put_blob(output, &destination.canonical_bytes()?)?;
            }
        }
        Ok(())
    }
}

/// Closed mapping between one aggregate root invocation and fixed domain rows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectionPlan<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> {
    state_paths: Box<[[ValuePath; STATE_SLOTS]; MACHINES]>,
    command_paths: Box<[ValuePath; MACHINES]>,
    context_paths: Box<[ValuePath; MACHINES]>,
    reason_domains: Box<[Box<[SemanticId]>; MACHINES]>,
    outputs: Box<[[ExternalOutput; PORTS]; MACHINES]>,
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    ProjectionPlan<MACHINES, STATE_SLOTS, PORTS>
{
    /// Creates a complete fixed projection and rejects overlapping root-state ownership.
    pub fn try_new(
        state_paths: [[ValuePath; STATE_SLOTS]; MACHINES],
        command_paths: [ValuePath; MACHINES],
        context_paths: [ValuePath; MACHINES],
        mut reason_domains: [Vec<SemanticId>; MACHINES],
        outputs: [[ExternalOutput; PORTS]; MACHINES],
    ) -> Result<Self, ComposedProgramError> {
        let flattened = state_paths.iter().flatten().collect::<Vec<_>>();
        for (index, left) in flattened.iter().enumerate() {
            if flattened
                .iter()
                .skip(index + 1)
                .any(|right| left.is_prefix_of(right) || right.is_prefix_of(left))
            {
                return Err(ComposedProgramError::OverlappingRootStatePaths);
            }
        }
        if state_paths
            .iter()
            .flatten()
            .chain(command_paths.iter())
            .chain(context_paths.iter())
            .flat_map(|path| path.segments())
            .any(|segment| matches!(segment, PathSegment::MapKey(_)))
        {
            return Err(ComposedProgramError::MapKeyProjectionUnsupported);
        }
        for reasons in &mut reason_domains {
            if reasons.is_empty() {
                return Err(ComposedProgramError::EmptyReasonDomain);
            }
            reasons.sort();
            if reasons.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(ComposedProgramError::DuplicateMachineReason);
            }
        }
        Ok(Self {
            state_paths: Box::new(state_paths),
            command_paths: Box::new(command_paths),
            context_paths: Box::new(context_paths),
            reason_domains: Box::new(reason_domains.map(Vec::into_boxed_slice)),
            outputs: Box::new(outputs),
        })
    }

    /// Returns the root-state path for every local state cell.
    #[must_use]
    pub const fn state_paths(&self) -> &[[ValuePath; STATE_SLOTS]; MACHINES] {
        &self.state_paths
    }

    /// Returns the root-command path for every component row.
    #[must_use]
    pub const fn command_paths(&self) -> &[ValuePath; MACHINES] {
        &self.command_paths
    }

    /// Returns the authenticated root-context path for every component row.
    #[must_use]
    pub const fn context_paths(&self) -> &[ValuePath; MACHINES] {
        &self.context_paths
    }

    /// Returns the complete stable reason domain for every component row.
    #[must_use]
    pub const fn reason_domains(&self) -> &[Box<[SemanticId]>; MACHINES] {
        &self.reason_domains
    }

    /// Returns the complete fixed output treatment matrix.
    #[must_use]
    pub const fn outputs(&self) -> &[[ExternalOutput; PORTS]; MACHINES] {
        &self.outputs
    }
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for ProjectionPlan<MACHINES, STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-COMPOSED-PROJECTION\0");
        output.extend_from_slice(&COMPOSED_PROGRAM_FORMAT_VERSION.to_be_bytes());
        put_usize(output, MACHINES)?;
        put_usize(output, STATE_SLOTS)?;
        put_usize(output, PORTS)?;
        for row in self.state_paths.iter() {
            for path in row {
                put_blob(output, &path.canonical_bytes()?)?;
            }
        }
        for path in self.command_paths.iter() {
            put_blob(output, &path.canonical_bytes()?)?;
        }
        for path in self.context_paths.iter() {
            put_blob(output, &path.canonical_bytes()?)?;
        }
        for reasons in self.reason_domains.iter() {
            put_usize(output, reasons.len())?;
            for reason in reasons.iter() {
                reason.encode_to(output)?;
            }
        }
        for row in self.outputs.iter() {
            for rule in row {
                put_blob(output, &rule.canonical_bytes()?)?;
            }
        }
        Ok(())
    }
}

/// One reviewed composed transition implementation owned by a catalog authority.
pub struct ComposedDomainProgram<
    H,
    M,
    const MACHINES: usize,
    const STATE_SLOTS: usize,
    const PORTS: usize,
> where
    H: ApprovedCommitmentProvider,
    M: DomainMachine<STATE_SLOTS, PORTS>,
{
    catalog_hash: Hash32,
    semantic_program_hash: Hash32,
    executable: ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>,
    machines: Box<[M; MACHINES]>,
    machine_build_hashes: Box<[Hash32; MACHINES]>,
    projection: ProjectionPlan<MACHINES, STATE_SLOTS, PORTS>,
    machine_limits: Box<[BudgetLimits; MACHINES]>,
    projection_limits: BudgetLimits,
    total_limits: BudgetLimits,
    marker: PhantomData<H>,
}

impl<H, M, const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    ComposedDomainProgram<H, M, MACHINES, STATE_SLOTS, PORTS>
where
    H: ApprovedCommitmentProvider,
    M: DomainMachine<STATE_SLOTS, PORTS>,
{
    /// Binds one catalog, composition, nominal machine type, complete projection, and budgets.
    pub fn try_new(
        catalog: &ProjectCatalog,
        executable: ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>,
        machines: [M; MACHINES],
        projection: ProjectionPlan<MACHINES, STATE_SLOTS, PORTS>,
        machine_build_hashes: [Hash32; MACHINES],
        machine_limits: [BudgetLimits; MACHINES],
        projection_limits: BudgetLimits,
    ) -> Result<Self, ComposedProgramError> {
        let catalog_hash = catalog.commitment::<H>()?;
        if machine_build_hashes.contains(&Hash32::ZERO) {
            return Err(ComposedProgramError::ZeroMachineBuildHash);
        }
        validate_interfaces(catalog, &executable, &machines)?;
        validate_outputs(catalog, &executable, &projection)?;
        validate_reason_domains(catalog, &executable, &projection)?;
        validate_projection_capacity::<MACHINES, STATE_SLOTS, PORTS>(
            &projection,
            projection_limits,
        )?;
        let total_limits = sum_limits(&machine_limits, projection_limits)?;
        let semantic_program_hash = derive_semantic_program_hash::<H, MACHINES, STATE_SLOTS, PORTS>(
            &executable,
            &projection,
            &machine_build_hashes,
            &machine_limits,
            projection_limits,
        )?;
        if semantic_program_hash != catalog.profile().bindings().algorithm_hash {
            return Err(ComposedProgramError::AlgorithmBindingMismatch);
        }
        Ok(Self {
            catalog_hash,
            semantic_program_hash,
            executable,
            machines: Box::new(machines),
            machine_build_hashes: Box::new(machine_build_hashes),
            projection,
            machine_limits: Box::new(machine_limits),
            projection_limits,
            total_limits,
            marker: PhantomData,
        })
    }

    /// Returns the exact executable composition.
    #[must_use]
    pub const fn executable(&self) -> &ExecutableComposition<MACHINES, STATE_SLOTS, PORTS> {
        &self.executable
    }

    /// Returns the complete root projection.
    #[must_use]
    pub const fn projection(&self) -> &ProjectionPlan<MACHINES, STATE_SLOTS, PORTS> {
        &self.projection
    }

    /// Returns exact per-machine logical limits.
    #[must_use]
    pub const fn machine_limits(&self) -> &[BudgetLimits; MACHINES] {
        &self.machine_limits
    }

    /// Returns reviewed implementation/configuration identities for every machine row.
    #[must_use]
    pub const fn machine_build_hashes(&self) -> &[Hash32; MACHINES] {
        &self.machine_build_hashes
    }

    /// Returns the independent projection-work limit.
    #[must_use]
    pub const fn projection_limits(&self) -> BudgetLimits {
        self.projection_limits
    }

    /// Returns the checked aggregate machine-plus-projection limit.
    #[must_use]
    pub const fn total_limits(&self) -> BudgetLimits {
        self.total_limits
    }

    /// Returns the program identity bound to the project algorithm and authority build fields.
    #[must_use]
    pub const fn semantic_program_hash(&self) -> Hash32 {
        self.semantic_program_hash
    }

    fn run(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, ComposedProgramError> {
        if input.catalog().commitment::<H>()? != self.catalog_hash {
            return Err(ComposedProgramError::CatalogMismatch);
        }
        let validation = ValidationLimits {
            max_depth: input.limits().max_state_depth(),
            max_nodes: input.limits().max_state_nodes(),
        };
        let mut projection_budget = Budget::new(self.projection_limits);
        let state_rows = project_state_with_ports::<H, MACHINES, STATE_SLOTS, PORTS>(
            input.catalog(),
            input.pre_state().value().value(),
            self.executable.interfaces(),
            self.projection.state_paths(),
            validation,
            &mut projection_budget,
        )?;
        let commands = project_invocations::<H, MACHINES, STATE_SLOTS, PORTS>(
            input.catalog(),
            input.command().value().value(),
            self.executable.interfaces(),
            self.projection.command_paths(),
            true,
            validation,
            &mut projection_budget,
        )?;
        let contexts = project_invocations::<H, MACHINES, STATE_SLOTS, PORTS>(
            input.catalog(),
            input.context().value().value(),
            self.executable.interfaces(),
            self.projection.context_paths(),
            false,
            validation,
            &mut projection_budget,
        )?;
        let state = self.executable.admit_state(state_rows)?;
        let invocation = self.executable.admit_invocation(commands, contexts)?;
        let machines: [&dyn DomainMachine<STATE_SLOTS, PORTS>; MACHINES] =
            array::from_fn(|index| &self.machines[index] as &dyn DomainMachine<STATE_SLOTS, PORTS>);
        let execution =
            self.executable
                .execute(machines, &state, &invocation, *self.machine_limits)?;

        let projected = project_decision(
            &execution,
            &state,
            &self.executable,
            &self.projection,
            input.catalog(),
            &mut projection_budget,
        )?;
        let used = aggregate_usage(
            self.total_limits,
            execution.reports(),
            projection_budget.used(),
        )?;
        self.seal(input, projected, used)
    }

    fn seal(
        &self,
        input: ReviewedTransitionInput<'_>,
        projected: ProjectedDecision,
        used: BudgetUsed,
    ) -> Result<TransitionDecision, ComposedProgramError> {
        let expected = input.expected_bindings();
        let mut builder = CataloguedTransitionBuilder::<H>::try_new(
            input.catalog(),
            input.pre_state().value().value(),
            input.state_domain(),
            expected.command_hash(),
            expected.context_hash(),
            used,
            input.limits(),
        )?;
        for row in self.projection.state_paths().iter() {
            for path in row {
                if let Some((_, value)) =
                    projected.updates.iter().find(|(target, _)| target == path)
                {
                    builder.update(path.clone(), value.clone())?;
                } else {
                    let _ = builder.read(path.clone())?;
                }
            }
        }
        for path in self.projection.context_paths().iter() {
            let access = canonical_access_path::<H>(
                input.catalog().profile().context_type().get(),
                path,
                input.limits().max_map_key_bytes(),
            )?;
            builder.observe_context(access)?;
        }
        for effect in projected.effects {
            builder.emit(effect)?;
        }
        for entry in projected.outbox {
            builder.enqueue(entry)?;
        }
        match projected.outcome {
            ProjectedOutcome::Accept => {}
            ProjectedOutcome::Reject(reason) => {
                builder.require(false, reason)?;
            }
            ProjectedOutcome::CommittedFailure(reason) => {
                builder.fail_if(true, reason)?;
            }
        }
        Ok(builder.seal()?)
    }
}

impl<H, M, const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    CatalogTransitionProgram<H> for ComposedDomainProgram<H, M, MACHINES, STATE_SLOTS, PORTS>
where
    H: ApprovedCommitmentProvider,
    M: DomainMachine<STATE_SLOTS, PORTS>,
{
    type Error = ComposedProgramError;

    fn transition_build_hash(&self) -> Hash32 {
        self.semantic_program_hash
    }

    fn execute(
        &self,
        input: ReviewedTransitionInput<'_>,
    ) -> Result<TransitionDecision, Self::Error> {
        self.run(input)
    }
}

struct ProjectedDecision {
    outcome: ProjectedOutcome,
    updates: Vec<(ValuePath, Value)>,
    effects: Vec<Effect>,
    outbox: Vec<OutboxEntry>,
}

enum ProjectedOutcome {
    Accept,
    Reject(SemanticId),
    CommittedFailure(SemanticId),
}

fn project_decision<const M: usize, const S: usize, const P: usize>(
    execution: &zeno_fcis_domain::SystemExecution<M, S, P>,
    pre_state: &zeno_fcis_domain::FixedStateMatrix<M, S>,
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
    catalog: &ProjectCatalog,
    budget: &mut Budget,
) -> Result<ProjectedDecision, ComposedProgramError> {
    match execution.decision() {
        Decision::Reject(rejected) => {
            validate_machine_reason(
                catalog,
                executable,
                projection,
                rejected.reason().component(),
                rejected.reason().reason(),
                DecisionKind::Reject,
            )?;
            Ok(ProjectedDecision {
                outcome: ProjectedOutcome::Reject(rejected.reason().reason()),
                updates: Vec::new(),
                effects: Vec::new(),
                outbox: Vec::new(),
            })
        }
        Decision::Accept(accepted) => project_candidate(
            ProjectedOutcome::Accept,
            accepted.candidate(),
            execution.reports(),
            pre_state,
            executable,
            projection,
            budget,
        ),
        Decision::CommittedFailure(failed) => {
            validate_machine_reason(
                catalog,
                executable,
                projection,
                failed.reason().component(),
                failed.reason().reason(),
                DecisionKind::CommittedFailure,
            )?;
            project_candidate(
                ProjectedOutcome::CommittedFailure(failed.reason().reason()),
                failed.candidate(),
                execution.reports(),
                pre_state,
                executable,
                projection,
                budget,
            )
        }
    }
}

fn project_candidate<const M: usize, const S: usize, const P: usize>(
    outcome: ProjectedOutcome,
    candidate: &SystemCandidate<M, S, P>,
    reports: &[Option<zeno_fcis_domain::MachineExecutionReport>; M],
    pre_state: &zeno_fcis_domain::FixedStateMatrix<M, S>,
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
    budget: &mut Budget,
) -> Result<ProjectedDecision, ComposedProgramError> {
    let mut updates = Vec::new();
    for machine in 0..M {
        for slot in 0..S {
            let before = &pre_state.rows()[machine][slot];
            let after = &candidate.post_state().rows()[machine][slot];
            if before != after {
                budget.charge(Resource::Write, 1)?;
                budget.charge(Resource::Byte, after.encoded_length())?;
                updates.push((
                    projection.state_paths()[machine][slot].clone(),
                    after.value().value().clone(),
                ));
            }
        }
    }

    let mut effects = Vec::new();
    let mut outbox = Vec::new();
    for (machine, report) in reports.iter().enumerate() {
        for port in 0..P {
            let value = candidate.outputs().rows()[machine][port].as_ref();
            if !validate_output_presence(
                machine,
                port,
                report.is_some(),
                value.is_some(),
                &projection.outputs()[machine][port],
            )? {
                continue;
            }
            let Some(value) = value else { continue };
            budget.charge(Resource::Byte, value.encoded_length())?;
            match &projection.outputs()[machine][port] {
                ExternalOutput::Internal => {
                    if executable.routes()[machine][port].is_none() {
                        return Err(ComposedProgramError::OutputRuleMismatch { machine, port });
                    }
                }
                ExternalOutput::Effect {
                    operation,
                    authority,
                    subject,
                } => {
                    budget.charge(Resource::Effect, 1)?;
                    effects.push(Effect::new(
                        u32_len(effects.len())?,
                        operation.get(),
                        *authority,
                        *subject,
                        value.value().value().clone(),
                    ));
                }
                ExternalOutput::Outbox {
                    channel,
                    destination,
                } => {
                    budget.charge(Resource::Effect, 1)?;
                    outbox.push(OutboxEntry::new(
                        u32_len(outbox.len())?,
                        channel.get(),
                        destination.clone(),
                        value.value().value().clone(),
                    ));
                }
                ExternalOutput::Inactive => {
                    return Err(ComposedProgramError::OutputRuleMismatch { machine, port });
                }
            }
        }
    }
    Ok(ProjectedDecision {
        outcome,
        updates,
        effects,
        outbox,
    })
}

fn validate_output_presence(
    machine: usize,
    port: usize,
    executed: bool,
    present: bool,
    rule: &ExternalOutput,
) -> Result<bool, ComposedProgramError> {
    if !executed {
        if present {
            return Err(ComposedProgramError::UnexecutedMachineOutput { machine, port });
        }
        return Ok(false);
    }
    if !present
        && matches!(
            rule,
            ExternalOutput::Effect { .. } | ExternalOutput::Outbox { .. }
        )
    {
        return Err(ComposedProgramError::MissingRequiredOutput { machine, port });
    }
    Ok(present)
}

fn project_state_with_ports<
    H: ApprovedCommitmentProvider,
    const M: usize,
    const S: usize,
    const P: usize,
>(
    catalog: &ProjectCatalog,
    root: &Value,
    interfaces: &[MachineInterface<S, P>; M],
    paths: &[[ValuePath; S]; M],
    validation: ValidationLimits,
    budget: &mut Budget,
) -> Result<[[SchemaAdmittedTypeEnvelope; S]; M], ComposedProgramError> {
    let mut rows = Vec::with_capacity(M);
    for (interface, machine_paths) in interfaces.iter().zip(paths.iter()) {
        let mut row = Vec::with_capacity(S);
        for (slot, path) in machine_paths.iter().enumerate() {
            row.push(project_one::<H>(
                catalog,
                root,
                path,
                interface.state()[slot].envelope().type_id(),
                validation,
                budget,
            )?);
        }
        rows.push(to_array(row)?);
    }
    to_array(rows)
}

fn project_invocations<
    H: ApprovedCommitmentProvider,
    const M: usize,
    const S: usize,
    const P: usize,
>(
    catalog: &ProjectCatalog,
    root: &Value,
    interfaces: &[MachineInterface<S, P>; M],
    paths: &[ValuePath; M],
    command: bool,
    validation: ValidationLimits,
    budget: &mut Budget,
) -> Result<[SchemaAdmittedTypeEnvelope; M], ComposedProgramError> {
    let mut values = Vec::with_capacity(M);
    for machine in 0..M {
        let type_id = if command {
            interfaces[machine].command().type_id()
        } else {
            interfaces[machine].context().envelope().type_id()
        };
        values.push(project_one::<H>(
            catalog,
            root,
            &paths[machine],
            type_id,
            validation,
            budget,
        )?);
    }
    to_array(values)
}

fn project_one<H: ApprovedCommitmentProvider>(
    catalog: &ProjectCatalog,
    root: &Value,
    path: &ValuePath,
    type_id: zeno_fcis_schema::TypeId,
    validation: ValidationLimits,
    budget: &mut Budget,
) -> Result<SchemaAdmittedTypeEnvelope, ComposedProgramError> {
    budget.charge(Resource::Read, 1)?;
    let value = value_at(root, path)?.clone();
    let envelope =
        SchemaAdmittedTypeEnvelope::try_new::<H>(catalog.schema(), type_id, value, validation)?;
    budget.charge(Resource::Byte, envelope.encoded_length())?;
    Ok(envelope)
}

fn validate_interfaces<M, const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>(
    catalog: &ProjectCatalog,
    executable: &ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>,
    machines: &[M; MACHINES],
) -> Result<(), ComposedProgramError>
where
    M: DomainMachine<STATE_SLOTS, PORTS>,
{
    for (machine, (implementation, interface)) in machines
        .iter()
        .zip(executable.interfaces().iter())
        .enumerate()
    {
        if implementation.component_id() != interface.component() {
            return Err(ComposedProgramError::MachineIdentityMismatch { machine });
        }
        let bindings = interface
            .state()
            .iter()
            .map(|value| value.envelope().schema_hash())
            .chain([interface.command().schema_hash()])
            .chain([interface.context().envelope().schema_hash()])
            .chain(
                interface
                    .inputs()
                    .iter()
                    .flatten()
                    .map(|value| value.envelope().schema_hash()),
            )
            .chain(
                interface
                    .outputs()
                    .iter()
                    .flatten()
                    .map(|value| value.envelope().schema_hash()),
            );
        if bindings
            .into_iter()
            .any(|hash| hash != catalog.schema_hash())
        {
            return Err(ComposedProgramError::InterfaceSchemaMismatch { machine });
        }
    }
    Ok(())
}

fn validate_outputs<const M: usize, const S: usize, const P: usize>(
    catalog: &ProjectCatalog,
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
) -> Result<(), ComposedProgramError> {
    for machine in 0..M {
        for port in 0..P {
            let binding = executable.interfaces()[machine].outputs()[port].as_ref();
            let routed = executable.routes()[machine][port].is_some();
            let rule = &projection.outputs()[machine][port];
            match (binding, routed, rule) {
                (None, false, ExternalOutput::Inactive)
                | (Some(_), true, ExternalOutput::Internal) => {}
                (
                    Some(binding),
                    false,
                    ExternalOutput::Effect {
                        operation,
                        authority,
                        subject,
                    },
                ) => {
                    let definition = catalog
                        .manifest()
                        .effect(*operation)
                        .ok_or(ComposedProgramError::UnknownEffect(*operation))?;
                    if definition.payload_type() != binding.envelope().type_id() {
                        return Err(ComposedProgramError::OutputPayloadTypeMismatch {
                            machine,
                            port,
                        });
                    }
                    if !definition.authority_requirement().admits(*authority)
                        || !definition.subject_requirement().admits(*subject)
                    {
                        return Err(ComposedProgramError::OutputHashRequirementMismatch {
                            machine,
                            port,
                        });
                    }
                }
                (
                    Some(binding),
                    false,
                    ExternalOutput::Outbox {
                        channel,
                        destination,
                    },
                ) => {
                    let definition = catalog
                        .manifest()
                        .channel(*channel)
                        .ok_or(ComposedProgramError::UnknownChannel(*channel))?;
                    if definition.payload_type() != binding.envelope().type_id() {
                        return Err(ComposedProgramError::OutputPayloadTypeMismatch {
                            machine,
                            port,
                        });
                    }
                    if !executable.spec().components()[machine]
                        .outbox()
                        .covers(binding.path())
                    {
                        return Err(ComposedProgramError::UndeclaredOutboxOutput { machine, port });
                    }
                    catalog.schema().validate_value(
                        definition.destination_type(),
                        destination,
                        ValidationLimits::default(),
                    )?;
                }
                _ => return Err(ComposedProgramError::OutputRuleMismatch { machine, port }),
            }
        }
    }
    Ok(())
}

fn validate_reason_domains<const M: usize, const S: usize, const P: usize>(
    catalog: &ProjectCatalog,
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
) -> Result<(), ComposedProgramError> {
    for reasons in projection.reason_domains().iter() {
        for reason in reasons.iter() {
            if catalog.manifest().reason(*reason).is_none() {
                return Err(ComposedProgramError::UnknownMachineReason(*reason));
            }
        }
    }
    let mut previous_max: Option<(u32, SemanticId)> = None;
    for component in executable.spec().merge_order() {
        let row = executable
            .interfaces()
            .iter()
            .position(|interface| interface.component() == *component)
            .ok_or(ComposedProgramError::InvalidShape)?;
        let mut keys = projection.reason_domains()[row]
            .iter()
            .map(|reason| {
                let definition = catalog
                    .manifest()
                    .reason(*reason)
                    .ok_or(ComposedProgramError::UnknownMachineReason(*reason))?;
                Ok((definition.precedence(), *reason))
            })
            .collect::<Result<Vec<_>, ComposedProgramError>>()?;
        keys.sort();
        let Some(first) = keys.first().copied() else {
            return Err(ComposedProgramError::EmptyReasonDomain);
        };
        if previous_max.is_some_and(|previous| previous > first) {
            return Err(ComposedProgramError::ReasonPrecedenceMismatch);
        }
        previous_max = keys.last().copied();
    }
    Ok(())
}

fn validate_machine_reason<const M: usize, const S: usize, const P: usize>(
    catalog: &ProjectCatalog,
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
    component: zeno_fcis_compose::ComponentId,
    reason: SemanticId,
    kind: DecisionKind,
) -> Result<(), ComposedProgramError> {
    let row = executable
        .interfaces()
        .iter()
        .position(|interface| interface.component() == component)
        .ok_or(ComposedProgramError::InvalidShape)?;
    if projection.reason_domains()[row]
        .binary_search(&reason)
        .is_err()
    {
        return Err(ComposedProgramError::UndeclaredMachineReason { component, reason });
    }
    catalog.validate_reason(reason.get(), kind)?;
    Ok(())
}

/// Derives the exact semantic program identity reviewed by profiles and authorities.
pub fn derive_semantic_program_hash<
    H: ApprovedCommitmentProvider,
    const M: usize,
    const S: usize,
    const P: usize,
>(
    executable: &ExecutableComposition<M, S, P>,
    projection: &ProjectionPlan<M, S, P>,
    machine_build_hashes: &[Hash32; M],
    machine_limits: &[BudgetLimits; M],
    projection_limits: BudgetLimits,
) -> Result<Hash32, ComposedProgramError> {
    if machine_build_hashes.contains(&Hash32::ZERO) {
        return Err(ComposedProgramError::ZeroMachineBuildHash);
    }
    let mut bytes = Vec::new();
    put_blob(&mut bytes, &executable.canonical_bytes()?)?;
    put_blob(&mut bytes, &projection.canonical_bytes()?)?;
    for hash in machine_build_hashes {
        bytes.extend_from_slice(hash.as_bytes());
    }
    for limits in machine_limits {
        encode_limits(*limits, &mut bytes);
    }
    encode_limits(projection_limits, &mut bytes);
    let domain = Domain::new(
        "zeno-fcis/composed-domain-program",
        COMPOSED_PROGRAM_FORMAT_VERSION,
    )?;
    Ok(commitment::<H>(domain, &bytes)?)
}

fn validate_projection_capacity<const M: usize, const S: usize, const P: usize>(
    projection: &ProjectionPlan<M, S, P>,
    limits: BudgetLimits,
) -> Result<(), ComposedProgramError> {
    let reads = usize_to_u64(M)?
        .checked_mul(
            usize_to_u64(S)?
                .checked_add(2)
                .ok_or(ComposedProgramError::LimitOverflow)?,
        )
        .ok_or(ComposedProgramError::LimitOverflow)?;
    let writes = usize_to_u64(M)?
        .checked_mul(usize_to_u64(S)?)
        .ok_or(ComposedProgramError::LimitOverflow)?;
    let effects = usize_to_u64(
        projection
            .outputs()
            .iter()
            .flatten()
            .filter(|rule| {
                matches!(
                    rule,
                    ExternalOutput::Effect { .. } | ExternalOutput::Outbox { .. }
                )
            })
            .count(),
    )?;
    for (resource, required) in [
        (Resource::Read, reads),
        (Resource::Write, writes),
        (Resource::Effect, effects),
    ] {
        if limits.limit(resource) < required {
            return Err(ComposedProgramError::ProjectionLimitTooSmall {
                resource,
                required,
                actual: limits.limit(resource),
            });
        }
    }
    Ok(())
}

fn sum_limits<const M: usize>(
    machine_limits: &[BudgetLimits; M],
    projection: BudgetLimits,
) -> Result<BudgetLimits, ComposedProgramError> {
    let mut total = BudgetLimits::zero();
    for resource in RESOURCE_ORDER {
        let mut amount = projection.limit(resource);
        for limits in machine_limits {
            amount = amount
                .checked_add(limits.limit(resource))
                .ok_or(ComposedProgramError::LimitOverflow)?;
        }
        total = total.with_limit(resource, amount);
    }
    Ok(total)
}

fn aggregate_usage<const M: usize>(
    limits: BudgetLimits,
    reports: &[Option<zeno_fcis_domain::MachineExecutionReport>; M],
    projection: BudgetUsed,
) -> Result<BudgetUsed, ComposedProgramError> {
    let mut budget = Budget::new(limits);
    for resource in RESOURCE_ORDER {
        budget.charge(resource, projection.used(resource))?;
        for report in reports.iter().flatten() {
            budget.charge(resource, report.used().used(resource))?;
        }
    }
    Ok(budget.used())
}

fn to_array<T, const N: usize>(values: Vec<T>) -> Result<[T; N], ComposedProgramError> {
    values
        .try_into()
        .map_err(|_| ComposedProgramError::InvalidShape)
}

fn usize_to_u64(value: usize) -> Result<u64, ComposedProgramError> {
    u64::try_from(value).map_err(|_| ComposedProgramError::LimitOverflow)
}

fn u32_len(value: usize) -> Result<u32, ComposedProgramError> {
    u32::try_from(value).map_err(|_| ComposedProgramError::LimitOverflow)
}

fn put_usize(output: &mut Vec<u8>, value: usize) -> Result<(), EncodeError> {
    let value = u32::try_from(value).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let len = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&len.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn encode_limits(limits: BudgetLimits, output: &mut Vec<u8>) {
    for resource in RESOURCE_ORDER {
        output.extend_from_slice(&limits.limit(resource).to_be_bytes());
    }
}

/// Fail-closed construction or execution error.
#[derive(Debug)]
pub enum ComposedProgramError {
    /// Compile-time dimensions could not be represented.
    InvalidShape,
    /// Two local state cells map to overlapping root paths.
    OverlappingRootStatePaths,
    /// Map-key projections are excluded from the direct-subtree v1 profile.
    MapKeyProjectionUnsupported,
    /// Every machine must declare a nonempty closed reason domain.
    EmptyReasonDomain,
    /// One machine reason domain contains the same identifier twice.
    DuplicateMachineReason,
    /// The authority supplied a catalog other than the program-bound catalog.
    CatalogMismatch,
    /// The derived program identity differs from the project algorithm binding.
    AlgorithmBindingMismatch,
    /// One machine implementation/configuration identity is zero.
    ZeroMachineBuildHash,
    /// One owned machine does not implement its fixed component row.
    MachineIdentityMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// One interface envelope belongs to another schema.
    InterfaceSchemaMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// Output route and external treatment disagree.
    OutputRuleMismatch {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// An external output does not match the catalogued payload type.
    OutputPayloadTypeMismatch {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// Fixed effect authority or subject violates its catalog definition.
    OutputHashRequirementMismatch {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// An effect projection names no catalog definition.
    UnknownEffect(SemanticId),
    /// An outbox projection names no catalog definition.
    UnknownChannel(SemanticId),
    /// One machine reason is absent from the catalog.
    UnknownMachineReason(SemanticId),
    /// Merge order disagrees with the catalog's global reason precedence.
    ReasonPrecedenceMismatch,
    /// A machine returned a reason outside its closed row domain.
    UndeclaredMachineReason {
        /// Component that returned the reason.
        component: zeno_fcis_compose::ComponentId,
        /// Undeclared stable reason.
        reason: SemanticId,
    },
    /// An outbox projection lacks the component's explicit outbox footprint.
    UndeclaredOutboxOutput {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// An active external output was absent from an accepted candidate.
    MissingRequiredOutput {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// A candidate contained output for a machine row that did not execute.
    UnexecutedMachineOutput {
        /// Canonical machine row.
        machine: usize,
        /// Fixed output position.
        port: usize,
    },
    /// One logical-limit sum overflowed.
    LimitOverflow,
    /// Projection work cannot fit its independent deterministic limit.
    ProjectionLimitTooSmall {
        /// Resource with insufficient capacity.
        resource: Resource,
        /// Fixed worst-case minimum.
        required: u64,
        /// Supplied limit.
        actual: u64,
    },
    /// Catalog construction or validation failed.
    Catalog(CatalogError),
    /// Canonical encoding or commitment failed.
    Encode(EncodeError),
    /// Fixed-domain construction or execution failed.
    Domain(DomainError),
    /// Value-path projection failed.
    Patch(PatchError),
    /// Schema-envelope admission failed.
    SchemaEnvelope(SchemaEnvelopeError),
    /// Projected value failed schema validation.
    ValueValidation(ValueValidationError),
    /// Deterministic logical work exceeded its bound.
    Budget(BudgetExceeded),
    /// Catalogued candidate construction failed.
    Transition(TransitionError),
}

impl From<CatalogError> for ComposedProgramError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}
impl From<EncodeError> for ComposedProgramError {
    fn from(value: EncodeError) -> Self {
        Self::Encode(value)
    }
}
impl From<DomainError> for ComposedProgramError {
    fn from(value: DomainError) -> Self {
        Self::Domain(value)
    }
}
impl From<PatchError> for ComposedProgramError {
    fn from(value: PatchError) -> Self {
        Self::Patch(value)
    }
}
impl From<SchemaEnvelopeError> for ComposedProgramError {
    fn from(value: SchemaEnvelopeError) -> Self {
        Self::SchemaEnvelope(value)
    }
}
impl From<ValueValidationError> for ComposedProgramError {
    fn from(value: ValueValidationError) -> Self {
        Self::ValueValidation(value)
    }
}
impl From<BudgetExceeded> for ComposedProgramError {
    fn from(value: BudgetExceeded) -> Self {
        Self::Budget(value)
    }
}
impl From<TransitionError> for ComposedProgramError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

impl fmt::Display for ComposedProgramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "composed domain program failed: {self:?}")
    }
}

impl core::error::Error for ComposedProgramError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect_rule() -> ExternalOutput {
        ExternalOutput::Effect {
            operation: SemanticId::try_new(20)
                .unwrap_or_else(|error| panic!("semantic id: {error}")),
            authority: Hash32::ZERO,
            subject: Hash32::ZERO,
        }
    }

    #[test]
    fn unexecuted_suffix_does_not_require_external_output() {
        let result = validate_output_presence(1, 0, false, false, &effect_rule());
        assert!(matches!(result, Ok(false)));
    }

    #[test]
    fn executed_machine_requires_its_external_output() {
        let result = validate_output_presence(1, 0, true, false, &effect_rule());
        assert!(matches!(
            result,
            Err(ComposedProgramError::MissingRequiredOutput {
                machine: 1,
                port: 0
            })
        ));
    }

    #[test]
    fn unexecuted_machine_cannot_supply_output() {
        let result = validate_output_presence(1, 0, false, true, &effect_rule());
        assert!(matches!(
            result,
            Err(ComposedProgramError::UnexecutedMachineOutput {
                machine: 1,
                port: 0
            })
        ));
    }
}
