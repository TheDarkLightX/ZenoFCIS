//! Opt-in commit/recovery and payout delivery through the production handler.
//! Retain the original invocation before committing; after uncertainty pass
//! that same invocation to `recover`. Recording is private in-memory evidence.

use std::{num::NonZeroU64, path::Path};
use zeno_fcis_shell::effects::{
    codec,
    driver::{Driver, Reply},
    handler::{self, Handled, Layer, Stack, Terminal},
    policy::{Monitor, Policy},
    port::{CommitReceipt, Fault, Operation, Outcome, Response, Store},
    program::{
        CommitInvocation, CommitPolicy, CommitProgram, DeliveryInvocation, DeliveryPolicy,
        DeliveryProgram, Program, Step,
    },
    record::{Implementation, Transcript},
    registry::{self, REGISTRY},
};
use zeno_fcis_shell_sqlite::{
    effects::production::{Production, Time, Transport},
    v2::{Error, Lineage},
};

/// Shared retry budget per run; exhaustion remains an explicit outcome.
pub const RETRIES: u8 = 2;
/// Additional per-run request cap, including reads, retries and acknowledgements.
pub const MAX_REQUESTS: usize = 4096;

/// Stateful shell handle, separate from the pure decision API. Its caller
/// supplies transport and time; neither can authorize a publication.
pub struct Effects<'a, 'p, T: Transport, C: Time> {
    production: Production<'a, 'p, T, C>,
    record: bool,
    last_transcript: Option<Transcript>,
    stack: Stack,
}

impl<'a, 'p, T: Transport, C: Time> Effects<'a, 'p, T, C> {
    /// Opens an existing single-version store. Transcript capture starts off.
    pub fn open(
        path: &Path,
        lineage: &'a Lineage<'a, 'p>,
        transport: T,
        time: C,
    ) -> Result<Self, Error> {
        Self::open_with_policies(path, lineage, transport, time, Vec::new())
    }

    /// Opens a policy lifetime before any effect. Reservations survive every
    /// run and terminal unwind on this handle. Policies require private capture
    /// and adapter provenance. Process loss requires durable restoration;
    /// opening a fresh handle is not recovery of a previous reservation log.
    pub fn open_with_policies(
        path: &Path,
        lineage: &'a Lineage<'a, 'p>,
        transport: T,
        time: C,
        policies: Vec<Policy>,
    ) -> Result<Self, Error> {
        let record = !policies.is_empty();
        Ok(Self {
            production: Production::open(path, lineage, 0, transport, Some(time))?,
            record,
            last_transcript: None,
            stack: Stack {
                layers: policies
                    .into_iter()
                    .map(Monitor::new)
                    .map(Layer::Policy)
                    .collect(),
            },
        })
    }

    /// The store handle to put in a retained invocation.
    pub fn store(&self) -> Store {
        self.production.store()
    }

    /// Enables private capture for subsequent runs. Policies require capture;
    /// disabling it on an unmonitored handle discards the previous transcript.
    /// No transcript is written to disk, exported or supplied to Authority.
    pub fn record_private(&mut self, enabled: bool) {
        self.record = enabled
            || self
                .stack
                .layers
                .iter()
                .any(|l| matches!(l, Layer::Policy(_)));
        self.last_transcript = None;
    }

    /// Private evidence of the last run, including a refusal or interruption.
    /// It is replaced by the next run; copy it explicitly to retain it.
    pub fn transcript(&self) -> Option<&Transcript> {
        self.last_transcript.as_ref()
    }

    /// Commits retained original inputs; Production rebuilds through Authority.
    pub fn commit(
        &mut self,
        invocation: &CommitInvocation,
    ) -> Result<Outcome<CommitReceipt>, String> {
        self.commit_run(invocation, false)
    }

    /// Recovers the original invocation, including its key, guard and inputs.
    /// Never prepare a new decision against the latest state for this retry.
    pub fn recover(
        &mut self,
        invocation: &CommitInvocation,
    ) -> Result<Outcome<CommitReceipt>, String> {
        self.commit_run(invocation, true)
    }

    fn commit_run(
        &mut self,
        retained: &CommitInvocation,
        recover: bool,
    ) -> Result<Outcome<CommitReceipt>, String> {
        let invocation = CommitInvocation {
            epoch: retained.epoch,
            store: retained.store,
            key: retained.key,
            publication: retained.publication.clone(),
            mode: retained.mode,
        };
        let epoch = invocation.epoch;
        let mut start = b"withdrawal-queue/commit/1".to_vec();
        start.extend_from_slice(&epoch.to_be_bytes());
        start.extend_from_slice(&[u8::from(recover), RETRIES]);
        start.extend_from_slice(&codec::operation(&Operation::Commit(
            invocation.store,
            invocation.key,
            invocation.publication.clone(),
            invocation.mode,
        )));
        let policy = CommitPolicy { retries: RETRIES };
        let first = if recover {
            CommitProgram::recover(invocation, policy)
        } else {
            CommitProgram::start(invocation, policy)
        };
        self.run::<CommitProgram>(epoch, start, first, commit_outcome)
    }

    /// Sends exact pending payouts and acknowledges only matching reports.
    /// The supplied clock's tick unit determines the ten-tick backoff.
    pub fn deliver(&mut self, epoch: u64) -> Result<Outcome<u64>, String> {
        let invocation = DeliveryInvocation {
            epoch,
            store: self.store(),
        };
        let policy = DeliveryPolicy {
            retries: RETRIES,
            backoff: 10,
            jitter: NonZeroU64::MIN,
        };
        let mut start = b"withdrawal-queue/delivery/1".to_vec();
        start.extend_from_slice(&epoch.to_be_bytes());
        start.push(RETRIES);
        start.extend_from_slice(&policy.backoff.to_be_bytes());
        start.extend_from_slice(&policy.jitter.get().to_be_bytes());
        start.extend_from_slice(&codec::operation(&Operation::ReadOutbox(invocation.store)));
        self.run::<DeliveryProgram>(
            epoch,
            start,
            DeliveryProgram::start(invocation, policy),
            |outcome| {
                codec::outcome(
                    outcome,
                    match outcome {
                        Outcome::Ok(n) => Some(*n),
                        _ => None,
                    },
                )
            },
        )
    }

    fn run<P: Program>(
        &mut self,
        epoch: u64,
        start: Vec<u8>,
        first: Step<P::Continuation, P::Output>,
        encode: fn(&Outcome<P::Output>) -> Vec<u8>,
    ) -> Result<Outcome<P::Output>, String>
    where
        P::Output: Clone,
    {
        self.last_transcript = None;
        handler::bind(self.production.ports(), P::PORTS).map_err(|e| format!("{e:?}"))?;
        let mut driver =
            Driver::<P>::start(REGISTRY, epoch, first).map_err(|e| format!("{e:?}"))?;
        // The field owns reservations before terminal invocation, including
        // when a transport unwinds. Remove only the previous run's recording.
        self.stack
            .layers
            .retain(|layer| !matches!(layer, Layer::Record(_)));
        if self.record {
            let mut manifest = self
                .production
                .manifest(start)
                .ok_or("private recording requires transport and time source/configuration/seed")?;
            let policy_count = self
                .stack
                .layers
                .iter()
                .filter(|l| matches!(l, Layer::Policy(_)))
                .count();
            for _ in 0..policy_count {
                manifest.stack.insert(0, "policy");
            }
            manifest.implementations.push(Implementation {
                name: "withdrawal-queue",
                source: registry::source_digest(&[(
                    "shell_effects.rs",
                    include_bytes!("shell_effects.rs"),
                )]),
            });
            manifest.configuration.push(RETRIES);
            manifest
                .configuration
                .extend_from_slice(&(MAX_REQUESTS as u64).to_be_bytes());
            self.stack
                .layers
                .push(Layer::Record(Transcript::new(manifest)));
        }
        let stack = &mut self.stack;
        let result = (|| {
            for _ in 0..MAX_REQUESTS {
                if let Some(outcome) = driver.outcome() {
                    stack.done(&encode(outcome));
                    return Ok(outcome.clone());
                }
                if let Some(error) = driver.failure() {
                    return Err(format!("driver: {error:?}"));
                }
                let (binding, operation) = driver.request().ok_or("driver has no request")?;
                let binding = binding.clone();
                match stack.handle(&mut self.production, &binding, operation) {
                    Handled::Answer(response) => driver
                        .reply(Reply { binding, response })
                        .map_err(|e| format!("{e:?}"))?,
                    Handled::Crash => {
                        return Err(
                            "interrupted: retain and recover the original invocation".into()
                        );
                    }
                    Handled::Suspend => return Err("production handler suspended".into()),
                }
            }
            // The last effect may have applied. Do not invent a completed
            // transcript or report non-execution when this bound stops a run.
            Ok(Outcome::Unknown(Fault::Io))
        })();
        self.last_transcript = stack.layers.iter().find_map(|layer| match layer {
            Layer::Record(transcript) => Some(transcript.clone()),
            Layer::Identity | Layer::Policy(_) => None,
        });
        stack
            .layers
            .retain(|layer| !matches!(layer, Layer::Record(_)));
        result
    }
}

fn commit_outcome(outcome: &Outcome<CommitReceipt>) -> Vec<u8> {
    codec::response(&Response::Commit(outcome.clone()))
}
