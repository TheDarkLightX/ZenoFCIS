//! Frozen relation intake and literal abstract source; no concrete operations.

use super::common::*;
use serde_json::{Value, json};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct State {
    pub(super) n: i128,
    pub(super) entries: Vec<(i128, i128, i128)>,
}

#[derive(Clone)]
pub(super) struct Relation {
    pub(super) value: Value,
    pub(super) states: Vec<State>,
    pub(super) rows: Vec<Value>,
}
pub(super) fn validate_files(
    relation_bytes: &[u8],
    report_bytes: &[u8],
    source_pin: &[u8],
    recompute: &[u8],
    script: &[u8],
) -> Result<Relation> {
    let relation = Relation::read(relation_bytes)?;
    let report = strict(report_bytes)?;
    if canonical(&relation.value)? != relation_bytes {
        return fail(65, "reference-noncanonical");
    }
    closed(
        &report,
        &[
            "schema",
            "revision",
            "states",
            "commands",
            "rows",
            "observations",
            "relation_sha256",
            "reference_source_sha256",
            "authority",
            "scope",
        ],
    )?;
    let expected = json!({"schema":"zeno-fcis/priority-reference-report/1","revision":REVISION,"states":45,"commands":16,"rows":720,"observations":29,
        "relation_sha256":sha(relation_bytes),"reference_source_sha256":sha(script),"authority":"none","scope":"complete declared finite map relation"});
    if report != expected || canonical(&report)? != report_bytes {
        return fail(65, "reference-report-or-source-pin");
    }
    if source_pin != format!("{}  priority_reference.py\n", sha(script)).as_bytes() {
        return fail(65, "reference-source-pin");
    }
    if strict(recompute)? != report {
        return fail(65, "reference-recompute-differs");
    }
    Ok(relation)
}
impl Relation {
    pub(super) fn read(bytes: &[u8]) -> Result<Self> {
        let value = strict(bytes)?;
        closed(
            &value,
            &[
                "schema", "revision", "genesis", "states", "commands", "rows",
            ],
        )?;
        if value["schema"] != "zeno-fcis/priority-relation/1"
            || value["revision"] != REVISION
            || number(&value["genesis"], 0, 0)? != 0
        {
            return fail(65, "relation-header");
        }
        let list = |key: &str, length: usize| -> Result<&Vec<Value>> {
            value[key]
                .as_array()
                .filter(|v| v.len() == length)
                .ok_or_else(|| malformed(format!("relation-{key}-count")))
        };
        let mut states = Vec::new();
        for state in list("states", 45)? {
            closed(state, &["n", "entries"])?;
            let n = number(&state["n"], 0, 2)?;
            let entries = state["entries"]
                .as_array()
                .filter(|e| e.len() <= 2)
                .ok_or_else(|| malformed("state-entries"))?;
            let mut parsed = Vec::new();
            for entry in entries {
                let parts = entry
                    .as_array()
                    .filter(|e| e.len() == 3)
                    .ok_or_else(|| malformed("entry-triple"))?;
                let item = (
                    number(&parts[0], 0, 2)?,
                    number(&parts[1], 0, 1)?,
                    number(&parts[2], 0, 1)?,
                );
                if item.2 >= n
                    || parsed
                        .last()
                        .is_some_and(|last: &(i128, i128, i128)| last.0 >= item.0)
                    || parsed.iter().any(|last| last.2 == item.2)
                {
                    return fail(65, "invalid-carrier-state");
                }
                parsed.push(item);
            }
            states.push(State { n, entries: parsed });
        }
        // The normative carrier is the valid identity-cell product, in exactly
        // this order. This check is independent of the relation emitter.
        let mut carrier = Vec::new();
        for n in 0..=2 {
            for a in 0..=4 {
                for b in 0..=4 {
                    for c in 0..=4 {
                        let entries: Vec<_> = [a, b, c]
                            .iter()
                            .enumerate()
                            .filter(|(_, v)| **v != 0)
                            .map(|(d, v)| (d as i128, (v - 1) % 2, (v - 1) / 2))
                            .collect();
                        if entries.len() <= 2
                            && entries.iter().all(|e| e.2 < n)
                            && !(entries.len() == 2 && entries[0].2 == entries[1].2)
                        {
                            carrier.push(State { n, entries });
                        }
                    }
                }
            }
        }
        if states != carrier {
            return fail(65, "carrier-order-or-coverage");
        }
        for (q, command) in list("commands", 16)?.iter().enumerate() {
            closed(command, &["q", "kind", "id", "priority"])?;
            let q = q as i128;
            let (kind, id, priority) = if q < 6 {
                ("Insert", Some(q / 2), Some(q % 2))
            } else if q < 9 {
                ("Cancel", Some(q - 6), None)
            } else if q < 15 {
                ("Change", Some((q - 9) / 2), Some((q - 9) % 2))
            } else {
                ("RemoveLowest", None, None)
            };
            if command != &json!({"q":q,"kind":kind,"id":id,"priority":priority}) {
                return fail(65, "command-order-or-encoding");
            }
        }
        let mut observations = std::collections::BTreeSet::new();
        let rows = list("rows", 720)?.clone();
        for (k, row) in rows.iter().enumerate() {
            closed(
                row,
                &[
                    "state",
                    "input",
                    "successor",
                    "class",
                    "reason",
                    "effects",
                    "outbox",
                ],
            )?;
            if number(&row["state"], 0, 44)? != (k / 16) as i128
                || number(&row["input"], 0, 15)? != (k % 16) as i128
            {
                return fail(65, "relation-row-order-or-coverage");
            }
            number(&row["successor"], 0, 44)?;
            if row["effects"] != json!([]) {
                return fail(65, "reference-effects");
            }
            let outbox = row["outbox"]
                .as_array()
                .ok_or_else(|| malformed("reference-outbox"))?;
            if row["class"] == "Reject" {
                number(&row["reason"], 201, 205)?;
                if !outbox.is_empty() || row["successor"] != row["state"] {
                    return fail(65, "reject-frame");
                }
            } else if row["class"] == "Accept" {
                if !row["reason"].is_null() || outbox.len() != 1 {
                    return fail(65, "accept-observation");
                }
                let delivery = &outbox[0];
                closed(
                    delivery,
                    &[
                        "ordinal",
                        "channel",
                        "destination",
                        "idempotency",
                        "payload",
                    ],
                )?;
                number(&delivery["ordinal"], 0, 0)?;
                number(&delivery["channel"], 300, 300)?;
                number(&delivery["idempotency"], 0, 0)?;
                if delivery["destination"] != "priority-events" {
                    return fail(65, "reference-destination");
                }
                let payload = delivery["payload"]
                    .as_array()
                    .filter(|p| p.len() == 4)
                    .ok_or_else(|| malformed("payload-count"))?;
                for (j, field) in payload.iter().enumerate() {
                    let parts = field
                        .as_array()
                        .filter(|p| p.len() == 2)
                        .ok_or_else(|| malformed("payload-field"))?;
                    number(&parts[0], 140 + j as i128, 140 + j as i128)?;
                    number(&parts[1], 0, if j == 0 { 3 } else { 2 })?;
                }
            } else {
                return fail(65, "reference-class");
            }
            observations.insert(canonical(&json!([
                row["class"],
                row["reason"],
                row["effects"],
                row["outbox"]
            ]))?);
        }
        if observations.len() != 29 {
            return fail(65, "reference-observation-count");
        }
        Ok(Self {
            value,
            states,
            rows,
        })
    }

    pub(super) fn abstract_source(&self) -> Result<(String, Vec<u8>)> {
        let project = project("abstract", &[44], None);
        let cases: Vec<_> = self.rows.iter().enumerate().map(|(k,row)| {
            let accept = row["class"] == "Accept";
            let outbox = if accept {
                let delivery = &row["outbox"][0];
                let payload: serde_json::Map<String,Value> = (0..4).map(|j| (format!("{}",140+j), json!(delivery["payload"][j][1].to_string()))).collect();
                json!([{"ordinal":0,"channel":300,"destination":"priority-events","payload":payload,"idempotency_ordinal":0}])
            } else { json!([]) };
            json!({"when":if k==719 {"true".to_string()} else {format!("state == {} && q == {}",k/16,k%16)},
                "class":row["class"],"reason":row["reason"],
                "post":if accept {json!({"110":row["successor"].to_string()})} else {json!({})},"outbox":outbox})
        }).collect();
        let policy = json!({"schema":"zeno-fcis/template-declarative-policy/2","template":"priority-abstract",
            "roots":{"state":100,"command":101,"context":102},"leaf_bindings":{"103":["Text",1,32]},
            "variables":{"state":"pre.100.110","q":"command.101.120"},"cases":cases,"genesis":{"110":0},
            "law_kinds":{"500":"StateInvariant"}});
        Ok((project, canonical(&policy)?))
    }
}

pub(super) fn project(layout: &str, bounds: &[i128], validity: Option<(&str, &str)>) -> String {
    let mut text = format!(
        "// SPDX-License-Identifier: MIT OR Apache-2.0\n// Private finite construction experiment.\nzeno 1;\nproject 1 priority_{layout};\nnamespace 2 priority;\ntype 100 state State;\ntype 101 command Request;\ntype 102 context Context;\ntype 103 destination Destination;\ntype 104 payload Event;\ntype 105 int Counter in 0..={};\ntype 106 int Command in 0..=15;\ntype 107 int Unit in 0..=0;\ntype 109 int Kind in 0..=3;\ntype 115 int Identity in 0..=2;\ntype 116 int Priority in 0..=2;\ntype 117 int Arrival in 0..=2;\n",
        bounds[0]
    );
    if bounds.len() > 1 {
        text.push_str(&format!("type 108 int Cell in 0..={};\n", bounds[1]));
    }
    for (j, _) in bounds.iter().enumerate() {
        let name = if layout == "abstract" {
            "state"
        } else if j == 0 {
            "n"
        } else if layout == "T" {
            ["s0", "s1"][j - 1]
        } else {
            ["cA", "cB", "cC"][j - 1]
        };
        text.push_str(&format!(
            "field {} 100 {name} {};\n",
            110 + j,
            if j == 0 { 105 } else { 108 }
        ));
    }
    text.push_str("field 120 101 q 106;\nfield 130 102 unit 107;\nfield 140 104 kind 109;\nfield 141 104 identity 115;\nfield 142 104 priority 116;\nfield 143 104 arrival 117;\n");
    for (j, name) in ["duplicate", "full", "exhausted", "missing", "empty"]
        .iter()
        .enumerate()
    {
        text.push_str(&format!("reason {} {name} precedence {j};\n", 201 + j));
    }
    text.push_str("channel 300 events destination 103 payload 104;\ncomponent 400 priority {\n  owns 100;\n  reads pre.100;\n  writes post.100;\n  contexts context.102;\n  outbox outbox.300;\n  budget steps 65536;\n}\nmerge [400];\n");
    if let Some((pre, post)) = validity {
        text.push_str(&format!(
            "law 499 valid_pre on any = {pre};\nlaw 500 valid_post on commit, genesis = {post};\n"
        ));
    } else {
        text.push_str(
            "law 500 valid_post on commit, genesis = post.100.110 >= 0 && post.100.110 <= 44;\n",
        );
    }
    text
}
