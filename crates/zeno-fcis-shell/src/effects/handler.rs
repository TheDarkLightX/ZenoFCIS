//! Handlers and stacks: `H_outer(H_inner(P))` with one terminal.
//!
//! A request passes through the stack's layers, innermost first, to the
//! terminal handler that carries it out; the answer passes back outward.
//! The identity forwarder passes requests unchanged, policies may veto them,
//! and the record layer appends them to its transcript. Before a
//! run, [`bind`] refuses a stack whose terminal does not cover every port the
//! program can request. Only the driver resumes a program.

use alloc::vec::Vec;

use super::driver::Binding;
use super::policy::{self, Monitor, Verdict};
use super::port::{Fault as FaultKind, Operation, Response, Sent};
use super::record::{Entry, Transcript};
use super::registry::Port;
use super::simulation::Script;

/// What a handler did with one request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Handled {
    /// It answered.
    Answer(Response),
    /// The process crashed: no answer, and the continuation is destroyed.
    Crash,
    /// A simulated fault point needs a choice the schedule does not make yet.
    Suspend,
}

/// A handler that carries requests out.
pub trait Terminal {
    /// The ports it covers.
    fn ports(&self) -> &'static [Port];
    /// Carries out one request.
    fn handle(&mut self, binding: &Binding, operation: &Operation) -> Handled;
    /// The process crashed around the request with this binding: drop its
    /// handle's volatile state. Durable state stays as the handler left it.
    fn crash(&mut self, binding: &Binding) {
        let _ = binding;
    }
}

/// The fault handler, `Fault(H)`: in front of a terminal, it may fail a
/// request before forwarding it or drop the answer after, following its
/// schedule's choices. It forwards a request at most once, except a Send it
/// duplicates in transit; it never repeats a Commit or an Acknowledge.
pub struct Fault<'a, T: Terminal + ?Sized> {
    inner: &'a mut T,
    script: &'a mut Script,
    /// The terminal's own answer to the last forwarded request.
    pub forwarded: Option<Handled>,
}

impl<'a, T: Terminal + ?Sized> Fault<'a, T> {
    /// Faults in front of `inner`, choosing from `script`.
    pub fn new(inner: &'a mut T, script: &'a mut Script) -> Self {
        Self {
            inner,
            script,
            forwarded: None,
        }
    }

    fn forward(&mut self, binding: &Binding, operation: &Operation) -> Handled {
        let handled = self.inner.handle(binding, operation);
        self.forwarded = Some(handled.clone());
        handled
    }

    fn crash(&mut self, binding: &Binding) -> Handled {
        self.inner.crash(binding);
        Handled::Crash
    }
}

impl<T: Terminal + ?Sized> Terminal for Fault<'_, T> {
    fn ports(&self) -> &'static [Port] {
        self.inner.ports()
    }

    fn handle(&mut self, binding: &Binding, operation: &Operation) -> Handled {
        self.forwarded = None;
        let port = operation.port();
        let options = match port {
            Port::Commit | Port::Acknowledge => 8,
            Port::Send => 7,
            Port::ReadState | Port::ReadHistory | Port::ReadOutbox | Port::Reopen => 5,
            Port::Clock | Port::Random => 4,
            Port::Wait => 5,
        };
        let Some(choice) = self.script.choose(options) else {
            return Handled::Suspend;
        };
        if matches!(port, Port::Clock | Port::Random | Port::Wait) {
            return if choice == 0 {
                self.forward(binding, operation)
            } else if port == Port::Wait && choice == 1 {
                Handled::Answer(Response::Wait(Err(super::port::WaitError::Interrupted)))
            } else {
                let offset = if port == Port::Wait {
                    choice - 2
                } else {
                    choice - 1
                };
                Handled::Answer(Response::not_applied(
                    port,
                    super::simulation::fault_choice(offset),
                ))
            };
        }
        if matches!(
            port,
            Port::ReadState | Port::ReadHistory | Port::ReadOutbox | Port::Reopen
        ) {
            return match choice {
                0 => self.forward(binding, operation),
                1 => Handled::Answer(Response::not_applied(port, FaultKind::Busy)),
                2 => self.crash(binding),
                3 => Handled::Answer(Response::not_applied(port, FaultKind::Full)),
                _ => Handled::Answer(Response::not_applied(port, FaultKind::Io)),
            };
        }
        // The order follows the simulation's: answer, busy or not sent, an
        // ambiguous answer without forwarding, a lost answer after it, a
        // crash before or after it, and for Send a duplicate in transit.
        match choice {
            0 => self.forward(binding, operation),
            1 => Handled::Answer(Response::not_applied(port, FaultKind::Busy)),
            2 => Handled::Answer(Response::unknown(port)),
            3 => match self.forward(binding, operation) {
                Handled::Answer(_) => Handled::Answer(Response::unknown(port)),
                other => other,
            },
            4 => self.crash(binding),
            5 => match self.forward(binding, operation) {
                Handled::Answer(_) => self.crash(binding),
                other => other,
            },
            6 if port == Port::Send => {
                let _ = self.forward(binding, operation);
                self.forward(binding, operation)
            }
            6 => Handled::Answer(Response::not_applied(port, FaultKind::Full)),
            _ => Handled::Answer(Response::not_applied(port, FaultKind::Io)),
        }
    }
}

/// One forwarding layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer {
    /// Passes requests and answers on unchanged.
    Identity,
    /// Also appends every request, answer, crash and outcome to a transcript.
    Record(Transcript),
    /// Pure veto monitor. Its state survives retries and simulated restarts.
    Policy(Monitor),
}

impl Layer {
    /// The layer's name, as a manifest binds it.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Record(_) => "record",
            Self::Policy(_) => "policy",
        }
    }
}

/// Why a stack was refused before running.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Unbound {
    /// The program can request a port the terminal does not cover.
    Uncovered(Port),
}

/// Refuses a terminal that does not cover every port a program can request.
pub fn bind(covered: &[Port], requested: &[Port]) -> Result<(), Unbound> {
    for port in requested {
        if !covered.contains(port) {
            return Err(Unbound::Uncovered(*port));
        }
    }
    Ok(())
}

/// Layers, innermost first, in front of a terminal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Stack {
    /// The layers, innermost first.
    pub layers: Vec<Layer>,
}

impl Stack {
    /// Passes one request inward-out to the terminal and its answer back.
    pub fn handle(
        &mut self,
        terminal: &mut dyn Terminal,
        binding: &Binding,
        operation: &Operation,
    ) -> Handled {
        // Tentative reservations are private invocation scratch. A rejection
        // by any layer or a simulation Suspend discards all of them. There is
        // one terminal call, with the caller's exact immutable operation.
        let mut next = Vec::new();
        let mut rejection = None;
        for (index, layer) in self.layers.iter().enumerate() {
            if let Layer::Policy(monitor) = layer {
                let (state, verdict) =
                    policy::step(&monitor.policy, &monitor.state, operation, None);
                match verdict {
                    Verdict::Allow => next.push((index, state)),
                    Verdict::Reject(code) => {
                        rejection = Some(code);
                        break;
                    }
                }
            }
        }
        let mut previous = Vec::new();
        if rejection.is_none() {
            for (index, state) in next {
                if let Layer::Policy(monitor) = &mut self.layers[index] {
                    previous.push((index, core::mem::replace(&mut monitor.state, state)));
                }
            }
        }
        let handled = match rejection {
            Some(code) => Handled::Answer(Response::Send(Sent::Refused(code))),
            None => terminal.handle(binding, operation),
        };
        // Reserve before the terminal call, including if it unwinds. Only a
        // model suspension certifies that no invocation took place.
        if handled == Handled::Suspend {
            for (index, state) in previous {
                if let Layer::Policy(monitor) = &mut self.layers[index] {
                    monitor.state = state;
                }
            }
        }
        let entry = match &handled {
            Handled::Answer(response) => Some(Entry::Response(binding.clone(), response.clone())),
            Handled::Crash => Some(Entry::Crash(binding.clone())),
            Handled::Suspend => None,
        };
        if let Some(entry) = entry {
            for layer in self.layers.iter_mut().rev() {
                if let Layer::Record(transcript) = layer {
                    transcript.append(Entry::Request(binding.clone(), operation.clone()));
                    transcript.append(entry.clone());
                }
            }
        }
        handled
    }

    /// Records the program's outcome.
    pub fn done(&mut self, outcome: &[u8]) {
        for layer in self.layers.iter_mut().rev() {
            if let Layer::Record(transcript) = layer {
                transcript.append(Entry::Done(outcome.to_vec()));
            }
        }
    }

    /// The transcripts of the record layers, innermost first.
    #[must_use]
    pub fn transcripts(&self) -> Vec<&Transcript> {
        let mut transcripts = Vec::new();
        for layer in &self.layers {
            if let Layer::Record(transcript) = layer {
                transcripts.push(transcript);
            }
        }
        transcripts
    }

    /// Owned policy layers for a restarted program under the same relay.
    #[must_use]
    pub fn policies(&self) -> Vec<Layer> {
        self.layers
            .iter()
            .filter_map(|layer| match layer {
                Layer::Policy(_) => Some(layer.clone()),
                _ => None,
            })
            .collect()
    }

    /// Canonical identities and states in stack order. Transcripts are absent.
    #[must_use]
    pub fn policy_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for layer in &self.layers {
            if let Layer::Policy(monitor) = layer {
                super::codec::hash(&mut bytes, monitor.identity());
                super::codec::bytes(&mut bytes, &monitor.state.bytes());
            }
        }
        bytes
    }
}
