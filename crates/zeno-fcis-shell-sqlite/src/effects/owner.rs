//! Trusted in-process dispatch over actual approved SQLite intents.
//!
//! Trusted administration configures `TrustedDispatcher`, selects authenticated identities
//! and grants access to an exact retained intent. Workers receive only a
//! `WorkerSession` and opaque handles. Safe Rust privacy does not contain native
//! code with ambient filesystem, process or credential access; ingress
//! authentication and OS confinement are separate obligations.
//!
//! Restart revokes every volatile grant. Opening restores existing durable
//! attempt reservations; it never creates replacement state. The deployment
//! must protect both database files against rollback, substitution and cloning.
//! Advisory locks exclude cooperating users of the same files, not independent
//! restored copies. The transport and its provenance remain trusted adapters.

use std::{fs::File, path::Path, sync::Arc};

use zeno_fcis_codec::Hash32;
use zeno_fcis_shell::effects::{
    driver::Binding,
    durable::{self, Relay, StateStore},
    handler::Handled,
    policy::Policy,
    port::{Delivery, OpId, Operation, Response, Sent},
    record::{Implementation, Transcript},
    registry,
};

use super::{
    durable::SqliteStateStore,
    production::{NoTime, Production, Transport},
};
use crate::v2::{self, Lineage};

/// Explicit owner-local resource limits. Rejected authorization does not consume
/// a dispatch slot; every admitted policy attempt does, including a policy refusal.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum simultaneously retained grants.
    pub grants: usize,
    /// Sum of retained grant structures and their destination/payload lengths.
    /// Allocator overhead and transient SQLite buffers are not included.
    pub intent_bytes: usize,
    /// Maximum recorded dispatch attempts in this owner incarnation.
    pub dispatches: u64,
}

/// Immutable trusted configuration. Namespace labels are not authentication.
#[derive(Clone, Copy, Debug)]
pub struct Configuration {
    /// Deployment identity, separate from the genesis-based store identity.
    pub deployment: Hash32,
    /// Tenant to which every grant in this owner belongs.
    pub tenant: Hash32,
    /// Transcript epoch selected by trusted administration; not a fencing token.
    pub epoch: u64,
    /// Local resource bounds.
    pub limits: Limits,
}

/// Admission refusal order, checked over all 128 valuations of the seven facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The handle belongs to another owner incarnation.
    Owner,
    /// The authenticated session belongs to another tenant.
    Tenant,
    /// The authenticated session is not the grant's recipient.
    Principal,
    /// The grant was revoked or is no longer present.
    Grant,
    /// The grant does not bind this logical intent and recipient.
    Intent,
    /// The original intent is absent or acknowledged.
    Pending,
    /// The full retained intent differs from the granted content.
    Content,
}

/// Failure before admission, or an unresolved failure after reservation.
#[derive(Debug)]
pub enum Error {
    /// The worker has no current authority for this operation.
    Refused(Refusal),
    /// The audited application store refused the request.
    Store(v2::Error),
    /// Durable reservation storage or restoration refused. Reopen to recover.
    Reservation(durable::Error),
    /// A declared resource limit was reached or an integer would overflow.
    Capacity,
    /// Required adapter provenance is absent.
    Provenance,
    /// An uncertain failure or unwind stopped this owner. Reopen to recover.
    Stopped,
    /// The transport returned a success report for another intent or content.
    Reply,
}

/// Opaque, non-cloneable capability for one volatile grant and original intent.
/// Public data, a hash or a serialized receipt cannot construct one.
///
/// ```compile_fail,E0451
/// use std::sync::Arc;
/// use zeno_fcis_codec::Hash32;
/// use zeno_fcis_shell_sqlite::effects::dispatcher::IntentHandle;
/// let handle = IntentHandle {
///     owner: Arc::new(()), grant: 1,
///     principal: Hash32::ZERO, intent: Hash32::ZERO,
/// };
/// ```
pub struct IntentHandle {
    owner: Arc<()>,
    grant: u64,
    principal: Hash32,
    intent: Hash32,
}

struct Grant {
    id: u64,
    principal: Hash32,
    delivery: Delivery,
    bytes: usize,
}

/// Trusted software dispatcher. Its terminal, transport, policy reservations
/// and grants have no mutable aliases exposed through the worker interface.
pub struct TrustedDispatcher<'a, 'p, T: Transport> {
    terminal: Production<'a, 'p, T>,
    relay: Relay<Reservations>,
    configuration: Configuration,
    brand: Arc<()>,
    grants: Vec<Grant>,
    retained: usize,
    next_grant: u64,
    attempts: u64,
    stopped: bool,
}

/// Compatibility name for the trusted software dispatcher. This does not mean
/// the human who authorizes the application's policy. Prefer [`TrustedDispatcher`].
pub type Owner<'a, 'p, T> = TrustedDispatcher<'a, 'p, T>;

/// A trusted ingress-created session. The worker cannot choose a new identity,
/// grant authority, acknowledge an intent, or access the raw terminal.
///
/// ```compile_fail,E0616
/// use zeno_fcis_shell_sqlite::effects::{dispatcher::WorkerSession, production::Transport};
/// fn bypass<T: Transport>(session: &mut WorkerSession<'_, '_, '_, T>) {
///     session.owner.terminal.transport();
/// }
/// ```
///
/// ```
/// use zeno_fcis_shell_sqlite::effects::{
///     dispatcher::{Error, IntentHandle, WorkerSession}, production::Transport,
/// };
/// use zeno_fcis_shell::effects::port::Sent;
/// fn dispatch<T: Transport>(session: &mut WorkerSession<'_, '_, '_, T>,
///                           intent: &IntentHandle) -> Result<Sent, Error> {
///     session.dispatch(intent)
/// }
/// ```
pub struct WorkerSession<'s, 'a, 'p, T: Transport> {
    owner: &'s mut TrustedDispatcher<'a, 'p, T>,
    tenant: Hash32,
    principal: Hash32,
}

// Pure admission relation. Facts come only from the owner's own comparisons.
fn admit(facts: [bool; 7]) -> Result<(), Error> {
    const REFUSALS: [Refusal; 7] = [
        Refusal::Owner,
        Refusal::Tenant,
        Refusal::Principal,
        Refusal::Grant,
        Refusal::Intent,
        Refusal::Pending,
        Refusal::Content,
    ];
    for (holds, refusal) in facts.into_iter().zip(REFUSALS) {
        if !holds {
            return Err(Error::Refused(refusal));
        }
    }
    Ok(())
}

impl<'a, 'p, T: Transport> TrustedDispatcher<'a, 'p, T> {
    /// Explicitly initialize a NEW reservation lifetime in a new file. Existing
    /// application state must already be approved. Never use this as an open or
    /// recovery fallback, nor create a new lifetime for previously attempted work.
    pub fn create(
        application: impl AsRef<Path>,
        reservations: impl AsRef<Path>,
        lineage: &'a Lineage<'a, 'p>,
        transport: T,
        configuration: Configuration,
        policies: &[Policy],
    ) -> Result<Self, Error> {
        Self::bind(
            application.as_ref(),
            reservations.as_ref(),
            lineage,
            transport,
            configuration,
            policies,
            true,
        )
    }

    /// Open the existing lifetime, with no initialization fallback. Grants start
    /// empty; regranting an intent cannot erase its durable send reservation.
    pub fn open(
        application: impl AsRef<Path>,
        reservations: impl AsRef<Path>,
        lineage: &'a Lineage<'a, 'p>,
        transport: T,
        configuration: Configuration,
        policies: &[Policy],
    ) -> Result<Self, Error> {
        Self::bind(
            application.as_ref(),
            reservations.as_ref(),
            lineage,
            transport,
            configuration,
            policies,
            false,
        )
    }

    fn bind(
        application: &Path,
        reservations: &Path,
        lineage: &'a Lineage<'a, 'p>,
        transport: T,
        configuration: Configuration,
        policies: &[Policy],
        create: bool,
    ) -> Result<Self, Error> {
        let terminal = Production::open_exclusive(application, lineage, 0, transport, Some(NoTime))
            .map_err(Error::Store)?;
        let mut manifest = terminal.manifest(Vec::new()).ok_or(Error::Provenance)?;
        manifest.program = source();
        manifest.stack.push("intent-owner");
        manifest.implementations.push(Implementation {
            name: "intent-owner",
            source: source(),
        });
        let namespace = namespace(configuration, terminal.store().identity);
        manifest
            .configuration
            .extend_from_slice(namespace.as_bytes());
        manifest
            .configuration
            .extend_from_slice(&configuration.epoch.to_be_bytes());
        for limit in [
            configuration.limits.grants,
            configuration.limits.intent_bytes,
        ] {
            let limit = u64::try_from(limit).map_err(|_| Error::Capacity)?;
            manifest
                .configuration
                .extend_from_slice(&limit.to_be_bytes());
        }
        manifest
            .configuration
            .extend_from_slice(&configuration.limits.dispatches.to_be_bytes());
        let store = if create {
            let initial = durable::initial(terminal.store().identity, policies)
                .map_err(Error::Reservation)?;
            SqliteStateStore::create(reservations, &envelope(namespace, &initial)?)
        } else {
            SqliteStateStore::open(reservations)
        }
        .map_err(Error::Reservation)?;
        let lock = File::options()
            .read(true)
            .write(true)
            .open(reservations)
            .map_err(|_| Error::Reservation(durable::Error::Storage))?;
        lock.try_lock()
            .map_err(|_| Error::Reservation(durable::Error::Conflict))?;
        let relay = Relay::open(
            Reservations {
                store,
                namespace,
                _lock: lock,
            },
            terminal.store().identity,
            policies,
            manifest,
        )
        .map_err(Error::Reservation)?;
        Ok(Self {
            terminal,
            relay,
            configuration,
            brand: Arc::new(()),
            grants: Vec::new(),
            retained: 0,
            next_grant: 0,
            attempts: 0,
            stopped: false,
        })
    }

    /// Trusted administration grants one principal access to an exact currently
    /// pending intent. No destination, payload or Boolean authority is accepted.
    pub fn grant(&mut self, principal: Hash32, intent: Hash32) -> Result<IntentHandle, Error> {
        if self.stopped {
            return Err(Error::Stopped);
        }
        if self.grants.len() >= self.configuration.limits.grants {
            return Err(Error::Capacity);
        }
        let id = self.next_grant.checked_add(1).ok_or(Error::Capacity)?;
        let delivery = self
            .terminal
            .pending_intent(intent)
            .map_err(pending_error)?;
        let bytes = delivery
            .destination
            .len()
            .checked_add(delivery.payload.len())
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Grant>()))
            .ok_or(Error::Capacity)?;
        let retained = self.retained.checked_add(bytes).ok_or(Error::Capacity)?;
        if retained > self.configuration.limits.intent_bytes {
            return Err(Error::Capacity);
        }
        self.grants.push(Grant {
            id,
            principal,
            delivery,
            bytes,
        });
        self.retained = retained;
        self.next_grant = id;
        Ok(IntentHandle {
            owner: Arc::clone(&self.brand),
            grant: id,
            principal,
            intent,
        })
    }

    /// Revoke future admissions; preserve every earlier durable reservation.
    /// A replacement grant receives a fresh ID, even after this record is removed.
    pub fn revoke(&mut self, handle: &IntentHandle) -> Result<bool, Error> {
        if !Arc::ptr_eq(&self.brand, &handle.owner) {
            return Err(Error::Refused(Refusal::Owner));
        }
        let Some(index) = self.grants.iter().position(|g| g.id == handle.grant) else {
            return Ok(false);
        };
        self.retained -= self.grants.swap_remove(index).bytes;
        Ok(true)
    }

    /// Trusted ingress supplies the authenticated identity. Only the returned
    /// session, never this administrative owner, may be lent to worker code.
    pub fn session(&mut self, tenant: Hash32, principal: Hash32) -> WorkerSession<'_, 'a, 'p, T> {
        WorkerSession {
            owner: self,
            tenant,
            principal,
        }
    }

    /// Immutable bounded audit data. This is not legacy DeliveryProgram replay
    /// evidence or proof that a transport performed the reported physical effect.
    #[must_use]
    pub fn transcript(&self) -> &Transcript {
        self.relay.transcript()
    }

    fn dispatch(
        &mut self,
        tenant: Hash32,
        principal: Hash32,
        handle: &IntentHandle,
    ) -> Result<Sent, Error> {
        let grant = self.grants.iter().find(|g| g.id == handle.grant);
        let facts = [
            Arc::ptr_eq(&self.brand, &handle.owner),
            tenant == self.configuration.tenant,
            principal == handle.principal,
            grant.is_some(),
            grant
                .is_some_and(|g| g.delivery.id == handle.intent && g.principal == handle.principal),
            true,
            true,
        ];
        admit(facts)?;
        if self.stopped {
            return Err(Error::Stopped);
        }
        let grant = grant.ok_or(Error::Refused(Refusal::Grant))?;
        let retained = self
            .terminal
            .pending_intent(handle.intent)
            .map_err(pending_error)?;
        let mut observed = facts;
        observed[6] = retained == grant.delivery;
        admit(observed)?;
        if self.attempts >= self.configuration.limits.dispatches {
            return Err(Error::Capacity);
        }
        let operation = Operation::Send(self.terminal.store(), retained);
        let binding = Binding::of(
            OpId {
                epoch: self.configuration.epoch,
                ordinal: self.attempts,
            },
            &operation,
        );
        self.attempts += 1;
        // Poison before a possibly effecting call, including unwinding adapters.
        self.stopped = true;
        let handled = self
            .relay
            .handle(&mut self.terminal, &binding, &operation)
            .map_err(Error::Reservation)?;
        let Handled::Answer(Response::Send(sent)) = handled else {
            return Err(Error::Stopped);
        };
        if let Sent::Sent(report) = &sent
            && (report.delivery_id != grant.delivery.id
                || report.payload_sha256 != grant.delivery.payload_sha256)
        {
            return Err(Error::Reply);
        }
        self.stopped = false;
        Ok(sent)
    }
}

impl<T: Transport> WorkerSession<'_, '_, '_, T> {
    /// Attempt the handle's original retained intent under current authority and
    /// durable policies. Unknown preserves the reservation and pending identity.
    pub fn dispatch(&mut self, handle: &IntentHandle) -> Result<Sent, Error> {
        self.owner.dispatch(self.tenant, self.principal, handle)
    }
}

fn pending_error(error: v2::Error) -> Error {
    match error {
        v2::Error::UnknownDelivery | v2::Error::AlreadyAcknowledged => {
            Error::Refused(Refusal::Pending)
        }
        error => Error::Store(error),
    }
}

fn source() -> Hash32 {
    registry::source_digest(&[("intent-owner.rs", include_bytes!("owner.rs"))])
}

fn namespace(configuration: Configuration, store: Hash32) -> Hash32 {
    registry::source_digest(&[
        ("owner", source().as_bytes()),
        ("deployment", configuration.deployment.as_bytes()),
        ("tenant", configuration.tenant.as_bytes()),
        ("store", store.as_bytes()),
    ])
}

const HEADER: &[u8] = b"ZFEFO001";

fn envelope(namespace: Hash32, bytes: &[u8]) -> Result<Vec<u8>, Error> {
    if bytes.len() > durable::MAX_BYTES - HEADER.len() - 32 {
        return Err(Error::Capacity);
    }
    let mut out = HEADER.to_vec();
    out.extend_from_slice(namespace.as_bytes());
    out.extend_from_slice(bytes);
    Ok(out)
}

// Reuse the existing durable mechanism. This envelope binds its otherwise
// genesis-only scope to the actual owner namespace; it is not a second policy.
struct Reservations {
    store: SqliteStateStore,
    namespace: Hash32,
    _lock: File,
}

impl StateStore for Reservations {
    fn load(&mut self) -> Result<Vec<u8>, durable::Error> {
        let bytes = self.store.load()?;
        let prefix = envelope(self.namespace, &[]).map_err(|_| durable::Error::Capacity)?;
        bytes
            .strip_prefix(prefix.as_slice())
            .map(<[u8]>::to_vec)
            .ok_or(durable::Error::Binding)
    }

    fn commit(&mut self, expected: &[u8], next: &[u8]) -> Result<(), durable::Error> {
        let expected = envelope(self.namespace, expected).map_err(|_| durable::Error::Capacity)?;
        let next = envelope(self.namespace, next).map_err(|_| durable::Error::Capacity)?;
        self.store.commit(&expected, &next)
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, Refusal, admit};

    #[test]
    fn admission_matches_every_checked_contract_case() {
        // Independent complete decision examples qualified by the source-pinned
        // ZenoFCIS contract judge. No runtime owner facts are supplied by workers.
        let mut count = 0;
        let mut accepts = 0;
        for line in include_str!("owner_admission_cases.txt").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (bits, expected) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("case separator"));
            let facts: [bool; 7] = bits
                .bytes()
                .map(|b| b == b'1')
                .collect::<Vec<_>>()
                .try_into()
                .unwrap_or_else(|_| panic!("seven facts"));
            let result = match admit(facts) {
                Ok(()) => "Accept",
                Err(Error::Refused(Refusal::Owner)) => "200",
                Err(Error::Refused(Refusal::Tenant)) => "201",
                Err(Error::Refused(Refusal::Principal)) => "202",
                Err(Error::Refused(Refusal::Grant)) => "203",
                Err(Error::Refused(Refusal::Intent)) => "204",
                Err(Error::Refused(Refusal::Pending)) => "205",
                Err(Error::Refused(Refusal::Content)) => "206",
                Err(other) => panic!("unexpected error {other:?}"),
            };
            assert_eq!(result, expected, "facts={bits}");
            count += 1;
            accepts += usize::from(result == "Accept");
        }
        assert_eq!((count, accepts), (128, 1));
    }
}
