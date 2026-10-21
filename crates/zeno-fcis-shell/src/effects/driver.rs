//! The trusted driver: one outstanding request and its response binding.
//!
//! Before a program runs, the driver checks the registry and that the
//! program declares the capability of every port it can request. It checks
//! each request against its port's argument schema, and admits a response
//! only when it is bound to the outstanding request: same epoch, ordinal,
//! port, store, handle and subject, with a result that fits its schema. A
//! refused response leaves the program where it was.

use alloc::vec::Vec;
use core::{fmt, mem};

use super::port::{OpId, Operation, Outcome, Store};
use super::program::{Program, Step};
use super::registry::{self, Port, Registry, RegistryError, TypeError};

/// What binds a response to the request that asked for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The request's epoch and ordinal.
    pub id: OpId,
    /// The requested port.
    pub port: Port,
    /// The addressed store handle, if any.
    pub store: Option<Store>,
    /// The publication subject or delivery ID, if any.
    pub subject: Vec<u8>,
}

impl Binding {
    /// The binding of one request.
    #[must_use]
    pub fn of(id: OpId, operation: &Operation) -> Self {
        Self {
            id,
            port: operation.port(),
            store: operation.store(),
            subject: operation.subject().to_vec(),
        }
    }
}

/// A handler's response with the binding of the request it answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reply {
    /// Copied from the request.
    pub binding: Binding,
    /// The handler's answer.
    pub response: super::port::Response,
}

/// Why a response was not admitted. The program does not advance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// No request is outstanding.
    Idle,
    /// The response repeats the answer to the request already answered.
    Duplicate,
    /// The response answers an earlier request.
    Stale,
    /// The response belongs to another process epoch.
    Epoch,
    /// The response names an ordinal the driver has not issued.
    Ordinal,
    /// The response answers another port, or has another port's kind.
    Tag,
    /// The response comes from another store.
    Store,
    /// The response comes from another handle on the same store.
    Handle,
    /// The response is about another publication or delivery.
    Subject,
    /// The result does not fit its schema.
    Type(TypeError),
}

/// Why the driver stopped a program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fatal {
    /// The registry fails its check.
    Registry(RegistryError),
    /// The program lists a port twice, lists one without declaring its
    /// capability, or requests one it does not list.
    Capability(Port),
    /// The program's request is not the next one of the epoch.
    Sequence,
    /// The program's request does not fit its schema.
    Type(Port, TypeError),
}

// One driver owns this value; boxing would add an allocation per request.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum State<K, T> {
    Waiting {
        binding: Binding,
        operation: Operation,
        continuation: K,
    },
    Done(Outcome<T>),
    Failed(Fatal),
}

/// Runs one program; a handler answers its requests one at a time.
pub struct Driver<P: Program> {
    registry: Registry,
    epoch: u64,
    answered: Option<OpId>,
    state: State<P::Continuation, P::Output>,
}

/// The epoch, the last answered request and the program's saved state;
/// the registry is a shared constant and is left out.
impl<P: Program> fmt::Debug for Driver<P>
where
    P::Continuation: fmt::Debug,
    P::Output: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Driver")
            .field("epoch", &self.epoch)
            .field("answered", &self.answered)
            .field("state", &self.state)
            .finish()
    }
}

impl<P: Program> Driver<P> {
    /// Checks the registry and the program's capabilities, then admits its
    /// first step, which must use ordinal zero of `epoch`.
    pub fn start(
        registry: Registry,
        epoch: u64,
        first: Step<P::Continuation, P::Output>,
    ) -> Result<Self, Fatal> {
        registry::check(&registry).map_err(Fatal::Registry)?;
        for (index, port) in P::PORTS.iter().enumerate() {
            let declared = match registry::signature(&registry, *port) {
                Some(signature) => P::CAPABILITIES.contains(&signature.capability),
                None => false,
            };
            if !declared || P::PORTS[..index].contains(port) {
                return Err(Fatal::Capability(*port));
            }
        }
        let state = admit::<P>(&registry, OpId::first(epoch), first)?;
        Ok(Self {
            registry,
            epoch,
            answered: None,
            state,
        })
    }

    /// The outstanding request, until the run ends.
    #[must_use]
    pub fn request(&self) -> Option<(&Binding, &Operation)> {
        match &self.state {
            State::Waiting {
                binding, operation, ..
            } => Some((binding, operation)),
            State::Done(_) | State::Failed(_) => None,
        }
    }

    /// The run's outcome, once it has ended.
    #[must_use]
    pub fn outcome(&self) -> Option<&Outcome<P::Output>> {
        match &self.state {
            State::Done(outcome) => Some(outcome),
            State::Waiting { .. } | State::Failed(_) => None,
        }
    }

    /// Why the driver stopped the program, if it did.
    #[must_use]
    pub fn failure(&self) -> Option<Fatal> {
        match &self.state {
            State::Failed(fatal) => Some(*fatal),
            State::Waiting { .. } | State::Done(_) => None,
        }
    }

    /// Admits a response bound to the outstanding request and resumes the
    /// program once. A refused response changes nothing.
    pub fn reply(&mut self, reply: Reply) -> Result<(), Refusal> {
        let got = &reply.binding;
        let State::Waiting { binding, .. } = &self.state else {
            return Err(if Some(got.id) == self.answered {
                Refusal::Duplicate
            } else {
                Refusal::Idle
            });
        };
        if got.id.epoch != self.epoch || got.id.epoch != binding.id.epoch {
            return Err(Refusal::Epoch);
        }
        if Some(got.id) == self.answered {
            return Err(Refusal::Duplicate);
        }
        if got.id.ordinal < binding.id.ordinal {
            return Err(Refusal::Stale);
        }
        if got.id.ordinal != binding.id.ordinal {
            return Err(Refusal::Ordinal);
        }
        if got.port != binding.port || reply.response.port() != binding.port {
            return Err(Refusal::Tag);
        }
        match (got.store, binding.store) {
            (None, None) => {}
            (Some(got), Some(expected)) if got.identity != expected.identity => {
                return Err(Refusal::Store);
            }
            (Some(got), Some(expected)) if got.handle != expected.handle => {
                return Err(Refusal::Handle);
            }
            (Some(_), Some(_)) => {}
            _ => return Err(Refusal::Store),
        }
        if got.subject != binding.subject {
            return Err(Refusal::Subject);
        }
        if let Some(value) = reply.response.value() {
            let Some(signature) = registry::signature(&self.registry, binding.port) else {
                return Err(Refusal::Tag);
            };
            registry::conforms(signature.answer.payload(), &value, "result")
                .map_err(Refusal::Type)?;
        }
        let id = binding.id;
        let State::Waiting { continuation, .. } =
            mem::replace(&mut self.state, State::Failed(Fatal::Sequence))
        else {
            return Err(Refusal::Idle);
        };
        self.answered = Some(id);
        let step = P::resume(continuation, reply.response);
        self.state = match id.next() {
            Some(next) => match admit::<P>(&self.registry, next, step) {
                Ok(state) => state,
                Err(fatal) => State::Failed(fatal),
            },
            None => match step {
                Step::Done(outcome) => State::Done(outcome),
                Step::Request(..) => State::Failed(Fatal::Sequence),
            },
        };
        Ok(())
    }
}

fn admit<P: Program>(
    registry: &Registry,
    expected: OpId,
    step: Step<P::Continuation, P::Output>,
) -> Result<State<P::Continuation, P::Output>, Fatal> {
    match step {
        Step::Done(outcome) => Ok(State::Done(outcome)),
        Step::Request(id, operation, continuation) => {
            if id != expected {
                return Err(Fatal::Sequence);
            }
            let port = operation.port();
            let Some(signature) = registry::signature(registry, port) else {
                return Err(Fatal::Capability(port));
            };
            if !P::PORTS.contains(&port) {
                return Err(Fatal::Capability(port));
            }
            registry::conforms(signature.arguments, &operation.value(), "arguments")
                .map_err(|error| Fatal::Type(port, error))?;
            Ok(State::Waiting {
                binding: Binding::of(id, &operation),
                operation,
                continuation,
            })
        }
    }
}
