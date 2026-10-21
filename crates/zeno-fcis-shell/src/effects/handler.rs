//! Handlers and stacks: `H_outer(H_inner(P))` with one terminal.
//!
//! A request passes through the stack's layers, innermost first, to the
//! terminal handler that carries it out; the answer passes back outward.
//! Layers here intercept nothing: the identity forwarder passes requests on
//! unchanged and the record layer appends them to its transcript. Before a
//! run, [`bind`] refuses a stack whose terminal does not cover every port the
//! program can request. Only the driver resumes a program.

use alloc::vec::Vec;

use super::driver::Binding;
use super::port::{Operation, Response};
use super::record::{Entry, Transcript};
use super::registry::Port;

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
}

/// One forwarding layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layer {
    /// Passes requests and answers on unchanged.
    Identity,
    /// Also appends every request, answer, crash and outcome to a transcript.
    Record(Transcript),
}

impl Layer {
    /// The layer's name, as a manifest binds it.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Record(_) => "record",
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
        let handled = terminal.handle(binding, operation);
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
}
