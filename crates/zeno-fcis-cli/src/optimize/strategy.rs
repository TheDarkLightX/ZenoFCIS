//! The strategy document: a small, versioned, closed JSON grammar that selects
//! phases, rounds, limits, the extractor and an optional artifact profile.
//! Unknown keys, duplicate keys, wrong types and out-of-range values are
//! refused. No user-supplied code runs.
//!
//! ```json
//! {
//!   "schema": "zeno-fcis/optimize-strategy/1",
//!   "phases": [{"phase": "boolean", "rounds": 3}],
//!   "limits": {"max_enodes": 20000, "max_classes": 10000,
//!              "max_rewrites_per_round": 10000, "max_extraction_rounds": 8},
//!   "extractor": "dag-greedy",
//!   "profile": "functional-bool-v1"
//! }
//! ```

use serde::Deserialize;

use super::egraph::Caps;
use crate::neural_loop::profiles::Profile;

/// Version of the strategy grammar.
pub(crate) const STRATEGY_SCHEMA: &str = "zeno-fcis/optimize-strategy/1";
/// Largest number of phases in one strategy.
pub(crate) const MAX_PHASES: usize = 16;
/// Largest number of rounds in one phase.
pub(crate) const MAX_ROUNDS: u32 = 8;
/// Largest value any limit may take.
pub(crate) const MAX_LIMIT: u32 = 100_000;

/// The default strategy, version 1. Every phase is followed by an
/// extraction and a check, so an early phase's candidate survives a later
/// phase that finds nothing better. `optimize` now runs the portfolio below
/// by default, whose first strategy extends this one; the document stays
/// fixed because tests and the neural-loop protocol script
/// (`docs/benchmarks/run_neural_loop_protocol.py`) propose it.
#[cfg(test)]
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

/// Version of the portfolio format.
pub(crate) const PORTFOLIO_SCHEMA: &str = "zeno-fcis/optimize-portfolio/1";

/// The fixed default portfolio, version 1: the default strategy, a
/// select-first order and three rule-and-merge cycles, each followed by
/// semantic merging, cut rewriting and a last merge (rules, then merges).
/// Each runs from the original under its own size budget (`max_enodes`) and
/// deterministic work budget (`max_work`, in millions of steps); one
/// incumbent spans the three runs.
pub(crate) const DEFAULT_PORTFOLIO_JSON: &str = r#"{
  "schema": "zeno-fcis/optimize-portfolio/1",
  "strategies": [
    {
      "schema": "zeno-fcis/optimize-strategy/1",
      "phases": [
        {"phase": "fold", "rounds": 1},
        {"phase": "share", "rounds": 1},
        {"phase": "boolean", "rounds": 3},
        {"phase": "select", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "boolean", "rounds": 2},
        {"phase": "select", "rounds": 2},
        {"phase": "fold", "rounds": 1},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "cut-rewrite", "rounds": 2},
        {"phase": "semantic-merge", "rounds": 1}
      ],
      "limits": {"max_enodes": 20000, "max_work": 400},
      "extractor": "dag-greedy"
    },
    {
      "schema": "zeno-fcis/optimize-strategy/1",
      "phases": [
        {"phase": "fold", "rounds": 1},
        {"phase": "share", "rounds": 1},
        {"phase": "select", "rounds": 3},
        {"phase": "boolean", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "select", "rounds": 2},
        {"phase": "boolean", "rounds": 2},
        {"phase": "fold", "rounds": 1},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "cut-rewrite", "rounds": 2},
        {"phase": "semantic-merge", "rounds": 1}
      ],
      "limits": {"max_enodes": 20000, "max_work": 400},
      "extractor": "dag-greedy"
    },
    {
      "schema": "zeno-fcis/optimize-strategy/1",
      "phases": [
        {"phase": "fold", "rounds": 1},
        {"phase": "share", "rounds": 1},
        {"phase": "boolean", "rounds": 3},
        {"phase": "select", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "boolean", "rounds": 3},
        {"phase": "select", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "boolean", "rounds": 3},
        {"phase": "select", "rounds": 3},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "semantic-merge", "rounds": 2},
        {"phase": "cut-rewrite", "rounds": 2},
        {"phase": "semantic-merge", "rounds": 1}
      ],
      "limits": {"max_enodes": 20000, "max_work": 400},
      "extractor": "dag-greedy"
    }
  ]
}
"#;

/// Largest number of strategies in one portfolio.
pub(crate) const MAX_STRATEGIES: usize = 8;

/// The closed set of phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize)]
pub(crate) enum Phase {
    /// Algebraic rewrites over And, Not, Eq, Lt and the Or form `Select(a, a, b)`.
    #[serde(rename = "boolean")]
    Boolean,
    /// Merge classes with identical exact tables and add single instructions
    /// over existing classes whose function an existing class already has.
    /// Only classes with exact tables take part.
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
    /// Exact rewriting of Boolean classes over cuts of at most three leaves,
    /// from a precomputed table of minimum circuits.
    #[serde(rename = "cut-rewrite")]
    CutRewrite,
}

impl Phase {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::SemanticMerge => "semantic-merge",
            Self::Select => "select",
            Self::Fold => "fold",
            Self::Share => "share",
            Self::CutRewrite => "cut-rewrite",
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
    /// Deterministic work, in millions of steps (rule matches, completion
    /// pairs, cut evaluations and table tuple evaluations), after which no
    /// further round starts. It stands in for a time budget; the default
    /// does not bind in practice.
    #[serde(default = "default_work")]
    pub(crate) max_work: u32,
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
const fn default_work() -> u32 {
    MAX_LIMIT
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_enodes: default_enodes(),
            max_classes: default_classes(),
            max_rewrites_per_round: default_rewrites(),
            max_extraction_rounds: default_extraction_rounds(),
            max_work: default_work(),
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

    /// The work limit in steps.
    pub(crate) fn work(&self) -> u64 {
        u64::from(self.max_work) * 1_000_000
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
    #[serde(default, deserialize_with = "present")]
    profile: Option<String>,
}

const fn default_extractor() -> Extractor {
    Extractor::DagGreedy
}

/// An optional key that, when present, must hold a string: `null` is
/// refused rather than read as absent.
fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

/// A validated strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Strategy {
    pub(crate) phases: Vec<PhaseSpec>,
    pub(crate) limits: Limits,
    pub(crate) extractor: Extractor,
    /// The artifact profile every proposed instruction and every candidate
    /// must stay within: instructions outside it are never added and have
    /// no finite extraction cost.
    pub(crate) profile: Option<Profile>,
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
            ("max_work", limits.max_work),
        ] {
            if value == 0 || value > MAX_LIMIT {
                return Err(StrategyError {
                    message: format!(
                        "limits.{name} must be between 1 and {MAX_LIMIT}, found {value}"
                    ),
                });
            }
        }
        let profile = match document.profile {
            None => None,
            Some(name) => Some(Profile::parse(&name).ok_or_else(|| StrategyError {
                message: format!(
                    "profile must be \"functional-bool-v1\" or \"checked-i64-v1\", found {name:?}"
                ),
            })?),
        };
        Ok(Self {
            phases: document.phases,
            limits,
            extractor: document.extractor,
            profile,
        })
    }

    /// The strategy as JSON for reports; `profile` appears only when set.
    pub(crate) fn json(&self) -> serde_json::Value {
        let mut document = serde_json::json!({
            "schema": STRATEGY_SCHEMA,
            "phases": self.phases.iter().map(|phase| serde_json::json!({
                "phase": phase.phase.name(), "rounds": phase.rounds
            })).collect::<Vec<_>>(),
            "limits": {
                "max_enodes": self.limits.max_enodes,
                "max_classes": self.limits.max_classes,
                "max_rewrites_per_round": self.limits.max_rewrites_per_round,
                "max_extraction_rounds": self.limits.max_extraction_rounds,
                "max_work": self.limits.max_work,
            },
            "extractor": self.extractor.name(),
        });
        if let Some(profile) = self.profile {
            document["profile"] = serde_json::json!(profile.name());
        }
        document
    }
}

/// What one search runs: a single strategy, or the fixed portfolio. Each
/// strategy of a portfolio starts again from the original, and one
/// incumbent spans every run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Plan {
    pub(crate) strategies: Vec<Strategy>,
    /// Whether the plan is a portfolio rather than one strategy.
    pub(crate) portfolio: bool,
}

impl Plan {
    pub(crate) fn single(strategy: Strategy) -> Self {
        Self {
            strategies: vec![strategy],
            portfolio: false,
        }
    }

    /// Parses a portfolio document: exactly `schema` and 1 to
    /// [`MAX_STRATEGIES`] strategy documents, each read by
    /// [`Strategy::parse`].
    pub(crate) fn parse_portfolio(bytes: &[u8]) -> Result<Self, StrategyError> {
        let error = |message: String| StrategyError { message };
        let document: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|failure| error(format!("not a portfolio document: {failure}")))?;
        let fields = document
            .as_object()
            .filter(|map| map.len() == 2)
            .ok_or_else(|| {
                error(String::from(
                    "a portfolio has exactly schema and strategies",
                ))
            })?;
        if fields.get("schema").and_then(serde_json::Value::as_str) != Some(PORTFOLIO_SCHEMA) {
            return Err(error(format!("schema must be {PORTFOLIO_SCHEMA:?}")));
        }
        let strategies = fields
            .get("strategies")
            .and_then(serde_json::Value::as_array)
            .filter(|strategies| (1..=MAX_STRATEGIES).contains(&strategies.len()))
            .ok_or_else(|| {
                error(format!(
                    "strategies must hold 1 to {MAX_STRATEGIES} strategy documents"
                ))
            })?;
        let strategies = strategies
            .iter()
            .enumerate()
            .map(|(index, strategy)| {
                Strategy::parse(strategy.to_string().as_bytes())
                    .map_err(|failure| error(format!("strategies[{index}]: {}", failure.message)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            strategies,
            portfolio: true,
        })
    }

    /// The fixed default portfolio, version 1.
    pub(crate) fn default_portfolio() -> Result<Self, StrategyError> {
        Self::parse_portfolio(DEFAULT_PORTFOLIO_JSON.as_bytes())
    }

    /// Runs every strategy within `profile`; a strategy that names another
    /// profile is refused.
    pub(crate) fn restrict(&mut self, profile: Profile) -> Result<(), StrategyError> {
        for strategy in &mut self.strategies {
            match strategy.profile {
                Some(named) if named != profile => {
                    return Err(StrategyError {
                        message: format!(
                            "the strategy's profile {} differs from --profile {}",
                            named.name(),
                            profile.name()
                        ),
                    });
                }
                _ => strategy.profile = Some(profile),
            }
        }
        Ok(())
    }

    /// The plan as JSON for reports: the strategy document, or the
    /// portfolio document.
    pub(crate) fn json(&self) -> serde_json::Value {
        if self.portfolio {
            serde_json::json!({
                "schema": PORTFOLIO_SCHEMA,
                "strategies": self.strategies.iter().map(Strategy::json).collect::<Vec<_>>(),
            })
        } else {
            self.strategies
                .first()
                .map_or(serde_json::Value::Null, Strategy::json)
        }
    }
}
