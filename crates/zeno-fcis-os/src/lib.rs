//! Architecture-neutral values for high-assurance microkernel design.
//!
//! This crate models authority, kernel inputs, scheduling resources, static
//! system architecture, and hardware-facing effect plans as closed immutable
//! values. It deliberately contains no privileged instructions, MMIO, assembly,
//! interrupt entry code, allocator, clock read, thread runtime, or executable
//! effect closure. A concrete kernel must refine these values through a small,
//! separately audited machine boundary.

#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use zeno_fcis_codec::{CanonicalEncode, EncodeError};

/// Stable semantics version of the architecture-neutral OS value profile.
pub const OS_PROFILE_VERSION: u16 = 1;
/// Maximum syscall machine words carried by one admitted event.
pub const MAX_SYSCALL_ARGUMENTS: usize = 16;
/// Maximum hardware operations in one atomic machine plan.
pub const MAX_MACHINE_OPERATIONS: usize = 4_096;
/// Maximum protection domains in one static system description.
pub const MAX_PROTECTION_DOMAINS: usize = 4_096;
/// Maximum channels in one static system description.
pub const MAX_CHANNELS: usize = 65_536;
/// Maximum capability slots declared by one protection domain.
pub const MAX_CAPABILITY_SLOTS: usize = 65_536;

/// Stable logical processor identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CoreId(u16);

impl CoreId {
    /// Creates an identifier.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl CanonicalEncode for CoreId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Stable interrupt identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IrqId(u32);

impl IrqId {
    /// Creates an identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for IrqId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Stable protection-domain identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProtectionDomainId(u32);

impl ProtectionDomainId {
    /// Creates an identifier.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl CanonicalEncode for ProtectionDomainId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Stable time-protection domain identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimeDomainId(u16);

impl TimeDomainId {
    /// Creates an identifier.
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    /// Returns the raw identifier.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl CanonicalEncode for TimeDomainId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Closed kernel-object kind registry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum ObjectKind {
    /// Schedulable thread-control object.
    Thread = 1,
    /// Virtual address-space root.
    AddressSpace = 2,
    /// Synchronous IPC endpoint.
    Endpoint = 3,
    /// Asynchronous notification object.
    Notification = 4,
    /// One-shot reply authority.
    Reply = 5,
    /// Budget and period authority.
    SchedulingContext = 6,
    /// Physical memory frame.
    Frame = 7,
    /// Translation-table object.
    PageTable = 8,
    /// Interrupt-control object.
    Interrupt = 9,
    /// Device-MMIO authority.
    Device = 10,
    /// IOMMU protection domain.
    IommuDomain = 11,
    /// Processor-core authority.
    Core = 12,
    /// Time-protection domain.
    TimeDomain = 13,
    /// Untyped memory from which typed objects may be derived.
    UntypedMemory = 14,
}

impl ObjectKind {
    const fn tag(self) -> u16 {
        self as u16
    }
}

impl CanonicalEncode for ObjectKind {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.tag().to_be_bytes());
        Ok(())
    }
}

/// Generation-tagged kernel-object identity.
///
/// Reusing an index with a new generation creates a different identity, which
/// prevents stale capabilities from silently naming a replacement object.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ObjectId {
    kind: ObjectKind,
    generation: u32,
    index: u64,
}

impl ObjectId {
    /// Creates an object identity.
    #[must_use]
    pub const fn new(kind: ObjectKind, generation: u32, index: u64) -> Self {
        Self {
            kind,
            generation,
            index,
        }
    }

    /// Returns the object kind.
    #[must_use]
    pub const fn kind(self) -> ObjectKind {
        self.kind
    }

    /// Returns the allocation generation.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Returns the stable index within the kind namespace.
    #[must_use]
    pub const fn index(self) -> u64 {
        self.index
    }
}

impl CanonicalEncode for ObjectId {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.kind.encode_to(output)?;
        output.extend_from_slice(&self.generation.to_be_bytes());
        output.extend_from_slice(&self.index.to_be_bytes());
        Ok(())
    }
}

/// Capability rights as a closed, non-amplifying bit set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Rights(u64);

impl Rights {
    /// Read object state or mapped memory.
    pub const READ: Self = Self(1 << 0);
    /// Modify object state or mapped memory.
    pub const WRITE: Self = Self(1 << 1);
    /// Execute mapped memory.
    pub const EXECUTE: Self = Self(1 << 2);
    /// Signal an endpoint or notification.
    pub const SIGNAL: Self = Self(1 << 3);
    /// Receive from an endpoint or notification.
    pub const RECEIVE: Self = Self(1 << 4);
    /// Transfer capabilities through IPC.
    pub const GRANT: Self = Self(1 << 5);
    /// Control lifecycle or configuration of the object.
    pub const CONTROL: Self = Self(1 << 6);
    /// Map or unmap the object in a translation structure.
    pub const MAP: Self = Self(1 << 7);
    /// Bind an object to another kernel object.
    pub const BIND: Self = Self(1 << 8);
    /// Derive a capability with a different badge.
    pub const MINT: Self = Self(1 << 9);
    /// Revoke capabilities derived from this authority.
    pub const REVOKE: Self = Self(1 << 10);
    /// Retype untyped memory into typed kernel objects.
    pub const RETYPE: Self = Self(1 << 11);

    const ALL_BITS: u64 = (1 << 12) - 1;
    const MEMORY_BITS: u64 = Self::READ.0 | Self::WRITE.0 | Self::EXECUTE.0;

    /// Creates a rights set only when no unknown bit is present.
    pub const fn from_bits(bits: u64) -> Result<Self, CapabilityError> {
        if bits & !Self::ALL_BITS == 0 {
            Ok(Self(bits))
        } else {
            Err(CapabilityError::UnknownRights)
        }
    }

    /// Returns the raw stable bit representation.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// Returns whether no right is present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns whether this set includes every requested right.
    #[must_use]
    pub const fn contains(self, requested: Self) -> bool {
        self.0 & requested.0 == requested.0
    }

    /// Returns whether this rights set is a subset of another.
    #[must_use]
    pub const fn is_subset_of(self, parent: Self) -> bool {
        parent.contains(self)
    }

    /// Returns memory-mapping rights only.
    #[must_use]
    pub const fn memory_rights(self) -> Self {
        Self(self.0 & Self::MEMORY_BITS)
    }
}

impl core::ops::BitOr for Rights {
    type Output = Self;

    fn bitor(self, right: Self) -> Self::Output {
        Self(self.0 | right.0)
    }
}

impl CanonicalEncode for Rights {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

/// Immutable capability value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Capability {
    object: ObjectId,
    rights: Rights,
    badge: u64,
}

impl Capability {
    /// Creates a non-empty capability.
    pub const fn try_new(
        object: ObjectId,
        rights: Rights,
        badge: u64,
    ) -> Result<Self, CapabilityError> {
        if rights.is_empty() {
            return Err(CapabilityError::EmptyRights);
        }
        Ok(Self {
            object,
            rights,
            badge,
        })
    }

    /// Returns the named object.
    #[must_use]
    pub const fn object(self) -> ObjectId {
        self.object
    }

    /// Returns the rights set.
    #[must_use]
    pub const fn rights(self) -> Rights {
        self.rights
    }

    /// Returns the IPC badge.
    #[must_use]
    pub const fn badge(self) -> u64 {
        self.badge
    }

    /// Derives a capability without authority amplification.
    ///
    /// Changing the badge additionally requires `MINT` in the parent.
    pub const fn derive(
        self,
        requested_rights: Rights,
        requested_badge: u64,
    ) -> Result<Self, CapabilityError> {
        if requested_rights.is_empty() {
            return Err(CapabilityError::EmptyRights);
        }
        if !requested_rights.is_subset_of(self.rights) {
            return Err(CapabilityError::RightsAmplification);
        }
        if requested_badge != self.badge && !self.rights.contains(Rights::MINT) {
            return Err(CapabilityError::BadgeMintWithoutAuthority);
        }
        Ok(Self {
            object: self.object,
            rights: requested_rights,
            badge: requested_badge,
        })
    }
}

impl CanonicalEncode for Capability {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.object.encode_to(output)?;
        self.rights.encode_to(output)?;
        output.extend_from_slice(&self.badge.to_be_bytes());
        Ok(())
    }
}

/// Capability construction or derivation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    /// A rights mask contains a bit outside the closed registry.
    UnknownRights,
    /// A present capability may not carry an empty rights set.
    EmptyRights,
    /// A child requested authority absent from its parent.
    RightsAmplification,
    /// A badge was changed without `MINT` authority.
    BadgeMintWithoutAuthority,
}

impl fmt::Display for CapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRights => formatter.write_str("capability rights contain unknown bits"),
            Self::EmptyRights => formatter.write_str("capability rights are empty"),
            Self::RightsAmplification => {
                formatter.write_str("derived capability would amplify authority")
            }
            Self::BadgeMintWithoutAuthority => {
                formatter.write_str("badge change requires mint authority")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for CapabilityError {}

/// Mixed-criticality class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Criticality {
    /// Best-effort work.
    BestEffort = 0,
    /// Ordinary application work.
    Normal = 1,
    /// Mission-critical work.
    Critical = 2,
    /// Safety-critical work.
    Safety = 3,
}

impl CanonicalEncode for Criticality {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Explicit scheduling and time-protection authority.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchedulingContext {
    budget_ticks: u64,
    deadline_ticks: u64,
    period_ticks: u64,
    criticality: Criticality,
    time_domain: TimeDomainId,
}

impl SchedulingContext {
    /// Creates a context satisfying the Lucy v1 bounded scheduling relation:
    /// `0 < budget <= deadline <= period`.
    pub const fn try_new(
        budget_ticks: u64,
        deadline_ticks: u64,
        period_ticks: u64,
        criticality: Criticality,
        time_domain: TimeDomainId,
    ) -> Result<Self, SchedulingError> {
        if budget_ticks == 0 || deadline_ticks == 0 || period_ticks == 0 {
            return Err(SchedulingError::ZeroDuration);
        }
        if budget_ticks > deadline_ticks {
            return Err(SchedulingError::BudgetExceedsDeadline);
        }
        if deadline_ticks > period_ticks {
            return Err(SchedulingError::DeadlineExceedsPeriod);
        }
        Ok(Self {
            budget_ticks,
            deadline_ticks,
            period_ticks,
            criticality,
            time_domain,
        })
    }

    /// Returns the execution budget.
    #[must_use]
    pub const fn budget_ticks(self) -> u64 {
        self.budget_ticks
    }

    /// Returns the relative deadline.
    #[must_use]
    pub const fn deadline_ticks(self) -> u64 {
        self.deadline_ticks
    }

    /// Returns the replenishment period.
    #[must_use]
    pub const fn period_ticks(self) -> u64 {
        self.period_ticks
    }

    /// Returns the criticality class.
    #[must_use]
    pub const fn criticality(self) -> Criticality {
        self.criticality
    }

    /// Returns the time-protection domain.
    #[must_use]
    pub const fn time_domain(self) -> TimeDomainId {
        self.time_domain
    }
}

impl CanonicalEncode for SchedulingContext {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.budget_ticks.to_be_bytes());
        output.extend_from_slice(&self.deadline_ticks.to_be_bytes());
        output.extend_from_slice(&self.period_ticks.to_be_bytes());
        self.criticality.encode_to(output)?;
        self.time_domain.encode_to(output)
    }
}

/// Scheduling-context construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulingError {
    /// Durations are discrete positive resources.
    ZeroDuration,
    /// The execution budget exceeds the relative deadline.
    BudgetExceedsDeadline,
    /// The relative deadline exceeds the replenishment period.
    DeadlineExceedsPeriod,
}

impl fmt::Display for SchedulingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDuration => formatter.write_str("scheduling durations must be non-zero"),
            Self::BudgetExceedsDeadline => {
                formatter.write_str("budget exceeds relative deadline")
            }
            Self::DeadlineExceedsPeriod => {
                formatter.write_str("relative deadline exceeds period")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SchedulingError {}

/// Memory access that triggered a fault.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum AccessType {
    /// Read access.
    Read = 0,
    /// Write access.
    Write = 1,
    /// Instruction fetch.
    Execute = 2,
}

impl CanonicalEncode for AccessType {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Inter-processor interrupt purpose.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum IpiReason {
    /// Request remote rescheduling.
    Reschedule = 0,
    /// Request remote TLB invalidation.
    TlbShootdown = 1,
    /// Request a time-domain transition.
    TimeDomainSwitch = 2,
    /// Request core shutdown.
    Shutdown = 3,
}

impl CanonicalEncode for IpiReason {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.push(*self as u8);
        Ok(())
    }
}

/// Closed input event admitted by the pure kernel transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KernelEvent {
    /// Userspace system call with a bounded machine-word argument vector.
    SystemCall {
        /// Calling thread.
        thread: ObjectId,
        /// Stable syscall number.
        number: u32,
        /// Exact argument words.
        arguments: Box<[u64]>,
    },
    /// Architecture-normalized page fault.
    PageFault {
        /// Faulting thread.
        thread: ObjectId,
        /// Active address space.
        address_space: ObjectId,
        /// Faulting virtual address.
        virtual_address: u64,
        /// Instruction pointer at the fault.
        instruction_pointer: u64,
        /// Requested access.
        access: AccessType,
    },
    /// External interrupt delivered to one core.
    Interrupt {
        /// Interrupt number.
        irq: IrqId,
        /// Receiving core.
        core: CoreId,
    },
    /// Explicit timer observation supplied by the machine boundary.
    Timer {
        /// Receiving core.
        core: CoreId,
        /// Monotonic logical tick value.
        now_ticks: u64,
    },
    /// Inter-processor interrupt.
    InterProcessorInterrupt {
        /// Sending core.
        source: CoreId,
        /// Receiving core.
        target: CoreId,
        /// Stable reason.
        reason: IpiReason,
    },
}

impl KernelEvent {
    /// Creates a bounded syscall event.
    pub fn system_call(
        thread: ObjectId,
        number: u32,
        arguments: Vec<u64>,
    ) -> Result<Self, EventError> {
        if thread.kind() != ObjectKind::Thread {
            return Err(EventError::WrongObjectKind);
        }
        if arguments.len() > MAX_SYSCALL_ARGUMENTS {
            return Err(EventError::TooManySyscallArguments);
        }
        Ok(Self::SystemCall {
            thread,
            number,
            arguments: arguments.into_boxed_slice(),
        })
    }

    /// Validates object kinds for a page-fault event.
    pub const fn page_fault(
        thread: ObjectId,
        address_space: ObjectId,
        virtual_address: u64,
        instruction_pointer: u64,
        access: AccessType,
    ) -> Result<Self, EventError> {
        if !matches!(thread.kind(), ObjectKind::Thread)
            || !matches!(address_space.kind(), ObjectKind::AddressSpace)
        {
            return Err(EventError::WrongObjectKind);
        }
        Ok(Self::PageFault {
            thread,
            address_space,
            virtual_address,
            instruction_pointer,
            access,
        })
    }
}

impl CanonicalEncode for KernelEvent {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&OS_PROFILE_VERSION.to_be_bytes());
        match self {
            Self::SystemCall {
                thread,
                number,
                arguments,
            } => {
                output.push(0);
                thread.encode_to(output)?;
                output.extend_from_slice(&number.to_be_bytes());
                put_length(output, arguments.len())?;
                for argument in arguments {
                    output.extend_from_slice(&argument.to_be_bytes());
                }
            }
            Self::PageFault {
                thread,
                address_space,
                virtual_address,
                instruction_pointer,
                access,
            } => {
                output.push(1);
                thread.encode_to(output)?;
                address_space.encode_to(output)?;
                output.extend_from_slice(&virtual_address.to_be_bytes());
                output.extend_from_slice(&instruction_pointer.to_be_bytes());
                access.encode_to(output)?;
            }
            Self::Interrupt { irq, core } => {
                output.push(2);
                irq.encode_to(output)?;
                core.encode_to(output)?;
            }
            Self::Timer { core, now_ticks } => {
                output.push(3);
                core.encode_to(output)?;
                output.extend_from_slice(&now_ticks.to_be_bytes());
            }
            Self::InterProcessorInterrupt {
                source,
                target,
                reason,
            } => {
                output.push(4);
                source.encode_to(output)?;
                target.encode_to(output)?;
                reason.encode_to(output)?;
            }
        }
        Ok(())
    }
}

/// Kernel-event construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventError {
    /// A referenced object has an incompatible kind.
    WrongObjectKind,
    /// A syscall exceeded the fixed argument bound.
    TooManySyscallArguments,
}

impl fmt::Display for EventError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongObjectKind => formatter.write_str("kernel event references wrong object kind"),
            Self::TooManySyscallArguments => {
                formatter.write_str("syscall argument count exceeds profile bound")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EventError {}

/// Closed hardware-facing operation produced by the pure kernel transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MachineOp {
    /// Selects the active translation root on a core.
    SwitchAddressSpace {
        /// Target core.
        core: CoreId,
        /// Address-space root.
        address_space: ObjectId,
    },
    /// Installs one translation.
    MapTranslation {
        /// Address-space root.
        address_space: ObjectId,
        /// Page-aligned virtual address.
        virtual_address: u64,
        /// Physical frame.
        frame: ObjectId,
        /// Read/write/execute rights only.
        rights: Rights,
        /// Base-two page size.
        page_bits: u8,
    },
    /// Removes one translation.
    UnmapTranslation {
        /// Address-space root.
        address_space: ObjectId,
        /// Page-aligned virtual address.
        virtual_address: u64,
        /// Base-two page size.
        page_bits: u8,
    },
    /// Invalidates a translation cache scope.
    InvalidateTlb {
        /// Target core.
        core: CoreId,
        /// Optional address-space scope; absent means global on that core.
        address_space: Option<ObjectId>,
        /// Optional virtual-address scope.
        virtual_address: Option<u64>,
    },
    /// Programs one timer deadline.
    ProgramTimer {
        /// Target core.
        core: CoreId,
        /// Absolute logical deadline.
        deadline_ticks: u64,
    },
    /// Acknowledges one external interrupt.
    AcknowledgeInterrupt {
        /// Interrupt number.
        irq: IrqId,
    },
    /// Sends an inter-processor interrupt.
    SendIpi {
        /// Sending core.
        source: CoreId,
        /// Receiving core.
        target: CoreId,
        /// Stable reason.
        reason: IpiReason,
    },
    /// Enters one userspace thread.
    EnterUser {
        /// Target core.
        core: CoreId,
        /// Thread object.
        thread: ObjectId,
        /// Userspace instruction pointer.
        instruction_pointer: u64,
        /// Userspace stack pointer.
        stack_pointer: u64,
    },
    /// Performs the architecture-specific mitigation required between time domains.
    FlushTimeDomain {
        /// Target core.
        core: CoreId,
        /// Domain being left.
        from: TimeDomainId,
        /// Domain being entered.
        to: TimeDomainId,
    },
    /// Configures an IOMMU mapping for a DMA-capable device.
    ConfigureIommu {
        /// IOMMU protection domain.
        domain: ObjectId,
        /// Device authority.
        device: ObjectId,
        /// Physical frame made visible to DMA.
        frame: ObjectId,
        /// Whether device writes are allowed.
        writable: bool,
    },
}

impl MachineOp {
    fn validate(&self) -> Result<(), MachinePlanError> {
        match self {
            Self::SwitchAddressSpace { address_space, .. } => {
                require_kind(*address_space, ObjectKind::AddressSpace)
            }
            Self::MapTranslation {
                address_space,
                virtual_address,
                frame,
                rights,
                page_bits,
            } => {
                require_kind(*address_space, ObjectKind::AddressSpace)?;
                require_kind(*frame, ObjectKind::Frame)?;
                validate_page(*virtual_address, *page_bits)?;
                if rights.is_empty() || rights.bits() != rights.memory_rights().bits() {
                    return Err(MachinePlanError::InvalidMappingRights);
                }
                Ok(())
            }
            Self::UnmapTranslation {
                address_space,
                virtual_address,
                page_bits,
            } => {
                require_kind(*address_space, ObjectKind::AddressSpace)?;
                validate_page(*virtual_address, *page_bits)
            }
            Self::InvalidateTlb { address_space, .. } => {
                if let Some(address_space) = address_space {
                    require_kind(*address_space, ObjectKind::AddressSpace)?;
                }
                Ok(())
            }
            Self::EnterUser { thread, .. } => require_kind(*thread, ObjectKind::Thread),
            Self::FlushTimeDomain { from, to, .. } => {
                if from == to {
                    Err(MachinePlanError::RedundantTimeDomainFlush)
                } else {
                    Ok(())
                }
            }
            Self::ConfigureIommu {
                domain,
                device,
                frame,
                ..
            } => {
                require_kind(*domain, ObjectKind::IommuDomain)?;
                require_kind(*device, ObjectKind::Device)?;
                require_kind(*frame, ObjectKind::Frame)
            }
            Self::ProgramTimer { .. }
            | Self::AcknowledgeInterrupt { .. }
            | Self::SendIpi { .. } => Ok(()),
        }
    }
}

impl CanonicalEncode for MachineOp {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        match self {
            Self::SwitchAddressSpace {
                core,
                address_space,
            } => {
                output.push(0);
                core.encode_to(output)?;
                address_space.encode_to(output)?;
            }
            Self::MapTranslation {
                address_space,
                virtual_address,
                frame,
                rights,
                page_bits,
            } => {
                output.push(1);
                address_space.encode_to(output)?;
                output.extend_from_slice(&virtual_address.to_be_bytes());
                frame.encode_to(output)?;
                rights.encode_to(output)?;
                output.push(*page_bits);
            }
            Self::UnmapTranslation {
                address_space,
                virtual_address,
                page_bits,
            } => {
                output.push(2);
                address_space.encode_to(output)?;
                output.extend_from_slice(&virtual_address.to_be_bytes());
                output.push(*page_bits);
            }
            Self::InvalidateTlb {
                core,
                address_space,
                virtual_address,
            } => {
                output.push(3);
                core.encode_to(output)?;
                put_option_object(output, address_space)?;
                put_option_u64(output, *virtual_address);
            }
            Self::ProgramTimer {
                core,
                deadline_ticks,
            } => {
                output.push(4);
                core.encode_to(output)?;
                output.extend_from_slice(&deadline_ticks.to_be_bytes());
            }
            Self::AcknowledgeInterrupt { irq } => {
                output.push(5);
                irq.encode_to(output)?;
            }
            Self::SendIpi {
                source,
                target,
                reason,
            } => {
                output.push(6);
                source.encode_to(output)?;
                target.encode_to(output)?;
                reason.encode_to(output)?;
            }
            Self::EnterUser {
                core,
                thread,
                instruction_pointer,
                stack_pointer,
            } => {
                output.push(7);
                core.encode_to(output)?;
                thread.encode_to(output)?;
                output.extend_from_slice(&instruction_pointer.to_be_bytes());
                output.extend_from_slice(&stack_pointer.to_be_bytes());
            }
            Self::FlushTimeDomain { core, from, to } => {
                output.push(8);
                core.encode_to(output)?;
                from.encode_to(output)?;
                to.encode_to(output)?;
            }
            Self::ConfigureIommu {
                domain,
                device,
                frame,
                writable,
            } => {
                output.push(9);
                domain.encode_to(output)?;
                device.encode_to(output)?;
                frame.encode_to(output)?;
                output.push(u8::from(*writable));
            }
        }
        Ok(())
    }
}

/// Ordered, bounded hardware-effect plan.
///
/// Operation order is semantic and is never sorted by the library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachinePlan {
    operations: Box<[MachineOp]>,
}

impl MachinePlan {
    /// Creates a plan after validating every operation and the global bound.
    pub fn try_new(operations: Vec<MachineOp>) -> Result<Self, MachinePlanError> {
        if operations.len() > MAX_MACHINE_OPERATIONS {
            return Err(MachinePlanError::TooManyOperations);
        }
        for operation in &operations {
            operation.validate()?;
        }
        Ok(Self {
            operations: operations.into_boxed_slice(),
        })
    }

    /// Returns operations in required execution order.
    #[must_use]
    pub fn operations(&self) -> &[MachineOp] {
        &self.operations
    }

    /// Returns whether the plan has no hardware effect.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

impl CanonicalEncode for MachinePlan {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&OS_PROFILE_VERSION.to_be_bytes());
        put_length(output, self.operations.len())?;
        for operation in &self.operations {
            operation.encode_to(output)?;
        }
        Ok(())
    }
}

/// Invalid machine-plan construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachinePlanError {
    /// Operation count exceeds the deterministic profile bound.
    TooManyOperations,
    /// One object has an incompatible kind.
    WrongObjectKind,
    /// A page size is outside the profile's supported range.
    InvalidPageBits,
    /// A virtual address is not aligned to its page size.
    MisalignedVirtualAddress,
    /// Mapping rights include non-memory authority or are empty.
    InvalidMappingRights,
    /// Flushing while remaining in the same time domain is rejected as a profile bug.
    RedundantTimeDomainFlush,
}

impl fmt::Display for MachinePlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyOperations => formatter.write_str("machine plan exceeds operation bound"),
            Self::WrongObjectKind => formatter.write_str("machine operation references wrong object kind"),
            Self::InvalidPageBits => formatter.write_str("page size is outside supported profile"),
            Self::MisalignedVirtualAddress => formatter.write_str("virtual address is not page aligned"),
            Self::InvalidMappingRights => formatter.write_str("mapping rights are not a non-empty memory subset"),
            Self::RedundantTimeDomainFlush => formatter.write_str("time-domain flush does not change domain"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for MachinePlanError {}

/// Capability slot in a protection domain's initial capability space.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CapabilitySlot {
    slot: u32,
    capability: Capability,
}

impl CapabilitySlot {
    /// Creates an initial capability slot.
    #[must_use]
    pub const fn new(slot: u32, capability: Capability) -> Self {
        Self { slot, capability }
    }

    /// Returns the slot index.
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    /// Returns the capability.
    #[must_use]
    pub const fn capability(self) -> Capability {
        self.capability
    }
}

impl CanonicalEncode for CapabilitySlot {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(&self.slot.to_be_bytes());
        self.capability.encode_to(output)
    }
}

/// One statically declared protection domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectionDomainSpec {
    id: ProtectionDomainId,
    initial_thread: ObjectId,
    initial_address_space: ObjectId,
    scheduling: SchedulingContext,
    capabilities: Box<[CapabilitySlot]>,
}

impl ProtectionDomainSpec {
    /// Creates a domain and canonicalizes its initial capability slots.
    pub fn try_new(
        id: ProtectionDomainId,
        initial_thread: ObjectId,
        initial_address_space: ObjectId,
        scheduling: SchedulingContext,
        mut capabilities: Vec<CapabilitySlot>,
    ) -> Result<Self, SystemDescriptionError> {
        if initial_thread.kind() != ObjectKind::Thread
            || initial_address_space.kind() != ObjectKind::AddressSpace
        {
            return Err(SystemDescriptionError::WrongObjectKind);
        }
        if capabilities.len() > MAX_CAPABILITY_SLOTS {
            return Err(SystemDescriptionError::TooManyCapabilitySlots);
        }
        capabilities.sort_by_key(|entry| entry.slot());
        if capabilities.windows(2).any(|pair| pair[0].slot() == pair[1].slot()) {
            return Err(SystemDescriptionError::DuplicateCapabilitySlot);
        }
        Ok(Self {
            id,
            initial_thread,
            initial_address_space,
            scheduling,
            capabilities: capabilities.into_boxed_slice(),
        })
    }

    /// Returns the protection-domain identifier.
    #[must_use]
    pub const fn id(&self) -> ProtectionDomainId {
        self.id
    }

    /// Returns initial capability slots in canonical slot order.
    #[must_use]
    pub fn capabilities(&self) -> &[CapabilitySlot] {
        &self.capabilities
    }
}

impl CanonicalEncode for ProtectionDomainSpec {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.id.encode_to(output)?;
        self.initial_thread.encode_to(output)?;
        self.initial_address_space.encode_to(output)?;
        self.scheduling.encode_to(output)?;
        put_length(output, self.capabilities.len())?;
        for slot in &self.capabilities {
            slot.encode_to(output)?;
        }
        Ok(())
    }
}

/// Statically declared IPC channel.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChannelSpec {
    sender: ProtectionDomainId,
    receiver: ProtectionDomainId,
    endpoint: ObjectId,
    badge: u64,
}

impl ChannelSpec {
    /// Creates a channel to an endpoint object.
    pub const fn try_new(
        sender: ProtectionDomainId,
        receiver: ProtectionDomainId,
        endpoint: ObjectId,
        badge: u64,
    ) -> Result<Self, SystemDescriptionError> {
        if !matches!(endpoint.kind(), ObjectKind::Endpoint) {
            return Err(SystemDescriptionError::WrongObjectKind);
        }
        Ok(Self {
            sender,
            receiver,
            endpoint,
            badge,
        })
    }
}

impl CanonicalEncode for ChannelSpec {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        self.sender.encode_to(output)?;
        self.receiver.encode_to(output)?;
        self.endpoint.encode_to(output)?;
        output.extend_from_slice(&self.badge.to_be_bytes());
        Ok(())
    }
}

/// Canonical static system architecture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDescription {
    domains: Box<[ProtectionDomainSpec]>,
    channels: Box<[ChannelSpec]>,
}

impl SystemDescription {
    /// Creates a system after canonical ordering and referential checks.
    pub fn try_new(
        mut domains: Vec<ProtectionDomainSpec>,
        mut channels: Vec<ChannelSpec>,
    ) -> Result<Self, SystemDescriptionError> {
        if domains.len() > MAX_PROTECTION_DOMAINS {
            return Err(SystemDescriptionError::TooManyProtectionDomains);
        }
        if channels.len() > MAX_CHANNELS {
            return Err(SystemDescriptionError::TooManyChannels);
        }
        domains.sort_by_key(ProtectionDomainSpec::id);
        if domains.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(SystemDescriptionError::DuplicateProtectionDomain);
        }
        channels.sort();
        if channels.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(SystemDescriptionError::DuplicateChannel);
        }
        for channel in &channels {
            if domains.binary_search_by_key(&channel.sender, ProtectionDomainSpec::id).is_err()
                || domains.binary_search_by_key(&channel.receiver, ProtectionDomainSpec::id).is_err()
            {
                return Err(SystemDescriptionError::UnknownProtectionDomain);
            }
        }
        Ok(Self {
            domains: domains.into_boxed_slice(),
            channels: channels.into_boxed_slice(),
        })
    }

    /// Returns domains in canonical identifier order.
    #[must_use]
    pub fn domains(&self) -> &[ProtectionDomainSpec] {
        &self.domains
    }

    /// Returns channels in canonical tuple order.
    #[must_use]
    pub fn channels(&self) -> &[ChannelSpec] {
        &self.channels
    }
}

impl CanonicalEncode for SystemDescription {
    fn encode_to(&self, output: &mut Vec<u8>) -> Result<(), EncodeError> {
        output.extend_from_slice(b"ZFCIS-OS-SYSTEM\0");
        output.extend_from_slice(&OS_PROFILE_VERSION.to_be_bytes());
        put_length(output, self.domains.len())?;
        for domain in &self.domains {
            domain.encode_to(output)?;
        }
        put_length(output, self.channels.len())?;
        for channel in &self.channels {
            channel.encode_to(output)?;
        }
        Ok(())
    }
}

/// Static system-description failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemDescriptionError {
    /// A referenced object has the wrong kind.
    WrongObjectKind,
    /// Domain count exceeds the deterministic profile bound.
    TooManyProtectionDomains,
    /// Channel count exceeds the deterministic profile bound.
    TooManyChannels,
    /// Capability-slot count exceeds the per-domain bound.
    TooManyCapabilitySlots,
    /// Two domains use the same stable identifier.
    DuplicateProtectionDomain,
    /// Two capability entries use the same slot.
    DuplicateCapabilitySlot,
    /// Two channels have identical identities.
    DuplicateChannel,
    /// A channel references a domain absent from the system description.
    UnknownProtectionDomain,
}

impl fmt::Display for SystemDescriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongObjectKind => formatter.write_str("system description references wrong object kind"),
            Self::TooManyProtectionDomains => formatter.write_str("protection-domain count exceeds bound"),
            Self::TooManyChannels => formatter.write_str("channel count exceeds bound"),
            Self::TooManyCapabilitySlots => formatter.write_str("capability-slot count exceeds bound"),
            Self::DuplicateProtectionDomain => formatter.write_str("duplicate protection-domain identifier"),
            Self::DuplicateCapabilitySlot => formatter.write_str("duplicate capability slot"),
            Self::DuplicateChannel => formatter.write_str("duplicate channel"),
            Self::UnknownProtectionDomain => formatter.write_str("channel references unknown protection domain"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SystemDescriptionError {}

fn require_kind(object: ObjectId, expected: ObjectKind) -> Result<(), MachinePlanError> {
    if object.kind() == expected {
        Ok(())
    } else {
        Err(MachinePlanError::WrongObjectKind)
    }
}

fn validate_page(virtual_address: u64, page_bits: u8) -> Result<(), MachinePlanError> {
    if !(12..=52).contains(&page_bits) {
        return Err(MachinePlanError::InvalidPageBits);
    }
    let mask = (1_u64 << page_bits) - 1;
    if virtual_address & mask != 0 {
        return Err(MachinePlanError::MisalignedVirtualAddress);
    }
    Ok(())
}

fn put_length(output: &mut Vec<u8>, length: usize) -> Result<(), EncodeError> {
    let length = u32::try_from(length).map_err(|_| EncodeError::LengthOverflow)?;
    output.extend_from_slice(&length.to_be_bytes());
    Ok(())
}

fn put_option_object(
    output: &mut Vec<u8>,
    value: &Option<ObjectId>,
) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            value.encode_to(output)?;
        }
    }
    Ok(())
}

fn put_option_u64(output: &mut Vec<u8>, value: Option<u64>) {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(kind: ObjectKind, index: u64) -> ObjectId {
        ObjectId::new(kind, 1, index)
    }

    fn scheduling() -> SchedulingContext {
        match SchedulingContext::try_new(
            10,
            20,
            100,
            Criticality::Critical,
            TimeDomainId::new(1),
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid scheduling context rejected: {error}"),
        }
    }

    fn capability(rights: Rights) -> Capability {
        match Capability::try_new(object(ObjectKind::Endpoint, 1), rights, 7) {
            Ok(value) => value,
            Err(error) => panic!("valid capability rejected: {error}"),
        }
    }

    #[test]
    fn derivation_cannot_amplify_rights() {
        let parent = capability(Rights::SIGNAL | Rights::MINT);
        assert_eq!(
            parent.derive(Rights::SIGNAL | Rights::RECEIVE, 7),
            Err(CapabilityError::RightsAmplification)
        );
        let child = parent.derive(Rights::SIGNAL, 9);
        assert!(child.is_ok());
    }

    #[test]
    fn badge_change_requires_mint_authority() {
        let parent = capability(Rights::SIGNAL);
        assert_eq!(
            parent.derive(Rights::SIGNAL, 9),
            Err(CapabilityError::BadgeMintWithoutAuthority)
        );
    }

    #[test]
    fn scheduling_relation_is_explicit() {
        assert_eq!(
            SchedulingContext::try_new(
                30,
                20,
                100,
                Criticality::Critical,
                TimeDomainId::new(1),
            ),
            Err(SchedulingError::BudgetExceedsDeadline)
        );
        assert_eq!(
            SchedulingContext::try_new(
                10,
                101,
                100,
                Criticality::Critical,
                TimeDomainId::new(1),
            ),
            Err(SchedulingError::DeadlineExceedsPeriod)
        );
    }

    #[test]
    fn mapping_rejects_non_memory_authority() {
        let result = MachinePlan::try_new(vec![MachineOp::MapTranslation {
            address_space: object(ObjectKind::AddressSpace, 1),
            virtual_address: 0x4000,
            frame: object(ObjectKind::Frame, 1),
            rights: Rights::READ | Rights::CONTROL,
            page_bits: 12,
        }]);
        assert_eq!(result, Err(MachinePlanError::InvalidMappingRights));
    }

    #[test]
    fn machine_plan_order_is_semantic() {
        let first = MachineOp::ProgramTimer {
            core: CoreId::new(0),
            deadline_ticks: 10,
        };
        let second = MachineOp::AcknowledgeInterrupt { irq: IrqId::new(5) };
        let left = match MachinePlan::try_new(vec![first.clone(), second.clone()]) {
            Ok(value) => value,
            Err(error) => panic!("valid plan rejected: {error}"),
        };
        let right = match MachinePlan::try_new(vec![second, first]) {
            Ok(value) => value,
            Err(error) => panic!("valid plan rejected: {error}"),
        };
        assert_ne!(left.canonical_bytes(), right.canonical_bytes());
    }

    #[test]
    fn generation_prevents_object_identity_reuse() {
        let old = ObjectId::new(ObjectKind::Thread, 1, 8);
        let replacement = ObjectId::new(ObjectKind::Thread, 2, 8);
        assert_ne!(old, replacement);
        assert_ne!(old.canonical_bytes(), replacement.canonical_bytes());
    }

    #[test]
    fn system_description_is_history_independent() {
        let sender = match ProtectionDomainSpec::try_new(
            ProtectionDomainId::new(2),
            object(ObjectKind::Thread, 2),
            object(ObjectKind::AddressSpace, 2),
            scheduling(),
            vec![],
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid domain rejected: {error}"),
        };
        let receiver = match ProtectionDomainSpec::try_new(
            ProtectionDomainId::new(1),
            object(ObjectKind::Thread, 1),
            object(ObjectKind::AddressSpace, 1),
            scheduling(),
            vec![],
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid domain rejected: {error}"),
        };
        let channel = match ChannelSpec::try_new(
            ProtectionDomainId::new(2),
            ProtectionDomainId::new(1),
            object(ObjectKind::Endpoint, 1),
            42,
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid channel rejected: {error}"),
        };
        let left = SystemDescription::try_new(vec![sender.clone(), receiver.clone()], vec![channel]);
        let right = SystemDescription::try_new(vec![receiver, sender], vec![channel]);
        assert_eq!(left, right);
    }

    #[test]
    fn channels_must_reference_declared_domains() {
        let domain = match ProtectionDomainSpec::try_new(
            ProtectionDomainId::new(1),
            object(ObjectKind::Thread, 1),
            object(ObjectKind::AddressSpace, 1),
            scheduling(),
            vec![],
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid domain rejected: {error}"),
        };
        let channel = match ChannelSpec::try_new(
            ProtectionDomainId::new(1),
            ProtectionDomainId::new(99),
            object(ObjectKind::Endpoint, 1),
            0,
        ) {
            Ok(value) => value,
            Err(error) => panic!("valid endpoint rejected: {error}"),
        };
        assert_eq!(
            SystemDescription::try_new(vec![domain], vec![channel]),
            Err(SystemDescriptionError::UnknownProtectionDomain)
        );
    }
}
