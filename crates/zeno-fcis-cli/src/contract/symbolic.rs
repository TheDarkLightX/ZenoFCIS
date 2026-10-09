//! `zeno-fcis contract check-symbolic`: whether every committing case of a
//! contract keeps every state law, and an owner's strengthening invariant,
//! on domains far beyond enumeration.
//!
//! For each committing case and each target (each state law, then each
//! strengthening clause) one query asks for an input tuple whose pre-state
//! satisfies every state law and every clause, on which the bound
//! Authority's decision program selects the case and the Authority would
//! construct and validate its decision, and whose successor breaks the
//! target. No such tuple means the case never needs a state-law refusal
//! from a lawful pre-state and preserves the strengthening. With genesis
//! satisfying every clause, which the library law evaluator checks, the
//! clauses then hold on every state the application can reach.
//!
//! Every run also asks planted controls that a working solver must refute:
//! that some lawful pre-state exists (genesis is one), that each case is
//! reached from one, and that each target can fail on some state of its
//! domain. A control that is not refuted makes the case vacuous, the target
//! implied by its domain, or, for the first, the whole run inconclusive.
//!
//! When the domain fits the exhaustive cap, every tuple also runs through
//! the bound Authority and the library law evaluator, and that route
//! decides: a symbolic result never replaces it, and the two must agree.
//!
//! Scope: the rules' own decision program and the declared state laws.
//! Laws that read the decision, the command or the context are not
//! targets; the Authority enforces them at run time and `contract review`
//! reports their refusals on small domains.

mod encode;
mod library;
mod strengthening;

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{v2_authority::Authority, v2_composition as c, v2_laws as l};

use super::ContractError;
use super::declarations::{Declarations, Source};
use super::graph::Atom;
use super::model::Contract;
use super::policy;
use super::review::domain::{self, Position};
use super::review::evaluate::{self, Framer};
use super::rules::Rules;
use super::{schema, schema_commitment};
use crate::symbolic::Unsupported;
use crate::symbolic::smt::Script;
use crate::symbolic::verdict::{Answers, Judgment, Query, Solve, judge};
use encode::{Branch, Decision, Typed};
use library::{Decided, LawProgram};

/// Version of the report format.
pub(crate) const REPORT_SCHEMA: &str = "zeno-fcis/symbolic-check/1";
/// The largest domain the exhaustive route enumerates by default: the
/// review's.
pub(crate) const DEFAULT_MAX_TUPLES: u64 = super::review::DEFAULT_MAX_TUPLES;
/// The first law ID strengthening clauses take in the library evaluator,
/// far above every generated law.
const CLAUSE_IDS: u32 = 4_000_000_000;

/// The files a check reads.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SymbolicSources<'a> {
    /// `project.zeno`.
    pub(crate) project: &'a str,
    /// `v2/policy.json`.
    pub(crate) rules: &'a str,
    /// The owner's strengthening file, when one is given.
    pub(crate) strengthening: Option<&'a str>,
}

/// The check's overall result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    /// Every target holds on every case by exhaustive enumeration.
    Proved,
    /// Every target holds on every case, by CVC5's `unsat` answers.
    Attested,
    /// A replayed counterexample breaks a target.
    Refuted,
    /// Something was not decided.
    Inconclusive,
}

impl Status {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Proved | Self::Attested => "holds",
            Self::Refuted => "refuted",
            Self::Inconclusive => "inconclusive",
        }
    }

    pub(crate) fn evidence(self) -> &'static str {
        match self {
            Self::Proved => "proved",
            Self::Attested => "attested",
            Self::Refuted => "checked",
            Self::Inconclusive => "none",
        }
    }
}

/// A finished check: its status, its report, and one line per finding.
#[derive(Clone, Debug)]
pub(crate) struct Checked {
    pub(crate) status: Status,
    pub(crate) report: Value,
    pub(crate) lines: Vec<String>,
}

/// A target: a state law or a strengthening clause.
struct Target<'a> {
    label: String,
    /// `law-500` or `clause-name`, for query labels.
    slug: String,
    program: LawProgram<'a>,
}

/// A replayed tuple and what the library made of it.
#[derive(Clone, Debug)]
struct Witness {
    tuple: Vec<i64>,
    case: Option<usize>,
    post: Vec<(u16, i128)>,
    target: Option<&'static str>,
    decided: Option<String>,
}

impl Witness {
    fn json(&self, positions: &[Position]) -> Value {
        json!({
            "input": positions.iter().zip(&self.tuple).map(|(position, value)| {
                json!({"name": position.name, "value": value.to_string()})
            }).collect::<Vec<_>>(),
            "values": self.tuple.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "case": self.case,
            "post": self.post.iter().map(|(field, value)| {
                json!({"field": field, "value": value.to_string()})
            }).collect::<Vec<_>>(),
            "target_on_successor": self.target,
            "authority": self.decided,
        })
    }
}

/// One query's question, answers and judgment.
struct Asked {
    label: String,
    query_sha256: Option<String>,
    answers: Option<Answers>,
    judgment: Judgment<Witness>,
}

impl Asked {
    fn json(&self, positions: &[Position]) -> Value {
        let mut value = match &self.judgment {
            Judgment::Holds { corroborated } => json!({
                "status": "unsat", "evidence": "attested", "corroborated_by_z3": corroborated
            }),
            Judgment::Refuted {
                witness,
                solver,
                disagreement,
            } => json!({
                "status": "sat", "evidence": "checked", "model_from": solver.name(),
                "solvers_disagree": disagreement, "replayed": witness.json(positions)
            }),
            Judgment::Inconclusive(reason) => json!({"status": "inconclusive", "reason": reason}),
        };
        value["query"] = json!(self.label);
        value["query_sha256"] = json!(self.query_sha256);
        value["solvers"] = self.answers.as_ref().map_or(Value::Null, Answers::json);
        value
    }
}

/// What enumeration found for one case and target.
#[derive(Clone, Debug)]
enum Enumerated {
    /// No lawful decision of the case breaks the target; `decisions` counts
    /// the lawful decisions that reached law evaluation.
    Holds { decisions: u64 },
    /// The first tuple that breaks it, in enumeration order, and the count.
    Refuted { witness: Box<Witness>, count: u64 },
}

/// Checks a contract. Every query goes through `solve`; a solver that is
/// not configured answers so, and the query is inconclusive.
///
/// # Errors
/// The contract or the strengthening file has no form this check reads, or
/// the library refuses the contract.
pub(crate) fn check(
    sources: SymbolicSources<'_>,
    max_tuples: u64,
    solve: &mut Solve<'_>,
) -> Result<Checked, ContractError> {
    let rules = Rules::read(sources.rules)?;
    let declarations = Declarations::read(sources.project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let commitment = schema_commitment(&schema)?;
    let contract = Contract::build(&declarations, &rules, commitment)?;
    let positions = domain::positions(&declarations)?;
    let framer = Framer::new(&positions, &contract);
    let clauses = match sources.strengthening {
        Some(text) => strengthening::read(text, &declarations)?,
        None => Vec::new(),
    };
    let clause_ops: Vec<Vec<l::Op<'_>>> = clauses
        .iter()
        .map(|clause| clause.nodes.iter().map(policy::law_op).collect())
        .collect();
    let case_texts = case_texts(sources.rules);
    let context = Context {
        positions: &positions,
        framer: &framer,
        rules: &rules,
        case_texts: &case_texts,
        contract: &contract,
        clauses: &clauses,
        clause_ops: &clause_ops,
        sources,
        adoptions: rules.adoptions.len(),
    };
    policy::with_authority(&contract, &schema, |authority| {
        context.run(authority, max_tuples, solve)
    })
}

/// Each case's `rule` and `when` as the rules file writes them; the
/// contract model keeps neither text.
fn case_texts(rules: &str) -> Vec<(Option<String>, Option<String>)> {
    let text = |case: &Value, key: &str| case.get(key).and_then(Value::as_str).map(str::to_owned);
    serde_json::from_str::<Value>(rules)
        .ok()
        .and_then(|value| {
            Some(
                value
                    .get("cases")?
                    .as_array()?
                    .iter()
                    .map(|case| (text(case, "rule"), text(case, "when")))
                    .collect(),
            )
        })
        .unwrap_or_default()
}

/// Everything a check reads, borrowed.
struct Context<'c> {
    positions: &'c [Position],
    framer: &'c Framer,
    rules: &'c Rules,
    case_texts: &'c [(Option<String>, Option<String>)],
    contract: &'c Contract<'c>,
    clauses: &'c [strengthening::Clause],
    clause_ops: &'c [Vec<l::Op<'c>>],
    sources: SymbolicSources<'c>,
    adoptions: usize,
}

impl Context<'_> {
    fn run(&self, authority: &Authority<'_>, max_tuples: u64, solve: &mut Solve<'_>) -> Checked {
        let descriptor = authority.descriptor();
        let state_laws = evaluate::state_laws(descriptor);
        let mut targets: Vec<Target<'_>> = descriptor
            .laws
            .iter()
            .filter(|law| state_laws.contains(&law.id))
            .map(|law| Target {
                label: format!("law {}", law.id),
                slug: format!("law-{}", law.id),
                program: LawProgram {
                    id: law.id,
                    nodes: law.program.nodes,
                    root: law.program.root,
                },
            })
            .collect();
        let laws = targets.len();
        for (index, (clause, ops)) in self.clauses.iter().zip(self.clause_ops).enumerate() {
            targets.push(Target {
                label: format!("strengthening {}", clause.name),
                slug: format!("clause-{}", clause.name),
                program: LawProgram {
                    id: CLAUSE_IDS + u32::try_from(index).unwrap_or(u32::MAX - CLAUSE_IDS),
                    nodes: ops,
                    root: clause.root,
                },
            });
        }
        let premises: Vec<LawProgram<'_>> = targets.iter().map(|target| target.program).collect();
        let committing: Vec<usize> = descriptor
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| {
                matches!(branch.class, c::Class::Accept | c::Class::CommittedFailure)
            })
            .map(|(index, _)| index)
            .collect();
        let mut causes = Vec::new();
        if !self.rules.delivery_laws.is_empty() {
            causes.push(format!(
                "unsupported delivery observations in laws {:?}: enforced by the library at runtime, \
                 but not symbolic state-law targets; this check cannot prove these delivery laws",
                self.rules.delivery_laws.keys().collect::<Vec<_>>()
            ));
        }

        // Genesis: the base of the induction the clauses rest on.
        let genesis = self.genesis_tuple();
        let genesis_state = library::state(self.framer, &genesis);
        let at_genesis: Vec<Value> = targets
            .iter()
            .map(|target| {
                let verdict = library::verdict(descriptor, target.program, &genesis_state);
                if verdict != Some(l::Verdict::Satisfied) {
                    causes.push(format!(
                        "{} does not hold on the genesis state ({})",
                        target.label,
                        library::verdict_name(verdict)
                    ));
                }
                json!({"target": target.label, "genesis": library::verdict_name(verdict)})
            })
            .collect();

        let replays = Replays {
            context: self,
            descriptor,
            authority,
            premises: &premises,
        };
        let mut ask = |label: String, script: Result<Script, Unsupported>, replay: &Replay<'_>| {
            match script {
                Ok(script) => {
                    let source = script.source();
                    let answers = solve(&Query {
                        label: &label,
                        #[cfg(test)]
                        script: &script,
                        source: &source,
                    });
                    let judgment = judge(&answers, replay);
                    Asked {
                        query_sha256: Some(crate::transform::sha256_hex(source.as_bytes())),
                        label,
                        answers: Some(answers),
                        judgment,
                    }
                }
                Err(unsupported) => Asked {
                    label,
                    query_sha256: None,
                    answers: None,
                    judgment: Judgment::Inconclusive(format!(
                        "unsupported: {}",
                        unsupported.reason()
                    )),
                },
            }
        };
        let queries = Queries {
            descriptor,
            positions: self.positions,
            premises: &premises,
        };

        let premise = ask("premise".to_owned(), queries.premise(), &|model| {
            replays.premise(model)
        });
        let reach: Vec<Asked> = committing
            .iter()
            .map(|case| {
                ask(
                    format!("reach-case-{case}"),
                    queries.reach(*case),
                    &|model| replays.reach(*case, model),
                )
            })
            .collect();
        let domain_only: Vec<Asked> = targets
            .iter()
            .map(|target| {
                ask(
                    format!("domain-{}", target.slug),
                    queries.domain_only(target.program),
                    &|model| replays.domain_only(target.program, model),
                )
            })
            .collect();
        let mut asked: Vec<Vec<Asked>> = Vec::new();
        for case in &committing {
            asked.push(
                targets
                    .iter()
                    .map(|target| {
                        ask(
                            format!("case-{case}-{}", target.slug),
                            queries.target(*case, target.program),
                            &|model| {
                                replays.target(*case, target, target.program.id < CLAUSE_IDS, model)
                            },
                        )
                    })
                    .collect(),
            );
        }

        let size = domain::domain_size(self.positions);
        let enumerated = match size {
            Some(size) if size <= u128::from(max_tuples) => {
                Some(replays.enumerate(&targets, &committing, max_tuples))
            }
            _ => None,
        };
        self.report(Assembled {
            descriptor,
            authority,
            targets: &targets,
            laws,
            committing: &committing,
            premise,
            reach,
            domain_only,
            asked,
            enumerated,
            size,
            max_tuples,
            at_genesis,
            causes,
        })
    }

    /// The genesis state, with every other input at its domain's least value.
    fn genesis_tuple(&self) -> Vec<i64> {
        self.positions
            .iter()
            .map(|position| {
                let genesis = match (position.source, position.field) {
                    (Source::State, Some(field)) => self
                        .contract
                        .genesis
                        .iter()
                        .find(|(id, _)| *id == field)
                        .and_then(|(_, atom)| match atom {
                            Atom::Bool(value) => Some(i64::from(*value)),
                            Atom::I128(value) => i64::try_from(*value).ok(),
                            Atom::Sum { variant, .. } => Some(i64::from(*variant)),
                            _ => None,
                        }),
                    _ => None,
                };
                genesis.unwrap_or_else(|| position.domain.least().unwrap_or(0))
            })
            .collect()
    }
}

/// Confirms a model through the library, or says why it does not replay.
type Replay<'r> = dyn Fn(&std::collections::BTreeMap<String, i128>) -> Result<Witness, String> + 'r;

/// Builds the queries of one check.
struct Queries<'q> {
    descriptor: &'q c::Descriptor<'q>,
    positions: &'q [Position],
    /// Every state law and clause, assumed on the pre-state.
    premises: &'q [LawProgram<'q>],
}

impl Queries<'_> {
    /// The inputs, the program, and every premise on the pre-state.
    fn base(&self, purpose: &str) -> Result<(Script, Decision), Unsupported> {
        let mut script = Script::default();
        script.comment(&format!("zeno-fcis/symbolic-check/1 {purpose}"));
        for (index, position) in self.positions.iter().enumerate() {
            script.comment(&format!("x{index} is {}", position.name));
        }
        let decision = Decision::encode(&mut script, self.descriptor, self.positions)?;
        for (index, premise) in self.premises.iter().enumerate() {
            let holds = encode::law(
                &mut script,
                &format!("pre{index}_"),
                premise.nodes,
                premise.root,
                &|field| decision.pre_state(field),
            )?;
            script.assert(holds);
        }
        Ok((script, decision))
    }

    /// Asserts that case `case` commits, and observes its successor.
    fn commit(
        &self,
        script: &mut Script,
        decision: &Decision,
        case: usize,
    ) -> Result<Vec<(u16, Typed)>, Unsupported> {
        let branch = Branch::encode(self.descriptor, decision, case)?;
        script.assert(branch.commits);
        for (field, value) in &branch.post {
            if let Some(number) = value.number() {
                script.observe(&format!("post_{field}"), number);
            }
        }
        Ok(branch.post)
    }

    fn premise(&self) -> Result<Script, Unsupported> {
        Ok(self.base("planted control: a lawful pre-state exists")?.0)
    }

    fn reach(&self, case: usize) -> Result<Script, Unsupported> {
        let (mut script, decision) = self.base(&format!(
            "planted control: case {case} commits from a lawful pre-state"
        ))?;
        self.commit(&mut script, &decision, case)?;
        Ok(script)
    }

    fn domain_only(&self, target: LawProgram<'_>) -> Result<Script, Unsupported> {
        let mut script = Script::default();
        script.comment(&format!(
            "zeno-fcis/symbolic-check/1 planted control: law {} can fail on some state",
            target.id
        ));
        let decision = Decision::encode(&mut script, self.descriptor, self.positions)?;
        let holds = encode::law(&mut script, "t_", target.nodes, target.root, &|field| {
            decision.pre_state(field)
        })?;
        script.assert(crate::symbolic::smt::Term::not(holds));
        Ok(script)
    }

    fn target(&self, case: usize, target: LawProgram<'_>) -> Result<Script, Unsupported> {
        let (mut script, decision) = self.base(&format!(
            "case {case} keeps law {} on its successor",
            target.id
        ))?;
        let post = self.commit(&mut script, &decision, case)?;
        let holds = encode::law(&mut script, "t_", target.nodes, target.root, &|field| {
            post.iter()
                .find(|(id, _)| *id == field)
                .map(|(_, value)| value.clone())
        })?;
        script.assert(crate::symbolic::smt::Term::not(holds));
        Ok(script)
    }
}

/// Replays models and enumerates tuples through the library.
struct Replays<'r> {
    context: &'r Context<'r>,
    descriptor: &'r c::Descriptor<'r>,
    authority: &'r Authority<'r>,
    premises: &'r [LawProgram<'r>],
}

impl Replays<'_> {
    /// The model's tuple, inside the declared domains.
    fn tuple(&self, model: &std::collections::BTreeMap<String, i128>) -> Result<Vec<i64>, String> {
        self.context
            .positions
            .iter()
            .enumerate()
            .map(|(index, position)| {
                let name = format!("x{index}");
                let value = model
                    .get(&name)
                    .and_then(|value| i64::try_from(*value).ok())
                    .ok_or_else(|| format!("the model has no 64-bit value for {name}"))?;
                if admits(&position.domain, value) {
                    Ok(value)
                } else {
                    Err(format!(
                        "{} = {value} is outside its declared domain",
                        position.name
                    ))
                }
            })
            .collect()
    }

    /// Whether every premise holds on the tuple's pre-state.
    fn lawful(&self, tuple: &[i64]) -> Result<(), String> {
        let state = library::state(self.context.framer, tuple);
        for premise in self.premises {
            let verdict = library::verdict(self.descriptor, *premise, &state);
            if verdict != Some(l::Verdict::Satisfied) {
                return Err(format!(
                    "law {} is {} on the pre-state",
                    premise.id,
                    library::verdict_name(verdict)
                ));
            }
        }
        Ok(())
    }

    /// The case the program selects on the tuple and its successor.
    fn decide(&self, tuple: &[i64]) -> Result<(usize, Vec<(u16, i128)>), String> {
        let outputs = library::run(self.descriptor, tuple)
            .ok_or_else(|| "the library program fails on the tuple".to_owned())?;
        let case = library::branch(self.descriptor, &outputs)
            .ok_or_else(|| "the program's outputs select no branch".to_owned())?;
        let post = library::successor(
            self.descriptor,
            self.context.positions,
            tuple,
            &outputs,
            case,
        )?;
        Ok((case, post))
    }

    /// The model's observed successor must be the library's.
    fn observed(
        model: &std::collections::BTreeMap<String, i128>,
        post: &[(u16, i128)],
    ) -> Result<(), String> {
        for (field, value) in post {
            if let Some(observed) = model.get(&format!("post_{field}"))
                && observed != value
            {
                return Err(format!(
                    "the query's successor field {field} is {observed}, the library's {value}"
                ));
            }
        }
        Ok(())
    }

    fn premise(&self, model: &std::collections::BTreeMap<String, i128>) -> Result<Witness, String> {
        let tuple = self.tuple(model)?;
        self.lawful(&tuple)?;
        Ok(Witness {
            tuple,
            case: None,
            post: Vec::new(),
            target: None,
            decided: None,
        })
    }

    fn reach(
        &self,
        case: usize,
        model: &std::collections::BTreeMap<String, i128>,
    ) -> Result<Witness, String> {
        let tuple = self.tuple(model)?;
        self.lawful(&tuple)?;
        let (selected, post) = self.decide(&tuple)?;
        if selected != case {
            return Err(format!("the program selects case {selected}, not {case}"));
        }
        Self::observed(model, &post)?;
        let decided = library::decide(self.authority, self.context.framer, &tuple);
        match &decided {
            Decided::Commits(committed) if *committed == post => {}
            Decided::LawRefuses(..) => {}
            other => {
                return Err(format!(
                    "the Authority {} instead of reaching law evaluation with this successor",
                    other.text()
                ));
            }
        }
        Ok(Witness {
            tuple,
            case: Some(case),
            post,
            target: None,
            decided: Some(decided.text()),
        })
    }

    fn domain_only(
        &self,
        target: LawProgram<'_>,
        model: &std::collections::BTreeMap<String, i128>,
    ) -> Result<Witness, String> {
        let tuple = self.tuple(model)?;
        let verdict = library::verdict(
            self.descriptor,
            target,
            &library::state(self.context.framer, &tuple),
        );
        if verdict == Some(l::Verdict::Satisfied) {
            return Err(format!("law {} holds on the model's state", target.id));
        }
        Ok(Witness {
            tuple,
            case: None,
            post: Vec::new(),
            target: Some(library::verdict_name(verdict)),
            decided: None,
        })
    }

    /// A counterexample: a lawful pre-state whose decision by `case` the
    /// Authority constructs, and whose successor breaks the target. For a
    /// declared state law the Authority must refuse the decision by a law;
    /// for a clause, which it does not enforce, it may commit, and then its
    /// successor must be the one resolved here.
    fn target(
        &self,
        case: usize,
        target: &Target<'_>,
        enforced: bool,
        model: &std::collections::BTreeMap<String, i128>,
    ) -> Result<Witness, String> {
        let tuple = self.tuple(model)?;
        self.lawful(&tuple)?;
        let (selected, post) = self.decide(&tuple)?;
        if selected != case {
            return Err(format!("the program selects case {selected}, not {case}"));
        }
        Self::observed(model, &post)?;
        let successor = library::with_state(self.context.positions, &tuple, &post);
        let verdict = library::verdict(
            self.descriptor,
            target.program,
            &library::state(self.context.framer, &successor),
        );
        if verdict == Some(l::Verdict::Satisfied) {
            return Err(format!("{} holds on the library's successor", target.label));
        }
        let decided = library::decide(self.authority, self.context.framer, &tuple);
        match &decided {
            Decided::LawRefuses(..) => {}
            Decided::Commits(committed) if !enforced && *committed == post => {}
            other => {
                return Err(format!(
                    "the Authority {} although {} fails on the successor",
                    other.text(),
                    target.label
                ));
            }
        }
        Ok(Witness {
            tuple,
            case: Some(case),
            post,
            target: Some(library::verdict_name(verdict)),
            decided: Some(decided.text()),
        })
    }

    /// Every tuple of the domain through the bound Authority and the
    /// library law evaluator.
    fn enumerate(
        &self,
        targets: &[Target<'_>],
        committing: &[usize],
        max_tuples: u64,
    ) -> Enumeration {
        let positions = self.context.positions;
        // `None` only for an empty domain beside a wide one: nothing to
        // enumerate, and the wide one is never collected.
        let values: Vec<Vec<i64>> = positions
            .iter()
            .map(|position| position.domain.values_within(max_tuples))
            .collect::<Option<_>>()
            .unwrap_or_else(|| vec![Vec::new(); positions.len()]);
        let state_width = positions
            .iter()
            .take_while(|position| position.source == Source::State)
            .count();
        let mut result = Enumeration {
            reached: vec![None; committing.len()],
            failing_states: vec![false; targets.len()],
            found: vec![vec![Enumerated::Holds { decisions: 0 }; targets.len()]; committing.len()],
            mismatch: None,
        };
        if values.iter().any(Vec::is_empty) {
            return result;
        }
        let mut indices = vec![0usize; positions.len()];
        let mut lawful_cache: Option<(Vec<i64>, bool)> = None;
        loop {
            let tuple: Vec<i64> = indices
                .iter()
                .zip(&values)
                .map(|(index, values)| values[*index])
                .collect();
            let pre = tuple[..state_width].to_vec();
            let lawful = match &lawful_cache {
                Some((cached, lawful)) if *cached == pre => *lawful,
                _ => {
                    let state = library::state(self.context.framer, &tuple);
                    let mut lawful = true;
                    for (index, target) in targets.iter().enumerate() {
                        let verdict = library::verdict(self.descriptor, target.program, &state);
                        if verdict != Some(l::Verdict::Satisfied) {
                            result.failing_states[index] = true;
                            lawful = false;
                        }
                    }
                    lawful_cache = Some((pre, lawful));
                    lawful
                }
            };
            if lawful {
                self.enumerate_one(&tuple, targets, committing, &mut result);
            }
            let mut position = indices.len();
            loop {
                if position == 0 {
                    return result;
                }
                position -= 1;
                indices[position] += 1;
                if indices[position] < values[position].len() {
                    break;
                }
                indices[position] = 0;
            }
        }
    }

    fn enumerate_one(
        &self,
        tuple: &[i64],
        targets: &[Target<'_>],
        committing: &[usize],
        result: &mut Enumeration,
    ) {
        let decided = library::decide(self.authority, self.context.framer, tuple);
        if !matches!(decided, Decided::Commits(_) | Decided::LawRefuses(..)) {
            return;
        }
        let (case, post) = match self.decide(tuple) {
            Ok(decision) => decision,
            Err(reason) => {
                result.mismatch.get_or_insert_with(|| {
                    format!("on {tuple:?} the Authority reached law evaluation, but {reason}")
                });
                return;
            }
        };
        if let Decided::Commits(committed) = &decided
            && *committed != post
        {
            result.mismatch.get_or_insert_with(|| {
                format!("on {tuple:?} the Authority commits {committed:?}, the case table resolves {post:?}")
            });
            return;
        }
        let Some(slot) = committing.iter().position(|committing| *committing == case) else {
            return;
        };
        let witness = |target: Option<&'static str>| Witness {
            tuple: tuple.to_vec(),
            case: Some(case),
            post: post.clone(),
            target,
            decided: Some(decided.text()),
        };
        if result.reached[slot].is_none() {
            result.reached[slot] = Some(witness(None));
        }
        let successor = library::with_state(self.context.positions, tuple, &post);
        let state = library::state(self.context.framer, &successor);
        for (index, target) in targets.iter().enumerate() {
            let verdict = library::verdict(self.descriptor, target.program, &state);
            let found = &mut result.found[slot][index];
            match (verdict == Some(l::Verdict::Satisfied), found) {
                (true, Enumerated::Holds { decisions }) => *decisions += 1,
                (true, Enumerated::Refuted { .. }) => {}
                (false, Enumerated::Refuted { count, .. }) => *count += 1,
                (false, found @ Enumerated::Holds { .. }) => {
                    *found = Enumerated::Refuted {
                        witness: Box::new(witness(Some(library::verdict_name(verdict)))),
                        count: 1,
                    };
                }
            }
        }
    }
}

/// What the exhaustive route found.
struct Enumeration {
    /// The first lawful tuple that reaches each committing case.
    reached: Vec<Option<Witness>>,
    /// Whether some state of the domain fails each target.
    failing_states: Vec<bool>,
    /// Each committing case's result for each target.
    found: Vec<Vec<Enumerated>>,
    /// The first tuple on which the Authority and the case table disagree.
    mismatch: Option<String>,
}

/// Everything a report holds.
struct Assembled<'a> {
    descriptor: &'a c::Descriptor<'a>,
    authority: &'a Authority<'a>,
    targets: &'a [Target<'a>],
    laws: usize,
    committing: &'a [usize],
    premise: Asked,
    reach: Vec<Asked>,
    domain_only: Vec<Asked>,
    asked: Vec<Vec<Asked>>,
    enumerated: Option<Enumeration>,
    size: Option<u128>,
    max_tuples: u64,
    at_genesis: Vec<Value>,
    causes: Vec<String>,
}

impl Context<'_> {
    fn report(&self, assembled: Assembled<'_>) -> Checked {
        let Assembled {
            descriptor,
            authority,
            targets,
            laws,
            committing,
            premise,
            reach,
            domain_only,
            asked,
            enumerated,
            size,
            max_tuples,
            at_genesis,
            mut causes,
        } = assembled;
        let positions = self.positions;
        let mut lines = Vec::new();
        let mut counts = Counts::default();
        if let Some(enumerated) = &enumerated
            && let Some(mismatch) = &enumerated.mismatch
        {
            causes.push(format!(
                "the library Authority and the case table disagree: {mismatch}"
            ));
        }
        if enumerated.is_none() && !matches!(premise.judgment, Judgment::Refuted { .. }) {
            causes.push(format!(
                "the planted premise control was not refuted, so a solver or the encoding is \
                 wrong: genesis is a lawful pre-state ({})",
                match &premise.judgment {
                    Judgment::Inconclusive(reason) => reason.clone(),
                    _ => "CVC5 proposed that none exists".to_owned(),
                }
            ));
        }
        let mut cases = Vec::new();
        for (slot, case) in committing.iter().enumerate() {
            let (rule, when) = self.case_texts.get(*case).cloned().unwrap_or_default();
            let reached = match &enumerated {
                Some(enumerated) => match &enumerated.reached[slot] {
                    Some(witness) => {
                        json!({"status": "reached", "evidence": "checked", "route": "exhaustive", "witness": witness.json(positions)})
                    }
                    None => {
                        json!({"status": "vacuous", "evidence": "proved", "route": "exhaustive"})
                    }
                },
                None => match &reach[slot].judgment {
                    Judgment::Refuted { .. } => {
                        let mut value = reach[slot].json(positions);
                        value["status"] = json!("reached");
                        value
                    }
                    Judgment::Holds { .. } => {
                        let mut value = reach[slot].json(positions);
                        value["status"] = json!("vacuous");
                        value
                    }
                    Judgment::Inconclusive(reason) => {
                        causes.push(format!(
                            "case {case}: the planted reachability control is inconclusive: {reason}"
                        ));
                        reach[slot].json(positions)
                    }
                },
            };
            let vacuous = reached["status"] == "vacuous";
            counts.vacuous += usize::from(vacuous);
            let mut results = Vec::new();
            for (index, target) in targets.iter().enumerate() {
                let symbolic = &asked[slot][index];
                let exhaustive = enumerated
                    .as_ref()
                    .map(|enumerated| &enumerated.found[slot][index]);
                let (status, evidence, disagree) = combine(&symbolic.judgment, exhaustive);
                counts.record(status, &symbolic.judgment, disagree);
                if disagree {
                    causes.push(format!(
                        "case {case}, {}: the symbolic and exhaustive routes disagree",
                        target.label
                    ));
                }
                let counterexample = match (exhaustive, &symbolic.judgment) {
                    (Some(Enumerated::Refuted { witness, count }), _) => Some(
                        json!({"route": "exhaustive", "tuples": count, "witness": witness.json(positions)}),
                    ),
                    (None, Judgment::Refuted { witness, .. }) => {
                        Some(json!({"route": "symbolic", "witness": witness.json(positions)}))
                    }
                    _ => None,
                };
                if status == "refuted" {
                    let witness = match (exhaustive, &symbolic.judgment) {
                        (Some(Enumerated::Refuted { witness, .. }), _) => Some(witness.as_ref()),
                        (_, Judgment::Refuted { witness, .. }) => Some(witness),
                        _ => None,
                    };
                    if let Some(witness) = witness {
                        lines.push(format!(
                            "refuted: case {case}{} breaks {} ({}) on {}; the Authority {}",
                            rule.as_ref()
                                .map_or_else(String::new, |rule| format!(" \"{rule}\"")),
                            target.label,
                            witness.target.unwrap_or("violated"),
                            named(positions, &witness.tuple),
                            witness.decided.as_deref().unwrap_or("was not asked")
                        ));
                    }
                }
                if status == "inconclusive"
                    && let Judgment::Inconclusive(reason) = &symbolic.judgment
                    && exhaustive.is_none()
                {
                    causes.push(format!("case {case}, {}: {reason}", target.label));
                }
                results.push(json!({
                    "target": target.label,
                    "status": status,
                    "evidence": evidence,
                    "vacuous": vacuous,
                    "symbolic": symbolic.json(positions),
                    "exhaustive": exhaustive.map(|found| match found {
                        Enumerated::Holds { decisions } => json!({"status": "holds", "evidence": "proved", "decisions": decisions}),
                        Enumerated::Refuted { count, .. } => json!({"status": "refuted", "evidence": "checked", "tuples": count}),
                    }),
                    "counterexample": counterexample,
                }));
            }
            cases.push(json!({
                "case": case,
                "rule": rule,
                "when": when,
                "class": match descriptor.branches[*case].class {
                    c::Class::Accept => "Accept",
                    _ => "CommittedFailure",
                },
                "reached": reached,
                "targets": results,
            }));
        }
        let domain_json: Vec<Value> = targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                let mut value = match &enumerated {
                    Some(enumerated) => json!({
                        "status": if enumerated.failing_states[index] { "substantive" } else { "implied-by-domain" },
                        "evidence": if enumerated.failing_states[index] { "checked" } else { "proved" },
                        "route": "exhaustive",
                    }),
                    None => {
                        let mut value = domain_only[index].json(positions);
                        value["status"] = json!(match &domain_only[index].judgment {
                            Judgment::Refuted { .. } => "substantive",
                            Judgment::Holds { .. } => "implied-by-domain",
                            Judgment::Inconclusive(reason) => {
                                causes.push(format!(
                                    "{}: the planted domain control is inconclusive: {reason}",
                                    target.label
                                ));
                                "inconclusive"
                            }
                        });
                        value
                    }
                };
                value["target"] = json!(target.label);
                value
            })
            .collect();
        let status = if counts.refuted > 0 {
            Status::Refuted
        } else if !causes.is_empty() || counts.inconclusive > 0 {
            Status::Inconclusive
        } else if enumerated.is_some() {
            Status::Proved
        } else {
            Status::Attested
        };
        let route = if enumerated.is_some() {
            "exhaustive-and-symbolic"
        } else {
            "symbolic"
        };
        lines.insert(
            0,
            format!(
                "{}: {} committing cases, {} targets ({}); {} tuples; {route} route",
                self.rules.template,
                committing.len(),
                targets.len(),
                targets
                    .iter()
                    .map(|target| target.label.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                size.map_or_else(|| "more than 2^128".to_owned(), |size| size.to_string()),
            ),
        );
        for cause in &causes {
            lines.push(format!("inconclusive: {cause}"));
        }
        lines.push(format!(
            "{} ({}): {} hold, {} refuted, {} inconclusive; {} vacuous cases; {} solver disagreements",
            status.name(),
            status.evidence(),
            counts.holds,
            counts.refuted,
            counts.inconclusive,
            counts.vacuous,
            counts.disagreements
        ));
        let sha256 = |text: &str| crate::transform::sha256_hex(text.as_bytes());
        let report = json!({
            "schema": REPORT_SCHEMA,
            "status": status.name(),
            "evidence": status.evidence(),
            "authority": "none",
            "application": self.rules.template,
            "identity": crate::transform::sha256_hex(authority.identity()),
            "sources": {
                "project_sha256": sha256(self.sources.project),
                "rules_sha256": sha256(self.sources.rules),
                "strengthening_sha256": self.sources.strengthening.map(sha256),
            },
            "program": {
                "checked": "the decision program the rules compile to, as the bound Authority's descriptor holds it; each adopted program is exhaustively equivalent to it by its receipt",
                "adoptions": self.adoptions,
            },
            "domain": {
                "positions": positions.iter().map(Position::json).collect::<Vec<_>>(),
                "size": size.map(|size| size.to_string()),
                "exhaustive_cap": max_tuples,
                "route": route,
            },
            "laws": {
                "targets": targets[..laws].iter().map(|target| target.program.id).collect::<Vec<_>>(),
                "not_targets": descriptor.laws.iter().map(|law| law.id).filter(|id| {
                    !targets[..laws].iter().any(|target| target.program.id == *id)
                }).collect::<Vec<_>>(),
            },
            "strengthening": self.clauses.iter().map(|clause| json!({
                "name": clause.name, "formula": clause.formula,
            })).collect::<Vec<_>>(),
            "genesis": at_genesis,
            "premise": premise.json(positions),
            "cases": cases,
            "domain_controls": domain_json,
            "summary": {
                "committing_cases": committing.len(),
                "targets": targets.len(),
                "holds": counts.holds,
                "refuted": counts.refuted,
                "inconclusive": counts.inconclusive,
                "vacuous_cases": counts.vacuous,
                "solver_disagreements": counts.disagreements,
            },
            "causes": causes,
        });
        Checked {
            status,
            report,
            lines,
        }
    }
}

/// Counts over every case and target.
#[derive(Default)]
struct Counts {
    holds: usize,
    refuted: usize,
    inconclusive: usize,
    vacuous: usize,
    disagreements: usize,
}

impl Counts {
    fn record(&mut self, status: &str, symbolic: &Judgment<Witness>, routes_disagree: bool) {
        match status {
            "holds" => self.holds += 1,
            "refuted" => self.refuted += 1,
            _ => self.inconclusive += 1,
        }
        if routes_disagree
            || matches!(
                symbolic,
                Judgment::Refuted {
                    disagreement: true,
                    ..
                }
            )
            || matches!(symbolic, Judgment::Inconclusive(reason) if reason.starts_with("the solvers disagree"))
        {
            self.disagreements += 1;
        }
    }
}

/// One case and target's status and evidence from both routes, and whether
/// they disagree. Enumeration decides wherever it ran.
fn combine(
    symbolic: &Judgment<Witness>,
    exhaustive: Option<&Enumerated>,
) -> (&'static str, &'static str, bool) {
    match (exhaustive, symbolic) {
        (Some(Enumerated::Holds { .. }), Judgment::Refuted { .. })
        | (Some(Enumerated::Refuted { .. }), Judgment::Holds { .. }) => {
            ("inconclusive", "none", true)
        }
        (Some(Enumerated::Holds { .. }), _) => ("holds", "proved", false),
        (Some(Enumerated::Refuted { .. }), _) => ("refuted", "checked", false),
        (None, judgment) => (judgment.status(), judgment.evidence(), false),
    }
}

/// Whether a position's domain holds a value.
fn admits(domain: &domain::LeafDomain, value: i64) -> bool {
    match domain {
        domain::LeafDomain::Bool => value == 0 || value == 1,
        domain::LeafDomain::Int { min, max } => (*min..=*max).contains(&value),
        domain::LeafDomain::Sum { variants, .. } => {
            u16::try_from(value).is_ok_and(|value| variants.contains(&value))
        }
    }
}

/// A tuple with each value named by its position.
fn named(positions: &[Position], tuple: &[i64]) -> String {
    positions
        .iter()
        .zip(tuple)
        .map(|(position, value)| format!("{}={value}", position.name))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests;
