//! Advisory contract review: what the contract decides on every input of a
//! small domain, or on a deterministic boundary set of a large one; whether
//! those decisions agree with the owner's reviewed decision examples; which
//! refusals fall on pre-states that satisfy every state law; and which rule
//! mutants the inputs distinguish, each with a witness written as a decision
//! example.
//!
//! Every decision, of the contract and of each mutant, is the library
//! Authority's, bound to the generated contract exactly as an application
//! binds it. Nothing here decides or interprets a rule. A review grants
//! nothing and changes no application file; its packet is canonical JSON,
//! byte-identical on repeat.

pub(super) mod domain;
pub(super) mod evaluate;
mod examples;
mod mutants;
mod packet;
mod refusals;
mod table;
#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::v2_authority::Authority;

use super::ContractError;
use super::declarations::Declarations;
use super::model::Contract;
use super::rules::Rules;
use super::{policy, schema, schema_commitment};
use domain::{Construction, InputSet};
use evaluate::{Framer, Outcome};
use examples::Example;
use mutants::Mutant;
use refusals::{Group, Tally};
use table::{Row, Table};

pub(crate) const PACKET_SCHEMA: &str = "zeno-fcis/contract-review/2";
/// The largest input set a review enumerates or probes by default.
pub(crate) const DEFAULT_MAX_TUPLES: u64 = 1 << 20;
/// Decision table rows are written out up to this many inputs; the table's
/// digest covers them all.
const MAX_INLINE_ROWS: usize = 1 << 17;
/// Threads a review uses at most; the packet does not depend on the count.
const MAX_THREADS: usize = 8;
/// Inputs evaluated between two sequential table updates.
const CHUNK: usize = 1 << 14;

const ADVISORY: &str = "Advisory only. This packet grants no authority and changes no \
application file. It records what the library Authority decides for the contract on the \
listed inputs, where those decisions differ from the owner's decision examples, which \
refusals fall on pre-states that satisfy every state law, and which rule mutants the inputs \
distinguish. Whether a pre-state is reachable from genesis is not decided: a pre-state that \
satisfies every state law may still be unreachable. An undistinguished mutant on a boundary \
set is not equivalent; equivalence is claimed only over a fully enumerated domain.";

/// The application files a review reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReviewSources<'a> {
    /// `project.zeno`.
    pub(crate) project: &'a str,
    /// `v2/policy.json`.
    pub(crate) rules: &'a str,
    /// `tests/decision-examples.txt` when the application keeps one.
    pub(crate) examples: Option<&'a str>,
}

/// A completed review: the packet and its counts.
#[derive(Clone, Debug)]
pub(crate) struct Review {
    packet: String,
    summary: Summary,
    law_refusals: Vec<LawRefusal>,
}

impl Review {
    /// The review packet: canonical JSON with one final newline.
    pub(crate) fn packet(&self) -> &str {
        &self.packet
    }

    pub(crate) fn summary(&self) -> &Summary {
        &self.summary
    }

    /// One line for each law-refusal finding.
    pub(crate) fn law_refusal_lines(&self) -> Vec<String> {
        self.law_refusals.iter().map(LawRefusal::line).collect()
    }
}

/// The counts of a review.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Summary {
    pub(crate) application: String,
    pub(crate) construction: &'static str,
    pub(crate) inputs: usize,
    pub(crate) examples: usize,
    pub(crate) disagreements: usize,
    pub(crate) mutants: usize,
    pub(crate) distinguished: usize,
    pub(crate) refused_by_generator: usize,
    pub(crate) refused_by_library: usize,
    pub(crate) equivalent: usize,
    pub(crate) undistinguished: usize,
    refusals: Tally,
    /// Findings of law refusals on pre-states that satisfy every state law.
    pub(crate) law_refusals: usize,
    pub(crate) findings: usize,
}

impl Summary {
    pub(crate) fn json(&self) -> Value {
        json!({
            "application": self.application,
            "inputs": {"construction": self.construction, "count": self.inputs},
            "examples": {"compared": self.examples, "disagreements": self.disagreements},
            "mutants": {
                "count": self.mutants,
                "distinguished": self.distinguished,
                "refused_by_generator": self.refused_by_generator,
                "refused_by_library": self.refused_by_library,
                "equivalent_over_full_domain": self.equivalent,
                "not_distinguished_within_boundary_set": self.undistinguished,
            },
            "refusals": self.refusals.json(),
            "law_refusal_findings": self.law_refusals,
            "findings": self.findings,
        })
    }

    /// One line on the refused inputs: their count by class, and how many
    /// fall on pre-states that satisfy every state law.
    pub(crate) fn refusals_line(&self) -> String {
        let refusals = &self.refusals;
        if refusals.all.total == 0 {
            return "refusals 0".to_owned();
        }
        format!(
            "refusals {} ({}): {} on pre-states that satisfy every state law ({}), {} on pre-states the state laws exclude ({})",
            refusals.all.total,
            refusals.all.text(),
            refusals.law_consistent.total,
            refusals.law_consistent.text(),
            refusals.excluded.total,
            refusals.excluded.text()
        )
    }
}

/// What the review made of one mutant.
#[derive(Clone, Debug)]
enum Classification {
    /// An input where the mutant decides differently.
    Distinguished(Box<Witness>),
    /// The generator's own checks refuse the mutant rules.
    RefusedByGenerator { place: String, reason: String },
    /// The library's catalog or Authority binding refuses the mutant.
    RefusedByLibrary { place: String, reason: String },
    /// Every input of the fully enumerated domain decides alike.
    EquivalentOverFullDomain,
    /// No input of the boundary set distinguishes the mutant; nothing more.
    NotDistinguishedWithinBoundarySet,
}

impl Classification {
    fn name(&self) -> &'static str {
        match self {
            Self::Distinguished(_) => "distinguished",
            Self::RefusedByGenerator { .. } => "refused-by-generator",
            Self::RefusedByLibrary { .. } => "refused-by-library",
            Self::EquivalentOverFullDomain => "equivalent-over-full-domain",
            Self::NotDistinguishedWithinBoundarySet => "not-distinguished-within-boundary-set",
        }
    }
}

#[derive(Debug)]
struct Classified<'m> {
    mutant: &'m Mutant,
    classification: Classification,
}

/// The first input where a mutant decides differently from the contract.
#[derive(Clone, Debug)]
struct Witness {
    source: WitnessSource,
    input: Vec<i64>,
    original: Outcome,
    mutant: Outcome,
    /// For an owner example: whether its outcome agrees with the contract,
    /// the mutant, both or neither.
    owner_agrees_with: Option<&'static str>,
}

/// Where a witness was found: owner examples are tried first, in file
/// order, then the input set in order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WitnessSource {
    OwnerExample { index: usize, line: usize },
    InputSet { ordinal: usize },
}

/// What the owner should look at.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Finding {
    /// The contract decides an owner example differently.
    ExampleDisagrees { line: usize, difference: String },
    /// An owner example agrees with a mutant and not with the contract.
    ExampleAgreesWithMutant { line: usize, mutant: String },
    /// A law refuses inputs whose pre-state satisfies every state law.
    LawRefusal(LawRefusal),
}

/// One refusal by one law, on inputs whose pre-state satisfies every state
/// law, with the first such input as the witness.
#[derive(Clone, Debug, Eq, PartialEq)]
struct LawRefusal {
    /// The refusing law, as the library's law diagnostics name it; `None`
    /// for a law refusal without a refusing verdict, such as a malformed law
    /// frame, which only the case and the input then name.
    law: Option<u32>,
    /// The refusal as the library reports it.
    refusal: String,
    inputs: usize,
    ordinal: usize,
    input: Vec<i64>,
    /// The case the decision program selects for the witness, and its
    /// `when`, as the library's program evaluator computes it.
    case: Option<(usize, String)>,
}

impl LawRefusal {
    /// The one-line summary a human reads.
    fn line(&self) -> String {
        let law = self
            .law
            .map_or_else(|| "a law".to_owned(), |law| format!("law {law}"));
        let case = self
            .case
            .as_ref()
            .map_or_else(String::new, |(index, when)| {
                format!(", decided by cases[{index}] `{when}`")
            });
        format!(
            "law refusal: {law} refuses {} inputs whose pre-state satisfies every state law ({}); first {}{case}",
            self.inputs,
            self.refusal,
            examples::join(&self.input)
        )
    }
}

/// Reviews a contract. The result is advisory; nothing is written.
///
/// # Errors
/// Returns the first declaration, rule or example with no contract form, or
/// the library's refusal of the contract itself.
pub(crate) fn review(sources: ReviewSources<'_>, max_tuples: u64) -> Result<Review, ContractError> {
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let contract = Contract::build(&declarations, &rules, commitment)?;
    let positions = domain::positions(&declarations)?;
    let state_width = declarations.state_fields()?.len();
    let framer = Framer::new(&positions, &contract);
    let Bound {
        identity,
        examples,
        inputs,
        state_laws,
        table,
        at_examples,
    } = policy::with_authority(&contract, &schema, |authority| {
        let descriptor = authority.descriptor();
        // The examples are read against the descriptor, as the application
        // reads them.
        let examples = match sources.examples {
            Some(text) => examples::parse(text, descriptor)?,
            None => Vec::new(),
        };
        let example_inputs: Vec<&[i64]> = examples
            .iter()
            .map(|example| example.inputs.as_slice())
            .collect();
        let inputs = domain::input_set(
            &positions,
            &rules,
            &declarations,
            &example_inputs,
            max_tuples.max(1),
        )?;
        let state_laws = evaluate::state_laws(descriptor);
        let mut table = Table::default();
        let mut start = 0;
        while start < inputs.len() {
            let end = (start + CHUNK).min(inputs.len());
            let outcomes = parallel(end - start, |offset| {
                let tuple = inputs.tuple(start + offset);
                let outcome = evaluate::evaluate(authority, &framer, tuple);
                // Only a refusal's pre-state is checked against the state laws.
                let unsatisfied = match outcome {
                    Outcome::Refused(_) => evaluate::first_unsatisfied_state_law(
                        descriptor,
                        &state_laws,
                        &framer,
                        tuple,
                    ),
                    Outcome::Decision(_) => None,
                };
                (outcome, unsatisfied)
            });
            for (outcome, unsatisfied) in &outcomes {
                table.record(outcome, *unsatisfied);
            }
            start = end;
        }
        let at_examples: Vec<Outcome> = examples
            .iter()
            .map(|example| evaluate::evaluate(authority, &framer, &example.inputs))
            .collect();
        Ok::<_, ContractError>(Bound {
            identity: evaluate::hex(&evaluate::digest(authority.identity())),
            examples,
            inputs,
            state_laws,
            table,
            at_examples,
        })
    })??;
    let agreement: Vec<Result<(), String>> = examples
        .iter()
        .zip(&at_examples)
        .map(|(example, outcome)| examples::agrees(example, outcome, state_width))
        .collect();

    let catalog = mutants::catalog(&rules, &declarations);
    let context = Context {
        declarations: &declarations,
        commitment,
        schema: &schema,
        framer: &framer,
        examples: &examples,
        at_examples: &at_examples,
        inputs: &inputs,
        table: &table,
        state_width,
    };
    let classifications = parallel(catalog.len(), |index| classify(&catalog[index], &context));
    let classified: Vec<Classified<'_>> = catalog
        .iter()
        .zip(classifications)
        .map(|(mutant, classification)| Classified {
            mutant,
            classification,
        })
        .collect();

    let mut findings = Vec::new();
    for (example, agreement) in examples.iter().zip(&agreement) {
        if let Err(difference) = agreement {
            findings.push(Finding::ExampleDisagrees {
                line: example.line,
                difference: difference.clone(),
            });
        }
    }
    for item in &classified {
        if let Classification::Distinguished(witness) = &item.classification
            && let WitnessSource::OwnerExample { line, .. } = witness.source
            && witness.owner_agrees_with == Some("mutant")
        {
            findings.push(Finding::ExampleAgreesWithMutant {
                line,
                mutant: item.mutant.id.clone(),
            });
        }
    }
    let groups = refusals::groups(&table);
    // Naming a witness's case is a courtesy; without the program it is omitted.
    let program = contract.program().ok();
    let law_refusals: Vec<LawRefusal> = groups
        .iter()
        .filter(|group| group.unsatisfied.is_none())
        .filter_map(|group| law_refusal(group, &table, &inputs, program.as_ref(), &rules))
        .collect();
    findings.extend(law_refusals.iter().cloned().map(Finding::LawRefusal));

    let count = |wanted: &str| {
        classified
            .iter()
            .filter(|item| item.classification.name() == wanted)
            .count()
    };
    let summary = Summary {
        application: rules.template.clone(),
        construction: inputs.construction.name(),
        inputs: inputs.len(),
        examples: examples.len(),
        disagreements: agreement.iter().filter(|result| result.is_err()).count(),
        mutants: classified.len(),
        distinguished: count("distinguished"),
        refused_by_generator: count("refused-by-generator"),
        refused_by_library: count("refused-by-library"),
        equivalent: count("equivalent-over-full-domain"),
        undistinguished: count("not-distinguished-within-boundary-set"),
        refusals: Tally::of(&table, &groups),
        law_refusals: law_refusals.len(),
        findings: findings.len(),
    };
    let sha256 = |bytes: &[u8]| evaluate::hex(&evaluate::digest(bytes));
    let report = packet::Report {
        application: &rules.template,
        identity: &identity,
        sources: json!({
            "project_sha256": sha256(sources.project.as_bytes()),
            "rules_sha256": sha256(sources.rules.as_bytes()),
            "schema_sha256": sha256(&schema),
            "examples_sha256": sources.examples.map(|text| sha256(text.as_bytes())),
        }),
        positions: &positions,
        inputs: &inputs,
        max_tuples,
        table: &table,
        state_laws: &state_laws,
        groups: &groups,
        examples_file: sources.examples.map(|_| examples::FILE),
        examples: &examples,
        at_examples: &at_examples,
        agreement: &agreement,
        classified: &classified,
        findings: &findings,
        summary: &summary,
        state_width,
    };
    Ok(Review {
        packet: crate::transform::canonical_json(&packet::build(&report)),
        summary,
        law_refusals,
    })
}

/// The finding a group of refused inputs makes when a law refused them and
/// their pre-states satisfy every state law, with the case the library's
/// program evaluator selects for the first of them.
fn law_refusal(
    group: &Group,
    table: &Table,
    inputs: &InputSet,
    program: Option<&zeno_fcis_synthesis::finite::Program>,
    rules: &Rules,
) -> Option<LawRefusal> {
    let refusal = &table.refusals[group.refusal as usize];
    if refusal.class != evaluate::RefusalClass::Law {
        return None;
    }
    let input = inputs.tuple(group.first).to_vec();
    // Output 0 of a generated decision program is the selected case's index.
    let case = program
        .and_then(|program| program.evaluate(&input).ok())
        .and_then(|outputs| usize::try_from(*outputs.first()?).ok())
        .and_then(|index| Some((index, mutants::render(&rules.cases.get(index)?.when))));
    Some(LawRefusal {
        law: refusal.law,
        refusal: refusal.text.clone(),
        inputs: group.count,
        ordinal: group.first,
        input,
        case,
    })
}

/// Runs `work` on every index below `count`, on as many threads as the host
/// offers up to `MAX_THREADS`, and returns the results in index order. The
/// results never depend on the thread count or the scheduling.
fn parallel<T: Send>(count: usize, work: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(MAX_THREADS)
        .min(count);
    if threads <= 1 {
        return (0..count).map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, T)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        if index >= count {
                            break done;
                        }
                        done.push((index, work(index)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|_| panic!("a review thread panicked"))
            })
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

/// What the review computes with the contract's Authority bound.
struct Bound {
    /// SHA-256 of the bound Authority's identity, in hexadecimal.
    identity: String,
    examples: Vec<Example>,
    inputs: InputSet,
    /// The contract's state laws, in its law order.
    state_laws: Vec<u32>,
    table: Table,
    at_examples: Vec<Outcome>,
}

/// What every mutant is classified against.
struct Context<'c> {
    declarations: &'c Declarations,
    commitment: [u8; 32],
    schema: &'c [u8],
    framer: &'c Framer,
    examples: &'c [Example],
    at_examples: &'c [Outcome],
    inputs: &'c InputSet,
    table: &'c Table,
    state_width: usize,
}

/// Binds the mutant as the generator and the library bind the original,
/// then looks for the first input that distinguishes it.
fn classify(mutant: &Mutant, context: &Context<'_>) -> Classification {
    let contract = match Contract::build(context.declarations, &mutant.rules, context.commitment) {
        Ok(contract) => contract,
        Err(error) => {
            return Classification::RefusedByGenerator {
                place: error.place().to_owned(),
                reason: error.reason().to_owned(),
            };
        }
    };
    match policy::with_authority(&contract, context.schema, |authority| {
        scan(authority, context)
    }) {
        Err(error) => Classification::RefusedByLibrary {
            place: error.place().to_owned(),
            reason: error.reason().to_owned(),
        },
        Ok(Some(witness)) => Classification::Distinguished(Box::new(witness)),
        Ok(None) if context.inputs.construction == Construction::FullDomain => {
            Classification::EquivalentOverFullDomain
        }
        Ok(None) => Classification::NotDistinguishedWithinBoundarySet,
    }
}

/// The first distinguishing input: an owner example, else the first input
/// where the contract decides and the mutant differs, else the first
/// differing input of any kind.
fn scan(authority: &Authority<'_>, context: &Context<'_>) -> Option<Witness> {
    for (index, example) in context.examples.iter().enumerate() {
        let mutant = evaluate::evaluate(authority, context.framer, &example.inputs);
        let original = &context.at_examples[index];
        if mutant != *original {
            let agrees =
                |outcome: &Outcome| examples::agrees(example, outcome, context.state_width).is_ok();
            let owner_agrees_with = match (agrees(original), agrees(&mutant)) {
                (true, true) => "both",
                (true, false) => "original",
                (false, true) => "mutant",
                (false, false) => "neither",
            };
            return Some(Witness {
                source: WitnessSource::OwnerExample {
                    index,
                    line: example.line,
                },
                input: example.inputs.clone(),
                original: original.clone(),
                mutant,
                owner_agrees_with: Some(owner_agrees_with),
            });
        }
    }
    let mut fallback = None;
    for (ordinal, tuple) in context.inputs.tuples().enumerate() {
        let row = context.table.rows[ordinal];
        let mutant = evaluate::evaluate(authority, context.framer, tuple);
        if context.table.matches(row, &mutant) {
            continue;
        }
        let witness = Witness {
            source: WitnessSource::InputSet { ordinal },
            input: tuple.to_vec(),
            original: context.table.outcome(row),
            mutant,
            owner_agrees_with: None,
        };
        if matches!(row, Row::Decided { .. }) {
            return Some(witness);
        }
        if fallback.is_none() {
            fallback = Some(witness);
        }
    }
    fallback
}
