//! The attempt state machine (NSL-001, NSL-002, NSL-004, NSL-006, NSL-007,
//! NSL-008, NSR-005, NSR-007, NSR-008, NSR-009).
//!
//! One attempt is in flight at a time. The shell reserves it (and any model
//! call) through the session, persists the new ledger entries, lets the
//! proposer work, hands the proposal back for admission, runs the resulting
//! [`CheckJob`] under its own supervision, and finally delivers the
//! [`CheckReport`]. Every exit of every stage settles the attempt without
//! refunding anything; only a completed `transform::check` equivalence that
//! passes the selection rule changes the incumbent. A report for a stage that
//! is no longer pending is late and changes nothing.

use super::feedback::{DuplicateOf, Feedback, Witness, WitnessFault, replay_witness};
use super::incumbent::{Cost, Incumbent, Replacement, Selection, select};
use super::ledger::{Head, Kind, Ledger, LedgerFault, Stage};
use super::profiles::ProfileRefusal;
use super::request::{ReadmitRefusal, Request};
use super::strategy::{SearchReport, Strategy, StrategyRefusal};
use crate::transform::{self, Equivalence, Inconclusive, Rejection, sha256_hex};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::Error;
use zeno_fcis_synthesis::finite_runtime::import_program;

pub(crate) const REPORT_SCHEMA: &str = "zeno-fcis/transform-loop-report/1";

/// Why a session stopped (NSL-008: the actual stop reason, always reported).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StopReason {
    AttemptsExhausted,
    ChecksExhausted,
    Deadline,
    ModelCallsExhausted,
    TokensExhausted,
    /// The proposer has nothing further to propose.
    ProposerExhausted,
    /// Enforcement or a required mode is unavailable.
    Unavailable(String),
    /// The operator closed the session.
    OperatorClosed,
}

impl StopReason {
    pub(crate) fn name(&self) -> String {
        match self {
            StopReason::AttemptsExhausted => "attempts-exhausted".to_owned(),
            StopReason::ChecksExhausted => "checks-exhausted".to_owned(),
            StopReason::Deadline => "deadline".to_owned(),
            StopReason::ModelCallsExhausted => "model-calls-exhausted".to_owned(),
            StopReason::TokensExhausted => "tokens-exhausted".to_owned(),
            StopReason::ProposerExhausted => "proposer-exhausted".to_owned(),
            StopReason::Unavailable(reason) => format!("unavailable:{reason}"),
            StopReason::OperatorClosed => "operator-closed".to_owned(),
        }
    }

    fn parse(name: &str) -> StopReason {
        match name {
            "attempts-exhausted" => StopReason::AttemptsExhausted,
            "checks-exhausted" => StopReason::ChecksExhausted,
            "deadline" => StopReason::Deadline,
            "model-calls-exhausted" => StopReason::ModelCallsExhausted,
            "tokens-exhausted" => StopReason::TokensExhausted,
            "proposer-exhausted" => StopReason::ProposerExhausted,
            "operator-closed" => StopReason::OperatorClosed,
            other => StopReason::Unavailable(
                other
                    .strip_prefix("unavailable:")
                    .unwrap_or(other)
                    .to_owned(),
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    Open,
    Closed(StopReason),
}

/// A reserved attempt. Only [`Session::reserve_attempt`] mints one.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Ticket {
    attempt: u8,
}

impl Ticket {
    pub(crate) fn attempt(&self) -> u8 {
        self.attempt
    }
}

/// A reserved model call for the attempt it names.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ModelCallTicket {
    attempt: u8,
}

impl ModelCallTicket {
    pub(crate) fn attempt(&self) -> u8 {
        self.attempt
    }
}

/// How a proposer failed to propose.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProposerFailure {
    /// The supervisor closed the stage first.
    Timeout,
    /// Nothing was proposed.
    Empty,
    /// The response could not be decoded as a proposal.
    Malformed(String),
    /// The provider or mode is unavailable.
    Unavailable(String),
    /// A deterministic proposer has no further candidates.
    Exhausted,
}

impl ProposerFailure {
    pub(crate) fn name(&self) -> String {
        match self {
            ProposerFailure::Timeout => "timeout".to_owned(),
            ProposerFailure::Empty => "empty".to_owned(),
            ProposerFailure::Malformed(reason) => format!("malformed:{reason}"),
            ProposerFailure::Unavailable(reason) => format!("unavailable:{reason}"),
            ProposerFailure::Exhausted => "exhausted".to_owned(),
        }
    }
}

/// What a proposer produced (NSL-003). Proposals are data; a strategy is raw
/// JSON until the grammar admits it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Proposal {
    Candidate(Vec<u8>),
    Strategy(Value),
    Failed(ProposerFailure),
}

/// Why a candidate was not admitted for checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AdmissionRefusal {
    TooLarge {
        bytes: usize,
        max: u32,
    },
    NotAdmitted {
        code: String,
    },
    Profile(ProfileRefusal),
    InputAbi,
    OutputAbi,
    Strategy(StrategyRefusal),
    /// F3 refused a pair this session had already admitted: an inconsistency,
    /// reported as the checker saw it.
    Checker(Value),
}

impl AdmissionRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            AdmissionRefusal::TooLarge { bytes, max } => {
                json!({"reason": "program-too-large", "bytes": bytes, "max_bytes": max})
            }
            AdmissionRefusal::NotAdmitted { code } => {
                json!({"reason": "candidate-not-admitted", "code": code})
            }
            AdmissionRefusal::Profile(refusal) => refusal.json(),
            AdmissionRefusal::InputAbi => json!({"reason": "input-abi-mismatch"}),
            AdmissionRefusal::OutputAbi => json!({"reason": "output-abi-mismatch"}),
            AdmissionRefusal::Strategy(refusal) => refusal.json(),
            AdmissionRefusal::Checker(detail) => {
                json!({"reason": "checker-refused", "detail": detail})
            }
        }
    }
}

/// How a supervised worker failed to deliver a completed check (NSL-007,
/// NSF-003: outer inconclusive, accounting marked incomplete).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum WorkerFailure {
    Timeout,
    Panicked,
    Died,
    Unavailable(String),
}

impl WorkerFailure {
    pub(crate) fn name(&self) -> String {
        match self {
            WorkerFailure::Timeout => "timeout".to_owned(),
            WorkerFailure::Panicked => "panicked".to_owned(),
            WorkerFailure::Died => "died".to_owned(),
            WorkerFailure::Unavailable(reason) => format!("unavailable:{reason}"),
        }
    }
}

/// A completed check. Fields are private: only [`CheckJob::run`] builds one,
/// from `transform::check` on the job's exact bytes.
#[derive(Debug)]
pub(crate) struct Completed {
    attempt: u8,
    candidate_sha256: String,
    result: Result<Equivalence, Rejection>,
}

/// What the shell delivers for a check stage.
#[derive(Debug)]
pub(crate) enum CheckReport {
    Completed(Completed),
    Failed(WorkerFailure),
}

/// An admitted candidate with its precharged work, ready for the checker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckJob {
    attempt: u8,
    original: Vec<u8>,
    candidate: Vec<u8>,
    candidate_sha256: String,
    cost: Cost,
    work: u64,
    limits: transform::Limits,
}

impl CheckJob {
    /// Runs F3's checker on exactly the admitted pair. This is the only
    /// constructor of a completed report (NSL-004).
    pub(crate) fn run(&self) -> CheckReport {
        CheckReport::Completed(Completed {
            attempt: self.attempt,
            candidate_sha256: self.candidate_sha256.clone(),
            result: transform::check(&self.original, &self.candidate, self.limits),
        })
    }

    pub(crate) fn attempt(&self) -> u8 {
        self.attempt
    }

    pub(crate) fn candidate(&self) -> &[u8] {
        &self.candidate
    }

    pub(crate) fn candidate_sha256(&self) -> &str {
        &self.candidate_sha256
    }

    pub(crate) fn cost(&self) -> Cost {
        self.cost
    }

    /// The deterministic work reserved for this check.
    pub(crate) fn work(&self) -> u64 {
        self.work
    }
}

/// A validated strategy for the shell's search worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SearchJob {
    attempt: u8,
    strategy: Strategy,
}

impl SearchJob {
    pub(crate) fn strategy(&self) -> &Strategy {
        &self.strategy
    }
}

/// The settled result of one attempt. No variant but `Equivalent` with
/// `CheckedImprovement` ever changes the incumbent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Failed(ProposerFailure),
    Refused(AdmissionRefusal),
    Duplicate(DuplicateOf),
    StrategyUnavailable(String),
    SearchFailed(String),
    /// The check's worst-case work cannot be reserved (NSF-003, NSR-005).
    WorkExhausted {
        needed: Option<u64>,
        check_limit: u64,
        session_remaining: u64,
    },
    ChecksExhausted,
    Different(Witness),
    /// F3 found a difference the witness replay did not reproduce: an
    /// inconsistency, so no counterexample is labeled factual.
    WitnessNotReproduced(WitnessFault),
    Inconclusive(Value),
    Equivalent {
        cost: Cost,
        selection: Selection,
    },
    CheckFailed(WorkerFailure),
    /// A report for a stage that is not pending: ignored, nothing changed.
    Late,
    /// A call for an attempt that is not pending: ignored, nothing changed.
    NotPending,
}

impl Outcome {
    /// Closed vocabulary for the ledger and reports.
    pub(crate) fn name(&self) -> String {
        match self {
            Outcome::Failed(failure) => format!("proposal-failed:{}", failure.name()),
            Outcome::Refused(_) => "refused".to_owned(),
            Outcome::Duplicate(DuplicateOf::Original) => "duplicate-of-original".to_owned(),
            Outcome::Duplicate(DuplicateOf::Attempt(attempt)) => {
                format!("duplicate-of-attempt:{attempt}")
            }
            Outcome::StrategyUnavailable(_) => "strategy-unavailable".to_owned(),
            Outcome::SearchFailed(_) => "search-failed".to_owned(),
            Outcome::WorkExhausted { .. } => "work-exhausted".to_owned(),
            Outcome::ChecksExhausted => "checks-exhausted".to_owned(),
            Outcome::Different(_) => "different".to_owned(),
            Outcome::WitnessNotReproduced(fault) => format!("inconclusive:{}", fault.name()),
            Outcome::Inconclusive(_) => "inconclusive".to_owned(),
            Outcome::Equivalent {
                selection: Selection::CheckedImprovement,
                ..
            } => "checked-improvement".to_owned(),
            Outcome::Equivalent { selection, .. } => format!(
                "equivalent-without-improvement:{}",
                selection.json()["reason"].as_str().unwrap_or("unknown")
            ),
            Outcome::CheckFailed(failure) => format!("check-failed:{}", failure.name()),
            Outcome::Late => "late".to_owned(),
            Outcome::NotPending => "not-pending".to_owned(),
        }
    }

    /// Whether this outcome changed any state at all.
    pub(crate) fn settled(&self) -> bool {
        !matches!(self, Outcome::Late | Outcome::NotPending)
    }

    pub(crate) fn json(&self, request: &Request) -> Value {
        let inputs = request.program().inputs();
        let outputs = request.program().outputs();
        match self {
            Outcome::Failed(failure) => json!({"outcome": self.name(), "failure": failure.name()}),
            Outcome::Refused(refusal) => json!({"outcome": "refused", "refusal": refusal.json()}),
            Outcome::Duplicate(_) => json!({"outcome": self.name()}),
            Outcome::StrategyUnavailable(reason) | Outcome::SearchFailed(reason) => {
                json!({"outcome": self.name(), "reason": reason})
            }
            Outcome::WorkExhausted {
                needed,
                check_limit,
                session_remaining,
            } => json!({
                "outcome": "work-exhausted", "needed": needed,
                "check_work": check_limit, "session_work_remaining": session_remaining
            }),
            Outcome::ChecksExhausted => json!({"outcome": "checks-exhausted"}),
            Outcome::Different(witness) => {
                json!({"outcome": "different", "witness": witness.json(inputs, outputs)})
            }
            Outcome::WitnessNotReproduced(fault) => {
                json!({"outcome": self.name(), "fault": fault.name()})
            }
            Outcome::Inconclusive(stop) => json!({"outcome": "inconclusive", "stop": stop}),
            Outcome::Equivalent { cost, selection } => json!({
                "outcome": self.name(), "cost": cost.json(), "selection": selection.json()
            }),
            Outcome::CheckFailed(failure) => {
                json!({"outcome": self.name(), "failure": failure.name()})
            }
            Outcome::Late | Outcome::NotPending => json!({"outcome": self.name()}),
        }
    }
}

/// What the loop asks the shell to do next for an attempt.
#[derive(Debug)]
pub(crate) enum Prepared {
    Check(CheckJob),
    Search(SearchJob),
    Settled(Outcome),
}

/// A settled attempt, kept for feedback and the report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AttemptRecord {
    pub(crate) attempt: u8,
    pub(crate) candidate_sha256: Option<String>,
    pub(crate) cost: Option<Cost>,
    pub(crate) outcome: String,
    pub(crate) feedback: Feedback,
    /// Search status retained as provenance (NSM-005).
    pub(crate) extraction: Option<String>,
}

#[derive(Debug)]
enum PendingStage {
    Proposing,
    Searching,
    Checking { candidate_sha256: String },
}

#[derive(Debug)]
struct Pending {
    attempt: u8,
    stage: PendingStage,
    extraction: Option<String>,
}

/// The session (DESIGN: `S = (R, incumbent, witnesses, bounded_history,
/// ledger, next_attempt, status)`).
#[derive(Debug)]
pub(crate) struct Session {
    request: Request,
    incumbent: Incumbent,
    ledger: Ledger,
    attempts: Vec<AttemptRecord>,
    checked: Vec<(String, u8)>,
    pending: Option<Pending>,
    next_attempt: u8,
    status: Status,
    /// Stored witnesses that did not replay on resume, by attempt.
    dropped_witnesses: Vec<u8>,
}

impl Session {
    /// Opens a session: the incumbent is the admitted original, with no
    /// receipt (NSL-001).
    pub(crate) fn open(request: Request) -> Session {
        let mut ledger = Ledger::new();
        ledger.append(Kind::Opened {
            request_id: request.id().to_owned(),
            checker: request.checker().clone(),
        });
        Session {
            request,
            incumbent: Incumbent::OriginalAdmitted,
            ledger,
            attempts: Vec::new(),
            checked: Vec::new(),
            pending: None,
            next_attempt: 0,
            status: Status::Open,
            dropped_witnesses: Vec::new(),
        }
    }

    pub(crate) fn request(&self) -> &Request {
        &self.request
    }

    pub(crate) fn incumbent(&self) -> &Incumbent {
        &self.incumbent
    }

    pub(crate) fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub(crate) fn status(&self) -> &Status {
        &self.status
    }

    pub(crate) fn attempts(&self) -> &[AttemptRecord] {
        &self.attempts
    }

    /// Checker-derived feedback of every settled attempt, in order; bounded by
    /// the attempt limit (NSF-005).
    pub(crate) fn feedback(&self) -> Vec<Feedback> {
        self.attempts
            .iter()
            .map(|record| record.feedback.clone())
            .collect()
    }

    /// Reserves the next attempt before any proposal work (NSL-002). Closes
    /// the session instead when the deadline or the attempt limit is reached.
    pub(crate) fn reserve_attempt(&mut self, elapsed_ms: u64) -> Result<Ticket, StopReason> {
        if let Status::Closed(reason) = &self.status {
            return Err(reason.clone());
        }
        if self.pending.is_some() {
            return Err(StopReason::Unavailable("attempt-in-flight".to_owned()));
        }
        if elapsed_ms >= self.request.limits().deadline_ms {
            self.close(StopReason::Deadline);
            return Err(StopReason::Deadline);
        }
        let reserved = self.ledger.accounting().attempts;
        if reserved >= self.request.limits().attempts {
            self.close(StopReason::AttemptsExhausted);
            return Err(StopReason::AttemptsExhausted);
        }
        let attempt = self.next_attempt;
        self.ledger.append(Kind::Reserved {
            attempt: Some(attempt),
            stage: Stage::Attempt,
        });
        self.pending = Some(Pending {
            attempt,
            stage: PendingStage::Proposing,
            extraction: None,
        });
        self.next_attempt = self.next_attempt.saturating_add(1);
        Ok(Ticket { attempt })
    }

    /// Reserves one model call at its worst-case tokens before dispatch
    /// (NSR-005). When the call allowance is exhausted the attempt is settled
    /// as unavailable and the session closes; nothing is dispatched.
    pub(crate) fn reserve_model_call(
        &mut self,
        ticket: &Ticket,
        elapsed_ms: u64,
    ) -> Result<ModelCallTicket, StopReason> {
        let attempt = ticket.attempt;
        if !self.is_pending(attempt, |stage| matches!(stage, PendingStage::Proposing)) {
            return Err(StopReason::Unavailable("attempt-not-pending".to_owned()));
        }
        let limits = *self.request.limits();
        let totals = self.ledger.accounting();
        let stop = if elapsed_ms >= limits.deadline_ms {
            Some(StopReason::Deadline)
        } else if totals.model_calls >= limits.model_calls {
            Some(StopReason::ModelCallsExhausted)
        } else if totals.tokens_reserved + limits.call_tokens() > u64::from(limits.total_tokens) {
            Some(StopReason::TokensExhausted)
        } else {
            None
        };
        if let Some(reason) = stop {
            self.settle(
                attempt,
                Outcome::Failed(ProposerFailure::Unavailable(reason.name())),
            );
            self.close(reason.clone());
            return Err(reason);
        }
        self.ledger.append(Kind::Reserved {
            attempt: Some(attempt),
            stage: Stage::ModelCall {
                input_tokens: limits.input_tokens,
                output_tokens: limits.output_tokens,
                // Zero in this build: no provider has an approved tariff.
                money_micros: self.request.policy().money_micros,
            },
        });
        Ok(ModelCallTicket { attempt })
    }

    /// Admits a proposal for the reserved attempt. A failed proposal settles
    /// the attempt; a candidate becomes a [`CheckJob`] or settles; a strategy
    /// becomes a [`SearchJob`] for the shell's engine or settles.
    pub(crate) fn prepare(&mut self, ticket: Ticket, proposal: Proposal) -> Prepared {
        let attempt = ticket.attempt;
        if !self.is_pending(attempt, |stage| matches!(stage, PendingStage::Proposing)) {
            return Prepared::Settled(Outcome::NotPending);
        }
        match proposal {
            Proposal::Failed(failure) => {
                Prepared::Settled(self.settle(attempt, Outcome::Failed(failure)))
            }
            Proposal::Candidate(bytes) => self.admit_candidate(attempt, bytes),
            Proposal::Strategy(value) => match Strategy::from_json(&value) {
                Ok(strategy) => {
                    if let Some(pending) = &mut self.pending {
                        pending.stage = PendingStage::Searching;
                    }
                    Prepared::Search(SearchJob { attempt, strategy })
                }
                Err(refusal) => Prepared::Settled(self.settle(
                    attempt,
                    Outcome::Refused(AdmissionRefusal::Strategy(refusal)),
                )),
            },
        }
    }

    /// Delivers the search worker's report for a strategy attempt. Emitted
    /// bytes enter the same admission as a whole candidate (NSM-004).
    pub(crate) fn searched(&mut self, job: SearchJob, report: SearchReport) -> Prepared {
        let attempt = job.attempt;
        if !self.is_pending(attempt, |stage| matches!(stage, PendingStage::Searching)) {
            return Prepared::Settled(Outcome::Late);
        }
        match report {
            SearchReport::Candidate { bytes, extraction } => {
                if let Some(pending) = &mut self.pending {
                    pending.extraction = Some(extraction);
                }
                self.admit_candidate(attempt, bytes)
            }
            SearchReport::Unavailable { reason } => {
                Prepared::Settled(self.settle(attempt, Outcome::StrategyUnavailable(reason)))
            }
            SearchReport::Failed { reason } => {
                Prepared::Settled(self.settle(attempt, Outcome::SearchFailed(reason)))
            }
        }
    }

    /// NSC precedence for a candidate: bounded byte intake, duplicate check,
    /// canonical decoding with the library's complete admission, profile
    /// checks, exact schema equality, then work feasibility and the check
    /// reservation.
    fn admit_candidate(&mut self, attempt: u8, bytes: Vec<u8>) -> Prepared {
        let limits = *self.request.limits();
        if bytes.len() > limits.artifact_bytes as usize {
            let refusal = AdmissionRefusal::TooLarge {
                bytes: bytes.len(),
                max: limits.artifact_bytes,
            };
            return Prepared::Settled(self.settle(attempt, Outcome::Refused(refusal)));
        }
        let candidate_sha256 = sha256_hex(&bytes);
        if candidate_sha256 == self.request.original_sha256() {
            return Prepared::Settled(
                self.settle(attempt, Outcome::Duplicate(DuplicateOf::Original)),
            );
        }
        if let Some((_, earlier)) = self
            .checked
            .iter()
            .find(|(digest, _)| *digest == candidate_sha256)
        {
            let earlier = *earlier;
            return Prepared::Settled(
                self.settle(attempt, Outcome::Duplicate(DuplicateOf::Attempt(earlier))),
            );
        }
        let program = match import_program(&bytes) {
            Ok(program) => program,
            Err(error) => {
                let refusal = AdmissionRefusal::NotAdmitted {
                    code: admission_code(&error).to_owned(),
                };
                return Prepared::Settled(self.settle(attempt, Outcome::Refused(refusal)));
            }
        };
        if let Err(refusal) = self.request.profile().admit(&program) {
            return Prepared::Settled(self.settle(
                attempt,
                Outcome::Refused(AdmissionRefusal::Profile(refusal)),
            ));
        }
        if program.inputs() != self.request.program().inputs() {
            return Prepared::Settled(
                self.settle(attempt, Outcome::Refused(AdmissionRefusal::InputAbi)),
            );
        }
        if program.outputs() != self.request.program().outputs() {
            return Prepared::Settled(
                self.settle(attempt, Outcome::Refused(AdmissionRefusal::OutputAbi)),
            );
        }
        let cost = Cost::measure(&program, &bytes);
        let totals = self.ledger.accounting();
        let session_remaining = limits.session_work.saturating_sub(totals.work_reserved);
        let needed = self.request.check_work(cost);
        let Some(work) =
            needed.filter(|work| *work <= limits.check_work && *work <= session_remaining)
        else {
            let outcome = Outcome::WorkExhausted {
                needed,
                check_limit: limits.check_work,
                session_remaining,
            };
            return Prepared::Settled(self.settle_with(
                attempt,
                outcome,
                Some(&candidate_sha256),
                Some(cost),
            ));
        };
        if totals.checks >= limits.checks {
            let outcome = self.settle_with(
                attempt,
                Outcome::ChecksExhausted,
                Some(&candidate_sha256),
                Some(cost),
            );
            self.close(StopReason::ChecksExhausted);
            return Prepared::Settled(outcome);
        }
        self.ledger.append(Kind::Reserved {
            attempt: Some(attempt),
            stage: Stage::Check { work },
        });
        if let Some(pending) = &mut self.pending {
            pending.stage = PendingStage::Checking {
                candidate_sha256: candidate_sha256.clone(),
            };
        }
        Prepared::Check(CheckJob {
            attempt,
            original: self.request.original().to_vec(),
            candidate: bytes,
            candidate_sha256,
            cost,
            work,
            limits: self.request.transform_limits(),
        })
    }

    /// Delivers the check stage's report. Only a completed equivalence for the
    /// pending job, passing the selection rule, replaces the incumbent
    /// (NSL-004, NSL-005). Anything else, including a late or mismatched
    /// report, leaves it unchanged (NSL-007).
    pub(crate) fn conclude(&mut self, job: CheckJob, report: CheckReport) -> Outcome {
        let attempt = job.attempt;
        let pending = self.is_pending(attempt, |stage| {
            matches!(stage, PendingStage::Checking { candidate_sha256 } if *candidate_sha256 == job.candidate_sha256)
        });
        if !pending {
            return Outcome::Late;
        }
        let digest = Some(job.candidate_sha256.as_str());
        let completed = match report {
            CheckReport::Failed(failure) => {
                return self.settle_with(
                    attempt,
                    Outcome::CheckFailed(failure),
                    digest,
                    Some(job.cost),
                );
            }
            CheckReport::Completed(completed) => completed,
        };
        if completed.attempt != attempt || completed.candidate_sha256 != job.candidate_sha256 {
            let failure = WorkerFailure::Unavailable("report-for-another-job".to_owned());
            return self.settle_with(
                attempt,
                Outcome::CheckFailed(failure),
                digest,
                Some(job.cost),
            );
        }
        self.checked.push((job.candidate_sha256.clone(), attempt));
        let outcome = match completed.result {
            Ok(equivalence) => self.select(&job, &equivalence),
            Err(Rejection::Counterexample(found)) => {
                let witness = Witness::bind(&self.request, &job.candidate, found);
                match replay_witness(&self.request, &job.candidate, &witness) {
                    Ok(()) => Outcome::Different(witness),
                    Err(fault) => Outcome::WitnessNotReproduced(fault),
                }
            }
            Err(Rejection::Inconclusive(stop)) => Outcome::Inconclusive(inconclusive_json(&stop)),
            Err(Rejection::Refused(refusal)) => {
                Outcome::Refused(AdmissionRefusal::Checker(json!(format!("{refusal:?}"))))
            }
        };
        self.settle_with(attempt, outcome, digest, Some(job.cost))
    }

    fn select(&mut self, job: &CheckJob, equivalence: &Equivalence) -> Outcome {
        let original = self.request.cost();
        let selection = select(original, self.incumbent.cost(original), job.cost);
        if selection == Selection::CheckedImprovement {
            match Replacement::from_equivalence(
                equivalence,
                self.request.original(),
                &job.candidate,
                job.cost,
                job.attempt,
            ) {
                Ok(replacement) => {
                    self.ledger.append(Kind::Replaced {
                        attempt: job.attempt,
                        candidate_sha256: replacement.sha256().to_owned(),
                        receipt_sha256: replacement.receipt_sha256().to_owned(),
                        cost: replacement.cost(),
                    });
                    self.incumbent = Incumbent::CheckedReplacement(replacement);
                }
                Err(mismatch) => {
                    return Outcome::CheckFailed(WorkerFailure::Unavailable(format!(
                        "receipt-binding:{}",
                        mismatch.field
                    )));
                }
            }
        }
        Outcome::Equivalent {
            cost: job.cost,
            selection,
        }
    }

    fn is_pending(&self, attempt: u8, stage: impl Fn(&PendingStage) -> bool) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.attempt == attempt && stage(&pending.stage))
    }

    fn settle(&mut self, attempt: u8, outcome: Outcome) -> Outcome {
        self.settle_with(attempt, outcome, None, None)
    }

    /// Records the attempt's outcome. Reservations are never refunded
    /// (NSR-005); the record only says what happened.
    fn settle_with(
        &mut self,
        attempt: u8,
        outcome: Outcome,
        candidate_sha256: Option<&str>,
        cost: Option<Cost>,
    ) -> Outcome {
        let extraction = self.pending.take().and_then(|pending| pending.extraction);
        self.ledger.append(Kind::Settled {
            attempt,
            outcome: outcome.name(),
            candidate_sha256: candidate_sha256.map(str::to_owned),
            cost,
        });
        let feedback = feedback_of(attempt, &outcome, cost);
        self.attempts.push(AttemptRecord {
            attempt,
            candidate_sha256: candidate_sha256.map(str::to_owned),
            cost,
            outcome: outcome.name(),
            feedback,
            extraction,
        });
        if self.ledger.accounting().attempts >= self.request.limits().attempts {
            self.close(StopReason::AttemptsExhausted);
        }
        outcome
    }

    /// Closes the session. A pending attempt is settled as timed out first;
    /// its reservations stay charged.
    pub(crate) fn close(&mut self, reason: StopReason) {
        if let Some(pending) = &self.pending {
            let attempt = pending.attempt;
            let outcome = match pending.stage {
                PendingStage::Checking { .. } => Outcome::CheckFailed(WorkerFailure::Timeout),
                PendingStage::Proposing | PendingStage::Searching => {
                    Outcome::Failed(ProposerFailure::Timeout)
                }
            };
            self.pending = None;
            self.ledger.append(Kind::Settled {
                attempt,
                outcome: outcome.name(),
                candidate_sha256: None,
                cost: None,
            });
            let feedback = feedback_of(attempt, &outcome, None);
            self.attempts.push(AttemptRecord {
                attempt,
                candidate_sha256: None,
                cost: None,
                outcome: outcome.name(),
                feedback,
                extraction: None,
            });
        }
        if self.status == Status::Open {
            self.ledger.append(Kind::Closed {
                reason: reason.name(),
            });
            self.status = Status::Closed(reason);
        }
    }

    /// NSL-008: the incumbent's status, the actual stop reason, the receipt
    /// status, every attempted cost and the accounting, with the claims the
    /// result does not make spelled out.
    pub(crate) fn report(&self) -> Value {
        let original = self.request.cost();
        let inputs = self.request.program().inputs();
        let outputs = self.request.program().outputs();
        json!({
            "schema": REPORT_SCHEMA,
            "authority": "none",
            "request_id": self.request.id(),
            "profile": self.request.profile().name(),
            "status": self.incumbent.status(),
            "incumbent": self.incumbent.json(self.request.original_sha256(), original),
            "original": {"sha256": self.request.original_sha256(), "cost": original.json()},
            "session": match &self.status {
                Status::Open => json!({"state": "open"}),
                Status::Closed(reason) => json!({"state": "closed", "stop_reason": reason.name()}),
            },
            "attempts": self.attempts.iter().map(|record| json!({
                "attempt": record.attempt,
                "outcome": record.outcome,
                "candidate_sha256": record.candidate_sha256,
                "cost": record.cost.map(Cost::json),
                "feedback": record.feedback.json(inputs, outputs),
                "extraction": record.extraction,
            })).collect::<Vec<_>>(),
            "accounting": self.ledger.accounting().json(),
            "ledger_head": self.ledger.head().json(),
            "witnesses_dropped_on_resume": self.dropped_witnesses,
            "claims": {
                "incumbent": "Equal to the original on every tuple of the declared input domain under eager semantics, by a complete transform check; the original is equal to itself without a receipt.",
                "costs": "Every incumbent is componentwise no more costly than the original; updates descend strictly in (nodes, bytes).",
                "not_claimed": [
                    "global optimality", "convergence", "success probability",
                    "model learning", "application or publication authority", "wall-time speedup"
                ]
            },
        })
    }

    /// NSR-007/NSR-008/NSR-009: resumes from persisted data. Binding and
    /// ledger faults precede any reuse; a retained replacement is replayed
    /// through the checker before it is trusted; stored witnesses are
    /// replayed before they are reused; a failed resume returns no incumbent.
    pub(crate) fn resume(stored: Stored<'_>, elapsed_ms: u64) -> Resumed {
        let request = match Request::readmit(stored.request, stored.original) {
            Ok(request) => request,
            Err(refusal) => return Resumed::Refused(ResumeRefusal::Request(refusal), None),
        };
        let entries = match Ledger::parse_lines(stored.ledger) {
            Ok(entries) => entries,
            Err(fault) => return Resumed::Refused(ResumeRefusal::Ledger(fault), None),
        };
        let mut ledger = match Ledger::from_entries(entries) {
            Ok(ledger) => ledger,
            Err(fault) => return Resumed::Refused(ResumeRefusal::Ledger(fault), None),
        };
        let Some(expected) = stored.head else {
            return Resumed::Refused(ResumeRefusal::Unverifiable, None);
        };
        let found = ledger.head();
        if expected != found {
            return Resumed::Refused(ResumeRefusal::RollbackSuspected { expected, found }, None);
        }
        match ledger.entries().first().map(|entry| &entry.kind) {
            Some(Kind::Opened {
                request_id,
                checker,
            }) if request_id == request.id() && checker == request.checker() => {}
            _ => return Resumed::Refused(ResumeRefusal::RequestMismatch, None),
        }
        let totals = ledger.accounting();
        let limits = *request.limits();
        let mut incumbent = Incumbent::OriginalAdmitted;
        let mut replay_work = 0_u64;
        let mut witnesses = Vec::new();
        let mut dropped = Vec::new();
        if let Some((_, candidate_sha256, receipt_sha256, cost)) = &totals.replaced {
            let Some((candidate, receipt)) = stored.replacement else {
                return Resumed::Refused(ResumeRefusal::ReplacementMissing, None);
            };
            if sha256_hex(candidate) != *candidate_sha256 || sha256_hex(receipt) != *receipt_sha256
            {
                return Resumed::Refused(ResumeRefusal::ReplacementDigest, None);
            }
            let program = match import_program(candidate) {
                Ok(program) => program,
                Err(_) => return Resumed::Refused(ResumeRefusal::ReplacementNotAdmitted, None),
            };
            let measured = Cost::measure(&program, candidate);
            if measured != *cost {
                return Resumed::Refused(ResumeRefusal::ReplacementDigest, None);
            }
            match request.check_work(measured) {
                Some(work) if work <= limits.check_work => replay_work = work,
                _ => {
                    return Resumed::Inconclusive(ResumeRefusal::InsufficientReplayAllowance, None);
                }
            }
        }
        for (attempt, value, candidate) in &stored.witnesses {
            let inputs = request.program().inputs();
            let outputs = request.program().outputs();
            match Witness::from_json(value, inputs, outputs) {
                Some(witness) => {
                    // One tuple on both programs: at most their node counts.
                    let work = import_program(candidate)
                        .ok()
                        .map_or(0, |program| Cost::measure(&program, candidate).nodes)
                        .saturating_add(request.cost().nodes)
                        .saturating_add(2);
                    replay_work = replay_work.saturating_add(work);
                    witnesses.push((*attempt, witness, candidate.as_slice()));
                }
                None => dropped.push(*attempt),
            }
        }
        let session_remaining = limits.session_work.saturating_sub(totals.work_reserved);
        if replay_work > session_remaining {
            return Resumed::Inconclusive(ResumeRefusal::InsufficientReplayAllowance, None);
        }
        if elapsed_ms >= limits.deadline_ms {
            return Resumed::Inconclusive(ResumeRefusal::Deadline, None);
        }
        if replay_work > 0 {
            ledger.append(Kind::Reserved {
                attempt: None,
                stage: Stage::Replay { work: replay_work },
            });
        }
        if let (Some((attempt, _, _, cost)), Some((candidate, receipt))) =
            (&totals.replaced, stored.replacement)
        {
            match transform::check(request.original(), candidate, request.transform_limits()) {
                Ok(equivalence) if equivalence.receipt() == receipt => {
                    match Replacement::from_equivalence(
                        &equivalence,
                        request.original(),
                        candidate,
                        *cost,
                        *attempt,
                    ) {
                        Ok(replacement) => incumbent = Incumbent::CheckedReplacement(replacement),
                        Err(mismatch) => {
                            return Resumed::Refused(
                                ResumeRefusal::ReceiptMismatch(mismatch.field.to_owned()),
                                Some(ledger),
                            );
                        }
                    }
                }
                Ok(_) => {
                    return Resumed::Refused(
                        ResumeRefusal::ReceiptMismatch("receipt-bytes".to_owned()),
                        Some(ledger),
                    );
                }
                Err(rejection) => {
                    return Resumed::Refused(
                        ResumeRefusal::ReceiptMismatch(format!("{rejection:?}")),
                        Some(ledger),
                    );
                }
            }
        }
        let mut verified = Vec::new();
        for (attempt, witness, candidate) in witnesses {
            match replay_witness(&request, candidate, &witness) {
                Ok(()) => verified.push(witness),
                Err(_) => dropped.push(attempt),
            }
        }
        let attempts = rebuild_attempts(&ledger, &verified);
        let checked = attempts
            .iter()
            .filter_map(|record| {
                record
                    .candidate_sha256
                    .clone()
                    .map(|digest| (digest, record.attempt))
            })
            .collect();
        let status = match totals.closed {
            Some(reason) => Status::Closed(StopReason::parse(&reason)),
            None => Status::Open,
        };
        Resumed::Session(Box::new(Session {
            next_attempt: totals.attempts,
            request,
            incumbent,
            ledger,
            attempts,
            checked,
            pending: None,
            status,
            dropped_witnesses: dropped,
        }))
    }
}

/// Persisted session data the shell loaded (NSR-007).
pub(crate) struct Stored<'a> {
    /// The canonical request record.
    pub(crate) request: &'a [u8],
    /// The exact original bytes.
    pub(crate) original: &'a [u8],
    /// The ledger, one canonical entry per line.
    pub(crate) ledger: &'a [u8],
    /// The separately kept head, if any.
    pub(crate) head: Option<Head>,
    /// The latest replacement's candidate and receipt bytes, if retained.
    pub(crate) replacement: Option<(&'a [u8], &'a [u8])>,
    /// Stored witnesses, each with its attempt and the candidate bytes it
    /// names.
    pub(crate) witnesses: Vec<(u8, Value, Vec<u8>)>,
}

/// Why resume yields no trusted incumbent (NSR-009).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResumeRefusal {
    Request(ReadmitRefusal),
    Ledger(LedgerFault),
    /// No head was kept, so continuity cannot be verified.
    Unverifiable,
    /// The ledger is not at its recorded head: shorter, longer or rewritten.
    RollbackSuspected {
        expected: Head,
        found: Head,
    },
    /// The ledger was opened for another request or checker.
    RequestMismatch,
    ReplacementMissing,
    ReplacementDigest,
    ReplacementNotAdmitted,
    /// The replay produced a different receipt, or no equivalence.
    ReceiptMismatch(String),
    InsufficientReplayAllowance,
    Deadline,
}

impl ResumeRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            ResumeRefusal::Request(refusal) => refusal.json(),
            ResumeRefusal::Ledger(fault) => fault.json(),
            ResumeRefusal::Unverifiable => json!({"reason": "ledger-head-missing"}),
            ResumeRefusal::RollbackSuspected { expected, found } => json!({
                "reason": "ledger-rollback-suspected", "expected": expected.json(), "found": found.json()
            }),
            ResumeRefusal::RequestMismatch => json!({"reason": "ledger-request-mismatch"}),
            ResumeRefusal::ReplacementMissing => json!({"reason": "replacement-missing"}),
            ResumeRefusal::ReplacementDigest => json!({"reason": "replacement-digest"}),
            ResumeRefusal::ReplacementNotAdmitted => json!({"reason": "replacement-not-admitted"}),
            ResumeRefusal::ReceiptMismatch(detail) => {
                json!({"reason": "receipt-mismatch", "detail": detail})
            }
            ResumeRefusal::InsufficientReplayAllowance => {
                json!({"reason": "insufficient-replay-allowance"})
            }
            ResumeRefusal::Deadline => json!({"reason": "deadline"}),
        }
    }
}

/// The result of a resume. Only `Session` carries an incumbent.
#[derive(Debug)]
pub(crate) enum Resumed {
    /// Boxed: a session is large and the refusals are small.
    Session(Box<Session>),
    /// `ResumeRefused`; the ledger, when present, carries a replay charge to
    /// persist.
    Refused(ResumeRefusal, Option<Ledger>),
    /// `ResumeInconclusive`: allowance or deadline prevented replay.
    Inconclusive(ResumeRefusal, Option<Ledger>),
}

fn feedback_of(attempt: u8, outcome: &Outcome, cost: Option<Cost>) -> Feedback {
    match outcome {
        Outcome::Different(witness) => Feedback::Difference {
            attempt,
            witness: witness.clone(),
        },
        Outcome::Refused(refusal) => Feedback::Refused {
            attempt,
            reason: refusal.json(),
        },
        Outcome::Equivalent { cost, selection } => Feedback::Equivalent {
            attempt,
            cost: *cost,
            selection: *selection,
        },
        Outcome::Duplicate(of) => Feedback::Duplicate { attempt, of: *of },
        Outcome::StrategyUnavailable(reason) => Feedback::Unavailable {
            attempt,
            reason: reason.clone(),
        },
        Outcome::Failed(failure) => Feedback::Failed {
            attempt,
            reason: failure.name(),
        },
        Outcome::SearchFailed(reason) => Feedback::Failed {
            attempt,
            reason: reason.clone(),
        },
        Outcome::WorkExhausted { .. }
        | Outcome::ChecksExhausted
        | Outcome::WitnessNotReproduced(_)
        | Outcome::Inconclusive(_)
        | Outcome::CheckFailed(_)
        | Outcome::Late
        | Outcome::NotPending => Feedback::Incomplete {
            attempt,
            stop: json!({"outcome": outcome.name(), "cost": cost.map(Cost::json)}),
        },
    }
}

/// Rebuilds bounded attempt history from settlements; a difference keeps its
/// witness only when the stored witness replayed.
fn rebuild_attempts(ledger: &Ledger, verified: &[Witness]) -> Vec<AttemptRecord> {
    ledger
        .entries()
        .iter()
        .filter_map(|entry| match &entry.kind {
            Kind::Settled {
                attempt,
                outcome,
                candidate_sha256,
                cost,
            } => {
                let feedback = if outcome == "different" {
                    match verified.iter().find(|witness| {
                        Some(&witness.candidate_sha256) == candidate_sha256.as_ref()
                    }) {
                        Some(witness) => Feedback::Difference {
                            attempt: *attempt,
                            witness: witness.clone(),
                        },
                        None => Feedback::Incomplete {
                            attempt: *attempt,
                            stop: json!({"outcome": "different", "witness": "not-retained"}),
                        },
                    }
                } else if let (Some(cost), "checked-improvement") = (cost, outcome.as_str()) {
                    Feedback::Equivalent {
                        attempt: *attempt,
                        cost: *cost,
                        selection: Selection::CheckedImprovement,
                    }
                } else {
                    Feedback::Incomplete {
                        attempt: *attempt,
                        stop: json!({"outcome": outcome, "cost": cost.map(Cost::json)}),
                    }
                };
                Some(AttemptRecord {
                    attempt: *attempt,
                    candidate_sha256: candidate_sha256.clone(),
                    cost: *cost,
                    outcome: outcome.clone(),
                    feedback,
                    extraction: None,
                })
            }
            _ => None,
        })
        .collect()
}

fn admission_code(error: &Error) -> &'static str {
    match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    }
}

fn inconclusive_json(stop: &Inconclusive) -> Value {
    match stop {
        Inconclusive::DomainTooLarge { size, limit } => json!({
            "cause": "domain-too-large",
            "domain_size": size.map(|size| size.to_string()),
            "max_input_tuples": limit
        }),
        Inconclusive::BudgetBoundary {
            over_limit,
            minimum_limit,
            usage,
        } => json!({
            "cause": "budget-boundary",
            "over_limit": {"original": over_limit[0], "candidate": over_limit[1]},
            "minimum_step_limit": minimum_limit,
            "usage": transform::usage_json(usage)
        }),
        Inconclusive::CoverageMismatch { expected, visited } => json!({
            "cause": "coverage-mismatch", "expected": expected, "visited": visited
        }),
    }
}
