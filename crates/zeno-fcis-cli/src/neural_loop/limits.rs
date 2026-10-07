//! Compiled resource ceilings and the request's effective limits (NSR-001).
//!
//! The effective limit of every item is the smaller of the compiled ceiling
//! and the reviewed request policy; a request asking for more is refused, never
//! clamped silently. Hosted spending has a zero ceiling in this build: raising
//! it is a code change that needs the owner's approved provider, disclosure
//! and billing policy (NSR-005).

use super::{json_u64, only_fields};
use serde_json::{Value, json};

/// Work-unit surcharge for constructing one receipt, charged before the check
/// alongside admission and node attempts (NSF-003).
pub(crate) const RECEIPT_WORK: u64 = 1_024;

/// Limits that bound one session. Every field is an exact count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Limits {
    /// Attempts, including malformed, empty, duplicate and failed proposals.
    pub(crate) attempts: u8,
    /// Complete candidate checks.
    pub(crate) checks: u8,
    /// Model calls, whichever provider answers them.
    pub(crate) model_calls: u8,
    /// Input tokens reserved per model call.
    pub(crate) input_tokens: u32,
    /// Output tokens reserved per model call.
    pub(crate) output_tokens: u32,
    /// Tokens reserved over the whole session.
    pub(crate) total_tokens: u32,
    /// Deterministic checker work units per check.
    pub(crate) check_work: u64,
    /// Checker work units per session, replays included.
    pub(crate) session_work: u64,
    /// Whole-session deadline in milliseconds; no stage starts after it.
    pub(crate) deadline_ms: u64,
    /// Largest artifact, in bytes.
    pub(crate) artifact_bytes: u32,
    /// Hosted spending allowance in micro-units of the billing currency.
    pub(crate) money_micros: u64,
    /// Retained transcript bytes (advisory history).
    pub(crate) transcript_bytes: u32,
    /// Retained request, artifact, receipt and witness bytes.
    pub(crate) storage_bytes: u32,
}

/// Why requested limits were refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LimitRefusal {
    /// A field exceeds its compiled ceiling.
    AboveCeiling {
        field: &'static str,
        requested: u64,
        ceiling: u64,
    },
    /// A count that must be positive is zero.
    Zero { field: &'static str },
    /// A per-call token figure exceeds the session total.
    TokensInconsistent,
    /// Per-check work exceeds the session work.
    WorkInconsistent,
}

impl Limits {
    /// DESIGN's initial ceilings, as compiled. NSR-001 forbids weaker ones.
    pub(crate) const CEILING: Limits = Limits {
        attempts: 8,
        checks: 8,
        model_calls: 4,
        input_tokens: 4_096,
        output_tokens: 2_048,
        total_tokens: 24_576,
        check_work: 1_000_000,
        session_work: 8_000_000,
        deadline_ms: 20_000,
        artifact_bytes: 64 * 1024,
        money_micros: 0,
        transcript_bytes: 512 * 1024,
        storage_bytes: 2 * 1024 * 1024,
    };

    /// Admits requested limits when every field is at most its ceiling and
    /// the counts are positive and mutually consistent.
    pub(crate) fn try_new(requested: Limits) -> Result<Limits, LimitRefusal> {
        let ceiling = Self::CEILING;
        let fields: [(&'static str, u64, u64); 13] = [
            (
                "attempts",
                requested.attempts.into(),
                ceiling.attempts.into(),
            ),
            ("checks", requested.checks.into(), ceiling.checks.into()),
            (
                "model_calls",
                requested.model_calls.into(),
                ceiling.model_calls.into(),
            ),
            (
                "input_tokens",
                requested.input_tokens.into(),
                ceiling.input_tokens.into(),
            ),
            (
                "output_tokens",
                requested.output_tokens.into(),
                ceiling.output_tokens.into(),
            ),
            (
                "total_tokens",
                requested.total_tokens.into(),
                ceiling.total_tokens.into(),
            ),
            ("check_work", requested.check_work, ceiling.check_work),
            ("session_work", requested.session_work, ceiling.session_work),
            ("deadline_ms", requested.deadline_ms, ceiling.deadline_ms),
            (
                "artifact_bytes",
                requested.artifact_bytes.into(),
                ceiling.artifact_bytes.into(),
            ),
            ("money_micros", requested.money_micros, ceiling.money_micros),
            (
                "transcript_bytes",
                requested.transcript_bytes.into(),
                ceiling.transcript_bytes.into(),
            ),
            (
                "storage_bytes",
                requested.storage_bytes.into(),
                ceiling.storage_bytes.into(),
            ),
        ];
        for (field, value, ceiling) in fields {
            if value > ceiling {
                return Err(LimitRefusal::AboveCeiling {
                    field,
                    requested: value,
                    ceiling,
                });
            }
        }
        for (field, value) in [
            ("attempts", u64::from(requested.attempts)),
            ("checks", u64::from(requested.checks)),
            ("check_work", requested.check_work),
            ("session_work", requested.session_work),
            ("deadline_ms", requested.deadline_ms),
            ("artifact_bytes", u64::from(requested.artifact_bytes)),
        ] {
            if value == 0 {
                return Err(LimitRefusal::Zero { field });
            }
        }
        if u64::from(requested.input_tokens) + u64::from(requested.output_tokens)
            > u64::from(requested.total_tokens)
        {
            return Err(LimitRefusal::TokensInconsistent);
        }
        if requested.check_work > requested.session_work {
            return Err(LimitRefusal::WorkInconsistent);
        }
        Ok(requested)
    }

    /// Tokens reserved by one model call: the worst case of both directions.
    pub(crate) fn call_tokens(&self) -> u64 {
        u64::from(self.input_tokens) + u64::from(self.output_tokens)
    }

    /// The limits as request JSON.
    pub(crate) fn json(&self) -> Value {
        json!({
            "attempts": self.attempts,
            "checks": self.checks,
            "model_calls": self.model_calls,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "total_tokens": self.total_tokens,
            "check_work": self.check_work,
            "session_work": self.session_work,
            "deadline_ms": self.deadline_ms,
            "artifact_bytes": self.artifact_bytes,
            "money_micros": self.money_micros,
            "transcript_bytes": self.transcript_bytes,
            "storage_bytes": self.storage_bytes,
        })
    }

    /// Reads limits back from request JSON. Every field is required, unknown
    /// fields are refused, and the result passes [`Limits::try_new`].
    pub(crate) fn from_json(value: &Value) -> Option<Limits> {
        if !only_fields(
            value,
            &[
                "attempts",
                "checks",
                "model_calls",
                "input_tokens",
                "output_tokens",
                "total_tokens",
                "check_work",
                "session_work",
                "deadline_ms",
                "artifact_bytes",
                "money_micros",
                "transcript_bytes",
                "storage_bytes",
            ],
        ) {
            return None;
        }
        let small = |field| u8::try_from(json_u64(value, field)?).ok();
        let medium = |field| u32::try_from(json_u64(value, field)?).ok();
        Limits::try_new(Limits {
            attempts: small("attempts")?,
            checks: small("checks")?,
            model_calls: small("model_calls")?,
            input_tokens: medium("input_tokens")?,
            output_tokens: medium("output_tokens")?,
            total_tokens: medium("total_tokens")?,
            check_work: json_u64(value, "check_work")?,
            session_work: json_u64(value, "session_work")?,
            deadline_ms: json_u64(value, "deadline_ms")?,
            artifact_bytes: medium("artifact_bytes")?,
            money_micros: json_u64(value, "money_micros")?,
            transcript_bytes: medium("transcript_bytes")?,
            storage_bytes: medium("storage_bytes")?,
        })
        .ok()
    }
}

impl LimitRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            LimitRefusal::AboveCeiling {
                field,
                requested,
                ceiling,
            } => json!({
                "reason": "limit-above-ceiling", "field": field,
                "requested": requested, "ceiling": ceiling
            }),
            LimitRefusal::Zero { field } => json!({"reason": "limit-zero", "field": field}),
            LimitRefusal::TokensInconsistent => json!({"reason": "limit-tokens-inconsistent"}),
            LimitRefusal::WorkInconsistent => json!({"reason": "limit-work-inconsistent"}),
        }
    }
}
