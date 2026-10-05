//! The strategy document: a small, versioned, closed JSON grammar that selects
//! phases, rounds, limits and the extractor. Unknown keys, duplicate keys,
//! wrong types and out-of-range values are refused. No user-supplied code runs.
//!
//! ```json
//! {
//!   "schema": "zeno-fcis/optimize-strategy/1",
//!   "phases": [{"phase": "boolean", "rounds": 3}],
//!   "limits": {"max_enodes": 20000, "max_classes": 10000,
//!              "max_rewrites_per_round": 10000, "max_extraction_rounds": 8},
//!   "extractor": "dag-greedy"
//! }
//! ```

use serde::Deserialize;

use super::egraph::Caps;

/// Version of the strategy grammar.
pub(crate) const STRATEGY_SCHEMA: &str = "zeno-fcis/optimize-strategy/1";
/// Largest number of phases in one strategy.
pub(crate) const MAX_PHASES: usize = 16;
/// Largest number of rounds in one phase.
pub(crate) const MAX_ROUNDS: u32 = 8;
/// Largest value any limit may take.
pub(crate) const MAX_LIMIT: u32 = 100_000;

/// The fixed default strategy, version 1. Every phase is followed by an
/// extraction and a check, so an early phase's candidate survives a later
/// phase that finds nothing better.
pub(crate) const DEFAULT_STRATEGY_JSON: &str = r#"{
  "schema": "zeno-fcis/optimize-strategy/1",
  "phases": [
    {"phase": "fold", "rounds": 1},
    {"phase": "share", "rounds": 1},
    {"phase": "boolean", "rounds": 3},
    {"phase": "select", "rounds": 3},
    {"phase": "semantic-merge", "rounds": 2},
    {"phase": "boolean", "rounds": 2},
    {"phase": "select", "rounds": 2},
    {"phase": "fold", "rounds": 1}
  ],
  "limits": {
    "max_enodes": 20000,
    "max_classes": 10000,
    "max_rewrites_per_round": 10000,
    "max_extraction_rounds": 8
  },
  "extractor": "dag-greedy"
}
"#;

/// The closed set of phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
pub(crate) enum Phase {
    /// Algebraic rewrites over And, Not, Eq, Lt and the Or form `Select(a, a, b)`.
    #[serde(rename = "boolean")]
    Boolean,
    /// Merge classes with identical exact signatures and add single
    /// instructions over existing classes whose signature an existing class
    /// already has. Only with at most 64 input tuples.
    #[serde(rename = "semantic-merge")]
    SemanticMerge,
    /// Select simplification.
    #[serde(rename = "select")]
    Select,
    /// Constant folding; never removes a possible trap.
    #[serde(rename = "fold")]
    Fold,
    /// Common-subexpression sharing modulo commutativity of And and Eq.
    #[serde(rename = "share")]
    Share,
}

impl Phase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::SemanticMerge => "semantic-merge",
            Self::Select => "select",
            Self::Fold => "fold",
            Self::Share => "share",
        }
    }
}

/// The closed set of extractors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
pub(crate) enum Extractor {
    /// Cheapest tree per class, bottom up; the reference extractor.
    #[serde(rename = "tree")]
    Tree,
    /// The tree extraction, then bounded rounds of sharing-aware single-class
    /// switches that strictly lower the whole candidate's cost.
    #[serde(rename = "dag-greedy")]
    DagGreedy,
}

impl Extractor {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::DagGreedy => "dag-greedy",
        }
    }
}

/// One phase and its round count.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PhaseSpec {
    pub(crate) phase: Phase,
    pub(crate) rounds: u32,
}

/// Size and work limits. Every field is optional and defaults as in
/// [`DEFAULT_STRATEGY_JSON`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Limits {
    /// E-nodes ever created, including retired duplicates.
    #[serde(default = "default_enodes")]
    pub(crate) max_enodes: u32,
    /// Live classes.
    #[serde(default = "default_classes")]
    pub(crate) max_classes: u32,
    /// Rewrites proposed or completions attempted in one round.
    #[serde(default = "default_rewrites")]
    pub(crate) max_rewrites_per_round: u32,
    /// Improvement rounds of the `dag-greedy` extractor.
    #[serde(default = "default_extraction_rounds")]
    pub(crate) max_extraction_rounds: u32,
}

const fn default_enodes() -> u32 {
    20_000
}
const fn default_classes() -> u32 {
    10_000
}
const fn default_rewrites() -> u32 {
    10_000
}
const fn default_extraction_rounds() -> u32 {
    8
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_enodes: default_enodes(),
            max_classes: default_classes(),
            max_rewrites_per_round: default_rewrites(),
            max_extraction_rounds: default_extraction_rounds(),
        }
    }
}

impl Limits {
    pub(crate) fn caps(&self) -> Caps {
        Caps {
            max_enodes: self.max_enodes,
            max_classes: self.max_classes,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    phases: Vec<PhaseSpec>,
    #[serde(default)]
    limits: Limits,
    #[serde(default = "default_extractor")]
    extractor: Extractor,
}

const fn default_extractor() -> Extractor {
    Extractor::DagGreedy
}

/// A validated strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Strategy {
    pub(crate) phases: Vec<PhaseSpec>,
    pub(crate) limits: Limits,
    pub(crate) extractor: Extractor,
}

/// Why a document is not a strategy. The message is deterministic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StrategyError {
    pub(crate) message: String,
}

impl Strategy {
    /// Parses and validates a strategy document strictly.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, StrategyError> {
        let document: Document = serde_json::from_slice(bytes).map_err(|error| StrategyError {
            message: format!("not a strategy document: {error}"),
        })?;
        if document.schema != STRATEGY_SCHEMA {
            return Err(StrategyError {
                message: format!(
                    "schema must be {STRATEGY_SCHEMA:?}, found {:?}",
                    document.schema
                ),
            });
        }
        if document.phases.is_empty() {
            return Err(StrategyError {
                message: String::from("phases must not be empty"),
            });
        }
        if document.phases.len() > MAX_PHASES {
            return Err(StrategyError {
                message: format!(
                    "at most {MAX_PHASES} phases, found {}",
                    document.phases.len()
                ),
            });
        }
        for (index, phase) in document.phases.iter().enumerate() {
            if phase.rounds == 0 || phase.rounds > MAX_ROUNDS {
                return Err(StrategyError {
                    message: format!(
                        "phases[{index}].rounds must be between 1 and {MAX_ROUNDS}, found {}",
                        phase.rounds
                    ),
                });
            }
        }
        let limits = document.limits;
        for (name, value) in [
            ("max_enodes", limits.max_enodes),
            ("max_classes", limits.max_classes),
            ("max_rewrites_per_round", limits.max_rewrites_per_round),
            ("max_extraction_rounds", limits.max_extraction_rounds),
        ] {
            if value == 0 || value > MAX_LIMIT {
                return Err(StrategyError {
                    message: format!(
                        "limits.{name} must be between 1 and {MAX_LIMIT}, found {value}"
                    ),
                });
            }
        }
        Ok(Self {
            phases: document.phases,
            limits,
            extractor: document.extractor,
        })
    }

    /// The strategy as JSON for reports.
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": STRATEGY_SCHEMA,
            "phases": self.phases.iter().map(|phase| serde_json::json!({
                "phase": phase.phase.name(), "rounds": phase.rounds
            })).collect::<Vec<_>>(),
            "limits": {
                "max_enodes": self.limits.max_enodes,
                "max_classes": self.limits.max_classes,
                "max_rewrites_per_round": self.limits.max_rewrites_per_round,
                "max_extraction_rounds": self.limits.max_extraction_rounds,
            },
            "extractor": self.extractor.name(),
        })
    }
}
