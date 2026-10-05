//! The incumbent, actual costs and the fixed selection rule (NSL-001, NSL-004,
//! NSL-005, NSL-006, NSC-006).
//!
//! `OriginalAdmitted` is the admitted original: equivalent to itself by
//! reflexivity, and it carries no manufactured transform receipt. A
//! `CheckedReplacement` can only be built from a genuine
//! [`crate::transform::Equivalence`], which only [`crate::transform::check`]
//! constructs, bound to the exact candidate bytes it was computed for.

use crate::transform::{Equivalence, sha256_hex};
use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::Program;

/// Objective V1: the lexicographic pair (stored instructions, canonical
/// artifact bytes). `Ord` on this struct is that order because `nodes` comes
/// first.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct Cost {
    /// Every stored instruction counted once, unused nodes included.
    pub(crate) nodes: u64,
    /// The actual canonical artifact length.
    pub(crate) bytes: u64,
}

impl Cost {
    /// Measures an admitted program and the exact bytes it was decoded from.
    pub(crate) fn measure(program: &Program, bytes: &[u8]) -> Cost {
        Cost {
            nodes: program.nodes().len() as u64,
            bytes: bytes.len() as u64,
        }
    }

    pub(crate) fn json(self) -> Value {
        json!({"nodes": self.nodes, "bytes": self.bytes})
    }

    pub(crate) fn from_json(value: &Value) -> Option<Cost> {
        if !super::only_fields(value, &["nodes", "bytes"]) {
            return None;
        }
        Some(Cost {
            nodes: super::json_u64(value, "nodes")?,
            bytes: super::json_u64(value, "bytes")?,
        })
    }
}

/// Why an equivalent candidate does not replace the incumbent (NSL-006).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoImprovement {
    /// A cost component exceeds the original's.
    OriginalGuard { nodes_over: bool, bytes_over: bool },
    /// Both components equal the original's: no strict improvement.
    OriginalTie,
    /// Both components equal the incumbent's.
    IncumbentTie,
    /// Within the original's bounds, but not lexicographically below the
    /// incumbent.
    NotBelowIncumbent,
}

/// The selection decision for an equivalent candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Selection {
    CheckedImprovement,
    EquivalentWithoutImprovement(NoImprovement),
}

/// NSL-005: select only if `N(Q) <= N(P0)`, `B(Q) <= B(P0)`, at least one
/// strict, and `C(Q) <lex C(I)`. Equal costs keep the incumbent. The documented
/// claim is original component bounds plus lexicographic descent between
/// incumbents, not componentwise descent between incumbents.
pub(crate) fn select(original: Cost, incumbent: Cost, candidate: Cost) -> Selection {
    let nodes_over = candidate.nodes > original.nodes;
    let bytes_over = candidate.bytes > original.bytes;
    if nodes_over || bytes_over {
        return Selection::EquivalentWithoutImprovement(NoImprovement::OriginalGuard {
            nodes_over,
            bytes_over,
        });
    }
    if candidate == original {
        return Selection::EquivalentWithoutImprovement(NoImprovement::OriginalTie);
    }
    if candidate == incumbent {
        return Selection::EquivalentWithoutImprovement(NoImprovement::IncumbentTie);
    }
    if candidate < incumbent {
        Selection::CheckedImprovement
    } else {
        Selection::EquivalentWithoutImprovement(NoImprovement::NotBelowIncumbent)
    }
}

impl Selection {
    pub(crate) fn json(self) -> Value {
        match self {
            Selection::CheckedImprovement => json!({"selection": "checked-improvement"}),
            Selection::EquivalentWithoutImprovement(reason) => json!({
                "selection": "equivalent-without-improvement",
                "reason": match reason {
                    NoImprovement::OriginalGuard { .. } => "original-cost-guard",
                    NoImprovement::OriginalTie => "tie-with-original",
                    NoImprovement::IncumbentTie => "tie-with-incumbent",
                    NoImprovement::NotBelowIncumbent => "not-below-incumbent",
                },
                "nodes_over": matches!(reason, NoImprovement::OriginalGuard { nodes_over: true, .. }),
                "bytes_over": matches!(reason, NoImprovement::OriginalGuard { bytes_over: true, .. }),
            }),
        }
    }
}

/// A checked replacement: owned immutable bytes and the receipt of the
/// equivalence that admitted them. Fields are private; only
/// [`Replacement::from_equivalence`] builds one.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Replacement {
    bytes: Vec<u8>,
    sha256: String,
    cost: Cost,
    receipt: Vec<u8>,
    receipt_sha256: String,
    attempt: u8,
}

/// The receipt an equivalence carries does not name the bytes it was built
/// from. This is an internal consistency failure, never a model-visible path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BindingMismatch {
    pub(crate) field: &'static str,
}

impl Replacement {
    /// Binds a genuine equivalence to the candidate bytes it was computed for:
    /// the receipt's candidate and original digests, byte length and node
    /// count must be the ones measured here (NSF-008: hashes derive from the
    /// actual bytes, not copied labels).
    pub(crate) fn from_equivalence(
        equivalence: &Equivalence,
        original: &[u8],
        candidate: &[u8],
        cost: Cost,
        attempt: u8,
    ) -> Result<Replacement, BindingMismatch> {
        let receipt = equivalence.receipt_value();
        let sha256 = sha256_hex(candidate);
        let bound = |side: &str, field: &str| receipt.get(side).and_then(|side| side.get(field));
        if bound("original", "sha256").and_then(Value::as_str) != Some(&sha256_hex(original)) {
            return Err(BindingMismatch { field: "original" });
        }
        if bound("candidate", "sha256").and_then(Value::as_str) != Some(&sha256) {
            return Err(BindingMismatch {
                field: "candidate.sha256",
            });
        }
        if bound("candidate", "bytes").and_then(Value::as_u64) != Some(cost.bytes) {
            return Err(BindingMismatch {
                field: "candidate.bytes",
            });
        }
        if bound("candidate", "nodes").and_then(Value::as_u64) != Some(cost.nodes) {
            return Err(BindingMismatch {
                field: "candidate.nodes",
            });
        }
        let receipt = equivalence.receipt();
        Ok(Replacement {
            bytes: candidate.to_vec(),
            sha256,
            cost,
            receipt_sha256: sha256_hex(&receipt),
            receipt,
            attempt,
        })
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(crate) fn cost(&self) -> Cost {
        self.cost
    }

    /// The canonical receipt bytes of the equivalence.
    pub(crate) fn receipt(&self) -> &[u8] {
        &self.receipt
    }

    pub(crate) fn receipt_sha256(&self) -> &str {
        &self.receipt_sha256
    }

    /// The attempt that produced this replacement.
    pub(crate) fn attempt(&self) -> u8 {
        self.attempt
    }
}

/// The session's incumbent (NSL-001).
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Incumbent {
    /// The admitted original, with no receipt.
    OriginalAdmitted,
    /// A genuine checked replacement.
    CheckedReplacement(Replacement),
}

impl Incumbent {
    /// The incumbent's cost; the original's when no replacement exists.
    pub(crate) fn cost(&self, original: Cost) -> Cost {
        match self {
            Incumbent::OriginalAdmitted => original,
            Incumbent::CheckedReplacement(replacement) => replacement.cost,
        }
    }

    pub(crate) fn replacement(&self) -> Option<&Replacement> {
        match self {
            Incumbent::OriginalAdmitted => None,
            Incumbent::CheckedReplacement(replacement) => Some(replacement),
        }
    }

    /// NSL-008: `no-checked-improvement` keeps the admitted original;
    /// `best-checked-so-far` names a genuine checked result. Neither implies
    /// global optimality.
    pub(crate) fn status(&self) -> &'static str {
        match self {
            Incumbent::OriginalAdmitted => "no-checked-improvement",
            Incumbent::CheckedReplacement(_) => "best-checked-so-far",
        }
    }

    pub(crate) fn json(&self, original_sha256: &str, original: Cost) -> Value {
        match self {
            Incumbent::OriginalAdmitted => json!({
                "kind": "original-admitted",
                "sha256": original_sha256,
                "cost": original.json(),
                "receipt": "none"
            }),
            Incumbent::CheckedReplacement(replacement) => json!({
                "kind": "checked-replacement",
                "sha256": replacement.sha256,
                "cost": replacement.cost.json(),
                "receipt": {"sha256": replacement.receipt_sha256, "bytes": replacement.receipt.len()},
                "attempt": replacement.attempt
            }),
        }
    }
}
