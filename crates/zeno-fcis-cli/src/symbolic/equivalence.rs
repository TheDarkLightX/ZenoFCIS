//! `transform check --symbolic`: equivalence of two admitted finite programs
//! on a domain larger than the exhaustive checker's tuple cap.
//!
//! The declared input domain is split by what the original program does. One
//! piece holds the inputs on which its first output has one value; a
//! contract's decision program selects its case there, so there is one piece
//! per case. Two more hold the inputs on which it traps and those on which an
//! output leaves its domain. Each piece is one query asking for an input on
//! which the candidate's result differs from the original's: another case,
//! another output, or another failure. A candidate case that overlaps an
//! original case is such an input unless the two are the same case, so every
//! overlapping pair of cases is covered. The pieces cover the domain by
//! construction: every input either traps, leaves an output domain, or
//! yields outputs, and then its first output is one of its domain's values.
//!
//! A counterexample is replayed: both programs run on it through the
//! library's `execute_v2` at the full Step budget, and their results must
//! differ. Every run also asks a planted control: the candidate with its first
//! output shifted by one must be refuted, replayed the same way, with the
//! model's encoded outputs equal to the evaluator's.
//!
//! The result is never an exhaustive receipt. Its receipt has its own schema,
//! which `transform replay`, `contract adopt` and contract generation do not
//! read.

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    Domain, PROFILE, Program, V2_EXECUTION_PROFILE, V2ExecutionFailure, V2Resource, execute_v2,
    v2_authority::EVALUATOR, v2_zero_limits,
};
use zeno_fcis_synthesis::finite_runtime::import_program;

use super::program::{self, Encoded};
use super::smt::{Script, Term};
use super::verdict::{Answers, Judgment, Query, Solve, judge};
use crate::transform::{self, Refusal, Side};

/// Version of the receipt format.
pub(crate) const RECEIPT_SCHEMA: &str = "zeno-fcis/transform-symbolic-receipt/1";
/// The semantics of this check: its pieces, queries, control and replay.
pub(crate) const CHECKER: &str = "zeno-fcis/transform-symbolic/1";
/// The most values of the original's first output that get a piece each; a
/// wider first output is checked as one piece.
const MAX_CASE_PIECES: u128 = 256;
/// The commands that never read a symbolic receipt.
const NOT_ACCEPTED_BY: [&str; 4] = [
    "transform replay",
    "contract adopt",
    "generate contract",
    "contract refresh-receipts",
];

/// What the check concluded.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// The programs cannot be compared, for the reason the exhaustive
    /// checker gives.
    Refused(Refusal),
    /// The domain fits the exhaustive cap, so the exhaustive check applies.
    ExhaustiveFits { size: u64, limit: u64 },
    /// Every piece held and the planted control was refuted.
    Equivalent(Box<Equivalence>),
    /// A replayed input on which the results differ.
    Counterexample(Box<Counterexample>),
    /// Nothing was decided.
    Inconclusive(Box<Inconclusive>),
}

/// A replayed difference: both programs' results on one input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Difference {
    pub(crate) input: Vec<i64>,
    pub(crate) original: Result<Vec<i64>, V2ExecutionFailure>,
    pub(crate) candidate: Result<Vec<i64>, V2ExecutionFailure>,
}

/// One piece of the domain and its query's judgment.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub(crate) label: String,
    pub(crate) meaning: String,
    pub(crate) query_sha256: String,
    pub(crate) answers: Answers,
    pub(crate) judgment: Judgment<Difference>,
}

/// The planted control and its judgment.
#[derive(Clone, Debug)]
pub(crate) struct Control {
    pub(crate) query_sha256: String,
    pub(crate) answers: Answers,
    pub(crate) judgment: Judgment<Difference>,
}

/// What a decided check knows about both programs.
#[derive(Clone, Debug)]
pub(crate) struct Compared {
    original: Artifact,
    candidate: Artifact,
    inputs: Vec<Domain>,
    outputs: Vec<Domain>,
    size: Option<u128>,
    limits: transform::Limits,
    pub(crate) pieces: Vec<Piece>,
    pub(crate) control: Control,
}

#[derive(Debug)]
pub(crate) struct Equivalence {
    pub(crate) compared: Compared,
}

#[derive(Debug)]
pub(crate) struct Counterexample {
    pub(crate) compared: Compared,
    /// The first refuted piece.
    pub(crate) piece: usize,
}

#[derive(Debug)]
pub(crate) struct Inconclusive {
    pub(crate) cause: String,
    pub(crate) compared: Option<Compared>,
}

#[derive(Clone, Debug)]
struct Artifact {
    sha256: String,
    bytes: usize,
    nodes: usize,
}

impl Artifact {
    fn new(bytes: &[u8], program: &Program) -> Self {
        Self {
            sha256: transform::sha256_hex(bytes),
            bytes: bytes.len(),
            nodes: program.nodes().len(),
        }
    }

    fn json(&self) -> Value {
        json!({"sha256": self.sha256, "bytes": self.bytes, "nodes": self.nodes})
    }
}

/// Admits both programs as the exhaustive checker does, then, for a domain
/// above `limits.input_tuples`, asks one query per piece and the planted
/// control through `solve`.
pub(crate) fn check(
    original_bytes: &[u8],
    candidate_bytes: &[u8],
    limits: transform::Limits,
    solve: &mut Solve<'_>,
) -> Outcome {
    let admitted = |bytes: &[u8], side: Side| {
        import_program(bytes).map_err(|error| {
            let code = match error {
                zeno_fcis_synthesis::finite::Error::Invalid(code)
                | zeno_fcis_synthesis::finite::Error::Limit(code) => code,
                _ => "unclassified",
            };
            Refusal::NotAdmitted { side, code }
        })
    };
    let original = match admitted(original_bytes, Side::Original) {
        Ok(program) => program,
        Err(refusal) => return Outcome::Refused(refusal),
    };
    let candidate = match admitted(candidate_bytes, Side::Candidate) {
        Ok(program) => program,
        Err(refusal) => return Outcome::Refused(refusal),
    };
    if original.inputs() != candidate.inputs() {
        return Outcome::Refused(Refusal::InputAbi {
            original: original.inputs().to_vec(),
            candidate: candidate.inputs().to_vec(),
        });
    }
    if original.outputs() != candidate.outputs() {
        return Outcome::Refused(Refusal::OutputAbi {
            original: original.outputs().to_vec(),
            candidate: candidate.outputs().to_vec(),
        });
    }
    let size = match transform::domain_size(original.inputs()) {
        Ok(size) => size,
        Err(position) => return Outcome::Refused(Refusal::EmptyInputDomain { position }),
    };
    if let Some(size) = size
        .and_then(|size| u64::try_from(size).ok())
        .filter(|size| *size <= limits.input_tuples)
    {
        return Outcome::ExhaustiveFits {
            size,
            limit: limits.input_tuples,
        };
    }
    let inconclusive = |cause: String| {
        Outcome::Inconclusive(Box::new(Inconclusive {
            cause,
            compared: None,
        }))
    };
    // A program that does not trap runs every node, one Step each.
    let most = original.nodes().len().max(candidate.nodes().len());
    if u64::try_from(most).map_or(true, |most| most > limits.steps) {
        return inconclusive(format!(
            "budget-boundary: the Step limit {} is below a program's {most} nodes, so it binds on \
             every input where that program does not trap; the symbolic check does not decide where",
            limits.steps
        ));
    }
    let queries = match Queries::build(&original, &candidate) {
        Ok(queries) => queries,
        Err(unsupported) => return inconclusive(format!("unsupported: {}", unsupported.reason())),
    };
    let replay =
        |model: &std::collections::BTreeMap<String, i128>| difference(&original, &candidate, model);
    let mut pieces = Vec::new();
    for prepared in &queries.pieces {
        let answers = solve(&prepared.query());
        let judgment = judge(&answers, replay);
        pieces.push(Piece {
            label: prepared.label.clone(),
            meaning: prepared.meaning.clone(),
            query_sha256: transform::sha256_hex(prepared.source.as_bytes()),
            answers,
            judgment,
        });
    }
    let answers = solve(&queries.control.query());
    let judgment = judge(&answers, |model| {
        planted_difference(&original, &candidate, model)
    });
    let control = Control {
        query_sha256: transform::sha256_hex(queries.control.source.as_bytes()),
        answers,
        judgment,
    };
    let compared = Compared {
        original: Artifact::new(original_bytes, &original),
        candidate: Artifact::new(candidate_bytes, &candidate),
        inputs: original.inputs().to_vec(),
        outputs: original.outputs().to_vec(),
        size,
        limits,
        pieces,
        control,
    };
    if let Some(piece) = compared
        .pieces
        .iter()
        .position(|piece| matches!(piece.judgment, Judgment::Refuted { .. }))
    {
        return Outcome::Counterexample(Box::new(Counterexample { compared, piece }));
    }
    let control = match &compared.control.judgment {
        Judgment::Refuted { .. } => None,
        Judgment::Inconclusive(reason) => {
            Some(format!("the planted control was not refuted: {reason}"))
        }
        Judgment::Holds { .. } => Some(
            "the planted control was not refuted: CVC5 proposed that no input gives both \
             programs outputs"
                .to_owned(),
        ),
    };
    let cause = control.or_else(|| {
        compared
            .pieces
            .iter()
            .find_map(|piece| match &piece.judgment {
                Judgment::Inconclusive(reason) => Some(format!("{}: {reason}", piece.label)),
                _ => None,
            })
    });
    match cause {
        Some(cause) => Outcome::Inconclusive(Box::new(Inconclusive {
            cause,
            compared: Some(compared),
        })),
        None => Outcome::Equivalent(Box::new(Equivalence { compared })),
    }
}

/// One query of a check, ready to run.
struct Prepared {
    label: String,
    meaning: String,
    #[cfg(test)]
    script: Script,
    source: String,
}

impl Prepared {
    fn new(label: String, meaning: String, script: Script) -> Self {
        Self {
            label,
            meaning,
            source: script.source(),
            #[cfg(test)]
            script,
        }
    }

    fn query(&self) -> Query<'_> {
        Query {
            label: &self.label,
            #[cfg(test)]
            script: &self.script,
            source: &self.source,
        }
    }
}

/// The queries of one check.
struct Queries {
    /// One query per piece of the domain, in order.
    pieces: Vec<Prepared>,
    control: Prepared,
}

impl Queries {
    fn build(original: &Program, candidate: &Program) -> Result<Self, super::Unsupported> {
        let base = |purpose: &str| -> Result<(Script, Encoded, Encoded), super::Unsupported> {
            let mut script = Script::default();
            script.comment(&format!("zeno-fcis/transform-symbolic/1 {purpose}"));
            for (index, domain) in original.inputs().iter().enumerate() {
                script.constant(&format!("x{index}"), program::input_values(*domain)?);
            }
            let input = |index: usize| Term::name(&format!("x{index}"));
            let left = program::encode(&mut script, "p", program::Program::of(original), &input)?;
            let right = program::encode(&mut script, "q", program::Program::of(candidate), &input)?;
            Ok((script, left, right))
        };
        let same = |left: &Encoded, right: &Encoded| {
            Term::and(
                std::iter::once(right.ok()).chain(
                    left.outputs
                        .iter()
                        .zip(&right.outputs)
                        .map(|(a, b)| Term::eq(a.clone(), b.clone())),
                ),
            )
        };
        let mut pieces = Vec::new();
        let first = original
            .outputs()
            .first()
            .map(|domain| domain.bounds())
            .map(|(min, max)| (i128::from(min), i128::from(max)));
        match first {
            Some((min, max))
                if u128::try_from(max - min).is_ok_and(|width| width < MAX_CASE_PIECES) =>
            {
                for value in min..=max {
                    let (mut script, left, right) = base(&format!("case {value}"))?;
                    script.assert(left.ok());
                    script.assert(Term::eq(left.outputs[0].clone(), Term::Int(value)));
                    script.assert(Term::not(same(&left, &right)));
                    pieces.push(Prepared::new(
                        format!("case-{value}"),
                        format!(
                            "inputs on which the original returns outputs with first output {value}"
                        ),
                        script,
                    ));
                }
            }
            _ => {
                let (mut script, left, right) = base("defined outputs")?;
                script.assert(left.ok());
                script.assert(Term::not(same(&left, &right)));
                pieces.push(Prepared::new(
                    "outputs".to_owned(),
                    "inputs on which the original returns outputs".to_owned(),
                    script,
                ));
            }
        }
        let (mut script, left, right) = base("arithmetic failure")?;
        script.assert(left.trap.clone());
        script.assert(Term::not(right.trap.clone()));
        pieces.push(Prepared::new(
            "arithmetic-failure".to_owned(),
            "inputs on which the original fails with Arithmetic".to_owned(),
            script,
        ));
        let (mut script, left, right) = base("output domain failure")?;
        script.assert(left.out_of_domain());
        script.assert(Term::not(right.out_of_domain()));
        pieces.push(Prepared::new(
            "output-domain-failure".to_owned(),
            "inputs on which the original fails with OutputDomain".to_owned(),
            script,
        ));
        let (mut script, left, right) =
            base("planted control: the candidate's first output plus one")?;
        script.assert(left.ok());
        script.assert(right.ok());
        if let (Some(a), Some(b)) = (left.outputs.first(), right.outputs.first()) {
            script.observe("original_first", a.clone());
            script.observe("candidate_first", b.clone());
            script.assert(Term::not(Term::eq(
                Term::add(b.clone(), Term::Int(1)),
                a.clone(),
            )));
        }
        Ok(Self {
            pieces,
            control: Prepared::new(
                "control-shifted-first-output".to_owned(),
                "the candidate with its first output plus one".to_owned(),
                script,
            ),
        })
    }
}

/// Runs a program through the library evaluator at the full Step budget.
fn run(program: &Program, input: &[i64]) -> Result<Vec<i64>, V2ExecutionFailure> {
    let meter = v2_zero_limits().with_limit(V2Resource::Step, transform::FULL_BUDGET);
    execute_v2(
        program.inputs(),
        program.outputs(),
        program.nodes(),
        program.roots(),
        input,
        meter,
    )
    .into_parts()
    .0
}

/// The model's input, inside the declared domains.
fn model_input(
    program: &Program,
    model: &std::collections::BTreeMap<String, i128>,
) -> Result<Vec<i64>, String> {
    program
        .inputs()
        .iter()
        .enumerate()
        .map(|(index, domain)| {
            let name = format!("x{index}");
            let value = *model
                .get(&name)
                .ok_or_else(|| format!("the model has no value for {name}"))?;
            let value = i64::try_from(value)
                .map_err(|_| format!("{name} = {value} is outside the 64-bit range"))?;
            if domain.contains(value) {
                Ok(value)
            } else {
                Err(format!("{name} = {value} is outside its declared domain"))
            }
        })
        .collect()
}

/// A counterexample model, confirmed only when the library evaluator gives
/// the two programs different results on its input.
fn difference(
    original: &Program,
    candidate: &Program,
    model: &std::collections::BTreeMap<String, i128>,
) -> Result<Difference, String> {
    let input = model_input(original, model)?;
    let left = run(original, &input);
    let right = run(candidate, &input);
    if left == right {
        return Err(format!(
            "the library evaluator gives both programs the same result on {input:?}"
        ));
    }
    Ok(Difference {
        input,
        original: left,
        candidate: right,
    })
}

/// The planted control's model, confirmed only when the evaluator gives
/// both programs outputs on its input, the model's encoded first outputs
/// are the evaluator's, and the candidate's first output plus one differs
/// from the original's.
fn planted_difference(
    original: &Program,
    candidate: &Program,
    model: &std::collections::BTreeMap<String, i128>,
) -> Result<Difference, String> {
    let input = model_input(original, model)?;
    let left = run(original, &input);
    let right = run(candidate, &input);
    let (Ok(left_values), Ok(right_values)) = (&left, &right) else {
        return Err(format!("a program fails on {input:?}"));
    };
    let (Some(a), Some(b)) = (left_values.first(), right_values.first()) else {
        return Err("the programs have no outputs".to_owned());
    };
    for (name, value) in [("original_first", a), ("candidate_first", b)] {
        if model.get(name) != Some(&i128::from(*value)) {
            return Err(format!(
                "the model's {name} differs from the evaluator's {value} on {input:?}"
            ));
        }
    }
    if i128::from(*b) + 1 == i128::from(*a) {
        return Err(format!(
            "the shifted first output equals the original's on {input:?}"
        ));
    }
    Ok(Difference {
        input,
        original: left,
        candidate: right,
    })
}

fn result_json(result: &Result<Vec<i64>, V2ExecutionFailure>) -> Value {
    match result {
        Ok(values) => json!({"ok": transform::values_json(values)}),
        Err(failure) => json!({"error": transform::failure_tag(*failure)}),
    }
}

impl Difference {
    pub(crate) fn json(&self) -> Value {
        json!({
            "input": transform::values_json(&self.input),
            "original": result_json(&self.original),
            "candidate": result_json(&self.candidate),
        })
    }
}

fn judgment_json(judgment: &Judgment<Difference>) -> Value {
    match judgment {
        Judgment::Holds { corroborated } => json!({
            "status": "holds", "evidence": "attested", "corroborated_by_z3": corroborated
        }),
        Judgment::Refuted {
            witness,
            solver,
            disagreement,
        } => json!({
            "status": "refuted", "evidence": "checked", "model_from": solver.name(),
            "solvers_disagree": disagreement, "replayed": witness.json()
        }),
        Judgment::Inconclusive(reason) => json!({"status": "inconclusive", "reason": reason}),
    }
}

impl Compared {
    /// Every piece and the control, with each solver's answer and time.
    pub(crate) fn json(&self) -> Value {
        json!({
            "pieces": self.pieces.iter().map(|piece| {
                let mut value = judgment_json(&piece.judgment);
                value["piece"] = json!(piece.label);
                value["meaning"] = json!(piece.meaning);
                value["query_sha256"] = json!(piece.query_sha256);
                value["solvers"] = piece.answers.json();
                value
            }).collect::<Vec<_>>(),
            "control": {
                "planted": "the candidate with its first output plus one",
                "query_sha256": self.control.query_sha256,
                "solvers": self.control.answers.json(),
                "result": judgment_json(&self.control.judgment),
            },
        })
    }

    fn domain_json(&self) -> Value {
        json!({
            "inputs": transform::domains_json(&self.inputs),
            "size": self.size.map(|size| size.to_string()),
            "max_input_tuples": self.limits.input_tuples,
        })
    }
}

impl Equivalence {
    /// The receipt's fields. `solvers` names each solver's checked identity.
    /// Times and process output are not part of it.
    pub(crate) fn receipt_value(&self, solvers: &Value) -> Value {
        let compared = &self.compared;
        json!({
            "schema": RECEIPT_SCHEMA,
            "verdict": "attested-equivalent",
            "evidence": "attested",
            "authority": "none",
            "claim": "On every input tuple of the declared domain, the candidate gives the same \
                outputs or the same failure as the original, as CVC5 proposed by answering unsat, \
                with proof steps that were not checked, to one query per piece of the domain. \
                Z3 corroborated each piece marked so. A planted shifted candidate was refuted and \
                replayed through the library evaluator. This is not an exhaustive equivalence: \
                commands that need one refuse this receipt.",
            "not_accepted_by": NOT_ACCEPTED_BY,
            "profile": {"program": PROFILE, "execution": V2_EXECUTION_PROFILE},
            "original": compared.original.json(),
            "candidate": compared.candidate.json(),
            "domain": compared.domain_json(),
            "outputs": transform::domains_json(&compared.outputs),
            "limits": {"steps": compared.limits.steps},
            "step_bound": {
                "original": compared.original.nodes,
                "candidate": compared.candidate.nodes,
                "usage_preserved": Value::Null,
            },
            "pieces": compared.pieces.iter().map(|piece| json!({
                "piece": piece.label,
                "query_sha256": piece.query_sha256,
                "corroborated_by_z3": matches!(piece.judgment, Judgment::Holds { corroborated: true }),
            })).collect::<Vec<_>>(),
            "control": {
                "planted": "the candidate with its first output plus one",
                "query_sha256": compared.control.query_sha256,
                "refuted_by": match &compared.control.judgment {
                    Judgment::Refuted { solver, .. } => json!(solver.name()),
                    _ => Value::Null,
                },
            },
            "solvers": solvers,
            "checker": {
                "semantics": CHECKER,
                "evaluator_identity": hex(&EVALUATOR),
            },
        })
    }

    /// Canonical receipt bytes, in the form every receipt this crate writes.
    pub(crate) fn receipt(&self, solvers: &Value) -> Vec<u8> {
        transform::canonical_json(&self.receipt_value(solvers)).into_bytes()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests;
