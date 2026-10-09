//! Examples-first drafting. This persisted state is tooling data, never an
//! authority token. All assessments rebind the current proposal through F1/F2.
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{ContractSources, GeneratedContract, generate_contract, review};
use crate::transform::canonical_json;

pub(crate) const SCHEMA: &str = "zeno-fcis/contract-draft/1";
pub(crate) const MAX_TEXT: usize = 1 << 20;
pub(crate) const MAX_LABELS: usize = 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Draft {
    schema: String,
    pub(crate) intent: String,
    pub(crate) project: String,
    pub(crate) initial_examples: String,
    pub(crate) provenance: String,
    pub(crate) rounds: u8,
    pub(crate) max_tuples: u64,
    pub(crate) proposals: Vec<Proposal>,
    pub(crate) labels: Vec<Label>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proposal {
    pub(crate) id: String,
    pub(crate) rules: String,
    provenance: String,
    submitted_revision: String,
    /// None means a reserved attempt interrupted before assessment completed.
    pub(crate) assessment: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Label {
    proposal: String,
    submitted_revision: String,
    text: String,
    provenance: String,
}

pub(crate) struct Assessment {
    pub(crate) report: Value,
    pub(crate) ready: bool,
    pub(crate) generated: GeneratedContract,
    pub(crate) examples: String,
    pub(crate) questions: BTreeSet<Vec<i64>>,
}

pub(crate) fn hash(bytes: &[u8]) -> String {
    crate::transform::sha256_hex(bytes)
}

fn text(value: &str, name: &str, required: bool) -> Result<(), String> {
    if value.len() > MAX_TEXT || (required && value.trim().is_empty()) {
        return Err(format!(
            "{name} must be {}nonempty UTF-8 of at most {MAX_TEXT} bytes",
            if required { "" } else { "optionally " }
        ));
    }
    Ok(())
}

impl Draft {
    pub(crate) fn start(
        intent: String,
        project: String,
        initial_examples: String,
        provenance: String,
        rounds: u8,
        max_tuples: u64,
    ) -> Result<Self, String> {
        let draft = Self {
            schema: SCHEMA.to_owned(),
            intent,
            project,
            initial_examples,
            provenance,
            rounds,
            max_tuples,
            proposals: Vec::new(),
            labels: Vec::new(),
        };
        draft.validate()?;
        Ok(draft)
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_value(self)
            .map(|v| canonical_json(&v).into_bytes())
            .map_err(|e| e.to_string())
    }

    pub(crate) fn revision(&self) -> Result<String, String> {
        Ok(hash(&self.bytes()?))
    }

    pub(crate) fn expect_revision(&self, expected: &str) -> Result<(), String> {
        if self.revision()? != expected {
            return Err("stale draft revision; read questions again".to_owned());
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA
            || !(1..=8).contains(&self.rounds)
            || !(1..=4096).contains(&self.max_tuples)
            || self.proposals.len() > usize::from(self.rounds)
            || self.labels.len() > MAX_LABELS
        {
            return Err("invalid draft schema or fixed work limits".to_owned());
        }
        for (name, value, required) in [
            ("intent", &self.intent, true),
            ("project", &self.project, true),
            ("initial examples", &self.initial_examples, false),
            ("provenance", &self.provenance, true),
        ] {
            text(value, name, required)?;
        }
        for (i, proposal) in self.proposals.iter().enumerate() {
            text(&proposal.rules, "rules", false)?;
            text(&proposal.provenance, "proposal provenance", true)?;
            if proposal.id != self.proposal_id(i, &proposal.rules)
                || proposal.submitted_revision.len() != 64
            {
                return Err("proposal identity differs from its sources".to_owned());
            }
        }
        for label in &self.labels {
            text(&label.text, "label", true)?;
            text(&label.provenance, "label provenance", true)?;
            if !self.proposals.iter().any(|p| p.id == label.proposal)
                || label.submitted_revision.len() != 64
            {
                return Err("label does not name a retained proposal and revision".to_owned());
            }
        }
        Ok(())
    }

    fn proposal_id(&self, ordinal: usize, rules: &str) -> String {
        hash(canonical_json(&json!([SCHEMA, self.project, self.intent, ordinal, rules])).as_bytes())
    }

    /// Reserve first. The shell must persist these bytes before calling assess.
    pub(crate) fn reserve(&mut self, rules: String, provenance: String) -> Result<(), String> {
        self.validate()?;
        if self.proposals.len() >= usize::from(self.rounds) {
            return Err("proposal budget exhausted".to_owned());
        }
        text(&rules, "rules", false)?;
        text(&provenance, "proposal provenance", true)?;
        let id = self.proposal_id(self.proposals.len(), &rules);
        let submitted_revision = self.revision()?;
        self.proposals.push(Proposal {
            id,
            rules,
            provenance,
            submitted_revision,
            assessment: None,
        });
        Ok(())
    }

    pub(crate) fn current(&self) -> Result<&Proposal, String> {
        self.proposals
            .last()
            .ok_or_else(|| "no proposal supplied".to_owned())
    }

    pub(crate) fn completed(&self) -> Result<(), String> {
        if self.current()?.assessment.is_none() {
            return Err("interrupted proposal assessment; submit a new proposal within the remaining budget".to_owned());
        }
        Ok(())
    }

    pub(crate) fn examples(&self) -> String {
        let mut examples = self.initial_examples.clone();
        examples.push('\n');
        for label in &self.labels {
            examples.push_str(&label.text);
            examples.push('\n');
        }
        examples
    }

    pub(crate) fn record_assessment(&mut self, report: Value) -> Result<(), String> {
        let proposal = self
            .proposals
            .last_mut()
            .ok_or_else(|| "no proposal supplied".to_owned())?;
        proposal.assessment = Some(report);
        Ok(())
    }

    /// No inference of an owner's label from a proposed/example decision.
    pub(crate) fn add_labels(
        &mut self,
        expected: &str,
        labels: String,
        provenance: String,
    ) -> Result<(), String> {
        self.expect_revision(expected)?;
        text(&labels, "label", true)?;
        text(&provenance, "label provenance", true)?;
        if self.labels.len() >= MAX_LABELS {
            return Err("label budget exhausted".to_owned());
        }
        self.completed()?;
        let assessment = self.assess()?;
        if self
            .examples()
            .len()
            .checked_add(labels.len())
            .and_then(|bytes| bytes.checked_add(1))
            .is_none_or(|bytes| bytes > MAX_TEXT)
        {
            return Err("combined examples exceed 1 MiB".to_owned());
        }
        let proposal = self.current()?;
        let (_, checked) = review::draft_examples(review::ReviewSources {
            project: &self.project,
            rules: &proposal.rules,
            examples: Some(&labels),
        })
        .map_err(|e| e.to_string())?;
        if checked.len()
            + assessment.report["labels_compared"]
                .as_array()
                .map_or(0, Vec::len)
            > MAX_LABELS
        {
            return Err("decision-example budget exhausted".to_owned());
        }
        if checked.is_empty() {
            return Err("no label supplied".to_owned());
        }
        if checked
            .iter()
            .any(|label| !assessment.questions.contains(&label.input))
        {
            return Err("label must name a current distinguishing input".to_owned());
        }
        self.labels.push(Label {
            proposal: proposal.id.clone(),
            submitted_revision: expected.to_owned(),
            text: labels,
            provenance,
        });
        Ok(())
    }

    /// Recompute, never trust cached assessment flags or counts after loading.
    pub(crate) fn assess(&self) -> Result<Assessment, String> {
        self.validate()?;
        let proposal = self.current()?;
        let sources = ContractSources {
            project: &self.project,
            rules: &proposal.rules,
            schema_origin: None,
            adoptions: &[],
            replayed: &[],
            evolutions: &[],
        };
        let generated = generate_contract(sources).map_err(|e| e.to_string())?;
        let examples = self.examples();
        if examples.len() > MAX_TEXT
            || examples
                .lines()
                .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                .count()
                > MAX_LABELS
        {
            return Err("decision-example budget exhausted".to_owned());
        }
        let inputs = review::ReviewSources {
            project: &self.project,
            rules: &proposal.rules,
            examples: Some(&examples),
        };
        let (shape, labels) = review::draft_examples(inputs).map_err(|e| e.to_string())?;
        // Bind all rounds to the first admitted proposal's actual schema/order.
        // Rebind raw sources; cached reports never select or authorize a shape.
        for prior in &self.proposals {
            if prior.id == proposal.id {
                break;
            }
            if generate_contract(ContractSources {
                project: &self.project,
                rules: &prior.rules,
                schema_origin: None,
                adoptions: &[],
                replayed: &[],
                evolutions: &[],
            })
            .is_ok()
            {
                let (original_shape, _) = review::draft_examples(review::ReviewSources {
                    project: &self.project,
                    rules: &prior.rules,
                    examples: None,
                })
                .map_err(|e| e.to_string())?;
                if original_shape != shape {
                    return Err("proposal changed the fixed schema/input order".to_owned());
                }
                break;
            }
        }
        if labels.len() > MAX_LABELS {
            return Err("decision-example budget exhausted".to_owned());
        }
        let reviewed = review::review(inputs, self.max_tuples).map_err(|e| e.to_string())?;
        let packet: Value = serde_json::from_str(reviewed.packet()).map_err(|e| e.to_string())?;
        let mut questions = BTreeSet::new();
        let mut details = Vec::new();
        let mutants = packet["mutants"]["list"]
            .as_array()
            .ok_or("review packet lacks mutants")?;
        for mutant in mutants {
            if mutant["classification"] != "distinguished" {
                continue;
            }
            let witness = &mutant["witness"];
            let values = witness["input"]
                .as_array()
                .ok_or("review witness lacks input")?;
            let input: Vec<i64> = values
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or("review input is not decimal".to_owned())
                        .and_then(|s| s.parse::<i64>().map_err(|e| e.to_string()))
                })
                .collect::<Result<_, _>>()?;
            if questions.insert(input.clone()) {
                details.push(
                    json!({"input": input, "proposed_example": witness["proposed_example"],
                    "alternative_example": witness["mutant_as_example"],
                    "label_supplied": labels.iter().any(|l| l.input == input)}),
                );
            }
        }
        let covered: BTreeSet<Vec<i64>> = labels.iter().map(|l| l.input.clone()).collect();
        let missing: Vec<_> = questions.difference(&covered).cloned().collect();
        let ready = !labels.is_empty() && missing.is_empty() && reviewed.summary().findings == 0;
        let report = json!({"shape": shape, "proposal": proposal.id, "ready": ready,
            "status": if ready {"labels-agree"} else {"needs-labels-or-revision"},
            "labels_compared": labels, "label_provenance": {"initial": self.provenance, "additional": self.labels},
            "questions": details, "unlabeled_inputs": missing,
            "review": packet, "authority": "none", "hosted_model": "off",
            "assumptions": "Labels and their provenance are supplied intent assumptions, not authenticated owner approval. F2 remains advisory."});
        Ok(Assessment {
            report,
            ready,
            generated,
            examples,
            questions,
        })
    }
}
