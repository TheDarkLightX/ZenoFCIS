//! Fixed-size executable domain machines with explicit global composition.
//!
//! This crate executes pure, narrow domain machines over a compile-time fixed
//! state matrix. Internal routes are derived exactly from a reviewed
//! [`CompositionSpec`]; commands, contexts, state cells, and ports are
//! [`SchemaAdmittedTypeEnvelope`] values.
//!
//! The result is an executable reference artifact. It is not production commit,
//! effect, delivery, or proof-promotion authority.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::array;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, CommitmentHasher, Domain, EncodeError, Hash32, commitment};
use zeno_fcis_compose::{AccessPath, ComponentId, CompositionSpec, PathAtom};
use zeno_fcis_core::{
    Accepted, BudgetLimits, BudgetUsed, BudgetedDecision, Decision, DecisionKind, Failed, Rejected,
    Resource,
};
use zeno_fcis_project::SemanticId;
use zeno_fcis_schema::{SchemaAdmittedTypeEnvelope, TypeId};

/// Canonical format version for fixed-size domain-machine artifacts.
pub const DOMAIN_MACHINE_FORMAT_VERSION: u16 = 1;
/// Hard maximum composed machine count.
pub const MAX_DOMAIN_MACHINES: usize = 256;
/// Hard maximum state cells in one machine row.
pub const MAX_STATE_SLOTS_PER_MACHINE: usize = 256;
/// Hard maximum input or output positions in one machine row.
pub const MAX_PORTS_PER_MACHINE: usize = 256;
/// Hard maximum state cells in one complete matrix.
pub const MAX_TOTAL_STATE_SLOTS: usize = 16_384;
/// Hard maximum input or output positions in one complete matrix.
pub const MAX_TOTAL_PORTS: usize = 16_384;

const RESOURCE_ORDER: [Resource; 7] = [
    Resource::Read,
    Resource::Write,
    Resource::Candidate,
    Resource::Effect,
    Resource::Byte,
    Resource::WitnessByte,
    Resource::Depth,
];

/// Exact schema type and schema commitment required at one boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnvelopeBinding {
    type_id: TypeId,
    schema_hash: Hash32,
}

impl EnvelopeBinding {
    /// Creates a nonzero schema-envelope binding.
    pub fn try_new(type_id: TypeId, schema_hash: Hash32) -> Result<Self, DomainError> {
        if type_id.get() == 0 {
            return Err(DomainError::ZeroTypeId);
        }
        if schema_hash == Hash32::ZERO {
            return Err(DomainError::ZeroSchemaHash);
        }
        Ok(Self {
            type_id,
            schema_hash,
        })
    }

    /// Returns the required schema type.
    #[must_use]
    pub const fn type_id(self) -> TypeId {
        self.type_id
    }

    /// Returns the required schema commitment.
    #[must_use]
    pub const fn schema_hash(self) -> Hash32 {
        self.schema_hash
    }

    fn accepts(self, envelope: &SchemaAdmittedTypeEnvelope) -> bool {
        envelope.type_id() == self.type_id && envelope.schema_hash() == self.schema_hash
    }
}

impl CanonicalEncode for EnvelopeBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.type_id.get().to_be_bytes());
        output.extend_from_slice(self.schema_hash.as_bytes());
        Ok(())
    }
}

/// One concrete typed state or port path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedPathBinding {
    path: AccessPath,
    envelope: EnvelopeBinding,
}

impl TypedPathBinding {
    /// Creates a concrete path binding.
    ///
    /// Wildcards are rejected because every matrix position represents one
    /// exact boundary value.
    pub fn try_new(path: AccessPath, envelope: EnvelopeBinding) -> Result<Self, DomainError> {
        if path
            .atoms()
            .iter()
            .any(|atom| matches!(atom, PathAtom::AnyDescendant))
        {
            return Err(DomainError::WildcardInterfacePath);
        }
        Ok(Self { path, envelope })
    }

    /// Returns the exact access path.
    #[must_use]
    pub const fn path(&self) -> &AccessPath {
        &self.path
    }

    /// Returns the exact envelope binding.
    #[must_use]
    pub const fn envelope(&self) -> EnvelopeBinding {
        self.envelope
    }
}

impl CanonicalEncode for TypedPathBinding {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.path.canonical_bytes()?)?;
        self.envelope.encode_to(output)
    }
}

/// Fixed narrow interface of one component row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineInterface<const STATE_SLOTS: usize, const PORTS: usize> {
    component: ComponentId,
    profile_hash: Hash32,
    command: EnvelopeBinding,
    context: TypedPathBinding,
    state: Box<[TypedPathBinding; STATE_SLOTS]>,
    inputs: Box<[Option<TypedPathBinding>; PORTS]>,
    outputs: Box<[Option<TypedPathBinding>; PORTS]>,
}

impl<const STATE_SLOTS: usize, const PORTS: usize> MachineInterface<STATE_SLOTS, PORTS> {
    /// Creates one exact fixed-row interface.
    pub fn try_new(
        component: ComponentId,
        profile_hash: Hash32,
        command: EnvelopeBinding,
        context: TypedPathBinding,
        state: [TypedPathBinding; STATE_SLOTS],
        inputs: [Option<TypedPathBinding>; PORTS],
        outputs: [Option<TypedPathBinding>; PORTS],
    ) -> Result<Self, DomainError> {
        validate_row_shape::<STATE_SLOTS, PORTS>()?;
        if component.get() == 0 {
            return Err(DomainError::ZeroComponentId);
        }
        if profile_hash == Hash32::ZERO {
            return Err(DomainError::ZeroProfileHash);
        }
        ensure_nonoverlapping_state(&state)?;
        ensure_unique_ports(&inputs)?;
        ensure_unique_ports(&outputs)?;
        Ok(Self {
            component,
            profile_hash,
            command,
            context,
            state: Box::new(state),
            inputs: Box::new(inputs),
            outputs: Box::new(outputs),
        })
    }

    /// Returns the component identifier.
    #[must_use]
    pub const fn component(&self) -> ComponentId {
        self.component
    }

    /// Returns the component profile commitment.
    #[must_use]
    pub const fn profile_hash(&self) -> Hash32 {
        self.profile_hash
    }

    /// Returns the command binding.
    #[must_use]
    pub const fn command(&self) -> EnvelopeBinding {
        self.command
    }

    /// Returns the context binding.
    #[must_use]
    pub const fn context(&self) -> &TypedPathBinding {
        &self.context
    }

    /// Returns the exact state-slot bindings.
    #[must_use]
    pub const fn state(&self) -> &[TypedPathBinding; STATE_SLOTS] {
        &self.state
    }

    /// Returns the fixed input positions.
    #[must_use]
    pub const fn inputs(&self) -> &[Option<TypedPathBinding>; PORTS] {
        &self.inputs
    }

    /// Returns the fixed output positions.
    #[must_use]
    pub const fn outputs(&self) -> &[Option<TypedPathBinding>; PORTS] {
        &self.outputs
    }
}

impl<const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for MachineInterface<STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        output.extend_from_slice(self.profile_hash.as_bytes());
        self.command.encode_to(output)?;
        put_blob(output, &self.context.canonical_bytes()?)?;
        put_u16_length(output, STATE_SLOTS)?;
        for slot in self.state.iter() {
            put_blob(output, &slot.canonical_bytes()?)?;
        }
        put_u16_length(output, PORTS)?;
        for input in self.inputs.iter() {
            encode_optional_binding(input, output)?;
        }
        for port_output in self.outputs.iter() {
            encode_optional_binding(port_output, output)?;
        }
        Ok(())
    }
}

/// Fixed matrix address of one input or output position.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PortAddress {
    machine: u16,
    port: u16,
}

impl PortAddress {
    fn new(machine: usize, port: usize) -> Result<Self, DomainError> {
        Ok(Self {
            machine: u16::try_from(machine).map_err(|_| DomainError::InvalidShape)?,
            port: u16::try_from(port).map_err(|_| DomainError::InvalidShape)?,
        })
    }

    /// Returns the canonical machine-row index.
    #[must_use]
    pub const fn machine(self) -> u16 {
        self.machine
    }

    /// Returns the port index inside that row.
    #[must_use]
    pub const fn port(self) -> u16 {
        self.port
    }
}

impl CanonicalEncode for PortAddress {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.machine.to_be_bytes());
        output.extend_from_slice(&self.port.to_be_bytes());
        Ok(())
    }
}

/// Exact executable global composition and route matrix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutableComposition<
    const MACHINES: usize,
    const STATE_SLOTS: usize,
    const PORTS: usize,
> {
    spec: CompositionSpec,
    interfaces: Box<[MachineInterface<STATE_SLOTS, PORTS>; MACHINES]>,
    routes: Box<[[Option<PortAddress>; PORTS]; MACHINES]>,
    merge_rows: Box<[usize; MACHINES]>,
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>
{
    /// Binds fixed interfaces and derives every internal route from `spec`.
    pub fn try_new(
        spec: CompositionSpec,
        interfaces: [MachineInterface<STATE_SLOTS, PORTS>; MACHINES],
    ) -> Result<Self, DomainError> {
        validate_shape::<MACHINES, STATE_SLOTS, PORTS>()?;
        if spec.components().len() != MACHINES {
            return Err(DomainError::ComponentCardinality);
        }

        for (component, interface) in spec.components().iter().zip(interfaces.iter()) {
            if component.id() != interface.component {
                return Err(DomainError::ComponentOrderMismatch);
            }
            if component.profile_hash() != interface.profile_hash {
                return Err(DomainError::ProfileMismatch {
                    component: interface.component,
                });
            }
            for path in component
                .footprint()
                .reads()
                .paths()
                .iter()
                .chain(component.footprint().writes().paths().iter())
            {
                if !interface.state.iter().any(|slot| slot.path.covers(path)) {
                    return Err(DomainError::UnownedStateFootprint {
                        component: interface.component,
                    });
                }
            }
            for slot in interface.state.iter() {
                if !component.footprint().reads().covers(slot.path()) {
                    return Err(DomainError::UndeclaredStateRead {
                        component: interface.component,
                    });
                }
            }
            for path in component.footprint().writes().paths() {
                if !interface.state.iter().any(|slot| path.covers(slot.path())) {
                    return Err(DomainError::PartialStateWrite {
                        component: interface.component,
                    });
                }
            }
            if !component
                .footprint()
                .contexts()
                .covers(interface.context.path())
            {
                return Err(DomainError::UndeclaredContextInput {
                    component: interface.component,
                });
            }
            for path in component.footprint().contexts().paths() {
                if !interface.context.path.covers(path) {
                    return Err(DomainError::UnownedContextFootprint {
                        component: interface.component,
                    });
                }
            }
            for output in interface.outputs.iter().flatten() {
                if !component.footprint().effects().covers(output.path()) {
                    return Err(DomainError::UndeclaredOutput {
                        component: interface.component,
                    });
                }
            }
        }
        ensure_global_state_partition(&interfaces)?;

        let mut merge_rows = [0_usize; MACHINES];
        let mut merge_positions = [0_usize; MACHINES];
        for (position, component) in spec.merge_order().iter().copied().enumerate() {
            let Some(row) = interfaces
                .iter()
                .position(|interface| interface.component == component)
            else {
                return Err(DomainError::ComponentOrderMismatch);
            };
            merge_rows[position] = row;
            merge_positions[row] = position;
        }

        let mut routes: [[Option<PortAddress>; PORTS]; MACHINES] =
            array::from_fn(|_| array::from_fn(|_| None));
        let mut occupied_inputs = [[false; PORTS]; MACHINES];

        for wiring in spec.wirings() {
            let source_row = find_component_row(&interfaces, wiring.source_component())
                .ok_or(DomainError::UnknownWiringComponent)?;
            let destination_row = find_component_row(&interfaces, wiring.destination_component())
                .ok_or(DomainError::UnknownWiringComponent)?;
            let source_port = find_port(&interfaces[source_row].outputs, wiring.source_effect())
                .ok_or(DomainError::MissingWiringSourcePort)?;
            let destination_port = find_port(
                &interfaces[destination_row].inputs,
                wiring.destination_path(),
            )
            .ok_or(DomainError::MissingWiringDestinationPort)?;
            let Some(source_binding) = interfaces[source_row].outputs[source_port].as_ref() else {
                return Err(DomainError::MissingWiringSourcePort);
            };
            let Some(destination_binding) =
                interfaces[destination_row].inputs[destination_port].as_ref()
            else {
                return Err(DomainError::MissingWiringDestinationPort);
            };
            if source_binding.envelope != destination_binding.envelope
                || source_binding.envelope.schema_hash() != wiring.schema_hash()
            {
                return Err(DomainError::WiringSchemaMismatch);
            }
            let permitted = spec.components()[destination_row]
                .frames()
                .iter()
                .any(|frame| {
                    frame.protected().covers(wiring.destination_path())
                        && frame
                            .allowed_writers()
                            .binary_search(&wiring.source_component())
                            .is_ok()
                });
            if !permitted {
                return Err(DomainError::UnauthorizedWiring);
            }
            if merge_positions[source_row] >= merge_positions[destination_row] {
                return Err(DomainError::BackwardWiring);
            }
            if routes[source_row][source_port].is_some() {
                return Err(DomainError::FanoutUnsupported);
            }
            if occupied_inputs[destination_row][destination_port] {
                return Err(DomainError::FaninUnsupported);
            }
            routes[source_row][source_port] =
                Some(PortAddress::new(destination_row, destination_port)?);
            occupied_inputs[destination_row][destination_port] = true;
        }

        for row in 0..MACHINES {
            for (binding, occupied) in interfaces[row]
                .inputs
                .iter()
                .zip(occupied_inputs[row].iter())
            {
                if binding.is_some() != *occupied {
                    return Err(DomainError::UnboundInputPort);
                }
            }
        }

        Ok(Self {
            spec,
            interfaces: Box::new(interfaces),
            routes: Box::new(routes),
            merge_rows: Box::new(merge_rows),
        })
    }

    /// Returns the exact proof-carrying composition specification.
    #[must_use]
    pub const fn spec(&self) -> &CompositionSpec {
        &self.spec
    }

    /// Returns canonical machine interfaces.
    #[must_use]
    pub const fn interfaces(&self) -> &[MachineInterface<STATE_SLOTS, PORTS>; MACHINES] {
        &self.interfaces
    }

    /// Returns the source-to-destination route matrix.
    #[must_use]
    pub const fn routes(&self) -> &[[Option<PortAddress>; PORTS]; MACHINES] {
        &self.routes
    }

    /// Computes the exact executable-composition commitment.
    pub fn commitment<H: CommitmentHasher>(&self) -> Result<Hash32, DomainError> {
        let domain = Domain::new(
            "zeno-fcis/executable-composition",
            DOMAIN_MACHINE_FORMAT_VERSION,
        )?;
        let bytes = self.canonical_bytes()?;
        Ok(commitment::<H>(domain, &bytes)?)
    }

    /// Admits an exact fixed state matrix against all interface slots.
    pub fn admit_state(
        &self,
        rows: [[SchemaAdmittedTypeEnvelope; STATE_SLOTS]; MACHINES],
    ) -> Result<FixedStateMatrix<MACHINES, STATE_SLOTS>, DomainError> {
        for (machine, (row, interface)) in rows.iter().zip(self.interfaces.iter()).enumerate() {
            for (slot, (value, binding)) in row.iter().zip(interface.state.iter()).enumerate() {
                if !binding.envelope.accepts(value) {
                    return Err(DomainError::StateEnvelopeMismatch { machine, slot });
                }
            }
        }
        Ok(FixedStateMatrix {
            composition: self.canonical_bytes()?.into_boxed_slice(),
            rows: Box::new(rows),
        })
    }

    /// Admits exact command and context rows against all interfaces.
    pub fn admit_invocation(
        &self,
        commands: [SchemaAdmittedTypeEnvelope; MACHINES],
        contexts: [SchemaAdmittedTypeEnvelope; MACHINES],
    ) -> Result<FixedInvocationMatrix<MACHINES>, DomainError> {
        for machine in 0..MACHINES {
            if !self.interfaces[machine].command.accepts(&commands[machine]) {
                return Err(DomainError::CommandEnvelopeMismatch { machine });
            }
            if !self.interfaces[machine]
                .context
                .envelope
                .accepts(&contexts[machine])
            {
                return Err(DomainError::ContextEnvelopeMismatch { machine });
            }
        }
        Ok(FixedInvocationMatrix {
            composition: self.canonical_bytes()?.into_boxed_slice(),
            commands: Box::new(commands),
            contexts: Box::new(contexts),
        })
    }

    /// Executes at most one step per machine in the exact specification merge order.
    ///
    /// Accepted local changes remain provisional until every machine accepts.
    /// A rejection discards all provisional changes. The first committed
    /// failure preserves changes through that component and terminates the
    /// global step.
    pub fn execute(
        &self,
        machines: [&dyn DomainMachine<STATE_SLOTS, PORTS>; MACHINES],
        pre_state: &FixedStateMatrix<MACHINES, STATE_SLOTS>,
        invocation: &FixedInvocationMatrix<MACHINES>,
        limits: [BudgetLimits; MACHINES],
    ) -> Result<SystemExecution<MACHINES, STATE_SLOTS, PORTS>, DomainError> {
        let composition = self.canonical_bytes()?;
        if pre_state.composition.as_ref() != composition.as_slice() {
            return Err(DomainError::StateCompositionMismatch);
        }
        if invocation.composition.as_ref() != composition.as_slice() {
            return Err(DomainError::InvocationCompositionMismatch);
        }
        let mut working_state = pre_state.clone();
        let mut inputs: [[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES] =
            array::from_fn(|_| array::from_fn(|_| None));
        let mut outputs: [[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES] =
            array::from_fn(|_| array::from_fn(|_| None));
        let mut reports: [Option<MachineExecutionReport>; MACHINES] = array::from_fn(|_| None);

        for row in self.merge_rows.iter().copied() {
            let machine = machines[row];
            if machine.component_id() != self.interfaces[row].component {
                return Err(DomainError::MachineIdentityMismatch { machine: row });
            }
            let budgeted = machine.step(
                &working_state.rows[row],
                &invocation.commands[row],
                &invocation.contexts[row],
                &inputs[row],
                limits[row],
            );
            let (decision, supplied_limits, used) = budgeted.into_parts();
            if supplied_limits != limits[row] {
                return Err(DomainError::BudgetLimitMismatch { machine: row });
            }
            reports[row] = Some(MachineExecutionReport {
                component: self.interfaces[row].component,
                kind: decision.kind(),
                limits: supplied_limits,
                used,
            });

            match decision {
                Decision::Accept(accepted) => {
                    self.apply_candidate(
                        row,
                        accepted.into_candidate(),
                        &mut working_state,
                        &mut inputs,
                        &mut outputs,
                    )?;
                }
                Decision::Reject(rejected) => {
                    return Ok(SystemExecution {
                        decision: Decision::Reject(Rejected::new(MachineRejection {
                            component: self.interfaces[row].component,
                            reason: *rejected.reason(),
                        })),
                        reports: Box::new(reports),
                    });
                }
                Decision::CommittedFailure(failed) => {
                    let (candidate, reason) = failed.into_parts();
                    self.apply_candidate(
                        row,
                        candidate,
                        &mut working_state,
                        &mut inputs,
                        &mut outputs,
                    )?;
                    return Ok(SystemExecution {
                        decision: Decision::CommittedFailure(Failed::new(
                            SystemCandidate {
                                post_state: working_state,
                                outputs: FixedOutputMatrix {
                                    rows: Box::new(outputs),
                                },
                            },
                            MachineFailure {
                                component: self.interfaces[row].component,
                                reason,
                            },
                        )),
                        reports: Box::new(reports),
                    });
                }
            }
        }

        Ok(SystemExecution {
            decision: Decision::Accept(Accepted::new(SystemCandidate {
                post_state: working_state,
                outputs: FixedOutputMatrix {
                    rows: Box::new(outputs),
                },
            })),
            reports: Box::new(reports),
        })
    }

    fn apply_candidate(
        &self,
        row: usize,
        candidate: MachineCandidate<STATE_SLOTS, PORTS>,
        working_state: &mut FixedStateMatrix<MACHINES, STATE_SLOTS>,
        inputs: &mut [[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES],
        outputs: &mut [[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES],
    ) -> Result<(), DomainError> {
        let (next_state, emitted) = candidate.into_parts();
        for (slot, (value, binding)) in next_state
            .iter()
            .zip(self.interfaces[row].state.iter())
            .enumerate()
        {
            if !binding.envelope.accepts(value) {
                return Err(DomainError::CandidateStateMismatch { machine: row, slot });
            }
            if !self.spec.components()[row]
                .footprint()
                .writes()
                .covers(binding.path())
                && value != &working_state.rows[row][slot]
            {
                return Err(DomainError::ReadOnlyStateMutation { machine: row, slot });
            }
        }
        for (port, (binding, value)) in self.interfaces[row]
            .outputs
            .iter()
            .zip(emitted.iter())
            .enumerate()
        {
            match (binding, value) {
                (None, None) => {}
                (None, Some(_)) => {
                    return Err(DomainError::CandidateOutputMismatch { machine: row, port });
                }
                (Some(_), None) => {}
                (Some(binding), Some(value)) if binding.envelope.accepts(value) => {}
                (Some(_), Some(_)) => {
                    return Err(DomainError::CandidateOutputMismatch { machine: row, port });
                }
            }
        }

        for (port, value) in emitted.iter().enumerate() {
            if let (Some(destination), Some(value)) = (self.routes[row][port], value) {
                inputs[usize::from(destination.machine)][usize::from(destination.port)] =
                    Some(value.clone());
            }
        }
        working_state.rows[row] = next_state;
        outputs[row] = emitted;
        Ok(())
    }
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for ExecutableComposition<MACHINES, STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-EXECUTABLE-COMPOSITION\0");
        output.extend_from_slice(&DOMAIN_MACHINE_FORMAT_VERSION.to_be_bytes());
        encode_dimensions::<MACHINES, STATE_SLOTS, PORTS>(output)?;
        put_blob(output, &self.spec.canonical_bytes()?)?;
        for interface in self.interfaces.iter() {
            put_blob(output, &interface.canonical_bytes()?)?;
        }
        for row in self.routes.iter() {
            for route in row {
                match route {
                    None => output.push(0),
                    Some(address) => {
                        output.push(1);
                        address.encode_to(output)?;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Exact fixed state matrix admitted against one executable composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedStateMatrix<const MACHINES: usize, const STATE_SLOTS: usize> {
    composition: Box<[u8]>,
    rows: Box<[[SchemaAdmittedTypeEnvelope; STATE_SLOTS]; MACHINES]>,
}

impl<const MACHINES: usize, const STATE_SLOTS: usize> FixedStateMatrix<MACHINES, STATE_SLOTS> {
    /// Returns every canonical component row.
    #[must_use]
    pub const fn rows(&self) -> &[[SchemaAdmittedTypeEnvelope; STATE_SLOTS]; MACHINES] {
        &self.rows
    }

    /// Returns one canonical component row.
    #[must_use]
    pub fn row(&self, machine: usize) -> Option<&[SchemaAdmittedTypeEnvelope; STATE_SLOTS]> {
        self.rows.get(machine)
    }
}

impl<const MACHINES: usize, const STATE_SLOTS: usize> CanonicalEncode
    for FixedStateMatrix<MACHINES, STATE_SLOTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-FIXED-STATE-MATRIX\0");
        output.extend_from_slice(&DOMAIN_MACHINE_FORMAT_VERSION.to_be_bytes());
        put_u16_length(output, MACHINES)?;
        put_u16_length(output, STATE_SLOTS)?;
        put_blob(output, &self.composition)?;
        for row in self.rows.iter() {
            for value in row {
                put_blob(output, &value.canonical_bytes()?)?;
            }
        }
        Ok(())
    }
}

/// Exact fixed command and context rows admitted for one execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedInvocationMatrix<const MACHINES: usize> {
    composition: Box<[u8]>,
    commands: Box<[SchemaAdmittedTypeEnvelope; MACHINES]>,
    contexts: Box<[SchemaAdmittedTypeEnvelope; MACHINES]>,
}

impl<const MACHINES: usize> FixedInvocationMatrix<MACHINES> {
    /// Returns commands in canonical component-row order.
    #[must_use]
    pub const fn commands(&self) -> &[SchemaAdmittedTypeEnvelope; MACHINES] {
        &self.commands
    }

    /// Returns contexts in canonical component-row order.
    #[must_use]
    pub const fn contexts(&self) -> &[SchemaAdmittedTypeEnvelope; MACHINES] {
        &self.contexts
    }
}

impl<const MACHINES: usize> CanonicalEncode for FixedInvocationMatrix<MACHINES> {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-FIXED-INVOCATION-MATRIX\0");
        output.extend_from_slice(&DOMAIN_MACHINE_FORMAT_VERSION.to_be_bytes());
        put_u16_length(output, MACHINES)?;
        put_blob(output, &self.composition)?;
        for command in self.commands.iter() {
            put_blob(output, &command.canonical_bytes()?)?;
        }
        for context in self.contexts.iter() {
            put_blob(output, &context.canonical_bytes()?)?;
        }
        Ok(())
    }
}

/// One local candidate returned by a pure domain machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineCandidate<const STATE_SLOTS: usize, const PORTS: usize> {
    next_state: [SchemaAdmittedTypeEnvelope; STATE_SLOTS],
    outputs: [Option<SchemaAdmittedTypeEnvelope>; PORTS],
}

impl<const STATE_SLOTS: usize, const PORTS: usize> MachineCandidate<STATE_SLOTS, PORTS> {
    /// Creates a candidate that will be checked against the exact interface by
    /// the global executor.
    #[must_use]
    pub const fn new(
        next_state: [SchemaAdmittedTypeEnvelope; STATE_SLOTS],
        outputs: [Option<SchemaAdmittedTypeEnvelope>; PORTS],
    ) -> Self {
        Self {
            next_state,
            outputs,
        }
    }

    /// Returns the proposed successor state row.
    #[must_use]
    pub const fn next_state(&self) -> &[SchemaAdmittedTypeEnvelope; STATE_SLOTS] {
        &self.next_state
    }

    /// Returns the proposed output row.
    #[must_use]
    pub const fn outputs(&self) -> &[Option<SchemaAdmittedTypeEnvelope>; PORTS] {
        &self.outputs
    }

    /// Consumes the candidate.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        [SchemaAdmittedTypeEnvelope; STATE_SLOTS],
        [Option<SchemaAdmittedTypeEnvelope>; PORTS],
    ) {
        (self.next_state, self.outputs)
    }
}

impl<const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for MachineCandidate<STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_u16_length(output, STATE_SLOTS)?;
        put_u16_length(output, PORTS)?;
        for value in &self.next_state {
            put_blob(output, &value.canonical_bytes()?)?;
        }
        for value in &self.outputs {
            encode_optional_envelope(value, output)?;
        }
        Ok(())
    }
}

/// Pure, deterministic machine with a narrow fixed interface.
///
/// Implementations must not observe clocks, randomness, filesystems, networks,
/// threads, process state, global mutable state, or other ambient effects.
pub trait DomainMachine<const STATE_SLOTS: usize, const PORTS: usize> {
    /// Returns the exact component row implemented by this machine.
    fn component_id(&self) -> ComponentId;

    /// Computes one total local decision.
    fn step(
        &self,
        state: &[SchemaAdmittedTypeEnvelope; STATE_SLOTS],
        command: &SchemaAdmittedTypeEnvelope,
        context: &SchemaAdmittedTypeEnvelope,
        inputs: &[Option<SchemaAdmittedTypeEnvelope>; PORTS],
        limits: BudgetLimits,
    ) -> BudgetedDecision<MachineCandidate<STATE_SLOTS, PORTS>, SemanticId, SemanticId>;
}

/// Fixed emitted-output matrix retained as execution evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedOutputMatrix<const MACHINES: usize, const PORTS: usize> {
    rows: Box<[[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES]>,
}

impl<const MACHINES: usize, const PORTS: usize> FixedOutputMatrix<MACHINES, PORTS> {
    /// Returns every output row.
    #[must_use]
    pub const fn rows(&self) -> &[[Option<SchemaAdmittedTypeEnvelope>; PORTS]; MACHINES] {
        &self.rows
    }
}

impl<const MACHINES: usize, const PORTS: usize> CanonicalEncode
    for FixedOutputMatrix<MACHINES, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-FIXED-OUTPUT-MATRIX\0");
        output.extend_from_slice(&DOMAIN_MACHINE_FORMAT_VERSION.to_be_bytes());
        put_u16_length(output, MACHINES)?;
        put_u16_length(output, PORTS)?;
        for row in self.rows.iter() {
            for value in row {
                encode_optional_envelope(value, output)?;
            }
        }
        Ok(())
    }
}

/// Complete candidate of one accepted or committed-failure global step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemCandidate<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> {
    post_state: FixedStateMatrix<MACHINES, STATE_SLOTS>,
    outputs: FixedOutputMatrix<MACHINES, PORTS>,
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    SystemCandidate<MACHINES, STATE_SLOTS, PORTS>
{
    /// Returns the exact successor matrix.
    #[must_use]
    pub const fn post_state(&self) -> &FixedStateMatrix<MACHINES, STATE_SLOTS> {
        &self.post_state
    }

    /// Returns the exact emitted-output matrix.
    #[must_use]
    pub const fn outputs(&self) -> &FixedOutputMatrix<MACHINES, PORTS> {
        &self.outputs
    }
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for SystemCandidate<MACHINES, STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        put_blob(output, &self.post_state.canonical_bytes()?)?;
        put_blob(output, &self.outputs.canonical_bytes()?)
    }
}

/// Component and stable reason that atomically rejected the global step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineRejection {
    component: ComponentId,
    reason: SemanticId,
}

impl MachineRejection {
    /// Returns the rejecting component.
    #[must_use]
    pub const fn component(self) -> ComponentId {
        self.component
    }

    /// Returns the project-stable reason identifier.
    #[must_use]
    pub const fn reason(self) -> SemanticId {
        self.reason
    }
}

impl CanonicalEncode for MachineRejection {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        self.reason.encode_to(output)
    }
}

/// Component and stable reason that terminated with an intentional commit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineFailure {
    component: ComponentId,
    reason: SemanticId,
}

impl MachineFailure {
    /// Returns the failing component.
    #[must_use]
    pub const fn component(self) -> ComponentId {
        self.component
    }

    /// Returns the project-stable reason identifier.
    #[must_use]
    pub const fn reason(self) -> SemanticId {
        self.reason
    }
}

impl CanonicalEncode for MachineFailure {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        self.reason.encode_to(output)
    }
}

/// Exact decision and resource use of one machine that ran.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineExecutionReport {
    component: ComponentId,
    kind: DecisionKind,
    limits: BudgetLimits,
    used: BudgetUsed,
}

impl MachineExecutionReport {
    /// Returns the component.
    #[must_use]
    pub const fn component(self) -> ComponentId {
        self.component
    }

    /// Returns the local decision kind.
    #[must_use]
    pub const fn kind(self) -> DecisionKind {
        self.kind
    }

    /// Returns the immutable supplied limits.
    #[must_use]
    pub const fn limits(self) -> BudgetLimits {
        self.limits
    }

    /// Returns exact logical usage.
    #[must_use]
    pub const fn used(self) -> BudgetUsed {
        self.used
    }
}

impl CanonicalEncode for MachineExecutionReport {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.component.encode_to(output)?;
        output.push(decision_tag(self.kind));
        encode_budget_limits(self.limits, output);
        encode_budget_used(self.used, output);
        Ok(())
    }
}

/// Complete global decision plus fixed execution reports.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemExecution<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> {
    decision:
        Decision<SystemCandidate<MACHINES, STATE_SLOTS, PORTS>, MachineRejection, MachineFailure>,
    reports: Box<[Option<MachineExecutionReport>; MACHINES]>,
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>
    SystemExecution<MACHINES, STATE_SLOTS, PORTS>
{
    /// Returns the complete global three-way decision.
    #[must_use]
    pub const fn decision(
        &self,
    ) -> &Decision<SystemCandidate<MACHINES, STATE_SLOTS, PORTS>, MachineRejection, MachineFailure>
    {
        &self.decision
    }

    /// Returns fixed reports in canonical component-row order.
    #[must_use]
    pub const fn reports(&self) -> &[Option<MachineExecutionReport>; MACHINES] {
        &self.reports
    }
}

impl<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize> CanonicalEncode
    for SystemExecution<MACHINES, STATE_SLOTS, PORTS>
{
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-SYSTEM-EXECUTION\0");
        output.extend_from_slice(&DOMAIN_MACHINE_FORMAT_VERSION.to_be_bytes());
        encode_dimensions::<MACHINES, STATE_SLOTS, PORTS>(output)?;
        match &self.decision {
            Decision::Accept(accepted) => {
                output.push(0);
                put_blob(output, &accepted.candidate().canonical_bytes()?)?;
            }
            Decision::Reject(rejected) => {
                output.push(1);
                rejected.reason().encode_to(output)?;
            }
            Decision::CommittedFailure(failed) => {
                output.push(2);
                put_blob(output, &failed.candidate().canonical_bytes()?)?;
                failed.reason().encode_to(output)?;
            }
        }
        for report in self.reports.iter() {
            match report {
                None => output.push(0),
                Some(value) => {
                    output.push(1);
                    value.encode_to(output)?;
                }
            }
        }
        Ok(())
    }
}

/// Fail-closed construction or execution error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// Compile-time dimensions violate the hard shape envelope.
    InvalidShape,
    /// A component identifier is zero.
    ZeroComponentId,
    /// A component profile commitment is zero.
    ZeroProfileHash,
    /// A schema type identifier is zero.
    ZeroTypeId,
    /// A schema commitment is zero.
    ZeroSchemaHash,
    /// An interface path contains a wildcard.
    WildcardInterfacePath,
    /// Two state cells inside one row overlap.
    OverlappingStateSlots,
    /// Two active ports reuse one path.
    DuplicatePortPath,
    /// The specification component count differs from the matrix shape.
    ComponentCardinality,
    /// Interface rows do not match canonical component order.
    ComponentOrderMismatch,
    /// An interface profile differs from its component contract.
    ProfileMismatch {
        /// Component with the mismatch.
        component: ComponentId,
    },
    /// A declared state read or write lies outside the owned row.
    UnownedStateFootprint {
        /// Component with the uncovered footprint.
        component: ComponentId,
    },
    /// A machine receives a state cell absent from its complete read footprint.
    UndeclaredStateRead {
        /// Component with excess read authority.
        component: ComponentId,
    },
    /// A declared write addresses less than one complete state cell.
    PartialStateWrite {
        /// Component with the partial-cell write.
        component: ComponentId,
    },
    /// The fixed context root is absent from the declared context footprint.
    UndeclaredContextInput {
        /// Component with excess context authority.
        component: ComponentId,
    },
    /// A declared context path lies outside the fixed context root.
    UnownedContextFootprint {
        /// Component with the uncovered context path.
        component: ComponentId,
    },
    /// State ownership overlaps across component rows.
    OverlappingStateOwnership,
    /// An output path is absent from the component effect footprint.
    UndeclaredOutput {
        /// Component with the output.
        component: ComponentId,
    },
    /// A wiring references a component absent from the interface matrix.
    UnknownWiringComponent,
    /// No exact output port matches a wiring source.
    MissingWiringSourcePort,
    /// No exact input port matches a wiring destination.
    MissingWiringDestinationPort,
    /// Source, destination, or wiring schema bindings disagree.
    WiringSchemaMismatch,
    /// No exact destination frame authorizes the source component.
    UnauthorizedWiring,
    /// The route points backward or sideways in merge order.
    BackwardWiring,
    /// One output port would feed more than one destination.
    FanoutUnsupported,
    /// One input port would receive more than one source.
    FaninUnsupported,
    /// An active input has no exact `CompositionSpec` wiring.
    UnboundInputPort,
    /// A state matrix was admitted by another executable composition.
    StateCompositionMismatch,
    /// An invocation matrix was admitted by another executable composition.
    InvocationCompositionMismatch,
    /// A pre-state cell does not match its interface envelope.
    StateEnvelopeMismatch {
        /// Canonical machine row.
        machine: usize,
        /// State-slot index.
        slot: usize,
    },
    /// A command does not match its interface envelope.
    CommandEnvelopeMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// A context does not match its interface envelope.
    ContextEnvelopeMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// A supplied machine implements a different component.
    MachineIdentityMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// A machine returned different limits from those supplied.
    BudgetLimitMismatch {
        /// Canonical machine row.
        machine: usize,
    },
    /// A successor-state cell does not match its interface envelope.
    CandidateStateMismatch {
        /// Canonical machine row.
        machine: usize,
        /// State-slot index.
        slot: usize,
    },
    /// A machine changed a state cell not covered by its write footprint.
    ReadOnlyStateMutation {
        /// Canonical machine row.
        machine: usize,
        /// State-slot index.
        slot: usize,
    },
    /// An emitted output is active or typed differently from its interface.
    CandidateOutputMismatch {
        /// Canonical machine row.
        machine: usize,
        /// Output-port index.
        port: usize,
    },
    /// Canonical encoding or commitment construction failed.
    Encode(EncodeError),
}

impl From<EncodeError> for DomainError {
    fn from(error: EncodeError) -> Self {
        Self::Encode(error)
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidShape => formatter.write_str("fixed matrix shape exceeds hard limits"),
            Self::ZeroComponentId => formatter.write_str("component identifier is zero"),
            Self::ZeroProfileHash => formatter.write_str("component profile hash is zero"),
            Self::ZeroTypeId => formatter.write_str("schema type identifier is zero"),
            Self::ZeroSchemaHash => formatter.write_str("schema commitment is zero"),
            Self::WildcardInterfacePath => {
                formatter.write_str("fixed interface path contains a wildcard")
            }
            Self::OverlappingStateSlots => formatter.write_str("state slots overlap inside a row"),
            Self::DuplicatePortPath => formatter.write_str("active port path is duplicated"),
            Self::ComponentCardinality => {
                formatter.write_str("component count differs from fixed matrix shape")
            }
            Self::ComponentOrderMismatch => {
                formatter.write_str("interfaces differ from canonical component order")
            }
            Self::ProfileMismatch { component } => {
                write!(
                    formatter,
                    "profile mismatch for component {}",
                    component.get()
                )
            }
            Self::UnownedStateFootprint { component } => write!(
                formatter,
                "state footprint is outside component {} row",
                component.get()
            ),
            Self::UndeclaredStateRead { component } => write!(
                formatter,
                "state cell is outside component {} read footprint",
                component.get()
            ),
            Self::PartialStateWrite { component } => write!(
                formatter,
                "write footprint covers less than one state cell for component {}",
                component.get()
            ),
            Self::UndeclaredContextInput { component } => write!(
                formatter,
                "context input is outside component {} footprint",
                component.get()
            ),
            Self::UnownedContextFootprint { component } => write!(
                formatter,
                "context footprint is outside component {} input",
                component.get()
            ),
            Self::OverlappingStateOwnership => {
                formatter.write_str("state ownership overlaps across component rows")
            }
            Self::UndeclaredOutput { component } => write!(
                formatter,
                "output is outside component {} effect footprint",
                component.get()
            ),
            Self::UnknownWiringComponent => {
                formatter.write_str("wiring component is absent from the interface matrix")
            }
            Self::MissingWiringSourcePort => {
                formatter.write_str("wiring source has no exact output port")
            }
            Self::MissingWiringDestinationPort => {
                formatter.write_str("wiring destination has no exact input port")
            }
            Self::WiringSchemaMismatch => {
                formatter.write_str("wiring source and destination schemas differ")
            }
            Self::UnauthorizedWiring => {
                formatter.write_str("destination frame does not authorize wiring")
            }
            Self::BackwardWiring => {
                formatter.write_str("wiring does not follow deterministic merge order")
            }
            Self::FanoutUnsupported => formatter.write_str("output fanout is not supported"),
            Self::FaninUnsupported => formatter.write_str("input fanin is not supported"),
            Self::UnboundInputPort => {
                formatter.write_str("active input is not bound by exact composition wiring")
            }
            Self::StateCompositionMismatch => {
                formatter.write_str("state matrix belongs to another executable composition")
            }
            Self::InvocationCompositionMismatch => {
                formatter.write_str("invocation matrix belongs to another executable composition")
            }
            Self::StateEnvelopeMismatch { machine, slot } => write!(
                formatter,
                "state envelope mismatch at machine {machine}, slot {slot}"
            ),
            Self::CommandEnvelopeMismatch { machine } => {
                write!(formatter, "command envelope mismatch at machine {machine}")
            }
            Self::ContextEnvelopeMismatch { machine } => {
                write!(formatter, "context envelope mismatch at machine {machine}")
            }
            Self::MachineIdentityMismatch { machine } => {
                write!(formatter, "machine identity mismatch at row {machine}")
            }
            Self::BudgetLimitMismatch { machine } => {
                write!(
                    formatter,
                    "machine returned different limits at row {machine}"
                )
            }
            Self::CandidateStateMismatch { machine, slot } => write!(
                formatter,
                "candidate state mismatch at machine {machine}, slot {slot}"
            ),
            Self::ReadOnlyStateMutation { machine, slot } => write!(
                formatter,
                "read-only state changed at machine {machine}, slot {slot}"
            ),
            Self::CandidateOutputMismatch { machine, port } => write!(
                formatter,
                "candidate output mismatch at machine {machine}, port {port}"
            ),
            Self::Encode(error) => write!(formatter, "domain artifact encoding failed: {error}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DomainError {}

fn validate_shape<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>()
-> Result<(), DomainError> {
    validate_row_shape::<STATE_SLOTS, PORTS>()?;
    let state_total = MACHINES
        .checked_mul(STATE_SLOTS)
        .ok_or(DomainError::InvalidShape)?;
    let port_total = MACHINES
        .checked_mul(PORTS)
        .ok_or(DomainError::InvalidShape)?;
    if MACHINES == 0
        || MACHINES > MAX_DOMAIN_MACHINES
        || state_total > MAX_TOTAL_STATE_SLOTS
        || port_total > MAX_TOTAL_PORTS
    {
        Err(DomainError::InvalidShape)
    } else {
        Ok(())
    }
}

fn validate_row_shape<const STATE_SLOTS: usize, const PORTS: usize>() -> Result<(), DomainError> {
    if STATE_SLOTS == 0
        || STATE_SLOTS > MAX_STATE_SLOTS_PER_MACHINE
        || PORTS > MAX_PORTS_PER_MACHINE
    {
        Err(DomainError::InvalidShape)
    } else {
        Ok(())
    }
}

fn ensure_nonoverlapping_state<const STATE_SLOTS: usize>(
    slots: &[TypedPathBinding; STATE_SLOTS],
) -> Result<(), DomainError> {
    for left in 0..STATE_SLOTS {
        for right in left + 1..STATE_SLOTS {
            if slots[left].path.overlaps(&slots[right].path) {
                return Err(DomainError::OverlappingStateSlots);
            }
        }
    }
    Ok(())
}

fn ensure_unique_ports<const PORTS: usize>(
    ports: &[Option<TypedPathBinding>; PORTS],
) -> Result<(), DomainError> {
    for left in 0..PORTS {
        let Some(left_binding) = ports[left].as_ref() else {
            continue;
        };
        for right_binding in ports.iter().skip(left + 1).flatten() {
            if left_binding.path == right_binding.path {
                return Err(DomainError::DuplicatePortPath);
            }
        }
    }
    Ok(())
}

fn ensure_global_state_partition<
    const MACHINES: usize,
    const STATE_SLOTS: usize,
    const PORTS: usize,
>(
    interfaces: &[MachineInterface<STATE_SLOTS, PORTS>; MACHINES],
) -> Result<(), DomainError> {
    for left_machine in 0..MACHINES {
        for left_slot in 0..STATE_SLOTS {
            for right_machine in left_machine + 1..MACHINES {
                for right_slot in 0..STATE_SLOTS {
                    if interfaces[left_machine].state[left_slot]
                        .path
                        .overlaps(&interfaces[right_machine].state[right_slot].path)
                    {
                        return Err(DomainError::OverlappingStateOwnership);
                    }
                }
            }
        }
    }
    Ok(())
}

fn find_component_row<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>(
    interfaces: &[MachineInterface<STATE_SLOTS, PORTS>; MACHINES],
    component: ComponentId,
) -> Option<usize> {
    interfaces
        .iter()
        .position(|interface| interface.component == component)
}

fn find_port<const PORTS: usize>(
    ports: &[Option<TypedPathBinding>; PORTS],
    path: &AccessPath,
) -> Option<usize> {
    ports.iter().position(|binding| {
        binding
            .as_ref()
            .is_some_and(|binding| binding.path == *path)
    })
}

fn encode_optional_binding(
    binding: &Option<TypedPathBinding>,
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match binding {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_blob(output, &value.canonical_bytes()?)?;
        }
    }
    Ok(())
}

fn encode_optional_envelope(
    envelope: &Option<SchemaAdmittedTypeEnvelope>,
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    match envelope {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_blob(output, &value.canonical_bytes()?)?;
        }
    }
    Ok(())
}

fn encode_dimensions<const MACHINES: usize, const STATE_SLOTS: usize, const PORTS: usize>(
    output: &mut Vec<u8>,
) -> Result<(), EncodeError> {
    put_u16_length(output, MACHINES)?;
    put_u16_length(output, STATE_SLOTS)?;
    put_u16_length(output, PORTS)
}

fn encode_budget_limits(limits: BudgetLimits, output: &mut Vec<u8>) {
    for resource in RESOURCE_ORDER {
        output.extend_from_slice(&limits.limit(resource).to_be_bytes());
    }
}

fn encode_budget_used(used: BudgetUsed, output: &mut Vec<u8>) {
    for resource in RESOURCE_ORDER {
        output.extend_from_slice(&used.used(resource).to_be_bytes());
    }
}

const fn decision_tag(kind: DecisionKind) -> u8 {
    match kind {
        DecisionKind::Accept => 0,
        DecisionKind::Reject => 1,
        DecisionKind::CommittedFailure => 2,
    }
}

fn put_u16_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u16::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_blob(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), EncodeError> {
    let length = u32::try_from(bytes.len()).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}
