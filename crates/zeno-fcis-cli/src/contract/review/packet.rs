//! The review packet as JSON. Every value is computed by the review; the
//! caller renders the packet canonically.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::domain::{Construction, InputSet, Position};
use super::evaluate::{Delivery, Outcome, RefusalClass, Value as Atom, digest, hex};
use super::examples::{self, Example};
use super::mutants::{CATALOG, OPERATORS};
use super::refusals::Group;
use super::table::{Row, Table};
use super::{
    ADVISORY, Classification, Classified, Finding, MAX_INLINE_ROWS, PACKET_SCHEMA, Summary,
    Witness, WitnessSource,
};

/// Everything a packet reports.
pub(super) struct Report<'r> {
    pub(super) application: &'r str,
    /// SHA-256 of the bound Authority's identity bytes.
    pub(super) identity: &'r str,
    pub(super) sources: Value,
    pub(super) positions: &'r [Position],
    pub(super) inputs: &'r InputSet,
    pub(super) max_tuples: u64,
    pub(super) table: &'r Table,
    /// The contract's state laws, in its law order.
    pub(super) state_laws: &'r [u32],
    pub(super) groups: &'r [Group],
    pub(super) examples_file: Option<&'static str>,
    pub(super) examples: &'r [Example],
    pub(super) at_examples: &'r [Outcome],
    pub(super) agreement: &'r [Result<(), String>],
    pub(super) classified: &'r [Classified<'r>],
    pub(super) findings: &'r [Finding],
    pub(super) summary: &'r Summary,
    pub(super) state_width: usize,
}

pub(super) fn build(report: &Report<'_>) -> Value {
    json!({
        "schema": PACKET_SCHEMA,
        "authority": "none",
        "advisory": ADVISORY,
        "application": report.application,
        "tool": {
            "crate": "zeno-fcis-cli",
            "version": env!("CARGO_PKG_VERSION"),
            "mutant_catalog": CATALOG,
            "library_identity_sha256": report.identity,
        },
        "sources": report.sources,
        "inputs": inputs(report),
        "decision_table": table(report),
        "refusals": refusals(report),
        "examples": examples(report),
        "mutants": mutants(report),
        "findings": report.findings.iter().map(finding).collect::<Vec<_>>(),
        "summary": report.summary.json(),
    })
}

fn inputs(report: &Report<'_>) -> Value {
    let inputs = report.inputs;
    let order = match inputs.construction {
        Construction::FullDomain => {
            "every tuple of the admitted domain; positions in program order, the last position fastest"
        }
        Construction::BoundaryProduct => {
            "every tuple of the boundary sets; positions in program order, the last position fastest"
        }
        Construction::BoundaryProbes => {
            "the bases in order; then each base varied in one position; then each base varied in two positions; positions and values ascending; a tuple seen before is dropped"
        }
    };
    json!({
        "positions": report.positions.iter().map(Position::json).collect::<Vec<_>>(),
        "domain_size": inputs.domain_size.map(|size| size.to_string()),
        "max_tuples": report.max_tuples,
        "construction": inputs.construction.name(),
        "construction_rule": "full-domain when the domain has at most max_tuples tuples; else boundary-product when the product of the boundary sets does; else boundary-probes, stopped at max_tuples",
        "boundary_rule": "an integer position takes its domain endpoints and every integer constant the rules compare with, add to, subtract from or assign to a value of its type, with that constant's two neighbours, where in domain; genesis values count as constants; a sum takes every variant; a boolean both values; probe bases are the genesis state with the first boundary value of every command and context position, then each owner example",
        "order": order,
        "count": inputs.len(),
        "boundary": inputs.boundary.as_ref().map(|boundary| json!({
            "constants_by_type": boundary.constants.iter().map(|(type_id, values)| {
                (type_id.to_string(), values.iter().map(|value| json!(value.to_string())).collect::<Vec<_>>())
            }).collect::<BTreeMap<_, _>>(),
            "values": boundary.values.iter().map(|values| numbers(values)).collect::<Vec<_>>(),
            "bases": boundary.bases.iter().map(|(origin, base)| {
                json!({"origin": origin, "input": numbers(base)})
            }).collect::<Vec<_>>(),
            "singles": boundary.singles,
            "pairs": boundary.pairs,
            "truncated": boundary.truncated,
        })),
    })
}

fn table(report: &Report<'_>) -> Value {
    let table = report.table;
    let inline = table.rows.len() <= MAX_INLINE_ROWS;
    let mut rows = Vec::new();
    let mut chain = [0u8; 32];
    let mut classes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for (tuple, row) in report.inputs.tuples().zip(&table.rows) {
        let line = match *row {
            Row::Decided {
                class,
                reason,
                post,
                outbox,
            } => {
                *classes.entry(class.name()).or_default() += 1;
                if let Some(reason) = reason {
                    *reasons.entry(reason.to_string()).or_default() += 1;
                }
                format!(
                    "{} | {} {} {post} {outbox}",
                    examples::join(tuple),
                    class.name(),
                    reason.map_or_else(|| "-".to_owned(), |reason| reason.to_string())
                )
            }
            Row::Refused {
                refusal,
                unsatisfied,
            } => {
                *classes.entry("refused").or_default() += 1;
                match unsatisfied {
                    None => format!("{} | refused {refusal}", examples::join(tuple)),
                    Some(law) => {
                        format!(
                            "{} | refused {refusal} unsatisfied {law}",
                            examples::join(tuple)
                        )
                    }
                }
            }
        };
        let mut bytes = chain.to_vec();
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        chain = digest(&bytes);
        if inline {
            rows.push(Value::String(line));
        }
    }
    json!({
        "count": table.rows.len(),
        "row_format": "input values in position order | class reason post-state-index outbox-index; or | refused refusal-index, followed by `unsatisfied` and a state law's ID when the input's pre-state does not satisfy that state law, the first in the contract's law order",
        "rows": inline.then_some(rows),
        "rows_omitted": !inline,
        "rows_digest": {
            "method": "sha256 chain over the rows in order: start from 32 zero bytes; each step hashes the previous digest, the row line and a newline",
            "sha256": hex(&chain),
        },
        "post_states": table.post_states.iter().map(|(digest, fields)| {
            json!({"sha256": hex(digest), "fields": fields_json(fields)})
        }).collect::<Vec<_>>(),
        "outboxes": table.outboxes.iter().map(|(digest, deliveries)| {
            json!({"sha256": hex(digest), "deliveries": deliveries.iter().map(delivery).collect::<Vec<_>>()})
        }).collect::<Vec<_>>(),
        "refusals": table.refusals.iter().map(|refusal| json!({
            "refusal": refusal.text,
            "class": refusal.class.name(),
            "law": refusal.law,
        })).collect::<Vec<_>>(),
        "tallies": {"classes": classes, "reasons": reasons},
    })
}

fn refusals(report: &Report<'_>) -> Value {
    let table = report.table;
    json!({
        "state_laws": report.state_laws,
        "state_law_rule": "the state laws are the contract's laws that apply at genesis and to every committing decision, so that every committed state satisfies them, except InitialCondition laws, which apply at genesis only; a pre-state satisfies them when the library's law evaluator, run on the contract's own law programs with the pre-state as a genesis state and the state laws first, finds each one satisfied",
        "law_rule": "a refusal's law is the law whose evaluation the library's law diagnostics report as refusing; a refusal no law evaluation made names none",
        "reachability": "not decided: a pre-state that satisfies every state law may still be unreachable from genesis; a pre-state that breaks one is never a committed state",
        "findings_rule": "each refusal of class law on inputs whose pre-state satisfies every state law is a finding",
        "classes": RefusalClass::ALL.iter().map(|class| json!({
            "class": class.name(),
            "holds": class.meaning(),
        })).collect::<Vec<_>>(),
        "counts": report.summary.refusals.json(),
        "by_unsatisfied_state_law": report.summary.refusals.by_state_law.iter().map(|(law, count)| {
            (law.to_string(), count)
        }).collect::<BTreeMap<_, _>>(),
        "groups": report.groups.iter().map(|group| {
            let refusal = &table.refusals[group.refusal as usize];
            json!({
                "refusal": group.refusal,
                "class": refusal.class.name(),
                "law": refusal.law,
                "unsatisfied_state_law": group.unsatisfied,
                "count": group.count,
                "first": {
                    "ordinal": group.first,
                    "input": numbers(report.inputs.tuple(group.first)),
                },
            })
        }).collect::<Vec<_>>(),
    })
}

fn examples(report: &Report<'_>) -> Value {
    let disagreements: Vec<Value> = report
        .examples
        .iter()
        .zip(report.agreement)
        .zip(report.at_examples)
        .filter_map(|((example, agreement), outcome)| {
            agreement.as_ref().err().map(|difference| {
                json!({
                    "line": example.line,
                    "example": example.text,
                    "difference": difference,
                    "library": self::outcome(outcome),
                })
            })
        })
        .collect();
    json!({
        "file": report.examples_file,
        "parsed": report.examples.len(),
        "compared": report.examples.len(),
        "agreeing": report.examples.len() - disagreements.len(),
        "disagreements": disagreements,
    })
}

fn mutants(report: &Report<'_>) -> Value {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for item in report.classified {
        *counts.entry(item.classification.name()).or_default() += 1;
    }
    json!({
        "catalog": CATALOG,
        "operators": OPERATORS.iter().map(|(name, changes)| json!({"name": name, "changes": changes})).collect::<Vec<_>>(),
        "witness_search": "owner examples in file order, then the input set in order; the first input where the mutant's class, reason, successor digest or outbox digest differs, preferring one where the contract decides over one where it refuses",
        "count": report.classified.len(),
        "classification_counts": counts,
        "list": report.classified.iter().map(|item| classified(item, report.state_width)).collect::<Vec<_>>(),
    })
}

fn classified(item: &Classified<'_>, state_width: usize) -> Value {
    let mut value = json!({
        "id": item.mutant.id,
        "operator": item.mutant.operator,
        "site": item.mutant.site,
        "change": item.mutant.change,
        "classification": item.classification.name(),
    });
    match &item.classification {
        Classification::Distinguished(found) => {
            value["witness"] = witness(found, state_width);
        }
        Classification::RefusedByGenerator { place, reason }
        | Classification::RefusedByLibrary { place, reason } => {
            value["refusal"] = json!({"place": place, "reason": reason});
        }
        Classification::EquivalentOverFullDomain
        | Classification::NotDistinguishedWithinBoundarySet => {}
    }
    value
}

fn witness(witness: &Witness, state_width: usize) -> Value {
    let example = |outcome: &Outcome| match outcome {
        Outcome::Decision(decision) => examples::render(&witness.input, decision, state_width),
        Outcome::Refused(_) => None,
    };
    json!({
        "source": match witness.source {
            WitnessSource::OwnerExample { line, .. } => json!({"kind": "owner-example", "line": line}),
            WitnessSource::InputSet { ordinal } => json!({"kind": "input-set", "ordinal": ordinal}),
        },
        "input": numbers(&witness.input),
        "original": outcome(&witness.original),
        "mutant": outcome(&witness.mutant),
        "proposed_example": example(&witness.original),
        "mutant_as_example": example(&witness.mutant),
        "owner_agrees_with": witness.owner_agrees_with,
    })
}

fn finding(finding: &Finding) -> Value {
    match finding {
        Finding::ExampleDisagrees { line, difference } => json!({
            "kind": "example-disagrees",
            "line": line,
            "difference": difference,
        }),
        Finding::ExampleAgreesWithMutant { line, mutant } => json!({
            "kind": "example-agrees-with-mutant",
            "line": line,
            "mutant": mutant,
        }),
        Finding::LawRefusal(found) => json!({
            "kind": "law-refusal-on-law-consistent-state",
            "law": found.law,
            "refusal": found.refusal,
            "inputs": found.inputs,
            "first": {
                "ordinal": found.ordinal,
                "input": numbers(&found.input),
                "case": found.case.as_ref().map(|(index, _)| index),
                "case_when": found.case.as_ref().map(|(_, when)| when),
            },
        }),
    }
}

pub(super) fn outcome(outcome: &Outcome) -> Value {
    match outcome {
        Outcome::Decision(decision) => json!({
            "class": decision.class.name(),
            "reason": decision.reason,
            "post": fields_json(&decision.post),
            "outbox": decision.outbox.iter().map(delivery).collect::<Vec<_>>(),
            "post_sha256": hex(&decision.post_digest),
            "outbox_sha256": hex(&decision.outbox_digest),
        }),
        Outcome::Refused(refusal) => json!({
            "refused": refusal.text,
            "class": refusal.class.name(),
            "law": refusal.law,
        }),
    }
}

fn delivery(delivery: &Delivery) -> Value {
    json!({
        "ordinal": delivery.ordinal,
        "channel": delivery.channel,
        "destination": delivery.destination.json(),
        "payload": fields_json(&delivery.payload),
        "idempotency": delivery.idempotency.json(),
    })
}

fn fields_json(fields: &[(u16, Atom)]) -> Value {
    Value::Array(
        fields
            .iter()
            .map(|(field, value)| json!({"field": field, "value": value.json()}))
            .collect(),
    )
}

/// Input values as decimal strings.
fn numbers(values: &[i64]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| Value::String(value.to_string()))
            .collect(),
    )
}
