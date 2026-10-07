//! The closed strategy DSL and the engine seam (NSL-003, NSM-004, NSM-005).
//!
//! A strategy is data in F4's grammar `zeno-fcis/optimize-strategy/1`: an
//! ordered list of named phases with bounded rounds, optional numeric limits,
//! one of a fixed set of extractors and an optional artifact profile. It
//! selects behavior of an already qualified fixed interpreter; it is never
//! code. This module admits the grammar's shape and bounds; the engine behind
//! [`StrategyEngine`] (F4's e-graph optimizer) owns the phase table and the
//! semantics; the shell wires that optimizer (`loop_command::OptimizeEngine`)
//! and passes it the request's profile. Whatever engine runs, the candidate
//! bytes it emits re-enter the ordinary admission and `transform::check` path;
//! no engine verdict is trusted.

use serde_json::{Value, json};

use super::profiles::Profile;

/// F4's strategy schema; the loop accepts the same document.
pub(crate) const STRATEGY_SCHEMA: &str = "zeno-fcis/optimize-strategy/1";
pub(crate) const MAX_PHASES: usize = 16;
pub(crate) const MAX_ROUNDS: u32 = 8;
/// Largest value any limit may take.
pub(crate) const MAX_LIMIT: u32 = 100_000;
pub(crate) const MAX_PHASE_NAME_BYTES: usize = 32;
const LIMIT_FIELDS: [&str; 5] = [
    "max_enodes",
    "max_classes",
    "max_rewrites_per_round",
    "max_extraction_rounds",
    "max_work",
];

/// A phase name from the engine's fixed table. The grammar only bounds its
/// alphabet and length; the engine decides whether it names a phase.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PhaseId(String);

impl PhaseId {
    /// `[a-z-]{1,32}`, so a name can never carry code or paths.
    pub(crate) fn parse(text: &str) -> Option<PhaseId> {
        let well_formed = !text.is_empty()
            && text.len() <= MAX_PHASE_NAME_BYTES
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-');
        well_formed.then(|| PhaseId(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One phase and its round count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Phase {
    pub(crate) name: PhaseId,
    pub(crate) rounds: u32,
}

/// F4's fixed extractors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Extractor {
    Tree,
    DagGreedy,
}

impl Extractor {
    fn name(self) -> &'static str {
        match self {
            Extractor::Tree => "tree",
            Extractor::DagGreedy => "dag-greedy",
        }
    }
}

/// Optional size and work limits, each at most [`MAX_LIMIT`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct SearchLimits {
    pub(crate) max_enodes: Option<u32>,
    pub(crate) max_classes: Option<u32>,
    pub(crate) max_rewrites_per_round: Option<u32>,
    pub(crate) max_extraction_rounds: Option<u32>,
    pub(crate) max_work: Option<u32>,
}

/// A validated strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Strategy {
    pub(crate) phases: Vec<Phase>,
    pub(crate) limits: Option<SearchLimits>,
    pub(crate) extractor: Extractor,
    /// The artifact profile the search must stay within, when named.
    pub(crate) profile: Option<Profile>,
}

/// Why strategy data is outside the grammar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StrategyRefusal {
    Shape,
    Schema,
    Phases {
        count: usize,
    },
    PhaseName {
        index: usize,
    },
    Rounds {
        index: usize,
        value: u64,
    },
    Limits,
    Limit {
        field: &'static str,
        value: u64,
    },
    Extractor,
    /// `profile` is not the name of a known profile.
    Profile,
}

impl Strategy {
    /// Strict reader: exact fields, bounded counts and values, closed names.
    pub(crate) fn from_json(value: &Value) -> Result<Strategy, StrategyRefusal> {
        if !super::only_fields(
            value,
            &["schema", "phases", "limits", "extractor", "profile"],
        ) {
            return Err(StrategyRefusal::Shape);
        }
        if value.get("schema").and_then(Value::as_str) != Some(STRATEGY_SCHEMA) {
            return Err(StrategyRefusal::Schema);
        }
        let phases = value
            .get("phases")
            .and_then(Value::as_array)
            .ok_or(StrategyRefusal::Shape)?;
        if phases.is_empty() || phases.len() > MAX_PHASES {
            return Err(StrategyRefusal::Phases {
                count: phases.len(),
            });
        }
        let phases = phases
            .iter()
            .enumerate()
            .map(|(index, value)| {
                if !super::only_fields(value, &["phase", "rounds"]) {
                    return Err(StrategyRefusal::Shape);
                }
                let name = value
                    .get("phase")
                    .and_then(Value::as_str)
                    .and_then(PhaseId::parse)
                    .ok_or(StrategyRefusal::PhaseName { index })?;
                let rounds = value
                    .get("rounds")
                    .and_then(Value::as_u64)
                    .ok_or(StrategyRefusal::Shape)?;
                let rounds = u32::try_from(rounds)
                    .ok()
                    .filter(|count| (1..=MAX_ROUNDS).contains(count))
                    .ok_or(StrategyRefusal::Rounds {
                        index,
                        value: rounds,
                    })?;
                Ok(Phase { name, rounds })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let limits = match value.get("limits") {
            None => None,
            Some(limits) => {
                if !super::only_fields(limits, &LIMIT_FIELDS) {
                    return Err(StrategyRefusal::Limits);
                }
                let field = |name: &'static str| -> Result<Option<u32>, StrategyRefusal> {
                    match limits.get(name) {
                        None => Ok(None),
                        Some(value) => {
                            let raw = value.as_u64().ok_or(StrategyRefusal::Shape)?;
                            u32::try_from(raw)
                                .ok()
                                .filter(|value| *value <= MAX_LIMIT)
                                .map(Some)
                                .ok_or(StrategyRefusal::Limit {
                                    field: name,
                                    value: raw,
                                })
                        }
                    }
                };
                Some(SearchLimits {
                    max_enodes: field("max_enodes")?,
                    max_classes: field("max_classes")?,
                    max_rewrites_per_round: field("max_rewrites_per_round")?,
                    max_extraction_rounds: field("max_extraction_rounds")?,
                    max_work: field("max_work")?,
                })
            }
        };
        let extractor = match value.get("extractor").and_then(Value::as_str) {
            Some("tree") => Extractor::Tree,
            Some("dag-greedy") => Extractor::DagGreedy,
            _ => return Err(StrategyRefusal::Extractor),
        };
        let profile = match value.get("profile") {
            None => None,
            Some(name) => Some(
                name.as_str()
                    .and_then(Profile::parse)
                    .ok_or(StrategyRefusal::Profile)?,
            ),
        };
        Ok(Strategy {
            phases,
            limits,
            extractor,
            profile,
        })
    }

    /// The strategy as the engine's document.
    pub(crate) fn json(&self) -> Value {
        let mut document = json!({
            "schema": STRATEGY_SCHEMA,
            "phases": self.phases.iter().map(|phase| json!({
                "phase": phase.name.as_str(),
                "rounds": phase.rounds,
            })).collect::<Vec<_>>(),
            "extractor": self.extractor.name(),
        });
        if let Some(limits) = &self.limits {
            let mut fields = serde_json::Map::new();
            for (name, value) in [
                ("max_enodes", limits.max_enodes),
                ("max_classes", limits.max_classes),
                ("max_rewrites_per_round", limits.max_rewrites_per_round),
                ("max_extraction_rounds", limits.max_extraction_rounds),
                ("max_work", limits.max_work),
            ] {
                if let Some(value) = value {
                    fields.insert(name.to_owned(), json!(value));
                }
            }
            document["limits"] = Value::Object(fields);
        }
        if let Some(profile) = self.profile {
            document["profile"] = json!(profile.name());
        }
        document
    }

    /// The phase names, in order.
    pub(crate) fn phase_names(&self) -> impl Iterator<Item = &str> {
        self.phases.iter().map(|phase| phase.name.as_str())
    }
}

impl StrategyRefusal {
    pub(crate) fn json(&self) -> Value {
        match self {
            StrategyRefusal::Shape => json!({"reason": "strategy-shape"}),
            StrategyRefusal::Schema => {
                json!({"reason": "strategy-schema", "expected": STRATEGY_SCHEMA})
            }
            StrategyRefusal::Phases { count } => {
                json!({"reason": "strategy-phases", "count": count, "max": MAX_PHASES})
            }
            StrategyRefusal::PhaseName { index } => {
                json!({"reason": "strategy-phase-name", "index": index})
            }
            StrategyRefusal::Rounds { index, value } => json!({
                "reason": "strategy-rounds", "index": index, "value": value, "max": MAX_ROUNDS
            }),
            StrategyRefusal::Limits => json!({"reason": "strategy-limits"}),
            StrategyRefusal::Limit { field, value } => json!({
                "reason": "strategy-limit", "field": field, "value": value, "max": MAX_LIMIT
            }),
            StrategyRefusal::Extractor => json!({"reason": "strategy-extractor"}),
            StrategyRefusal::Profile => json!({"reason": "strategy-profile"}),
        }
    }
}

/// What a bounded search worker reports back to the loop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SearchReport {
    /// A complete candidate artifact, with the engine's own extraction status
    /// retained as provenance (unknown optimality is allowed; partial output
    /// is not a candidate). Only an engine constructs it.
    Candidate { bytes: Vec<u8>, extraction: String },
    /// No qualified engine, or unsupported enforcement, for this strategy.
    Unavailable { reason: String },
    /// The engine ran and failed, timed out or produced no decodable output.
    Failed { reason: String },
}

/// The seam F4's optimizer implements. The shell runs `run` under the search
/// worker's deadline (NSR-002); the loop never calls it. Wiring F4 is one
/// `impl`: parse `strategy.json()` with the optimizer's own strict parser,
/// call its optimizer on the original bytes under the request's profile with
/// the loop's checked candidates, and map its best extracted candidate to
/// `SearchReport::Candidate` (any other outcome to `Failed`, or
/// `Unavailable` when the strategy names another profile).
pub(crate) trait StrategyEngine {
    /// Engine identity for provenance (never assurance).
    fn identity(&self) -> &str;
    /// The fixed phase table the grammar's phase names are checked against.
    fn phases(&self) -> &[&str];
    /// Runs one validated strategy on the original's exact bytes, proposing
    /// only programs within the request's `profile`. `checked` holds the
    /// loop's checked replacement, if any, as canonical bytes; an engine may
    /// fuse it into its search but must not trust it.
    fn run(
        &self,
        original: &[u8],
        strategy: &Strategy,
        profile: Profile,
        checked: &[Vec<u8>],
    ) -> SearchReport;
}

/// An engine that runs nothing: every strategy is unavailable. Tests use it to
/// pin the unavailable path; the shell wires F4's optimizer.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoEngine;

#[cfg(test)]
impl StrategyEngine for NoEngine {
    fn identity(&self) -> &str {
        "none"
    }

    fn phases(&self) -> &[&str] {
        &[]
    }

    fn run(
        &self,
        _original: &[u8],
        _strategy: &Strategy,
        _profile: Profile,
        _checked: &[Vec<u8>],
    ) -> SearchReport {
        SearchReport::Unavailable {
            reason: String::from("strategy mode is unavailable: this engine runs nothing"),
        }
    }
}
