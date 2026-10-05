//! Proposers for `zeno-fcis loop run`: the deterministic local rewriter, the
//! scripted fake provider used by tests and the evaluation protocol, and the
//! hosted-model adapter, which is disabled in this build (NSR-005, NSM-003).
//!
//! A proposer only produces [`Proposal`] data plus untrusted provenance. The
//! loop core admits and checks whatever it says; a `claims` object in a fake
//! script, or any provider-asserted field, is recorded as provenance and never
//! consulted (NSM-002: no `passed` flag is accepted).

use crate::neural_loop::local::{self, LocalProposer};
use crate::neural_loop::program_json::{encode, program_from_json};
use crate::neural_loop::session::{Proposal, ProposerFailure};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use zeno_fcis_synthesis::finite::Program;

pub(crate) const SCRIPT_SCHEMA: &str = "zeno-fcis/fake-provider-script/1";
pub(crate) const HOSTED_CONFIG_SCHEMA: &str = "zeno-fcis/hosted-provider-config/1";
/// Largest fake script or hosted configuration file.
pub(crate) const CONFIG_LIMIT: u64 = 256 * 1024;

/// One proposal with the provenance the proposer attached to it.
pub(crate) struct Proposed {
    pub(crate) proposal: Proposal,
    /// Untrusted: provider, prompt and response digests, asserted fields.
    pub(crate) provenance: Value,
}

/// What the shell needs from a proposer.
pub(crate) trait Proposer {
    /// Identity for provenance.
    fn identity(&self) -> &str;
    /// Whether an attempt reserves a model call and its tokens first.
    fn uses_model_calls(&self) -> bool;
    /// Whether the request view must follow the hosted disclosure policy.
    fn is_hosted(&self) -> bool;
    /// Whether the proposer has nothing further to propose; checked before an
    /// attempt is reserved so a deterministic proposer stops honestly.
    fn exhausted(&self) -> bool;
    /// Produces the next proposal from the policy-permitted view and the
    /// checker-derived feedback of earlier attempts.
    fn propose(&mut self, view: &Value, feedback: &[Value]) -> Proposed;
}

/// The deterministic local rewriter over the finite IR.
pub(crate) struct Local {
    proposer: LocalProposer,
}

impl Local {
    pub(crate) fn new(program: &Program, original: &[u8]) -> Local {
        Local {
            proposer: LocalProposer::new(program, original),
        }
    }
}

impl Proposer for Local {
    fn identity(&self) -> &str {
        local::IDENTITY
    }

    fn uses_model_calls(&self) -> bool {
        false
    }

    fn is_hosted(&self) -> bool {
        false
    }

    fn exhausted(&self) -> bool {
        self.proposer.remaining() == 0
    }

    fn propose(&mut self, _view: &Value, _feedback: &[Value]) -> Proposed {
        let proposal = match self.proposer.next() {
            Some(bytes) => Proposal::Candidate(bytes),
            None => Proposal::Failed(ProposerFailure::Exhausted),
        };
        Proposed {
            proposal,
            provenance: json!({
                "proposer": local::IDENTITY,
                "remaining": self.proposer.remaining(),
                "feedback_used": false,
            }),
        }
    }
}

/// One scripted answer of the fake provider.
#[derive(Clone, Debug)]
enum Step {
    Candidate(PathBuf),
    CandidateJson(PathBuf),
    Malformed(Vec<u8>),
    Strategy(Value),
    Timeout,
    Late,
    Empty,
}

/// A fake provider replaying a fixed script. It stands in for a model: each
/// answer costs a model call and its worst-case tokens.
pub(crate) struct Fake {
    steps: Vec<(Step, Value)>,
    next: usize,
}

/// Why a script was refused.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptError {
    Schema,
    Step { index: usize },
}

impl Fake {
    /// Reads a script. Paths inside it resolve against `base`. Any `claims`
    /// object in a step becomes provenance only.
    pub(crate) fn from_script(script: &Value, base: &Path) -> Result<Fake, ScriptError> {
        if script.get("schema").and_then(Value::as_str) != Some(SCRIPT_SCHEMA) {
            return Err(ScriptError::Schema);
        }
        let steps = script
            .get("steps")
            .and_then(Value::as_array)
            .ok_or(ScriptError::Schema)?;
        let steps = steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let error = ScriptError::Step { index };
                let fields = step.as_object().ok_or(error.clone())?;
                let claims = fields.get("claims").cloned().unwrap_or(Value::Null);
                let kind = fields.keys().filter(|key| *key != "claims").count();
                if kind != 1 {
                    return Err(error);
                }
                let path = |value: &Value| -> Result<PathBuf, ScriptError> {
                    let text = value.as_str().ok_or(error.clone())?;
                    Ok(base.join(text))
                };
                let parsed = if let Some(value) = fields.get("candidate") {
                    Step::Candidate(path(value)?)
                } else if let Some(value) = fields.get("candidate_json") {
                    Step::CandidateJson(path(value)?)
                } else if let Some(value) = fields.get("malformed") {
                    Step::Malformed(value.as_str().ok_or(error.clone())?.as_bytes().to_vec())
                } else if let Some(value) = fields.get("strategy") {
                    Step::Strategy(value.clone())
                } else if fields.get("timeout").and_then(Value::as_bool) == Some(true) {
                    Step::Timeout
                } else if fields.get("late").and_then(Value::as_bool) == Some(true) {
                    Step::Late
                } else if fields.get("empty").and_then(Value::as_bool) == Some(true) {
                    Step::Empty
                } else {
                    return Err(error);
                };
                Ok((parsed, claims))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Fake { steps, next: 0 })
    }
}

impl Proposer for Fake {
    fn identity(&self) -> &str {
        "zeno-fcis/fake-provider/1"
    }

    fn uses_model_calls(&self) -> bool {
        true
    }

    fn is_hosted(&self) -> bool {
        false
    }

    fn exhausted(&self) -> bool {
        self.next >= self.steps.len()
    }

    fn propose(&mut self, _view: &Value, feedback: &[Value]) -> Proposed {
        let Some((step, claims)) = self.steps.get(self.next).cloned() else {
            return Proposed {
                proposal: Proposal::Failed(ProposerFailure::Exhausted),
                provenance: json!({"provider": self.identity(), "script_exhausted": true}),
            };
        };
        self.next += 1;
        let (proposal, kind) = match step {
            Step::Candidate(path) => match std::fs::read(&path) {
                Ok(bytes) => (Proposal::Candidate(bytes), "candidate"),
                Err(error) => (
                    Proposal::Failed(ProposerFailure::Malformed(format!(
                        "script candidate unreadable: {error}"
                    ))),
                    "candidate-unreadable",
                ),
            },
            Step::CandidateJson(path) => {
                let encoded = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .and_then(|value| program_from_json(&value).ok())
                    .and_then(|program| encode(&program).ok());
                match encoded {
                    Some(bytes) => (Proposal::Candidate(bytes), "candidate-json"),
                    None => (
                        Proposal::Failed(ProposerFailure::Malformed(
                            "script program JSON did not admit".to_owned(),
                        )),
                        "candidate-json-invalid",
                    ),
                }
            }
            Step::Malformed(bytes) => (Proposal::Candidate(bytes), "malformed"),
            Step::Strategy(value) => (Proposal::Strategy(value), "strategy"),
            Step::Timeout => (Proposal::Failed(ProposerFailure::Timeout), "timeout"),
            // The answer arrived after the stage closed: it is dropped, the
            // call stays charged (NSL-007, NSR-005).
            Step::Late => (
                Proposal::Failed(ProposerFailure::Timeout),
                "late-response-dropped",
            ),
            Step::Empty => (Proposal::Failed(ProposerFailure::Empty), "empty"),
        };
        Proposed {
            proposal,
            provenance: json!({
                "provider": self.identity(),
                "step": self.next - 1,
                "kind": kind,
                "feedback_seen": feedback.len(),
                // Whatever the script asserts about itself is provenance.
                "provider_asserted": claims,
                "tokens": "not-reported",
            }),
        }
    }
}

/// Operator configuration for a hosted model provider. In this build the
/// adapter exists so the wiring is explicit, and it is disabled: no provider,
/// disclosure policy or billing ceiling is approved, so every proposal is
/// unavailable before any network, credential or spending path exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HostedConfig {
    pub(crate) enabled: bool,
    pub(crate) provider: Option<String>,
    /// Name of the environment variable holding the credential. Never read in
    /// this build; never written to provenance (NSR-010).
    pub(crate) credential_env: Option<String>,
    pub(crate) money_micros: u64,
}

impl HostedConfig {
    pub(crate) const DISABLED: HostedConfig = HostedConfig {
        enabled: false,
        provider: None,
        credential_env: None,
        money_micros: 0,
    };

    pub(crate) fn from_json(value: &Value) -> Option<HostedConfig> {
        if value.get("schema")?.as_str()? != HOSTED_CONFIG_SCHEMA
            || !crate::neural_loop::only_fields(
                value,
                &[
                    "schema",
                    "enabled",
                    "provider",
                    "credential_env",
                    "money_micros",
                ],
            )
        {
            return None;
        }
        let text = |field: &str| match value.get(field) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(text)) => Some(Some(text.clone())),
            _ => None,
        };
        Some(HostedConfig {
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            provider: text("provider")?,
            credential_env: text("credential_env")?,
            money_micros: value
                .get("money_micros")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
    }
}

/// The hosted adapter. It never reaches a network in this build.
pub(crate) struct Hosted {
    config: HostedConfig,
    answered: bool,
}

impl Hosted {
    pub(crate) fn new(config: HostedConfig) -> Hosted {
        Hosted {
            config,
            answered: false,
        }
    }

    /// Why the adapter is unavailable, as a stable reason.
    pub(crate) fn unavailable_reason(&self) -> &'static str {
        if !self.config.enabled {
            "hosted-provider-disabled"
        } else {
            // An enabled configuration still has no approved provider,
            // disclosure policy, tariff or enforceable spend ceiling here.
            "hosted-provider-unapproved"
        }
    }
}

impl Proposer for Hosted {
    fn identity(&self) -> &str {
        "zeno-fcis/hosted-provider-adapter/1"
    }

    fn uses_model_calls(&self) -> bool {
        true
    }

    fn is_hosted(&self) -> bool {
        true
    }

    fn exhausted(&self) -> bool {
        // One unavailable answer is enough; the attempt it cost is recorded.
        self.answered
    }

    fn propose(&mut self, _view: &Value, _feedback: &[Value]) -> Proposed {
        self.answered = true;
        Proposed {
            proposal: Proposal::Failed(ProposerFailure::Unavailable(
                self.unavailable_reason().to_owned(),
            )),
            provenance: json!({
                "provider": self.identity(),
                "configured_provider": self.config.provider,
                "enabled": self.config.enabled,
                "spend_ceiling_micros": self.config.money_micros,
                "credential": "never-read",
                "network": "none",
            }),
        }
    }
}
