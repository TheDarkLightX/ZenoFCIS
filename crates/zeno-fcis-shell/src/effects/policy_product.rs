//! Bounded payment product using the real delivery program and policy stack.
//!
//! Authorization is an explicit two-invoice model input, not an Authority
//! publication. Provider idempotency and duplicate transport copies are separate
//! environment assumptions. This module performs no I/O and grants no authority.

use alloc::{vec, vec::Vec};
use core::num::NonZeroU64;
use zeno_fcis_codec::{CommitmentHasher, Hash32, RustCryptoSha256};

use super::driver::{Binding, Driver, Reply};
use super::handler::{Handled, Layer, Stack, Terminal};
use super::policy::{EffectKey, Monitor, Policy};
use super::port::{
    Code, Delivery, EpochTick, Fault, Operation, Outcome, Report, Response, Sent, Store, WaitError,
};
use super::program::{DeliveryInvocation, DeliveryPolicy, DeliveryProgram};
use super::record::{self, Ended, Manifest, Transcript};
use super::registry::{self, Port, REGISTRY};
use super::{codec, policy};

/// Complete finite response alphabets, in choice order. Time successes use one
/// exact tick and zero jitter; failures remain distinct, explicit responses.
pub const ALPHABETS: &[&str] = &[
    "ReadOutbox: exact, busy, full, io, refused, crash",
    "Send: sent, not-sent, unknown-before, hidden-success, crash-before, crash-after, refused, mismatched-report; optional duplicate-copies",
    "Acknowledge: applied, busy, unknown-before, applied-unknown, crash-before, applied-crash, full, io, refused",
    "Clock/Random: exact, busy, full, io",
    "Wait: exact, interrupted, busy, full, io",
];

/// Explicit finite domain and environment assumptions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Profile {
    /// Maximum handled requests; a stopped action remains incomplete.
    pub bound: u8,
    /// Run-wide retries, as used by the actual delivery program.
    pub retries: u8,
    /// Whether the bound allowlist admits the invoice destination.
    pub allowed: bool,
    /// Whether the runtime mounts the declared monitors.
    pub monitored: bool,
    /// Assumed atomic provider deduplication, never a shell guarantee.
    pub provider_idempotent: bool,
    /// Include two transport copies from one invocation in the alphabet.
    pub duplicate_copies: bool,
}

/// One exact operation, environment choice and immutable policy successor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    /// Canonical requested bytes; no policy may rewrite them.
    pub operation: Vec<u8>,
    /// Environment choice, or 255 for a policy refusal before invocation.
    pub choice: u8,
    /// Individual policy verdicts, in declaration order (0 allow, else code+1).
    pub verdicts: Vec<u8>,
    /// Exact response bytes; empty on a crash.
    pub response: Vec<u8>,
    /// Canonical state of each monitor after the handled request.
    pub states: Vec<Vec<u8>>,
}

/// Complete observation of one bounded schedule.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Run {
    /// Choice arity at an unexpanded environment node, otherwise zero.
    pub next: u8,
    /// 0 done, 1 crash, 2 request-bound stop, 3 next environment choice.
    pub end: u8,
    /// Canonical program outcome; empty unless it finished.
    pub outcome: Vec<u8>,
    /// Exact operation/verdict/response trace.
    pub trace: Vec<Event>,
    /// External payment occurrence counts, independently of monitor state.
    pub commits: [u8; 2],
    /// Pending authorized records; even a refusal must not erase them silently.
    pub pending: [bool; 2],
    /// Keys explicitly refused by a policy or transport.
    pub refused: [bool; 2],
    /// First independent acceptance failure, if any.
    pub violation: Option<&'static str>,
    /// Whether a terminal transcript replayed; false for incomplete prefixes.
    pub replayed: bool,
}

/// Refused input, distinct from an incomplete bound or a counterexample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// K must be at most 12 and retries at most 2.
    Profile,
    /// A choice is out of range or the schedule has an unused suffix.
    Schedule,
    /// A driver invariant or response binding failed.
    Driver,
}

fn store() -> Store {
    Store {
        identity: Hash32::new([8; 32]),
        handle: 1,
    }
}

/// Exact two-invoice authorization fixture, with deterministic ascending order.
#[must_use]
pub fn invoices() -> Vec<Delivery> {
    (0u8..2)
        .map(|index| {
            let payload = vec![index + 1];
            Delivery {
                id: Hash32::new([index + 1; 32]),
                version: 1,
                lane: 1,
                ordinal: u32::from(index),
                destination: b"receiver".to_vec(),
                payload_sha256: RustCryptoSha256::hash(&payload),
                payload,
            }
        })
        .collect()
}

/// Independent acceptance over occurrence bags. Policy reservations are not
/// consulted here. Two identical commits exceed a bound even with one set key.
#[must_use]
pub fn check_actions(
    authorized: &[EffectKey],
    committed: &[EffectKey],
    refused: &[EffectKey],
    pending: &[EffectKey],
) -> Option<&'static str> {
    if committed.iter().any(|key| !authorized.contains(key)) {
        return Some("unauthorized commit");
    }
    for key in authorized {
        let count = committed.iter().filter(|actual| *actual == key).count();
        if count > 1 {
            return Some("duplicate commit");
        }
        if count == 0 && !refused.contains(key) && !pending.contains(key) {
            return Some("silent drop");
        }
    }
    None
}

struct Environment<'a> {
    profile: Profile,
    schedule: &'a [u8],
    used: usize,
    next: u8,
    invalid: bool,
    invoices: Vec<Delivery>,
    pending: [bool; 2],
    commits: Vec<EffectKey>,
    refused: [bool; 2],
    last: Option<u8>,
}

impl Environment<'_> {
    fn pay(&mut self, index: usize) {
        let key = (store().identity, self.invoices[index].id);
        if !self.profile.provider_idempotent || !self.commits.contains(&key) {
            self.commits.push(key);
        }
    }
    fn select(&mut self, choices: u8) -> Option<u8> {
        self.last = None;
        match self.schedule.get(self.used).copied() {
            None => {
                self.next = choices;
                None
            }
            Some(choice) if choice < choices => {
                self.used += 1;
                self.last = Some(choice);
                Some(choice)
            }
            Some(_) => {
                self.invalid = true;
                None
            }
        }
    }
    fn index(&mut self, id: Hash32) -> Option<usize> {
        let index = self.invoices.iter().position(|d| d.id == id);
        if index.is_none() {
            self.invalid = true;
        }
        index
    }
}

impl Terminal for Environment<'_> {
    fn handle_with_invocation(
        &mut self,
        binding: &Binding,
        operation: &Operation,
    ) -> super::handler::Handling {
        let outcome = self.handle(binding, operation);
        // Every direct suspension precedes the selected branch's mutations.
        // Composed handlers must combine this evidence with earlier calls.
        let invocation = if outcome == Handled::Suspend {
            super::handler::Invocation::NotInvoked
        } else {
            super::handler::Invocation::MayHaveInvoked
        };
        super::handler::Handling {
            outcome,
            invocation,
        }
    }

    fn ports(&self) -> &'static [Port] {
        DeliveryProgram::PORTS
    }
    fn handle(&mut self, _: &Binding, operation: &Operation) -> Handled {
        let choices = match operation {
            Operation::ReadOutbox(_) => 6,
            Operation::Send(..) => 8 + u8::from(self.profile.duplicate_copies),
            Operation::Acknowledge(..) => 9,
            Operation::Clock | Operation::Random(..) => 4,
            Operation::Wait(_) => 5,
            _ => {
                self.invalid = true;
                return Handled::Suspend;
            }
        };
        let Some(choice) = self.select(choices) else {
            return Handled::Suspend;
        };
        let response = match operation {
            Operation::ReadOutbox(_) => match choice {
                0 => Response::ReadOutbox(Outcome::Ok(
                    self.invoices
                        .iter()
                        .zip(self.pending)
                        .filter(|(_, p)| *p)
                        .map(|(d, _)| d.clone())
                        .collect(),
                )),
                1..=3 => Response::ReadOutbox(Outcome::NotApplied(fault(choice - 1))),
                4 => Response::ReadOutbox(Outcome::Refused(Code::History)),
                _ => return Handled::Crash,
            },
            Operation::Send(address, delivery) => {
                let Some(index) = self.index(delivery.id) else {
                    return Handled::Suspend;
                };
                if *address != store() || *delivery != self.invoices[index] {
                    self.invalid = true;
                    return Handled::Suspend;
                }
                if matches!(choice, 0 | 3 | 5 | 7 | 8) {
                    self.pay(index);
                }
                if choice == 8 {
                    self.pay(index);
                }
                match choice {
                    0 | 8 => Response::Send(Sent::Sent(Report {
                        delivery_id: delivery.id,
                        payload_sha256: delivery.payload_sha256,
                    })),
                    1 => Response::Send(Sent::NotSent(Code::Delivery)),
                    2 | 3 => Response::Send(Sent::Unknown(Fault::Io)),
                    4 | 5 => return Handled::Crash,
                    6 => {
                        self.refused[index] = true;
                        Response::Send(Sent::Refused(Code::Delivery))
                    }
                    _ => Response::Send(Sent::Sent(Report {
                        delivery_id: Hash32::ZERO,
                        payload_sha256: delivery.payload_sha256,
                    })),
                }
            }
            Operation::Acknowledge(_, id, _) => {
                let Some(index) = self.index(*id) else {
                    return Handled::Suspend;
                };
                let already = !self.pending[index];
                if matches!(choice, 0 | 3 | 5) {
                    self.pending[index] = false;
                }
                match choice {
                    0 if already => {
                        Response::Acknowledge(Outcome::Refused(Code::AlreadyAcknowledged))
                    }
                    0 => Response::Acknowledge(Outcome::Ok(())),
                    1 => Response::Acknowledge(Outcome::NotApplied(Fault::Busy)),
                    2 | 3 => Response::Acknowledge(Outcome::Unknown(Fault::Io)),
                    4 | 5 => return Handled::Crash,
                    6 => Response::Acknowledge(Outcome::NotApplied(Fault::Full)),
                    7 => Response::Acknowledge(Outcome::NotApplied(Fault::Io)),
                    _ => Response::Acknowledge(Outcome::Refused(Code::Delivery)),
                }
            }
            Operation::Clock => Response::Clock(if choice == 0 {
                Ok(EpochTick { epoch: 1, tick: 0 })
            } else {
                Err(fault(choice - 1))
            }),
            Operation::Random(..) => Response::Random(if choice == 0 {
                Ok(0)
            } else {
                Err(fault(choice - 1))
            }),
            Operation::Wait(tick) => Response::Wait(match choice {
                0 => Ok(*tick),
                1 => Err(WaitError::Interrupted),
                _ => Err(WaitError::Failed(fault(choice - 2))),
            }),
            _ => {
                self.invalid = true;
                return Handled::Suspend;
            }
        };
        Handled::Answer(response)
    }
}

fn fault(index: u8) -> Fault {
    match index {
        0 => Fault::Busy,
        1 => Fault::Full,
        _ => Fault::Io,
    }
}
fn encode(outcome: &Outcome<u64>) -> Vec<u8> {
    codec::outcome(
        outcome,
        match outcome {
            Outcome::Ok(n) => Some(*n),
            _ => None,
        },
    )
}
fn first(retries: u8) -> super::program::Step<super::program::DeliveryContinuation, u64> {
    DeliveryProgram::start(
        DeliveryInvocation {
            epoch: 1,
            store: store(),
        },
        DeliveryPolicy {
            retries,
            backoff: 1,
            jitter: NonZeroU64::MIN,
        },
    )
}

use super::program::Program;

/// Execute a schedule against actual library programs. Missing choices expose
/// the next environment node. K exhaustion preserves pending obligations and is
/// an incomplete action result, never a claim of eventual delivery.
pub fn run(profile: Profile, schedule: &[u8]) -> Result<Run, Refusal> {
    if profile.bound > 12 || profile.retries > 2 {
        return Err(Refusal::Profile);
    }
    let mut environment = Environment {
        profile,
        schedule,
        used: 0,
        next: 0,
        invalid: false,
        invoices: invoices(),
        pending: [true; 2],
        commits: Vec::new(),
        refused: [false; 2],
        last: None,
    };
    let policies = if profile.monitored {
        vec![
            Policy::AtMostOne { capacity: 2 },
            Policy::allowlist(if profile.allowed {
                vec![b"receiver".to_vec()]
            } else {
                Vec::new()
            }),
        ]
    } else {
        Vec::new()
    };
    let mut layers: Vec<_> = policies
        .into_iter()
        .map(Monitor::new)
        .map(Layer::Policy)
        .collect();
    let mut names = vec!["policy"; layers.len()];
    names.extend(["record", "payment-model"]);
    let mut initial = Vec::new();
    for delivery in &environment.invoices {
        codec::bytes(
            &mut initial,
            &codec::operation(&Operation::Send(store(), delivery.clone())),
        );
    }
    let start = vec![profile.retries];
    let mut manifest = Manifest::current(
        names,
        schedule.to_vec(),
        RustCryptoSha256::hash(&initial),
        start.clone(),
    );
    manifest.configuration = vec![
        profile.bound,
        profile.retries,
        u8::from(profile.allowed),
        u8::from(profile.monitored),
        u8::from(profile.provider_idempotent),
        u8::from(profile.duplicate_copies),
    ];
    manifest.implementations.push(record::Implementation {
        name: "payment-model",
        source: source(),
    });
    layers.push(Layer::Record(Transcript::new(manifest)));
    let mut stack = Stack { layers };
    let mut driver = Driver::<DeliveryProgram>::start(REGISTRY, 1, first(profile.retries))
        .map_err(|_| Refusal::Driver)?;
    let mut trace = Vec::new();
    let mut end = 2;
    let mut outcome = Vec::new();
    let mut replayed = false;
    for handled_count in 0..=profile.bound {
        if let Some(result) = driver.outcome() {
            outcome = encode(result);
            stack.done(&outcome);
            end = 0;
            break;
        }
        if handled_count == profile.bound {
            break;
        }
        let (binding, operation) = driver.request().ok_or(Refusal::Driver)?;
        let binding = binding.clone();
        environment.last = None;
        let handled = stack.handle(&mut environment, &binding, operation);
        if environment.invalid {
            return Err(Refusal::Schedule);
        }
        if handled == Handled::Suspend {
            end = 3;
            break;
        }
        let check = stack.transcripts()[0]
            .links
            .iter()
            .rev()
            .find_map(|link| match &link.entry {
                record::Entry::Policy(_, check) => Some(check),
                _ => None,
            });
        let verdicts = check
            .map(|check| {
                check
                    .decisions
                    .iter()
                    .map(|d| match d.verdict {
                        policy::Verdict::Allow => 0,
                        policy::Verdict::Reject(code) => code as u8 + 1,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Operation::Send(_, delivery) = operation
            && matches!(handled, Handled::Answer(Response::Send(Sent::Refused(_))))
            && let Some(index) = environment.index(delivery.id)
        {
            environment.refused[index] = true;
        }
        trace.push(Event {
            operation: codec::operation(operation),
            choice: environment.last.unwrap_or(255),
            verdicts,
            response: match &handled {
                Handled::Answer(response) => codec::response(response),
                _ => Vec::new(),
            },
            states: stack
                .layers
                .iter()
                .filter_map(|layer| match layer {
                    Layer::Policy(monitor) => Some(monitor.state.bytes()),
                    _ => None,
                })
                .collect(),
        });
        match handled {
            Handled::Answer(response) => driver
                .reply(Reply { binding, response })
                .map_err(|_| Refusal::Driver)?,
            Handled::Crash => {
                end = 1;
                break;
            }
            Handled::Suspend => return Err(Refusal::Driver),
        }
    }
    if environment.used != schedule.len() {
        return Err(Refusal::Schedule);
    }
    if end <= 1 {
        let replay = record::replay::<DeliveryProgram>(
            stack.transcripts()[0],
            &start,
            1,
            first(profile.retries),
            encode,
        );
        replayed = matches!(replay, Ok(Ended::Done(_)) | Ok(Ended::Crashed));
    }
    let authorized: Vec<_> = environment
        .invoices
        .iter()
        .map(|d| (store().identity, d.id))
        .collect();
    let refused: Vec<_> = authorized
        .iter()
        .zip(environment.refused)
        .filter(|(_, flag)| *flag)
        .map(|(key, _)| *key)
        .collect();
    let pending: Vec<_> = authorized
        .iter()
        .zip(environment.pending)
        .filter(|(_, flag)| *flag)
        .map(|(key, _)| *key)
        .collect();
    let violation = check_actions(&authorized, &environment.commits, &refused, &pending);
    let mut commits = [0u8; 2];
    for (index, key) in authorized.iter().enumerate() {
        commits[index] = environment
            .commits
            .iter()
            .filter(|actual| *actual == key)
            .count() as u8;
    }
    Ok(Run {
        next: if end == 3 { environment.next } else { 0 },
        end,
        outcome,
        trace,
        commits,
        pending: environment.pending,
        refused: environment.refused,
        violation,
        replayed,
    })
}

/// Source closure for the bounded product, its checker and runtime semantics.
#[must_use]
pub fn source() -> Hash32 {
    registry::source_digest(&[
        ("policy_product.rs", include_bytes!("policy_product.rs")),
        ("program.rs", include_bytes!("program.rs")),
        ("driver.rs", include_bytes!("driver.rs")),
        ("registry.rs", include_bytes!("registry.rs")),
        ("port.rs", include_bytes!("port.rs")),
        ("policy.rs", include_bytes!("policy.rs")),
        ("handler.rs", include_bytes!("handler.rs")),
        ("policy_record.rs", include_bytes!("policy_record.rs")),
        ("record.rs", include_bytes!("record.rs")),
        ("codec.rs", include_bytes!("codec.rs")),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        Profile {
            bound: 6,
            retries: 1,
            allowed: true,
            monitored: true,
            provider_idempotent: false,
            duplicate_copies: false,
        }
    }
    #[test]
    fn runtime_policy_blocks_hidden_success() {
        let schedule = [0, 3, 0, 0, 0];
        let mut observed = run(profile(), &schedule).unwrap_or_else(|e| panic!("{e:?}"));
        // If the guard is bypassed, give the exposed second send a successful
        // response: the timeout's hidden payment and this payment share a key.
        if observed.next != 0 {
            observed = run(profile(), &[0, 3, 0, 0, 0, 0]).unwrap_or_else(|e| panic!("{e:?}"));
        }
        assert_eq!(
            observed.commits,
            [1, 0],
            "double payment after hidden success"
        );
        assert_eq!(observed.violation, None);
        assert!(observed.replayed);
        assert_eq!(observed.trace.last().map(|e| e.choice), Some(255));
    }
    #[test]
    fn multiset_checker_rejects_two_identical_commits() {
        let key = (Hash32::ZERO, Hash32::new([1; 32]));
        assert_eq!(
            check_actions(&[key], &[key, key], &[], &[]),
            Some("duplicate commit"),
            "two commits of one key accepted"
        );
    }
    #[test]
    fn checker_rejects_silent_drop() {
        let key = (Hash32::ZERO, Hash32::new([1; 32]));
        assert_eq!(
            check_actions(&[key], &[], &[], &[]),
            Some("silent drop"),
            "authorized send ended in no state"
        );
        assert_eq!(check_actions(&[key], &[], &[key], &[]), None);
        assert_eq!(check_actions(&[key], &[], &[], &[key]), None);
        assert_eq!(
            check_actions(&[], &[key], &[], &[]),
            Some("unauthorized commit")
        );
    }
    #[test]
    fn provider_idempotency_is_only_an_assumption() {
        let p = Profile {
            bound: 2,
            duplicate_copies: true,
            ..profile()
        };
        assert_eq!(
            run(p, &[0, 8])
                .unwrap_or_else(|e| panic!("{e:?}"))
                .violation,
            Some("duplicate commit")
        );
        let p = Profile {
            provider_idempotent: true,
            ..p
        };
        assert_eq!(
            run(p, &[0, 8])
                .unwrap_or_else(|e| panic!("{e:?}"))
                .violation,
            None
        );
    }
}
