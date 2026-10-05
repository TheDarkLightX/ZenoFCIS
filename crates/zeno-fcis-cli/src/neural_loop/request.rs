//! The frozen canonical request `R` (NSC-001, NSC-004, NSC-006, NSC-007,
//! NSC-008) and the operator policy it binds (NSR-005, NSR-006, NSR-010).
//!
//! Admission freezes the exact original bytes, the complete ordered domain and
//! ABI derived from the admitted original, the semantic and enumeration
//! versions, the checker's identity, the cost objective, the limits and
//! the provider/disclosure policy in one canonical JSON record. `RequestId`
//! is a domain-separated commitment to that record. The record never replaces
//! exact binding: resume re-admits the bytes and recomputes the record.

use super::incumbent::Cost;
use super::limits::{LimitRefusal, Limits, RECEIPT_WORK};
use super::profiles::{Profile, ProfileRefusal};
use super::program_json::program_to_json;
use super::{canonical_json, json_u64, only_fields, tagged_sha256};
use crate::transform::{self, CHECKER, DEFAULT_STEP_LIMIT, ENUMERATION, sha256_hex};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{Error, PROFILE, Program, V2_EXECUTION_PROFILE, v2_authority};
use zeno_fcis_synthesis::finite_runtime::import_program;

pub(crate) const REQUEST_SCHEMA: &str = "zeno-fcis/transform-request/1";
pub(crate) const VIEW_SCHEMA: &str = "zeno-fcis/transform-request-view/1";
/// Objective V1: the lexicographic pair (instructions, bytes) with original
/// component guards (NSC-006, NSL-005).
pub(crate) const OBJECTIVE: &str = "lexicographic-nodes-bytes-v1";
const REQUEST_ID_TAG: &str = "zeno-fcis/transform-request-id/1";

/// The identity of the checker whose receipts this request binds, F3's
/// transform checker, as its receipts record it: the checker's semantics
/// version and the library's evaluator identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CheckerIdentity {
    pub(crate) semantics: String,
    pub(crate) evaluator_identity: String,
}

impl CheckerIdentity {
    /// The checker compiled into this binary.
    pub(crate) fn current() -> CheckerIdentity {
        CheckerIdentity {
            semantics: CHECKER.to_owned(),
            evaluator_identity: v2_authority::EVALUATOR
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        }
    }

    pub(crate) fn json(&self) -> Value {
        json!({
            "semantics": self.semantics,
            "evaluator_identity": self.evaluator_identity,
        })
    }

    pub(crate) fn from_json(value: &Value) -> Option<CheckerIdentity> {
        if !only_fields(value, &["semantics", "evaluator_identity"]) {
            return None;
        }
        let text = |field: &str| Some(value.get(field)?.as_str()?.to_owned());
        Some(CheckerIdentity {
            semantics: text("semantics")?,
            evaluator_identity: text("evaluator_identity")?,
        })
    }
}

/// Hosted-model mode. Only `Disabled` is admissible in this build (NSR-005).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HostedMode {
    Disabled,
    /// Named provider; requires an owner-approved disclosure and billing
    /// policy, which this build has none of.
    Enabled {
        provider: String,
    },
}

/// What the operator permits a proposer to see (NSM-003, NSR-010).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Disclosure {
    /// Only in-process proposers see the program; nothing leaves the host.
    Local,
    /// The original program may be sent to the named hosted provider.
    SourceToProvider,
}

/// Operator-authorized provider and disclosure policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Policy {
    pub(crate) hosted: HostedMode,
    pub(crate) disclosure: Disclosure,
    /// Monetary allowance for hosted calls, in micro-units.
    pub(crate) money_micros: u64,
}

/// Why a policy is refused before any dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PolicyRefusal {
    /// No hosted provider is approved in this build; the mode is unavailable.
    HostedUnavailable { provider: String },
    /// Source disclosure needs an enabled provider.
    DisclosureWithoutProvider,
    /// Spending needs an enabled provider with an enforceable ceiling.
    MoneyWithoutProvider,
    /// Spending above the request's limit.
    MoneyAboveLimit { requested: u64, limit: u64 },
}

impl Policy {
    /// Hosted disabled, local disclosure only, zero allowance.
    pub(crate) const DISABLED: Policy = Policy {
        hosted: HostedMode::Disabled,
        disclosure: Disclosure::Local,
        money_micros: 0,
    };

    fn admit(&self, limits: &Limits) -> Result<(), PolicyRefusal> {
        if let HostedMode::Enabled { provider } = &self.hosted {
            return Err(PolicyRefusal::HostedUnavailable {
                provider: provider.clone(),
            });
        }
        if self.disclosure != Disclosure::Local {
            return Err(PolicyRefusal::DisclosureWithoutProvider);
        }
        if self.money_micros > limits.money_micros {
            return Err(PolicyRefusal::MoneyAboveLimit {
                requested: self.money_micros,
                limit: limits.money_micros,
            });
        }
        if self.money_micros > 0 {
            return Err(PolicyRefusal::MoneyWithoutProvider);
        }
        Ok(())
    }

    pub(crate) fn json(&self) -> Value {
        json!({
            "hosted": match &self.hosted {
                HostedMode::Disabled => json!("disabled"),
                HostedMode::Enabled { provider } => json!({"provider": provider}),
            },
            "disclosure": match self.disclosure {
                Disclosure::Local => "local",
                Disclosure::SourceToProvider => "source-to-provider",
            },
            "money_micros": self.money_micros,
        })
    }

    pub(crate) fn from_json(value: &Value) -> Option<Policy> {
        if !only_fields(value, &["hosted", "disclosure", "money_micros"]) {
            return None;
        }
        let hosted = match value.get("hosted")? {
            Value::String(mode) if mode == "disabled" => HostedMode::Disabled,
            Value::Object(fields) if only_fields(&Value::Object(fields.clone()), &["provider"]) => {
                HostedMode::Enabled {
                    provider: fields.get("provider")?.as_str()?.to_owned(),
                }
            }
            _ => return None,
        };
        let disclosure = match value.get("disclosure")?.as_str()? {
            "local" => Disclosure::Local,
            "source-to-provider" => Disclosure::SourceToProvider,
            _ => return None,
        };
        Some(Policy {
            hosted,
            disclosure,
            money_micros: json_u64(value, "money_micros")?,
        })
    }
}

impl PolicyRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            PolicyRefusal::HostedUnavailable { provider } => json!({
                "reason": "hosted-provider-unavailable", "provider": provider,
                "message": "no hosted provider, disclosure policy or billing ceiling is approved in this build"
            }),
            PolicyRefusal::DisclosureWithoutProvider => {
                json!({"reason": "disclosure-without-provider"})
            }
            PolicyRefusal::MoneyWithoutProvider => json!({"reason": "money-without-provider"}),
            PolicyRefusal::MoneyAboveLimit { requested, limit } => {
                json!({"reason": "money-above-limit", "requested": requested, "limit": limit})
            }
        }
    }
}

/// Why the original could not become a request. An invalid original is a
/// refusal, never a fallback labeled checked.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RequestRefusal {
    Limits(LimitRefusal),
    Policy(PolicyRefusal),
    /// Bounded byte intake refused the artifact before hashing or decoding.
    ArtifactTooLarge {
        bytes: usize,
        max: u32,
    },
    /// The library importer refused the canonical bytes.
    NotAdmitted {
        code: String,
    },
    Profile(ProfileRefusal),
    /// Even the original's self-check exceeds the per-check work limit.
    WorkInfeasible {
        needed: Option<u64>,
        limit: u64,
    },
}

impl RequestRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            RequestRefusal::Limits(refusal) => refusal.json(),
            RequestRefusal::Policy(refusal) => refusal.json(),
            RequestRefusal::ArtifactTooLarge { bytes, max } => {
                json!({"reason": "program-too-large", "bytes": bytes, "max_bytes": max})
            }
            RequestRefusal::NotAdmitted { code } => {
                json!({"reason": "original-not-admitted", "code": code})
            }
            RequestRefusal::Profile(refusal) => refusal.json(),
            RequestRefusal::WorkInfeasible { needed, limit } => {
                json!({"reason": "work-infeasible", "needed": needed, "check_work": limit})
            }
        }
    }
}

/// An admitted, immutable request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Request {
    original: Vec<u8>,
    original_sha256: String,
    program: Program,
    profile: Profile,
    limits: Limits,
    policy: Policy,
    checker: CheckerIdentity,
    domain_size: u64,
    cost: Cost,
    canonical: Vec<u8>,
    id: String,
}

impl Request {
    /// Admits an original. Precedence (NSC "Precedence and scenarios"): limits
    /// and policy, then bounded byte intake, canonical decoding with the
    /// library's complete admission, the profile's node and schema checks,
    /// and last the checked cardinality and work feasibility.
    pub(crate) fn admit(
        original: &[u8],
        profile: Profile,
        limits: Limits,
        policy: Policy,
    ) -> Result<Request, RequestRefusal> {
        let limits = Limits::try_new(limits).map_err(RequestRefusal::Limits)?;
        policy.admit(&limits).map_err(RequestRefusal::Policy)?;
        if original.len() > limits.artifact_bytes as usize {
            return Err(RequestRefusal::ArtifactTooLarge {
                bytes: original.len(),
                max: limits.artifact_bytes,
            });
        }
        let program = import_program(original).map_err(|error| RequestRefusal::NotAdmitted {
            code: admission_code(&error).to_owned(),
        })?;
        let domain_size = profile.admit(&program).map_err(RequestRefusal::Profile)?;
        let cost = Cost::measure(&program, original);
        let needed = check_work(domain_size, cost, cost);
        if needed.is_none_or(|needed| needed > limits.check_work) {
            return Err(RequestRefusal::WorkInfeasible {
                needed,
                limit: limits.check_work,
            });
        }
        let checker = CheckerIdentity::current();
        let original_sha256 = sha256_hex(original);
        let record = request_json(
            &original_sha256,
            cost,
            &program,
            profile,
            domain_size,
            &limits,
            &policy,
            &checker,
        );
        let canonical = canonical_json(&record);
        let id = tagged_sha256(REQUEST_ID_TAG, &canonical);
        Ok(Request {
            original: original.to_vec(),
            original_sha256,
            program,
            profile,
            limits,
            policy,
            checker,
            domain_size,
            cost,
            canonical,
            id,
        })
    }

    /// Re-admits a persisted request: the stored canonical record must be
    /// readable, and admitting the stored original bytes under the stored
    /// profile, limits and policy must reproduce it exactly, checker identity
    /// included (NSR-007).
    pub(crate) fn readmit(stored: &[u8], original: &[u8]) -> Result<Request, ReadmitRefusal> {
        let record: Value =
            serde_json::from_slice(stored).map_err(|_| ReadmitRefusal::Unreadable)?;
        if record.get("schema").and_then(Value::as_str) != Some(REQUEST_SCHEMA) {
            return Err(ReadmitRefusal::Unreadable);
        }
        let profile = record
            .get("profile")
            .and_then(Value::as_str)
            .and_then(Profile::parse)
            .ok_or(ReadmitRefusal::Unreadable)?;
        let limits = record
            .get("limits")
            .and_then(Limits::from_json)
            .ok_or(ReadmitRefusal::Unreadable)?;
        let policy = record
            .get("policy")
            .and_then(Policy::from_json)
            .ok_or(ReadmitRefusal::Unreadable)?;
        let stored_checker = record
            .get("checker")
            .and_then(CheckerIdentity::from_json)
            .ok_or(ReadmitRefusal::Unreadable)?;
        let current = CheckerIdentity::current();
        if stored_checker != current {
            return Err(ReadmitRefusal::StaleChecker {
                stored: Box::new(stored_checker),
                current: Box::new(current),
            });
        }
        let request =
            Request::admit(original, profile, limits, policy).map_err(ReadmitRefusal::Admission)?;
        if request.canonical != stored {
            return Err(ReadmitRefusal::Mismatch);
        }
        Ok(request)
    }

    /// The exact original bytes.
    pub(crate) fn original(&self) -> &[u8] {
        &self.original
    }

    pub(crate) fn original_sha256(&self) -> &str {
        &self.original_sha256
    }

    /// The admitted original program.
    pub(crate) fn program(&self) -> &Program {
        &self.program
    }

    pub(crate) fn profile(&self) -> Profile {
        self.profile
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.limits
    }

    pub(crate) fn policy(&self) -> &Policy {
        &self.policy
    }

    pub(crate) fn checker(&self) -> &CheckerIdentity {
        &self.checker
    }

    /// Cardinality of the complete ordered input domain.
    pub(crate) fn domain_size(&self) -> u64 {
        self.domain_size
    }

    /// The original's actual cost.
    pub(crate) fn cost(&self) -> Cost {
        self.cost
    }

    /// The canonical request record.
    pub(crate) fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// `RequestId`: the domain-separated commitment to the canonical record.
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// The checker limits every check under this request uses: the Step limit
    /// that never binds and the exact domain size as tuple cap.
    pub(crate) fn transform_limits(&self) -> transform::Limits {
        transform::Limits {
            steps: DEFAULT_STEP_LIMIT,
            input_tuples: self.domain_size,
        }
    }

    /// Deterministic work units for checking one candidate against the
    /// original (NSF-003); `None` on arithmetic overflow.
    pub(crate) fn check_work(&self, candidate: Cost) -> Option<u64> {
        check_work(self.domain_size, self.cost, candidate)
    }

    /// The view a proposer receives (NSM-003): policy-permitted request data
    /// only. The program itself is included only for local disclosure or when
    /// the policy permits sending it to the provider; nothing here is
    /// assurance, and the instructions say so.
    pub(crate) fn view(&self, to_hosted_provider: bool) -> Value {
        let disclose =
            !to_hosted_provider || self.policy.disclosure == Disclosure::SourceToProvider;
        let mut original = json!({
            "sha256": self.original_sha256,
            "bytes": self.cost.bytes,
            "nodes": self.cost.nodes,
        });
        if disclose {
            original["program"] = program_to_json(&self.program);
        } else {
            original["program"] = json!("withheld-by-disclosure-policy");
        }
        json!({
            "schema": VIEW_SCHEMA,
            "request_id": self.id,
            "profile": self.profile.name(),
            "semantics": {"program": PROFILE, "execution": V2_EXECUTION_PROFILE},
            "domain": {"enumeration": ENUMERATION, "size": self.domain_size},
            "objective": OBJECTIVE,
            "original": original,
            "limits": {
                "attempts": self.limits.attempts,
                "checks": self.limits.checks,
                "artifact_bytes": self.limits.artifact_bytes,
                "check_work": self.limits.check_work,
            },
            "instructions": [
                "Propose a complete program with exactly the original's input and output domains, in the fixture vocabulary (inputs, outputs, nodes, roots).",
                "The request cannot be changed: a different domain, ABI or profile is refused.",
                "Every proposal is checked against the original on every input tuple; a selection needs fewer stored nodes or fewer canonical bytes, neither larger than the original's.",
                "Your explanations are advisory. Feedback below is checker-derived and replayed; the checker never trusts a claimed result."
            ],
        })
    }

    /// The canonical request record as JSON.
    pub(crate) fn json(&self) -> Value {
        serde_json::from_slice(&self.canonical).unwrap_or(Value::Null)
    }
}

/// Why a stored request could not be re-admitted. None of these yields a
/// trusted incumbent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReadmitRefusal {
    /// Not a request record of this schema with readable fields.
    Unreadable,
    /// The record was written by a different checker (NSF-007: stale checker
    /// identity prevents reuse, it never migrates evidence).
    StaleChecker {
        stored: Box<CheckerIdentity>,
        current: Box<CheckerIdentity>,
    },
    /// Fresh admission of the stored bytes refused.
    Admission(RequestRefusal),
    /// Fresh admission reproduces a different canonical record.
    Mismatch,
}

impl ReadmitRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            ReadmitRefusal::Unreadable => json!({"reason": "request-unreadable"}),
            ReadmitRefusal::StaleChecker { stored, current } => json!({
                "reason": "stale-checker", "stored": stored.json(), "current": current.json()
            }),
            ReadmitRefusal::Admission(refusal) => {
                json!({"reason": "request-readmission-refused", "refusal": refusal.json()})
            }
            ReadmitRefusal::Mismatch => json!({"reason": "request-mismatch"}),
        }
    }
}

/// `admission + tuples * (node attempts + comparison) + receipt`, all
/// deterministic. Each program attempts at most its node count of Steps per
/// tuple, so this bounds the evaluator's Step usage from above.
fn check_work(domain_size: u64, original: Cost, candidate: Cost) -> Option<u64> {
    let per_tuple = original
        .nodes
        .checked_add(candidate.nodes)?
        .checked_add(2)?;
    original
        .bytes
        .checked_add(candidate.bytes)?
        .checked_add(domain_size.checked_mul(per_tuple)?)?
        .checked_add(RECEIPT_WORK)
}

fn admission_code(error: &Error) -> &'static str {
    match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    }
}

#[allow(clippy::too_many_arguments)]
fn request_json(
    original_sha256: &str,
    cost: Cost,
    program: &Program,
    profile: Profile,
    domain_size: u64,
    limits: &Limits,
    policy: &Policy,
    checker: &CheckerIdentity,
) -> Value {
    json!({
        "schema": REQUEST_SCHEMA,
        "profile": profile.name(),
        "semantics": {"program": PROFILE, "execution": V2_EXECUTION_PROFILE},
        "original": {"sha256": original_sha256, "bytes": cost.bytes, "nodes": cost.nodes},
        "domain": {
            "enumeration": ENUMERATION,
            "inputs": transform::domains_json(program.inputs()),
            "size": domain_size,
        },
        "outputs": transform::domains_json(program.outputs()),
        "objective": OBJECTIVE,
        "limits": limits.json(),
        "policy": policy.json(),
        "checker": checker.json(),
    })
}
