//! LLM-generated bounded-search diagnostics. These requests cannot change a
//! requirement, a resource cap or a checker verdict. All JSON is owned output.

use serde_json::{Value, json};

fn blocked(status: &str, reason: &str) -> bool {
    matches!(
        status,
        "outside-vocabulary"
            | "not-found"
            | "no-repair-found"
            | "no-candidate-proposed"
            | "unimproved"
            | "no-completion"
    ) || (matches!(status, "inconclusive" | "refused" | "failed")
        && (reason.contains("budget")
            || reason.contains("limit")
            || reason.contains("no-cover")
            || reason.contains("no-case-tree")
            || reason.contains("primitive")
            || reason.contains("operator")
            || reason.contains("standard-macro-required")
            || reason.contains("strategy-split-tuples")
            || reason.contains("strategy-split-items")
            || reason.contains("bounded-search")
            || reason.contains("no-qualified-candidate")
            || reason.contains("opaque")
            || reason.contains("deadline")))
}

/// Add an actionable, explicitly bounded request without relabelling the result.
/// The envelope is named when search cannot isolate a unique causal bound.
pub(super) fn annotate(door: &str, status: &str, detail: &mut Value) {
    let reason = detail["reason"].as_str().unwrap_or(status).to_owned();
    let normalized = reason.to_ascii_lowercase();
    if blocked(status, &normalized) && detail.get("search_limit").is_none() {
        let reason = normalized.as_str();
        let bound = if reason.contains("primitive")
            || reason.contains("operator")
            || reason.contains("standard-macro-required")
            || reason.contains("opaque")
        {
            "supported-lowerable-vocabulary"
        } else if status == "outside-vocabulary" {
            "max_size-and-declared-vocabulary"
        } else if reason.contains("query-budget") {
            "queries"
        } else if reason.contains("split-items") {
            "max_terms"
        } else if reason.contains("work") {
            "work"
        } else if reason.contains("cell") {
            "cells"
        } else if reason.contains("meaning") {
            "meanings-per-kind"
        } else if reason.contains("tuple") || reason.contains("domain") {
            "domain-tuples"
        } else if reason.contains("conflict-subset") {
            "conflict-subsets"
        } else if reason.contains("gate-budget") {
            "check-requests-or-gate-pairs"
        } else if reason.contains("deadline") || reason == "stage-budget" {
            "invocation-time-or-control-frames"
        } else if reason.contains("no-cover") {
            "declared-learn-terms"
        } else if reason.contains("no-case-tree") {
            "case-depth-and-declared-predicates"
        } else if door == "sketch" {
            "synth_depth-and-declared-hole-candidates"
        } else if door == "repair" {
            "synth_depth-and-greedy-repair-sites"
        } else if door == "version-space" {
            "declared-sketch-and-finite-vocabulary"
        } else if matches!(door, "pipeline" | "strategy") {
            "declared-stages-and-synthesis-envelope"
        } else {
            "declared-search-envelope"
        };
        let limits = detail
            .get("limits")
            .or_else(|| detail["budget"].get("limits"));
        detail["search_limit"] = json!({"bound":bound,"reason":reason,
            "limits":limits,"search_bounds":detail["search_bounds"],
            "action":"request-better-primitive","retry_attempted":false,
            "request":"Provide a supported lowerable primitive or sketch for the same behavior within the named bounds; keep domains, examples, laws and checker limits unchanged.",
            "scope":"bounded search only; no global impossibility claim"});
    }
    // Only result-bearing branches are traversed. Input documents and canonical
    // program JSON remain byte-for-byte data, even if they contain status keys.
    for key in [
        "repair",
        "sketch",
        "transfer",
        "rows",
        "trace",
        "searched",
        "runs",
        "trials",
        "evaluation",
    ] {
        if let Some(child) = detail.get_mut(key) {
            if let Some(rows) = child.as_array_mut() {
                for row in rows {
                    let status = row["status"]
                        .as_str()
                        .or_else(|| row["verdict"].as_str())
                        .unwrap_or("")
                        .to_owned();
                    annotate(door, &status, row);
                }
            } else if child.is_object() {
                let status = child["status"]
                    .as_str()
                    .or_else(|| child["verdict"].as_str())
                    .unwrap_or("")
                    .to_owned();
                annotate(door, &status, child);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_failure_requests_a_primitive_without_changing_requirements() {
        let mut detail =
            json!({"reason":"learn-work-budget","limits":{"max_work":7},"usage":{"work":7}});
        annotate("learn", "inconclusive", &mut detail);
        assert_eq!(detail["search_limit"]["bound"], "work");
        assert_eq!(detail["search_limit"]["action"], "request-better-primitive");
        assert_eq!(detail["search_limit"]["limits"]["max_work"], 7);
        assert_eq!(detail["usage"]["work"], 7);
        assert!(detail.get("candidate").is_none());
        let mut stopped = json!({"reason":"search-stop:WorkBudget { limit: 7 }"});
        annotate("pipeline", "inconclusive", &mut stopped);
        assert_eq!(stopped["search_limit"]["bound"], "work");
        let mut empty = json!({"reason":"bounded-search-no-qualified-candidate"});
        annotate("pipeline", "inconclusive", &mut empty);
        assert_eq!(empty["search_limit"]["action"], "request-better-primitive");
    }

    #[test]
    fn four_node_construction_names_the_size_bound_and_unsupported_opaque_vocabulary() {
        let mut detail = json!({"search_bounds":{"max_size":3}});
        annotate("learn", "outside-vocabulary", &mut detail);
        assert_eq!(detail["search_limit"]["search_bounds"]["max_size"], 3);
        assert_eq!(detail["search_limit"]["action"], "request-better-primitive");
        let mut opaque = json!({"reason":"learn-document:unknown-field:opaque"});
        annotate("learn", "refused", &mut opaque);
        assert_eq!(
            opaque["search_limit"]["bound"],
            "supported-lowerable-vocabulary"
        );
    }
}
