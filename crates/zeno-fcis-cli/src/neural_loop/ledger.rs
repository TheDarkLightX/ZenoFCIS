//! Append-only reservation ledger and its accounting (NSR-005, NSR-007,
//! NSR-008, NSL-002).
//!
//! Every allowance is reserved by an entry appended before the work it
//! covers; the shell persists that entry before dispatch. Settlements record
//! outcomes and never refund: a failed, refused, duplicate, timed-out or
//! crash-pending attempt keeps its charges. Entries form a hash chain, which
//! detects truncation and in-place edits of a stored ledger relative to a
//! separately kept head. It does not by itself detect a rollback of both the
//! ledger and its head to an older consistent state: that is the shell's
//! documented host-integrity assumption (NSR-008, NSR-009).

use super::incumbent::Cost;
use super::request::CheckerIdentity;
use super::{canonical_json, json_u64, only_fields, tagged_sha256};
use serde_json::{Value, json};

pub(crate) const LEDGER_SCHEMA: &str = "zeno-fcis/transform-ledger/1";
const LINK_TAG: &str = "zeno-fcis/transform-ledger-link/1";
/// The digest an empty ledger links from.
pub(crate) const GENESIS_DIGEST: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// What a reservation pays for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    /// One attempt, whatever it turns out to be.
    Attempt,
    /// One model call at its worst-case tokens and money.
    ModelCall {
        input_tokens: u32,
        output_tokens: u32,
        money_micros: u64,
    },
    /// One candidate check at its precharged work.
    Check { work: u64 },
    /// One resume replay at its precharged work.
    Replay { work: u64 },
}

/// One ledger record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// The session opened for exactly this request under this checker.
    Opened {
        request_id: String,
        checker: CheckerIdentity,
    },
    /// An allowance reserved before work. `attempt` is `None` for replays.
    Reserved { attempt: Option<u8>, stage: Stage },
    /// An attempt's outcome. Reservations stay charged.
    Settled {
        attempt: u8,
        outcome: String,
        candidate_sha256: Option<String>,
        cost: Option<Cost>,
    },
    /// The incumbent became this checked replacement.
    Replaced {
        attempt: u8,
        candidate_sha256: String,
        receipt_sha256: String,
        cost: Cost,
    },
    /// The session closed with this stop reason.
    Closed { reason: String },
}

/// A chained entry: `digest` commits to `sequence`, `previous` and `kind`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Entry {
    pub(crate) sequence: u64,
    pub(crate) previous: String,
    pub(crate) digest: String,
    pub(crate) kind: Kind,
}

/// The position a stored ledger must be at: entry count and last digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Head {
    pub(crate) entries: u64,
    pub(crate) digest: String,
}

/// Why stored ledger bytes are not this ledger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LedgerFault {
    /// A line is not a ledger entry of this schema.
    Unreadable { line: usize },
    /// Sequence numbers are not `0, 1, 2, ...`.
    Sequence { line: usize },
    /// `previous` does not name the preceding digest.
    Link { line: usize },
    /// The recorded digest is not the recomputed one.
    Digest { line: usize },
    /// The first entry is not `Opened`, or `Opened` recurs.
    Opened { line: usize },
    /// An entry follows `Closed`.
    AfterClose { line: usize },
}

/// Totals derived from a ledger.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Accounting {
    pub(crate) attempts: u8,
    pub(crate) checks: u8,
    pub(crate) replays: u8,
    pub(crate) model_calls: u8,
    pub(crate) tokens_reserved: u64,
    pub(crate) money_reserved: u64,
    pub(crate) work_reserved: u64,
    pub(crate) settled: u8,
    /// Attempts reserved but never settled: crash-pending, still charged.
    pub(crate) unresolved: Vec<u8>,
    /// The latest replacement, if any.
    pub(crate) replaced: Option<(u8, String, String, Cost)>,
    pub(crate) closed: Option<String>,
}

impl Accounting {
    pub(crate) fn json(&self) -> Value {
        json!({
            "attempts": self.attempts,
            "checks": self.checks,
            "replays": self.replays,
            "model_calls": self.model_calls,
            "tokens_reserved": self.tokens_reserved,
            "money_reserved_micros": self.money_reserved,
            "work_reserved": self.work_reserved,
            "settled": self.settled,
            "unresolved_attempts": self.unresolved,
            "complete": self.unresolved.is_empty(),
        })
    }
}

/// The in-memory ledger: the shell persists each new entry in order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Ledger {
    entries: Vec<Entry>,
}

impl Ledger {
    pub(crate) fn new() -> Ledger {
        Ledger::default()
    }

    /// Appends an entry linked to the current head.
    pub(crate) fn append(&mut self, kind: Kind) -> &Entry {
        let sequence = self.entries.len() as u64;
        let previous = self.head().digest;
        let digest = link_digest(sequence, &previous, &kind);
        self.entries.push(Entry {
            sequence,
            previous,
            digest,
            kind,
        });
        &self.entries[self.entries.len() - 1]
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub(crate) fn head(&self) -> Head {
        Head {
            entries: self.entries.len() as u64,
            digest: self
                .entries
                .last()
                .map_or_else(|| GENESIS_DIGEST.to_owned(), |entry| entry.digest.clone()),
        }
    }

    /// Rebuilds a ledger from stored entries, verifying every link and digest.
    pub(crate) fn from_entries(entries: Vec<Entry>) -> Result<Ledger, LedgerFault> {
        let mut previous = GENESIS_DIGEST.to_owned();
        let mut closed = false;
        for (line, entry) in entries.iter().enumerate() {
            if entry.sequence != line as u64 {
                return Err(LedgerFault::Sequence { line });
            }
            if entry.previous != previous {
                return Err(LedgerFault::Link { line });
            }
            if entry.digest != link_digest(entry.sequence, &entry.previous, &entry.kind) {
                return Err(LedgerFault::Digest { line });
            }
            if matches!(entry.kind, Kind::Opened { .. }) != (line == 0) {
                return Err(LedgerFault::Opened { line });
            }
            // After a close only replay charges may follow: resuming a closed
            // session for its report still replays and pays for the incumbent.
            let replay = matches!(
                entry.kind,
                Kind::Reserved {
                    attempt: None,
                    stage: Stage::Replay { .. }
                }
            );
            if closed && !replay {
                return Err(LedgerFault::AfterClose { line });
            }
            closed |= matches!(entry.kind, Kind::Closed { .. });
            previous.clone_from(&entry.digest);
        }
        Ok(Ledger { entries })
    }

    /// Parses one canonical entry per line.
    pub(crate) fn parse_lines(bytes: &[u8]) -> Result<Vec<Entry>, LedgerFault> {
        let text = std::str::from_utf8(bytes).map_err(|_| LedgerFault::Unreadable { line: 0 })?;
        text.lines()
            .enumerate()
            .map(|(line, text)| {
                serde_json::from_str(text)
                    .ok()
                    .and_then(|value| Entry::from_json(&value))
                    .ok_or(LedgerFault::Unreadable { line })
            })
            .collect()
    }

    /// Derives the totals. Reservations count whether or not they settled.
    pub(crate) fn accounting(&self) -> Accounting {
        let mut totals = Accounting::default();
        let mut reserved = Vec::new();
        let mut settled = Vec::new();
        for entry in &self.entries {
            match &entry.kind {
                Kind::Opened { .. } => {}
                Kind::Reserved { attempt, stage } => match stage {
                    Stage::Attempt => {
                        totals.attempts = totals.attempts.saturating_add(1);
                        if let Some(attempt) = attempt {
                            reserved.push(*attempt);
                        }
                    }
                    Stage::ModelCall {
                        input_tokens,
                        output_tokens,
                        money_micros,
                    } => {
                        totals.model_calls = totals.model_calls.saturating_add(1);
                        totals.tokens_reserved = totals
                            .tokens_reserved
                            .saturating_add(u64::from(*input_tokens))
                            .saturating_add(u64::from(*output_tokens));
                        totals.money_reserved = totals.money_reserved.saturating_add(*money_micros);
                    }
                    Stage::Check { work } => {
                        totals.checks = totals.checks.saturating_add(1);
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }
                    Stage::Replay { work } => {
                        totals.replays = totals.replays.saturating_add(1);
                        totals.work_reserved = totals.work_reserved.saturating_add(*work);
                    }
                },
                Kind::Settled { attempt, .. } => {
                    totals.settled = totals.settled.saturating_add(1);
                    settled.push(*attempt);
                }
                Kind::Replaced {
                    attempt,
                    candidate_sha256,
                    receipt_sha256,
                    cost,
                } => {
                    totals.replaced = Some((
                        *attempt,
                        candidate_sha256.clone(),
                        receipt_sha256.clone(),
                        *cost,
                    ));
                }
                Kind::Closed { reason } => totals.closed = Some(reason.clone()),
            }
        }
        totals.unresolved = reserved
            .into_iter()
            .filter(|attempt| !settled.contains(attempt))
            .collect();
        totals
    }
}

fn link_digest(sequence: u64, previous: &str, kind: &Kind) -> String {
    let body = canonical_json(&entry_body(sequence, previous, kind));
    let mut input = previous.as_bytes().to_vec();
    input.extend_from_slice(&body);
    tagged_sha256(LINK_TAG, &input)
}

fn stage_json(stage: &Stage) -> Value {
    match stage {
        Stage::Attempt => json!({"stage": "attempt"}),
        Stage::ModelCall {
            input_tokens,
            output_tokens,
            money_micros,
        } => json!({
            "stage": "model-call", "input_tokens": input_tokens,
            "output_tokens": output_tokens, "money_micros": money_micros
        }),
        Stage::Check { work } => json!({"stage": "check", "work": work}),
        Stage::Replay { work } => json!({"stage": "replay", "work": work}),
    }
}

fn stage_from_json(value: &Value) -> Option<Stage> {
    match value.get("stage")?.as_str()? {
        "attempt" if only_fields(value, &["stage"]) => Some(Stage::Attempt),
        "model-call"
            if only_fields(
                value,
                &["stage", "input_tokens", "output_tokens", "money_micros"],
            ) =>
        {
            Some(Stage::ModelCall {
                input_tokens: u32::try_from(json_u64(value, "input_tokens")?).ok()?,
                output_tokens: u32::try_from(json_u64(value, "output_tokens")?).ok()?,
                money_micros: json_u64(value, "money_micros")?,
            })
        }
        "check" if only_fields(value, &["stage", "work"]) => Some(Stage::Check {
            work: json_u64(value, "work")?,
        }),
        "replay" if only_fields(value, &["stage", "work"]) => Some(Stage::Replay {
            work: json_u64(value, "work")?,
        }),
        _ => None,
    }
}

fn entry_body(sequence: u64, previous: &str, kind: &Kind) -> Value {
    let mut body = match kind {
        Kind::Opened {
            request_id,
            checker,
        } => json!({"kind": "opened", "request_id": request_id, "checker": checker.json()}),
        Kind::Reserved { attempt, stage } => {
            json!({"kind": "reserved", "attempt": attempt, "reservation": stage_json(stage)})
        }
        Kind::Settled {
            attempt,
            outcome,
            candidate_sha256,
            cost,
        } => json!({
            "kind": "settled", "attempt": attempt, "outcome": outcome,
            "candidate_sha256": candidate_sha256, "cost": cost.map(Cost::json)
        }),
        Kind::Replaced {
            attempt,
            candidate_sha256,
            receipt_sha256,
            cost,
        } => json!({
            "kind": "replaced", "attempt": attempt, "candidate_sha256": candidate_sha256,
            "receipt_sha256": receipt_sha256, "cost": cost.json()
        }),
        Kind::Closed { reason } => json!({"kind": "closed", "reason": reason}),
    };
    body["schema"] = json!(LEDGER_SCHEMA);
    body["sequence"] = json!(sequence);
    body["previous"] = json!(previous);
    body
}

impl Entry {
    /// The entry as one canonical line.
    pub(crate) fn line(&self) -> Vec<u8> {
        canonical_json(&self.json())
    }

    pub(crate) fn json(&self) -> Value {
        let mut body = entry_body(self.sequence, &self.previous, &self.kind);
        body["digest"] = json!(self.digest);
        body
    }

    /// Strict reader: schema, exact field sets and a recorded digest, which
    /// [`Ledger::from_entries`] then recomputes.
    pub(crate) fn from_json(value: &Value) -> Option<Entry> {
        if value.get("schema")?.as_str()? != LEDGER_SCHEMA {
            return None;
        }
        let text = |field: &str| Some(value.get(field)?.as_str()?.to_owned());
        let common = ["schema", "sequence", "previous", "digest", "kind"];
        let fields = |extra: &[&str]| {
            let mut allowed = common.to_vec();
            allowed.extend_from_slice(extra);
            only_fields(value, &allowed)
        };
        let attempt = |field: &str| -> Option<Option<u8>> {
            match value.get(field)? {
                Value::Null => Some(None),
                number => Some(Some(u8::try_from(number.as_u64()?).ok()?)),
            }
        };
        let kind = match value.get("kind")?.as_str()? {
            "opened" if fields(&["request_id", "checker"]) => Kind::Opened {
                request_id: text("request_id")?,
                checker: CheckerIdentity::from_json(value.get("checker")?)?,
            },
            "reserved" if fields(&["attempt", "reservation"]) => Kind::Reserved {
                attempt: attempt("attempt")?,
                stage: stage_from_json(value.get("reservation")?)?,
            },
            "settled" if fields(&["attempt", "outcome", "candidate_sha256", "cost"]) => {
                Kind::Settled {
                    attempt: attempt("attempt")??,
                    outcome: text("outcome")?,
                    candidate_sha256: match value.get("candidate_sha256")? {
                        Value::Null => None,
                        digest => Some(digest.as_str()?.to_owned()),
                    },
                    cost: match value.get("cost")? {
                        Value::Null => None,
                        cost => Some(Cost::from_json(cost)?),
                    },
                }
            }
            "replaced" if fields(&["attempt", "candidate_sha256", "receipt_sha256", "cost"]) => {
                Kind::Replaced {
                    attempt: attempt("attempt")??,
                    candidate_sha256: text("candidate_sha256")?,
                    receipt_sha256: text("receipt_sha256")?,
                    cost: Cost::from_json(value.get("cost")?)?,
                }
            }
            "closed" if fields(&["reason"]) => Kind::Closed {
                reason: text("reason")?,
            },
            _ => return None,
        };
        Some(Entry {
            sequence: json_u64(value, "sequence")?,
            previous: text("previous")?,
            digest: text("digest")?,
            kind,
        })
    }
}

impl Head {
    pub(crate) fn json(&self) -> Value {
        json!({"schema": LEDGER_SCHEMA, "entries": self.entries, "digest": self.digest})
    }

    pub(crate) fn from_json(value: &Value) -> Option<Head> {
        if !only_fields(value, &["schema", "entries", "digest"])
            || value.get("schema")?.as_str()? != LEDGER_SCHEMA
        {
            return None;
        }
        Some(Head {
            entries: json_u64(value, "entries")?,
            digest: value.get("digest")?.as_str()?.to_owned(),
        })
    }
}

impl LedgerFault {
    pub(crate) fn json(&self) -> Value {
        let (reason, line) = match self {
            LedgerFault::Unreadable { line } => ("ledger-unreadable", line),
            LedgerFault::Sequence { line } => ("ledger-sequence", line),
            LedgerFault::Link { line } => ("ledger-link", line),
            LedgerFault::Digest { line } => ("ledger-digest", line),
            LedgerFault::Opened { line } => ("ledger-opened", line),
            LedgerFault::AfterClose { line } => ("ledger-after-close", line),
        };
        json!({"reason": reason, "line": line})
    }
}
