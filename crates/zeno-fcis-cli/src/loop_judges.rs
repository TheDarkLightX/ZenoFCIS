//! `zeno-fcis loop question` and `loop teach-back`, and the core table that
//! `loop decompose` also reads: the library Authority's decision on every
//! tuple of a contract's declared domain, the case that fires there, the
//! states reachable from genesis, and which decisions the contract's laws
//! admit on each tuple.
//!
//! Every decision here is the library Authority's on the contract bound as
//! an application binds it, and every law verdict is the library's law
//! evaluator's, run on frames built the way the contract judges build them
//! (`contract review --gaps`). The doors group, describe and explain; they
//! decide nothing about the contract:
//!
//! - **question**: inputs where the laws admit more than one decision are
//!   grouped by the case that fires there, and the largest group becomes a
//!   question with an exact formula for its inputs. An answer becomes laws
//!   over exactly those inputs; the laws are checked by the gaps judge for
//!   false alarms on the core, and must leave every input some admissible
//!   decision. An answer that differs from the core is a disagreement and is
//!   never adopted into the contract. Reason requirements become native
//!   reason-scoped laws, with their answer provenance retained;
//! - **teach-back**: a restated rule is exact, holds with inputs missing,
//!   wrong with exceptions, or vacuous, on the whole declared domain and on
//!   the reachable states.
//!
//! Results hold over the declared finite domain only.
//!
//! This module is LLM-generated.

use crate::contract::review::decision_space::{
    DecisionSpace, DeliveryChoices, Determination, determination, determination_scope,
};
use crate::contract::review::deliveries::{self, DeliveryUniverse};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use serde_json::{Value, json};
use zeno_fcis_contract::internals::ContractError;
use zeno_fcis_contract::internals::declarations::{Declarations, Form, Kind as TypeKind, Source};
use zeno_fcis_contract::internals::model::Contract;
use zeno_fcis_contract::internals::review::domain::{self, LeafDomain, Position};
use zeno_fcis_contract::internals::review::evaluate::{
    self as library, Framer, LawSet, Outcome, Proposed,
};
use zeno_fcis_contract::internals::rules::{Class, Constant, LawKind, Rules};
use zeno_fcis_contract::internals::{policy, schema, schema_commitment};
use zeno_fcis_loop_doors::expr::{
    Allow, Env, Expr, Function, Kind, Op, Vocabulary, children, evaluate, kind_of, parse, show,
    size, substitute,
};
use zeno_fcis_loop_doors::inverse::{Synthesis, Synthesizer, describe_region};
use zeno_fcis_loop_doors::subject::Subject;
use zeno_fcis_synthesis::finite::Domain;

use crate::transform::sha256_hex;

/// Largest declared domain these doors enumerate.
pub(super) const MAX_TUPLES: u64 = 1 << 17;
/// Most law evaluations one admissibility table may take.
const MAX_CHECKS: u64 = 4_000_000;
/// Largest region formula, in expression nodes.
const MAX_FORMULA: usize = 512;
/// Exception groups reported per rule.
const MAX_GROUPS: usize = 3;
/// The kind every law these doors add takes, as the gaps judge's proposals
/// do: it accepts every scope.
const ADDED_KIND: LawKind = LawKind::AuthoritySubjectRecipient;

/// Why a door stopped.
#[derive(Debug)]
pub(super) enum Stop {
    /// The contract or a request has no contract form.
    Invalid(String),
    /// The scalar decision universe cannot cover the contract's profile.
    OutOfScope(String),
    /// A budget was exceeded; nothing is claimed.
    Inconclusive(String),
}

impl From<ContractError> for Stop {
    fn from(error: ContractError) -> Self {
        Self::Invalid(format!("{}: {}", error.place(), error.reason()))
    }
}

impl From<zeno_fcis_loop_doors::Refusal> for Stop {
    fn from(refusal: zeno_fcis_loop_doors::Refusal) -> Self {
        Self::Invalid(refusal.0)
    }
}

pub(super) type Result<T> = std::result::Result<T, Stop>;

fn invalid<T>(reason: impl Into<String>) -> Result<T> {
    Err(Stop::Invalid(reason.into()))
}

// --- the core table -----------------------------------------------------------

/// The library Authority's decision on one tuple; `next` is the successor
/// state's index (the pre-state's for a reject).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Dec {
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) next: usize,
}

/// A decision label as a question offers it: a class and, for a reject or a
/// committed failure, a reason.
pub(super) type Label = (u8, Option<u32>);

pub(super) fn label(class: Class, reason: Option<u32>) -> Label {
    let code = u8::try_from(class.code()).unwrap_or(u8::MAX);
    (code, if class == Class::Accept { None } else { reason })
}

pub(super) fn class_of(code: u8) -> Class {
    match code {
        0 => Class::Accept,
        1 => Class::Reject,
        _ => Class::CommittedFailure,
    }
}

/// One case of the contract's case table, over the input names.
#[derive(Clone, Debug)]
pub(super) struct CaseText {
    pub(super) when: Expr,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// Successor expressions by state position.
    pub(super) post: BTreeMap<usize, Expr>,
}

/// A contract's whole declared domain and the Authority's decisions on it.
pub(super) struct Core {
    project: String,
    rules_text: String,
    rules: Rules,
    /// The names rules use for each position, in program order.
    pub(super) names: Vec<String>,
    /// The law paths of each position: `pre.100.110`, `command.101.120`, ...
    paths: Vec<String>,
    pub(super) bools: Vec<bool>,
    /// Every value of each position, in enumeration order.
    pub(super) values: Vec<Vec<i128>>,
    pub(super) state_width: usize,
    pub(super) states: usize,
    pub(super) inputs: usize,
    /// One column per position over every tuple, in ordinal order.
    pub(super) columns: Vec<Vec<i128>>,
    /// The decision on each tuple; `None` where the Authority refuses.
    pub(super) decisions: Vec<Option<Dec>>,
    /// Owned native diagnostics, independently of the lawful-outcome relation.
    refusals: BTreeMap<usize, library::Refusal>,
    /// The case that fires on each tuple.
    pub(super) case_of: Vec<usize>,
    /// Whether each state is reachable from genesis.
    pub(super) reachable: Vec<bool>,
    /// Reason IDs and names, ascending by ID.
    pub(super) reasons: Vec<(u32, String)>,
    pub(super) cases: Vec<CaseText>,
    pub(super) vocab: Vocabulary,
    /// Whether some case commits a failure.
    pub(super) failure: bool,
    declared_laws: Vec<u32>,
    program: zeno_fcis_synthesis::finite::Program,
    delivery_universe: Option<DeliveryUniverse>,
    native_deliveries: Vec<Arc<[library::Delivery]>>,
}

/// `pre.100.110` as written in rules and laws.
fn variables_of(policy: &Value) -> BTreeMap<String, String> {
    policy["variables"]
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(name, text)| Some((name.clone(), text.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default()
}

/// `text` with every position path written as its name, longest path first
/// so `pre.100.11` never matches inside `pre.100.110`.
fn names_for_paths(text: &str, paths: &[String], names: &[String]) -> String {
    let mut order: Vec<usize> = (0..paths.len()).collect();
    order.sort_by_key(|k| std::cmp::Reverse(paths[*k].len()));
    let mut out = text.to_owned();
    for k in order {
        out = out.replace(&paths[k], &names[k]);
    }
    out
}

impl Core {
    /// Reads `project.zeno` and `v2/policy.json` from `dir` and observes the
    /// whole declared domain. Question preparation first applies the gaps profile.
    ///
    /// # Errors
    /// No contract form, or a domain over [`MAX_TUPLES`] (inconclusive).
    pub(super) fn load(dir: &Path, question_profile: bool) -> Result<Self> {
        let read = |name: &str| -> Result<String> {
            let path = dir.join(name);
            let bytes = crate::transform_command::read_bounded(&path, 1 << 20)
                .map_err(|error| Stop::Invalid(format!("{}: {error}", path.display())))?;
            String::from_utf8(bytes).map_err(|_| Stop::Invalid(format!("{name} is not UTF-8")))
        };
        let project = read("project.zeno")?;
        let rules = read("v2/policy.json")?;
        if question_profile
            && let Some(reason) = crate::contract::review::gaps::profile_scope(&project, &rules)?
        {
            return Err(Stop::OutOfScope(reason));
        }
        Self::from_sources(project, rules)
    }

    fn from_sources(project: String, rules_text: String) -> Result<Self> {
        let rules = Rules::read_for_project(&project, &rules_text)?;
        rules.refuse_unsigned("loop binding")?;
        let declarations = Declarations::read(&project, &rules.leaf_bindings)?;
        let delivery_universe = DeliveryUniverse::new(&declarations, &rules);
        let has_channels = !declarations.channels.is_empty();
        let positions = domain::positions(&declarations)?;
        match domain::domain_size(&positions) {
            Some(size) if size <= u128::from(MAX_TUPLES) => {}
            size => {
                return Err(Stop::Inconclusive(format!(
                    "domain-over-limit: {} tuples, limit {MAX_TUPLES}",
                    size.map_or_else(|| "more than 2^128".to_owned(), |s| s.to_string())
                )));
            }
        }
        let expanded_rules = Rules::expand_for_project(&project, &rules_text)?;
        let policy: Value = serde_json::from_str(&expanded_rules)
            .map_err(|_| Stop::Invalid("v2/policy.json is not JSON".into()))?;
        let variables = variables_of(&policy);
        let paths: Vec<String> = positions.iter().map(|p| p.name.clone()).collect();
        let names: Vec<String> = positions
            .iter()
            .map(|position| field_name(&declarations, position))
            .collect();
        let bools: Vec<bool> = positions
            .iter()
            .map(|p| p.domain == LeafDomain::Bool)
            .collect();
        let values: Vec<Vec<i128>> = positions
            .iter()
            .map(|p| p.domain.values_within(MAX_TUPLES))
            .collect::<Option<_>>()
            .ok_or_else(|| Stop::Inconclusive("domain-over-limit".into()))?;
        let state_width = positions
            .iter()
            .filter(|p| p.source == Source::State)
            .count();
        let count = |lists: &[Vec<i128>]| lists.iter().map(Vec::len).product::<usize>();
        let states = count(&values[..state_width]);
        let inputs = count(&values[state_width..]);
        let tuples = states * inputs;
        let mut columns: Vec<Vec<i128>> = vec![Vec::with_capacity(tuples); positions.len()];
        for ordinal in 0..tuples {
            for (k, value) in element(&values, ordinal).into_iter().enumerate() {
                columns[k].push(value);
            }
        }
        let mut field_variants: BTreeMap<String, BTreeMap<String, i128>> = BTreeMap::new();
        for (position, name) in positions.iter().zip(&names) {
            if let LeafDomain::Sum { type_id, .. } = &position.domain
                && let Ok(TypeKind::Sum(variants)) = declarations.kind(*type_id)
            {
                field_variants.insert(
                    name.clone(),
                    variants
                        .iter()
                        .map(|v| (v.name.clone(), i128::from(v.id)))
                        .collect(),
                );
            }
        }
        let domains: Vec<Domain> = positions.iter().map(|p| domain_of(&p.domain)).collect();
        let vocab = Vocabulary::new(
            names.clone(),
            domains,
            vec![Domain::Int { min: 0, max: 0 }],
            &field_variants,
        )?;
        let schema = schema::encode(&declarations)?;
        let commitment = schema_commitment(&schema)?;
        let contract = Contract::build(&declarations, &rules, commitment)?;
        let program = contract.program()?;
        let framer = Framer::new(&positions, &contract);
        let genesis_values: Vec<i128> = positions[..state_width]
            .iter()
            .map(|p| match p.field.and_then(|f| rules.genesis.get(&f)) {
                Some(Constant::U128(_)) => invalid("U128SubjectUnsupported"),
                Some(Constant::Bool(b)) => Ok(i128::from(*b)),
                Some(Constant::Int(v)) => Ok(*v),
                None => invalid("v2/policy.json genesis misses a state field"),
            })
            .collect::<Result<_>>()?;
        let decided = policy::with_authority(&contract, &schema, |authority| {
            let mut initial = genesis_values.clone();
            initial.extend(element(&values[state_width..], 0));
            zeno_fcis_contract::internals::genesis::admit(authority, &framer.frame(&initial)[0])
                .map_err(|refusal| refusal.error())?;
            Ok::<_, crate::contract::ContractError>(
                (0..tuples)
                    .map(|ordinal| {
                        let tuple = element(&values, ordinal);
                        library::evaluate(authority, &framer, &tuple)
                    })
                    .collect::<Vec<Outcome>>(),
            )
        })??;
        let state_values = &values[..state_width];
        let mut decisions = Vec::with_capacity(tuples);
        let mut native_deliveries = Vec::new();
        let mut refusals = BTreeMap::new();
        for (ordinal, outcome) in decided.into_iter().enumerate() {
            let state = ordinal / inputs;
            decisions.push(match outcome {
                Outcome::Refused(refusal) => {
                    if has_channels { native_deliveries.push(Arc::from([])); }
                    refusals.insert(ordinal, refusal);
                    None
                }
                Outcome::Decision(decision) => {
                    if has_channels { native_deliveries.push(Arc::from(decision.outbox)); }
                    let next = if decision.class == Class::Reject {
                        state
                    } else {
                        let by_field: BTreeMap<u16, Option<i128>> = decision
                            .post
                            .iter()
                            .map(|(field, value)| (*field, value.number()))
                            .collect();
                        let post: Option<Vec<i128>> = positions[..state_width]
                            .iter()
                            .map(|p| {
                                let number = (*by_field.get(&p.field.unwrap_or(0))?)?;
                                Some(number)
                            })
                            .collect();
                        post.and_then(|post| index_of(state_values, &post))
                            .ok_or_else(|| {
                                Stop::Invalid(format!(
                                    "the Authority committed a successor outside the declared state domain at input {ordinal}"
                                ))
                            })?
                    };
                    Some(Dec {
                        class: decision.class,
                        reason: decision.reason,
                        next,
                    })
                }
            });
        }
        let case_of: Vec<usize> = (0..tuples)
            .map(|ordinal| {
                program
                    .evaluate(&element(&values, ordinal))
                    .ok()
                    .and_then(|outputs| outputs.first().copied())
                    .and_then(|case| usize::try_from(case).ok())
                    .unwrap_or(usize::MAX)
            })
            .collect();
        let genesis = index_of(state_values, &genesis_values).ok_or_else(|| {
            Stop::Invalid("genesis lies outside the declared state domain".into())
        })?;
        let mut reachable = vec![false; states];
        let mut queue = std::collections::VecDeque::from([genesis]);
        reachable[genesis] = true;
        while let Some(state) = queue.pop_front() {
            for input in 0..inputs {
                let next = decisions[state * inputs + input].map_or(state, |d| d.next);
                if !reachable[next] {
                    reachable[next] = true;
                    queue.push_back(next);
                }
            }
        }
        let mut reasons: Vec<(u32, String)> = declarations
            .reasons
            .iter()
            .map(|id| {
                let name = declarations
                    .notes
                    .reasons
                    .get(id)
                    .map_or_else(|| id.to_string(), |(name, _)| name.clone());
                (*id, name)
            })
            .collect();
        reasons.sort_unstable();
        let mut core = Self {
            declared_laws: declarations.laws.iter().map(|law| law.id).collect(),
            project,
            rules_text,
            failure: rules
                .cases
                .iter()
                .any(|c| c.class == Class::CommittedFailure),
            rules,
            names,
            paths,
            bools,
            values,
            state_width,
            states,
            inputs,
            columns,
            decisions,
            refusals,
            case_of,
            reachable,
            reasons,
            cases: Vec::new(),
            vocab,
            program,
            delivery_universe,
            native_deliveries,
        };
        core.cases = core.case_texts(&policy, &variables, &positions)?;
        Ok(core)
    }

    /// The case table over the input names, variables expanded.
    fn case_texts(
        &self,
        policy: &Value,
        variables: &BTreeMap<String, String>,
        positions: &[Position],
    ) -> Result<Vec<CaseText>> {
        // A variable naming a position is another name for its field; any
        // other variable is an expression, expanded where cases use it.
        let mut defined: BTreeMap<String, Expr> = BTreeMap::new();
        for (name, text) in variables {
            if let Some(k) = self.paths.iter().position(|path| path == text) {
                if self.names[k] != *name {
                    defined.insert(name.clone(), Expr::Var(self.names[k].clone()));
                }
                continue;
            }
            defined.insert(name.clone(), self.read_names(text)?);
        }
        let expand = |node: Expr| -> Result<Expr> {
            let mut node = node;
            for _ in 0..16 {
                let before = node.clone();
                node = substitute(&node, &mut |n| {
                    Ok(match n {
                        Expr::Var(name) => defined.get(name).cloned(),
                        _ => None,
                    })
                })?;
                if node == before {
                    return Ok(node);
                }
            }
            invalid("case-variable-cycle")
        };
        let mut out = Vec::new();
        for (index, (case, raw)) in self
            .rules
            .cases
            .iter()
            .zip(policy["cases"].as_array().into_iter().flatten())
            .enumerate()
        {
            let when = match &raw["when"] {
                Value::String(text) => expand(self.read_names(text)?)?,
                Value::Bool(b) => Expr::Int(i128::from(*b)),
                _ => Expr::Int(1),
            };
            let mut post = BTreeMap::new();
            for (field, text) in raw["post"].as_object().into_iter().flatten() {
                let field: u16 = field
                    .parse()
                    .map_err(|_| Stop::Invalid(format!("case {index} post field {field}")))?;
                let Some(k) = positions[..self.state_width]
                    .iter()
                    .position(|p| p.field == Some(field))
                else {
                    return invalid(format!("case {index} sets an unknown field {field}"));
                };
                let value = match text {
                    Value::String(text) => expand(self.read_names(text)?)?,
                    Value::Bool(b) => Expr::Int(i128::from(*b)),
                    Value::Number(_) => match case.post.get(&field) {
                        Some(zeno_fcis_contract::internals::expr::Ast::Int(value)) => {
                            Expr::Int(*value)
                        }
                        _ => return invalid(format!("case {index} post")),
                    },
                    _ => return invalid(format!("case {index} post")),
                };
                post.insert(k, value);
            }
            out.push(CaseText {
                when,
                class: case.class,
                reason: case.reason,
                post,
            });
        }
        Ok(out)
    }

    /// An expression over the input names (paths are accepted too).
    pub(super) fn read_names(&self, text: &str) -> Result<Expr> {
        let text = names_for_paths(text, &self.paths, &self.names);
        let node = parse(&text, 0, Allow::PLAIN)?;
        Ok(self.vocab.resolve_variants(&node)?)
    }

    /// An expression a request names: every name must be an input field or
    /// a variant.
    pub(super) fn read_checked(&self, text: &str) -> Result<Expr> {
        let node = self.read_names(text)?;
        self.vocab.check_names(&node)?;
        Ok(node)
    }

    pub(super) fn tuples(&self) -> usize {
        self.states * self.inputs
    }

    fn native_outbox(&self, ordinal: usize) -> &[library::Delivery] {
        self.native_deliveries
            .get(ordinal)
            .map(|v| v.as_ref())
            .unwrap_or(&[])
    }

    fn outcome_count(&self, candidates: &[Candidate], fixed: bool) -> usize {
        determination(
            candidates.iter().map(|c| (c.class, c.reason, c.next)),
            None,
            &[],
            if self.delivery_universe.is_some() && !fixed {
                DeliveryChoices::Unrestricted
            } else {
                DeliveryChoices::Fixed(&[])
            },
        )
        .outcomes
    }

    fn determination(
        &self,
        ordinal: usize,
        candidates: &[Candidate],
        fixed: Option<&[library::Delivery]>,
    ) -> Determination {
        determination(
            candidates.iter().map(|c| (c.class, c.reason, c.next)),
            self.decisions[ordinal].map(|d| (d.class, d.reason, d.next)),
            self.native_outbox(ordinal),
            match fixed {
                Some(wanted) => DeliveryChoices::Fixed(wanted),
                None if self.delivery_universe.is_some() => DeliveryChoices::Unrestricted,
                None => DeliveryChoices::Fixed(&[]),
            },
        )
    }

    /// The values of the tuple at `ordinal`.
    pub(super) fn tuple(&self, ordinal: usize) -> Vec<i128> {
        self.columns.iter().map(|c| c[ordinal]).collect()
    }

    /// The values of state `index`.
    pub(super) fn state(&self, index: usize) -> Vec<i128> {
        element(&self.values[..self.state_width], index)
    }

    pub(super) fn env(&self) -> Env<'_> {
        Env {
            vocab: &self.vocab,
            columns: &self.columns,
            keep: None,
            len: self.tuples(),
        }
    }

    /// `node` on every tuple.
    pub(super) fn eval(&self, node: &Expr) -> Result<Vec<i128>> {
        Ok(evaluate(node, &self.env(), &BTreeMap::new())?)
    }

    /// The tuple at `ordinal`, field by field, as people read it.
    pub(super) fn shown(&self, ordinal: usize) -> Value {
        let mut out = serde_json::Map::new();
        for (k, name) in self.names.iter().enumerate() {
            let value = self.columns[k][ordinal];
            out.insert(name.clone(), self.value_json(k, value));
        }
        Value::Object(out)
    }

    pub(super) fn value_json(&self, k: usize, value: i128) -> Value {
        if self.bools[k] {
            return json!(value == 1);
        }
        match self.vocab.variant_name(&self.names[k], value) {
            Some(variant) => json!(variant),
            None => crate::report_numbers::integer(value),
        }
    }

    pub(super) fn reason_name(&self, reason: Option<u32>) -> Option<String> {
        let reason = reason?;
        Some(
            self.reasons
                .iter()
                .find(|(id, _)| *id == reason)
                .map_or_else(|| reason.to_string(), |(_, name)| name.clone()),
        )
    }

    pub(super) fn reason_id(&self, name: &str) -> Option<u32> {
        self.reasons
            .iter()
            .find(|(id, reason)| reason == name || id.to_string() == name)
            .map(|(id, _)| *id)
    }

    /// A decision as people read it: class, reason and, when it commits,
    /// the successor's state fields.
    pub(super) fn decision_json(
        &self,
        class: Class,
        reason: Option<u32>,
        next: Option<usize>,
    ) -> Value {
        let mut out = json!({"class": class.name(), "reason": self.reason_name(reason)});
        if let Some(next) = next.filter(|_| class != Class::Reject) {
            let state = self.state(next);
            out["sets"] = Value::Object(
                (0..self.state_width)
                    .map(|k| (self.names[k].clone(), self.value_json(k, state[k])))
                    .collect(),
            );
        }
        out
    }

    /// The first-match formula of case `k`: no earlier case's guard holds,
    /// and its own does.
    pub(super) fn case_formula(&self, k: usize) -> Expr {
        // `k` past the last case (no case fired) negates every guard.
        let mut parts: Vec<Expr> = self
            .cases
            .get(..k)
            .unwrap_or(&self.cases)
            .iter()
            .map(|c| match &c.when {
                Expr::Not(inner) => (**inner).clone(),
                other => Expr::negation(other.clone()),
            })
            .collect();
        if let Some(case) = self.cases.get(k)
            && (parts.is_empty() || case.when != Expr::Int(1))
        {
            parts.push(case.when.clone());
        }
        parts
            .into_iter()
            .reduce(|a, b| Expr::bin(Op::And, a, b))
            .unwrap_or(Expr::Int(1))
    }

    /// An exact formula over the input names for `mask`: the case's own
    /// first-match formula when the mask is the whole case, else a
    /// synthesized one, else an exact decomposition by field values. Every
    /// result is evaluated on every tuple before it is returned.
    pub(super) fn region_formula(&self, mask: &[bool], case: Option<usize>) -> Result<Expr> {
        let exact = |node: &Expr| -> Result<bool> {
            Ok(self
                .eval(node)?
                .iter()
                .zip(mask)
                .all(|(v, m)| (*v == 1) == *m))
        };
        if let Some(k) = case {
            let base = self.case_formula(k);
            if exact(&base)? {
                return Ok(base);
            }
        }
        if let Some(node) = self.synthesized_region(mask)?
            && size(&node) <= MAX_FORMULA
            && exact(&node)?
        {
            return Ok(node);
        }
        let all: Vec<usize> = (0..self.tuples()).collect();
        let mut order: Vec<usize> = (0..self.names.len()).collect();
        order.sort_by_key(|k| (self.values[*k].len(), *k));
        let node = self.shannon(mask, &all, &order, 0)?;
        if size(&node) > MAX_FORMULA {
            return invalid("question-region-too-complex");
        }
        if !exact(&node)? {
            return invalid("question-region-not-exact");
        }
        Ok(node)
    }

    /// A region formula by top-down synthesis over the contract's program
    /// inputs, when the program's domain is the declared one.
    fn synthesized_region(&self, mask: &[bool]) -> Result<Option<Expr>> {
        let Ok(vocab) = Vocabulary::new(
            self.names.clone(),
            self.program.inputs().to_vec(),
            self.program.outputs().to_vec(),
            &BTreeMap::new(),
        ) else {
            return Ok(None);
        };
        let Ok(subject) = Subject::new(
            self.program.clone(),
            vocab,
            BTreeMap::new(),
            None,
            MAX_TUPLES,
        ) else {
            return Ok(None);
        };
        if subject.columns != self.columns {
            return Ok(None);
        }
        let synthesizer = Synthesizer::new(&subject, Synthesis::default());
        Ok(describe_region(mask, &synthesizer, 3)?)
    }

    /// An exact formula for `mask` on `indices`, splitting on the field at
    /// `order[depth]`; equal adjacent subtrees share one range guard.
    fn shannon(
        &self,
        mask: &[bool],
        indices: &[usize],
        order: &[usize],
        depth: usize,
    ) -> Result<Expr> {
        if indices.iter().all(|o| !mask[*o]) {
            return Ok(Expr::Int(0));
        }
        if indices.iter().all(|o| mask[*o]) {
            return Ok(Expr::Int(1));
        }
        let Some(&k) = order.get(depth) else {
            return invalid("question-region-not-exact");
        };
        let mut groups: Vec<(i128, i128, Expr)> = Vec::new();
        for value in &self.values[k] {
            let part: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|o| self.columns[k][*o] == *value)
                .collect();
            if part.is_empty() {
                continue;
            }
            let child = self.shannon(mask, &part, order, depth + 1)?;
            match groups.last_mut() {
                Some(last) if last.2 == child && last.1 + 1 == *value => last.1 = *value,
                _ => groups.push((*value, *value, child)),
            }
        }
        let Some((_, _, last)) = groups.last().cloned() else {
            return Ok(Expr::Int(0));
        };
        let variable = Expr::Var(self.names[k].clone());
        let mut result = last;
        for (lo, hi, child) in groups.into_iter().rev().skip(1) {
            if child == result {
                continue;
            }
            let guard = if self.bools[k] {
                if lo == 0 {
                    Expr::negation(variable.clone())
                } else {
                    variable.clone()
                }
            } else if lo == hi {
                Expr::bin(Op::Eq, variable.clone(), Expr::Int(lo))
            } else {
                Expr::bin(
                    Op::And,
                    Expr::bin(Op::Ge, variable.clone(), Expr::Int(lo)),
                    Expr::bin(Op::Le, variable.clone(), Expr::Int(hi)),
                )
            };
            result = Expr::select(guard, child, result);
            if size(&result) > MAX_FORMULA {
                return invalid("question-region-too-complex");
            }
        }
        Ok(result)
    }

    /// `node` shown over the input names, variants by name.
    pub(super) fn show(&self, node: &Expr) -> String {
        show(node, &self.vocab, Some(Kind::Bool))
    }

    // --- laws ---------------------------------------------------------------

    /// `node` as a contract law formula: `pre.`, `command.` and `context.`
    /// paths, and `post.` paths for `posted` state names.
    pub(super) fn law_text(&self, node: &Expr) -> Result<String> {
        self.check_conditionals(node)?;
        self.law_bool(node, false)
    }

    /// Refuses a conditional anywhere in `node` whose branches mix a truth
    /// value and a number: the doors' evaluator passes the number through,
    /// and the writers keep no law text for that mixture.
    fn check_conditionals(&self, node: &Expr) -> Result<()> {
        if let Expr::If(_, a, b) = node
            && ((self.is_number(a) && self.is_bool(b)) || (self.is_bool(a) && self.is_number(b)))
        {
            return invalid("law-conditional-mixes-a-truth-value-and-a-number");
        }
        for child in children(node) {
            self.check_conditionals(child)?;
        }
        Ok(())
    }

    fn path_of(&self, name: &str, post: bool) -> Result<String> {
        let Some(k) = self.names.iter().position(|n| n == name) else {
            return invalid(format!("law-unknown-name:{name}"));
        };
        let path = &self.paths[k];
        Ok(if post {
            path.replacen("pre.", "post.", 1)
        } else {
            path.clone()
        })
    }

    fn is_bool(&self, node: &Expr) -> bool {
        match node {
            Expr::Var(name) => self
                .names
                .iter()
                .position(|n| n == name)
                .is_some_and(|k| self.bools[k]),
            Expr::Not(_) => true,
            Expr::Call(function, _) => function.kind() == Kind::Bool,
            Expr::Bin(op, _, _) => matches!(
                op,
                Op::And
                    | Op::Or
                    | Op::Implies
                    | Op::Eq
                    | Op::Ne
                    | Op::Lt
                    | Op::Le
                    | Op::Gt
                    | Op::Ge
            ),
            Expr::If(_, a, b) => self.is_bool(a) && self.is_bool(b),
            _ => false,
        }
    }

    /// Whether `node` is a number, not a truth value: a non-Boolean field,
    /// a sum, or a literal other than 0 and 1.
    fn is_number(&self, node: &Expr) -> bool {
        match node {
            Expr::Int(v) => !(0..=1).contains(v),
            // Either branch: `kind_of` takes the first one only.
            Expr::If(_, a, b) => self.is_number(a) || self.is_number(b),
            _ => matches!(kind_of(node, &self.vocab), Ok(Some(Kind::Int))),
        }
    }

    /// `node` in a truth position, with the doors' evaluator's meaning: a
    /// value is true when it is 1, and `!a` holds when `a` is 0 (so on a
    /// non-Boolean field `!x` is `x == 0`, never `!(x == 1)`).
    fn law_bool(&self, node: &Expr, post: bool) -> Result<String> {
        Ok(match node {
            Expr::Int(1) => "true".to_owned(),
            Expr::Int(0) => "false".to_owned(),
            Expr::Var(name) => format!("{} == 1", self.path_of(name, post)?),
            Expr::Not(a) if self.is_number(a) => format!("{} == 0", self.law_left_value(a, post)?),
            Expr::Not(a) => format!("!({})", self.law_bool(a, post)?),
            Expr::Call(Function::ProductsGe, args) => {
                self.law_call(Function::ProductsGe, args, post)?
            }
            Expr::If(c, a, b) => {
                let (c, a, b) = (
                    self.law_bool(c, post)?,
                    self.law_bool(a, post)?,
                    self.law_bool(b, post)?,
                );
                format!("(({c}) && ({a})) || (!({c}) && ({b}))")
            }
            // `||` and `->` pass a number through in the doors' evaluator
            // (`x || y` is `y` when `x` is not 1), so only `&&` takes one.
            Expr::Bin(Op::Or | Op::Implies, a, b) if self.is_number(a) || self.is_number(b) => {
                return invalid("law-truth-operator-on-a-number");
            }
            Expr::Bin(op @ (Op::And | Op::Or | Op::Implies), a, b) => format!(
                "({}) {} ({})",
                self.law_bool(a, post)?,
                op.symbol(),
                self.law_bool(b, post)?
            ),
            Expr::Bin(op @ (Op::Eq | Op::Ne), a, b) if self.is_bool(a) || self.is_bool(b) => {
                // A truth value against a number compares raw values in
                // the doors' evaluator; no law form says that exactly.
                if self.is_number(a) || self.is_number(b) {
                    return invalid("law-compares-a-truth-value-with-a-number");
                }
                let (a, b) = (self.law_bool(a, post)?, self.law_bool(b, post)?);
                let both = format!("(({a}) && ({b})) || (!({a}) && !({b}))");
                if *op == Op::Eq {
                    both
                } else {
                    format!("!({both})")
                }
            }
            Expr::Bin(op @ (Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge), a, b) => format!(
                "{} {} {}",
                self.law_left_value(a, post)?,
                op.symbol(),
                self.law_value(b, post)?
            ),
            _ => return invalid("law-not-boolean"),
        })
    }

    fn law_left_value(&self, node: &Expr, post: bool) -> Result<String> {
        let value = self.law_value(node, post)?;
        // The spec grammar reads an initial '(' as a relation group. An
        // identity sum keeps scalar parentheses and their refusal order.
        Ok(if value.starts_with('(') {
            format!("0 + {value}")
        } else {
            value
        })
    }

    fn law_value(&self, node: &Expr, post: bool) -> Result<String> {
        Ok(match node {
            Expr::Int(v) if *v < 0 => format!("(0 - {})", v.unsigned_abs()),
            Expr::Int(v) => v.to_string(),
            Expr::Var(name) => self.path_of(name, post)?,
            Expr::Call(function, args) if function.kind() == Kind::Int => {
                self.law_call(*function, args, post)?
            }
            Expr::Bin(Op::Mul, a, b) => format!(
                "({}) * ({})",
                self.law_value(a, post)?,
                self.law_value(b, post)?
            ),
            Expr::Bin(op @ (Op::Add | Op::Sub), a, b) => {
                let right = self.law_value(b, post)?;
                let right = if matches!(**b, Expr::Bin(..)) {
                    format!("({right})")
                } else {
                    right
                };
                format!("{} {} {right}", self.law_value(a, post)?, op.symbol())
            }
            _ => return invalid("law-value-must-be-a-sum"),
        })
    }

    fn law_call(&self, function: Function, args: &[Expr], post: bool) -> Result<String> {
        if !function.accepts(args.len()) {
            return invalid("law-call-arity");
        }
        let args = args
            .iter()
            .map(|a| self.law_value(a, post))
            .collect::<Result<Vec<_>>>()?;
        Ok(format!("{}({})", function.name(), args.join(", ")))
    }

    /// `post.F == value` for state position `k`, `value` over the pre-state
    /// and the request.
    pub(super) fn successor_law(&self, k: usize, value: &Expr) -> Result<String> {
        self.check_conditionals(value)?;
        let target = self.path_of(&self.names[k], true)?;
        if self.bools[k] {
            let value = self.law_bool(value, false)?;
            return Ok(format!(
                "(({target} == 1) && ({value})) || (!({target} == 1) && !({value}))"
            ));
        }
        Ok(format!("{target} == {}", self.law_value(value, false)?))
    }

    /// Which decisions the declared laws, and `added` (scope, formula)
    /// laws, admit on every input tuple, independently of native refusals:
    /// rejects with each reason, accepts with each successor, and committed
    /// failures with each reason and successor.
    ///
    /// # Errors
    /// An added law with no contract form; inconclusive over the budget.
    pub(super) fn admissible(&self, added: &[(String, String)]) -> Result<Vec<Vec<Candidate>>> {
        self.admissible_from(added, None, None)
    }

    /// Narrow an invocation-owned table with only the next answer's laws,
    /// on its exact input mask. Outside that mask every pinned law is true.
    /// The base laws have already admitted every retained candidate.
    fn admissible_from(
        &self,
        added: &[(String, String)],
        previous: Option<&[Vec<Candidate>]>,
        mask: Option<&[i128]>,
    ) -> Result<Vec<Vec<Candidate>>> {
        let decisions = DecisionSpace::new(&self.rules);
        let per_tuple = decisions.count(self.states);
        let checks = if self.delivery_universe.is_some() {
            per_tuple.saturating_mul(2).saturating_add(1)
        } else {
            per_tuple
        }
        .saturating_mul(self.tuples() as u128);
        if checks > u128::from(MAX_CHECKS) {
            return Err(Stop::Inconclusive(format!(
                "admissibility-over-limit: {checks} law checks, limit {MAX_CHECKS}"
            )));
        }
        let (project, rules, ids) = self.with_laws(added)?;
        let declarations = Declarations::read(&project, &rules.leaf_bindings)?;
        if !declarations.channels.is_empty()
            && !self
                .delivery_universe
                .as_ref()
                .is_some_and(|u| u.covers(&declarations, &rules))
        {
            return Err(Stop::OutOfScope(
                "declared channels require a complete delivery decision universe".into(),
            ));
        }
        let positions = domain::positions(&declarations)?;
        let schema = schema::encode(&declarations)?;
        let commitment = schema_commitment(&schema)?;
        let contract = Contract::build(&declarations, &rules, commitment)?;
        let framer = Framer::new(&positions, &contract);
        let mut all = if previous.is_some() {
            Vec::new()
        } else {
            self.declared_laws.clone()
        };
        // As in the gaps judge, the closed no-failure profile is independent
        // of the cases. Its mandatory false law excludes failure candidates.
        if previous.is_none()
            && !self
                .rules
                .law_kinds
                .values()
                .any(|kind| *kind == LawKind::CommittedFailureEffects)
        {
            all.push(self.rules.failure_law);
        }
        all.extend(ids);
        let states: Vec<Vec<i128>> = (0..self.states).map(|s| self.state(s)).collect();
        let result = policy::with_authority(&contract, &schema, |authority| {
            let laws = LawSet::new(authority.descriptor(), &all);
            if self.delivery_universe.is_some() && !laws.delivery_independent() {
                return Err(Stop::OutOfScope(
                    "delivery-observing laws require a complete delivery decision universe".into(),
                ));
            }
            if previous.is_none()
                && added.is_empty()
                && let Some(universe) = &self.delivery_universe
            {
                for (ordinal, decision) in self.decisions.iter().enumerate() {
                    if let Some(d) = decision {
                        let outbox = self.native_outbox(ordinal);
                        if !universe.allows(outbox)
                            || !laws
                                .on_transition_with_outbox(
                                    &framer,
                                    &self.tuple(ordinal),
                                    &Proposed {
                                        class: d.class,
                                        reason: d.reason,
                                        post: &states[d.next],
                                    },
                                    outbox,
                                )
                                .0
                        {
                            return invalid(
                                "question-delivery-frames-disagree-with-native-decision",
                            );
                        }
                    }
                }
            }

            let table = (0..self.tuples())
                .map(|ordinal| {
                    let tuple = self.tuple(ordinal);
                    let admits = |class: Class,
                                  reason: Option<u32>,
                                  post: &[i128]|
                     -> Result<bool> {
                        let admitted = laws
                            .on_transition(
                                &framer,
                                &tuple,
                                &Proposed {
                                    class,
                                    reason,
                                    post,
                                },
                            )
                            .0;
                        if admitted
                            && class != Class::Reject
                            && let Some(u) = &self.delivery_universe
                        {
                            let witness = u.example();
                            if witness.is_empty()
                                || !u.allows(&witness)
                                || !laws
                                    .on_transition_with_outbox(
                                        &framer,
                                        &tuple,
                                        &Proposed {
                                            class,
                                            reason,
                                            post,
                                        },
                                        &witness,
                                    )
                                    .0
                            {
                                return invalid("question-delivery-multiplicity-witness-refused");
                            }
                        }
                        Ok(admitted)
                    };
                    if let Some(previous) = previous {
                        let candidates = &previous[ordinal];
                        if mask.is_some_and(|mask| mask[ordinal] != 1) {
                            return Ok(candidates.clone());
                        }
                        let mut out = Vec::new();
                        for c in candidates {
                            if admits(c.class, c.reason, &states[c.next])? {
                                out.push(*c);
                            }
                        }
                        return Ok(out);
                    }
                    let mut out = Vec::new();
                    for (class, reason, successor) in decisions.candidates(self.states) {
                        let next = successor.unwrap_or(ordinal / self.inputs);
                        if admits(class, reason, &states[next])? {
                            out.push(Candidate {
                                class,
                                reason,
                                next,
                            });
                        }
                    }
                    Ok(out)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(table)
        })??;
        Ok(result)
    }

    /// The contract with `added` laws appended to `project.zeno`, given the
    /// added-law kind, and their IDs: above every number `project.zeno`
    /// holds and every framework law's.
    fn with_laws(&self, added: &[(String, String)]) -> Result<(String, Rules, Vec<u32>)> {
        let mut source = self.project.clone();
        if !source.ends_with('\n') {
            source.push('\n');
        }
        let mut next = self
            .project
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|digits| digits.parse::<u32>().ok())
            .chain([990, 991, 909, self.rules.failure_law, self.rules.reject_law])
            .max()
            .unwrap_or(0);
        let mut rules = self.rules.clone();
        let mut ids = Vec::new();
        for (scope, formula) in added {
            if [";", "\n", "\r", "//", "/*", "{", "}"]
                .iter()
                .any(|bad| formula.contains(bad))
            {
                return invalid("law-formula-has-forbidden-text");
            }
            next = next
                .checked_add(1)
                .ok_or_else(|| Stop::Invalid("no free law ID".into()))?;
            source.push_str(&format!(
                "law {next} loop_requirement_{next} on {scope} = {formula};\n"
            ));
            rules.law_kinds.insert(next, ADDED_KIND);
            ids.push(next);
        }
        Ok((source, rules, ids))
    }

    /// The answers' pinned laws as `law` declarations the gaps judge reads
    /// as proposals, and the answer each law ID belongs to.
    pub(super) fn pinned_lines(
        &self,
        answers: &[Answer],
    ) -> Result<(String, BTreeMap<u32, usize>)> {
        let mut added = Vec::new();
        let mut owners = Vec::new();
        for (index, answer) in answers.iter().enumerate() {
            for law in self.pin(answer)? {
                added.push(law);
                owners.push(index);
            }
        }
        let (source, _, ids) = self.with_laws(&added)?;
        let lines = source[self.project.len()..]
            .trim_start_matches('\n')
            .to_owned();
        Ok((lines, ids.into_iter().zip(owners).collect()))
    }

    pub(super) fn project(&self) -> &str {
        &self.project
    }

    pub(super) fn source_digest(&self) -> String {
        meaning_digest(
            "loop-contract-sources/1",
            &[
                self.project.as_bytes().to_vec(),
                self.rules_text.as_bytes().to_vec(),
            ],
        )
    }

    pub(super) fn rules_text(&self) -> &str {
        &self.rules_text
    }
}

/// One admissible decision on one tuple.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Candidate {
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    pub(super) next: usize,
}

fn field_name(declarations: &Declarations, position: &Position) -> String {
    let root = match position.source {
        Source::State => 100,
        Source::Command => 101,
        Source::Context => 102,
    };
    position
        .field
        .and_then(
            |field| match declarations.types.get(&root).map(|t| &t.form) {
                Some(Form::Record(fields)) => fields
                    .iter()
                    .find(|f| f.id == field)
                    .map(|f| f.name.clone()),
                _ => None,
            },
        )
        .unwrap_or_else(|| {
            match position.source {
                Source::State => "state",
                Source::Command => "command",
                Source::Context => "context",
            }
            .to_owned()
        })
}

fn domain_of(domain: &LeafDomain) -> Domain {
    match domain {
        LeafDomain::Bool => Domain::Bool,
        LeafDomain::Int { min, max } => Domain::Int {
            min: *min,
            max: *max,
        },
        LeafDomain::Sum { variants, .. } => Domain::Int {
            min: variants.iter().map(|v| i128::from(*v)).min().unwrap_or(0),
            max: variants.iter().map(|v| i128::from(*v)).max().unwrap_or(0),
        },
    }
}

/// The values of the `index`th element of the product of `lists`, the last
/// list fastest.
pub(super) fn element(lists: &[Vec<i128>], mut index: usize) -> Vec<i128> {
    let mut out = vec![0; lists.len()];
    for (slot, list) in out.iter_mut().zip(lists).rev() {
        *slot = list[index % list.len()];
        index /= list.len();
    }
    out
}

/// The index of `values` in the product of `lists`.
pub(super) fn index_of(lists: &[Vec<i128>], values: &[i128]) -> Option<usize> {
    if values.len() != lists.len() {
        return None;
    }
    lists
        .iter()
        .zip(values)
        .try_fold(0usize, |index, (list, value)| {
            Some(index * list.len() + list.iter().position(|v| v == value)?)
        })
}

/// SHA-256, in hexadecimal, of a meaning: its profile, then each part with
/// its length.
pub(super) fn meaning_digest(kind: &str, parts: &[Vec<u8>]) -> String {
    let mut bytes = format!("zeno-fcis/loop-meaning/1:{kind}").into_bytes();
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    sha256_hex(&bytes)
}

// --- requirements: answers -----------------------------------------------------

#[derive(Clone, Debug)]
pub(super) enum DeliveryAnswer {
    Case,
    Exact(Arc<[library::Delivery]>),
}

/// An owner's answer: a decision for exactly the inputs `where` names.
#[derive(Clone, Debug)]
pub(super) struct Answer {
    pub(super) deliveries: Option<DeliveryAnswer>,
    pub(super) where_: Expr,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// The successor an accept or committed failure gives, field by field;
    /// fields not listed keep their value. `None`: no successor is
    /// required.
    pub(super) sets: Option<BTreeMap<usize, Expr>>,
}

impl Core {
    /// The answers in a `zeno-fcis/loop-answers/1` document.
    pub(super) fn answers(&self, document: &Value) -> Result<Vec<Answer>> {
        if document["schema"] != "zeno-fcis/loop-answers/1" {
            return invalid("answers-schema");
        }
        let mut out = Vec::new();
        for (index, item) in document["answers"]
            .as_array()
            .ok_or_else(|| Stop::Invalid("answers must be a list".into()))?
            .iter()
            .enumerate()
        {
            let at = |what: &str| Stop::Invalid(format!("answer {index}: {what}"));
            let where_ = self.read_checked(item["where"].as_str().ok_or_else(|| at("where"))?)?;
            let class = match item["class"].as_str() {
                Some("Accept") => Class::Accept,
                Some("Reject") => Class::Reject,
                Some("CommittedFailure") => Class::CommittedFailure,
                _ => return Err(at("class")),
            };
            let reason = match item["reason"].as_str() {
                Some(name) => Some(self.reason_id(name).ok_or_else(|| at("unknown reason"))?),
                None if class == Class::Accept => None,
                None => return Err(at("a reject or failure needs a reason")),
            };
            let sets = match &item["sets"] {
                Value::Null => match item["case"].as_u64() {
                    Some(case) if class != Class::Reject => Some(
                        self.cases
                            .get(usize::try_from(case).unwrap_or(usize::MAX))
                            .ok_or_else(|| at("case"))?
                            .post
                            .clone(),
                    ),
                    _ => None,
                },
                Value::Object(fields) => {
                    let mut sets = BTreeMap::new();
                    for (name, text) in fields {
                        let k = self.names[..self.state_width]
                            .iter()
                            .position(|n| n == name)
                            .ok_or_else(|| at("sets names a non-state field"))?;
                        sets.insert(
                            k,
                            self.read_checked(text.as_str().ok_or_else(|| at("sets"))?)?,
                        );
                    }
                    Some(sets)
                }
                _ => return Err(at("sets")),
            };
            if class == Class::Reject && sets.as_ref().is_some_and(|s| !s.is_empty()) {
                return Err(at("a reject keeps the state"));
            }
            let deliveries = match &item["deliveries"] {
                Value::Null => None,
                Value::String(value) if value == "case" => Some(DeliveryAnswer::Case),
                Value::Array(values) if values.is_empty() => {
                    Some(DeliveryAnswer::Exact(Arc::from([])))
                }
                Value::Array(_) => Some(DeliveryAnswer::Exact(Arc::from(
                    self.delivery_universe
                        .as_ref()
                        .ok_or_else(|| at("delivery universe unavailable"))?
                        .read(&item["deliveries"])?,
                ))),
                _ => return Err(at("deliveries must be a complete array or case")),
            };
            if class != Class::Reject && self.delivery_universe.is_some() && deliveries.is_none() {
                return Err(at(
                    "deliveries required for a committing answer; use [] or \"case\"",
                ));
            }
            out.push(Answer {
                deliveries,
                where_,
                class,
                reason,
                sets,
            });
        }
        Ok(out)
    }

    /// The laws an answer pins over exactly its inputs. A reject or failure
    /// reason is pinned by excluding every other declared rejection reason.
    pub(super) fn pin(&self, answer: &Answer) -> Result<Vec<(String, String)>> {
        let w = self.law_text(&answer.where_)?;
        let not_here = format!("!({w})");
        let mut laws = Vec::new();
        match answer.class {
            Class::Reject => {
                for (reason, _) in &self.reasons {
                    if Some(*reason) != answer.reason
                        && self
                            .cases
                            .iter()
                            .any(|case| case.class == Class::Reject && case.reason == Some(*reason))
                    {
                        laws.push((format!("reject:{reason}"), not_here.clone()));
                    }
                }
                laws.push(("accept".to_owned(), not_here.clone()));
                if self.failure {
                    laws.push(("failure".to_owned(), not_here));
                }
            }
            Class::Accept => {
                laws.push(("reject".to_owned(), not_here.clone()));
                if self.failure {
                    laws.push(("failure".to_owned(), not_here));
                }
                if let Some(sets) = &answer.sets {
                    for k in 0..self.state_width {
                        let value = sets
                            .get(&k)
                            .cloned()
                            .unwrap_or_else(|| Expr::Var(self.names[k].clone()));
                        laws.push((
                            "accept".to_owned(),
                            format!("({w}) -> ({})", self.successor_law(k, &value)?),
                        ));
                    }
                }
            }
            Class::CommittedFailure => {
                laws.push(("reject".to_owned(), not_here.clone()));
                laws.push(("accept".to_owned(), not_here));
                let reason = answer
                    .reason
                    .ok_or_else(|| Stop::Invalid("failure-reason-required".into()))?;
                laws.push((
                    "failure".to_owned(),
                    format!("({w}) -> (decision_reason == {reason})"),
                ));
                if let Some(sets) = &answer.sets {
                    for k in 0..self.state_width {
                        let value = sets
                            .get(&k)
                            .cloned()
                            .unwrap_or_else(|| Expr::Var(self.names[k].clone()));
                        laws.push((
                            "failure".to_owned(),
                            format!("({w}) -> ({})", self.successor_law(k, &value)?),
                        ));
                    }
                }
            }
        }
        Ok(laws)
    }
}

/// Inputs and their admissible decisions under the laws and the recorded
/// reason requirements.
pub(super) struct Freedom {
    pub(super) stats: Value,
    pub(super) regions: Vec<Region>,
    /// Per tuple: every decision the laws and the recorded reasons admit
    /// (`None` where the Authority refuses), determined tuples included.
    pub(super) admitted: Vec<Vec<Candidate>>,
    pub(super) residual_witnesses: Vec<Value>,
    pub(super) disagreement_witnesses: Vec<Value>,
}

/// Open inputs of one case that admit the same decisions, or the inputs of
/// one case and the decisions every one of them admits.
#[derive(Clone, Debug)]
pub(super) struct Region {
    pub(super) case: usize,
    pub(super) labels: Vec<Label>,
    pub(super) ordinals: Vec<usize>,
}

impl Core {
    fn apply_delivery_answer(
        &self,
        answer: &Answer,
        mask: &[i128],
        admitted: &mut [Vec<Candidate>],
        fixed: &mut Vec<Option<Arc<[library::Delivery]>>>,
    ) -> Result<()> {
        let Some(requirement) = &answer.deliveries else {
            return Ok(());
        };
        if fixed.is_empty() {
            fixed.resize(self.tuples(), None);
        }
        for ordinal in 0..self.tuples() {
            if mask[ordinal] != 1 {
                continue;
            }
            let wanted = match requirement {
                DeliveryAnswer::Exact(values) => values.clone(),
                DeliveryAnswer::Case => {
                    if self.decisions[ordinal].is_none() {
                        return invalid("delivery-case-has-no-native-decision");
                    }
                    self.native_deliveries
                        .get(ordinal)
                        .cloned()
                        .unwrap_or_else(|| Arc::from([]))
                }
            };
            if answer.class == Class::Reject && !wanted.is_empty() {
                return invalid("a-reject-cannot-deliver");
            }
            if self
                .delivery_universe
                .as_ref()
                .map_or(!wanted.is_empty(), |u| !u.allows(&wanted))
            {
                return invalid("delivery-answer-outside-declared-universe");
            }
            if fixed[ordinal].as_ref().is_some_and(|old| old != &wanted) {
                admitted[ordinal].clear();
            } else {
                if !wanted.is_empty() {
                    admitted[ordinal].retain(|c| c.class != Class::Reject);
                }
                fixed[ordinal] = Some(wanted);
            }
        }
        Ok(())
    }

    /// Which inputs the laws and requirements leave open, grouped into
    /// regions by the case that fires there.
    pub(super) fn freedom(&self, answers: &[Answer]) -> Result<Freedom> {
        let mut admissible = self.admissible(&[])?;
        let mut fixed = Vec::new();
        let mut reasons: Vec<(Vec<i128>, u32)> = Vec::new();
        for answer in answers {
            let mask = self.eval(&answer.where_)?;
            admissible =
                self.admissible_from(&self.pin(answer)?, Some(&admissible), Some(&mask))?;
            self.apply_delivery_answer(answer, &mask, &mut admissible, &mut fixed)?;
            if answer.class == Class::Reject
                && let Some(reason) = answer.reason
            {
                reasons.push((mask, reason));
            }
        }
        Ok(self.freedom_from(admissible, &reasons, &fixed))
    }

    fn freedom_from(
        &self,
        mut admissible: Vec<Vec<Candidate>>,
        reasons: &[(Vec<i128>, u32)],
        fixed: &[Option<Arc<[library::Delivery]>>],
    ) -> Freedom {
        let determined_by_laws = admissible
            .iter()
            .enumerate()
            .filter(|(ordinal, c)| self.determination(*ordinal, c, None).determined)
            .count();
        // A recorded reason requirement removes rejects with other reasons
        // where it applies.
        for (ordinal, candidates) in admissible.iter_mut().enumerate() {
            candidates.retain(|c| {
                c.class != Class::Reject
                    || reasons
                        .iter()
                        .all(|(mask, reason)| mask[ordinal] != 1 || c.reason == Some(*reason))
            });
        }
        let determined_with_reasons = admissible
            .iter()
            .enumerate()
            .filter(|(o, c)| self.determination(*o, c, None).determined)
            .count();
        let mut refused = 0usize;
        let mut determined = 0usize;
        let mut inconsistent = 0usize;
        let mut residual_witnesses = Vec::new();
        let mut disagreement_witnesses = Vec::new();
        let mut groups: Vec<Region> = Vec::new();
        for (ordinal, candidates) in admissible.iter().enumerate() {
            let refusal = self.refusals.get(&ordinal);
            if refusal.is_some() {
                refused += 1;
            }
            if candidates.is_empty() {
                inconsistent += 1;
            }
            if candidates.is_empty() || refusal.is_some() {
                residual_witnesses.push(json!({
                    "input_ordinal": ordinal,
                    "row": self.shown(ordinal),
                    "laws": self.declared_laws,
                    "lawful_outcomes": self.outcome_count(candidates, fixed.get(ordinal).is_some_and(Option::is_some)),
                    "reason": if candidates.is_empty() { "zero-lawful-outcomes" } else { "execution-refusal" },
                    "execution_refusal": refusal.map(|r| json!({"law": r.law, "reason": r.text})),
                }));
                continue;
            }
            let wanted = fixed.get(ordinal).and_then(Option::as_deref);
            let measure = self.determination(ordinal, candidates, wanted);
            if measure.disagreement {
                disagreement_witnesses.push(json!({
                    "input_ordinal": ordinal,
                    "row": self.shown(ordinal),
                    "reason": "answer-disagreement",
                    "core": self.decisions[ordinal].map(|d| self.decision_json(d.class, d.reason, Some(d.next))),
                    "core_deliveries": deliveries::shown(self.native_outbox(ordinal)),
                    "required_deliveries": wanted.map(deliveries::shown),
                }));
            }
            if measure.determined {
                determined += 1;
            } else if !measure.disagreement {
                let set: BTreeSet<Label> = candidates
                    .iter()
                    .map(|c| label(c.class, c.reason))
                    .collect();
                let case = self.case_of[ordinal];
                let ordered: Vec<Label> = set.iter().copied().collect();
                match groups
                    .iter_mut()
                    .find(|g| g.case == case && g.labels == ordered)
                {
                    Some(group) => group.ordinals.push(ordinal),
                    None => groups.push(Region {
                        case,
                        labels: ordered,
                        ordinals: vec![ordinal],
                    }),
                }
            }
        }
        // One question per case when some decision is admitted on all of it.
        let mut by_case: BTreeMap<usize, Vec<Region>> = BTreeMap::new();
        let mut first_seen: Vec<usize> = Vec::new();
        for group in groups {
            if !by_case.contains_key(&group.case) {
                first_seen.push(group.case);
            }
            by_case.entry(group.case).or_default().push(group);
        }
        let mut regions = Vec::new();
        for case in first_seen {
            let members = by_case.remove(&case).unwrap_or_default();
            let common: BTreeSet<Label> = members
                .iter()
                .map(|m| m.labels.iter().copied().collect::<BTreeSet<_>>())
                .reduce(|a, b| a.intersection(&b).copied().collect())
                .unwrap_or_default();
            if common.is_empty() {
                regions.extend(members);
            } else {
                let mut ordinals: Vec<usize> =
                    members.iter().flat_map(|m| m.ordinals.clone()).collect();
                ordinals.sort_unstable();
                regions.push(Region {
                    case,
                    labels: common.into_iter().collect(),
                    ordinals,
                });
            }
        }
        regions.sort_by_key(|r| (std::cmp::Reverse(r.ordinals.len()), r.case));
        let tuples = self.tuples();
        let stats = json!({
            "tuples": tuples,
            "reference_refused": refused,
            "determined": determined,
            "determined_by_laws": determined_by_laws,
            "determined_with_reasons": determined_with_reasons,
            "determined_with_delivery_requirements": determined,
            "determined_rate": if tuples == 0 { 0.0 } else { determined as f64 / tuples as f64 },
            "inconsistent": inconsistent,
            "disagreements": disagreement_witnesses.len(),
            "residual": residual_witnesses.len(),
            "open": tuples - determined - residual_witnesses.len(),
            "regions": regions.len(),
        });
        Freedom {
            stats,
            regions,
            admitted: admissible,
            residual_witnesses,
            disagreement_witnesses,
        }
    }

    /// The core's own label on `ordinal`.
    pub(super) fn core_label(&self, ordinal: usize) -> Option<Label> {
        self.decisions[ordinal].map(|d| label(d.class, d.reason))
    }

    /// A label as an option a person reads.
    pub(super) fn option_json(
        &self,
        label: Label,
        case: Option<usize>,
        current: Option<Label>,
    ) -> Value {
        let class = class_of(label.0);
        let mut out = json!({
            "class": class.name(),
            "reason": self.reason_name(label.1),
            "current": current == Some(label),
        });
        if class != Class::Reject
            && let Some(case) = case.and_then(|k| self.cases.get(k))
        {
            out["sets"] = Value::Object(
                case.post
                    .iter()
                    .map(|(k, e)| (self.names[*k].clone(), json!(self.show(e))))
                    .collect(),
            );
        }
        out
    }

    /// Labels come from the shared law-checked universe. A case's successor
    /// expressions are optional guidance and may describe another class: keep
    /// them only when that exact successor is admitted on the whole region.
    fn offered_option(
        &self,
        chosen: Label,
        region: &Region,
        admitted: &[Vec<Candidate>],
        current: Option<Label>,
    ) -> Result<Value> {
        let mut offered = self.option_json(chosen, None, current);
        if class_of(chosen.0) == Class::Reject {
            return Ok(offered);
        }
        if self.delivery_universe.is_some() {
            offered["deliveries"] = json!("case");
        }
        let Some(case) = self.cases.get(region.case) else {
            return Ok(offered);
        };
        let columns = (0..self.state_width)
            .map(|k| {
                case.post
                    .get(&k)
                    .map_or_else(|| Ok(self.columns[k].clone()), |expr| self.eval(expr))
            })
            .collect::<Result<Vec<_>>>()?;
        let lawful = region.ordinals.iter().all(|ordinal| {
            admitted[*ordinal].iter().any(|candidate| {
                label(candidate.class, candidate.reason) == chosen && {
                    let post = self.state(candidate.next);
                    columns
                        .iter()
                        .enumerate()
                        .all(|(k, values)| values[*ordinal] == post[k])
                }
            })
        });
        if lawful {
            offered["sets"] = self.option_json(chosen, Some(region.case), current)["sets"].clone();
        }
        Ok(offered)
    }
}

// --- the question door ---------------------------------------------------------

/// `loop question`: the recorded answers checked in order, then the largest
/// open region as a question.
pub(super) fn question(core: &Core, answers: &[Answer]) -> Result<Value> {
    // Each answer must leave every input some admissible decision, given
    // the answers before it.
    let mut records = Vec::new();
    let mut admitted = core.admissible(&[])?;
    let mut reasons = Vec::new();
    let mut fixed = Vec::new();
    for (index, answer) in answers.iter().enumerate() {
        let mask = core.eval(&answer.where_)?;
        let laws = core.pin(answer)?;
        admitted = core.admissible_from(&laws, Some(&admitted), Some(&mask))?;
        core.apply_delivery_answer(answer, &mask, &mut admitted, &mut fixed)?;
        if answer.class == Class::Reject
            && let Some(reason) = answer.reason
        {
            reasons.push((mask.clone(), reason));
        }
        let freedom = core.freedom_from(admitted.clone(), &reasons, &fixed);
        if freedom.stats["inconsistent"].as_u64().unwrap_or(0) > 0 {
            let Some(ordinal) = freedom.admitted.iter().position(Vec::is_empty) else {
                return invalid("question-inconsistent-statistics-without-an-input");
            };
            return Ok(json!({
                "status": "conflict",
                "answer": index,
                "reason": "answer-conflicts-with-earlier-answers",
                "stats": freedom.stats,
                "determination": determination_scope(core.delivery_universe.is_some(), "declared-laws-and-recorded-answers"),
                "residual_witnesses": freedom.residual_witnesses,
                "input_ordinal": ordinal,
                "example": core.shown(ordinal),
                "requirement": {"answer": index, "where": core.show(&answer.where_),
                    "class": answer.class.name(), "reason": core.reason_name(answer.reason),
                    "laws": laws.iter().map(|(scope, formula)| json!({"scope": scope, "formula": formula})).collect::<Vec<_>>()},
                "earlier_answers": answers[..index].iter().enumerate().filter_map(|(k, earlier)| {
                    core.eval(&earlier.where_).ok().filter(|m| m[ordinal] == 1).map(|_| json!({
                        "answer": k, "where": core.show(&earlier.where_), "class": earlier.class.name(),
                        "reason": core.reason_name(earlier.reason),
                    }))
                }).collect::<Vec<_>>(),
            }));
        }
        let chosen = label(answer.class, answer.reason);
        let inside: Vec<usize> = (0..core.tuples()).filter(|o| mask[*o] == 1).collect();
        // An accept's successor, when the answer gives one, on every tuple.
        let successor: Option<Vec<Vec<i128>>> = match (&answer.sets, answer.class) {
            (Some(sets), Class::Accept | Class::CommittedFailure) => Some(
                (0..core.state_width)
                    .map(|k| match sets.get(&k) {
                        Some(e) => core.eval(e),
                        None => Ok(core.columns[k].clone()),
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            _ => None,
        };
        let differs: Vec<usize> = inside
            .iter()
            .copied()
            .filter(|o| {
                core.core_label(*o) != Some(chosen)
                    || fixed
                        .get(*o)
                        .and_then(Option::as_ref)
                        .is_some_and(|wanted| wanted.as_ref() != core.native_outbox(*o))
                    || successor.as_ref().is_some_and(|columns| {
                        core.decisions[*o].is_some_and(|d| {
                            let post = core.state(d.next);
                            (0..core.state_width).any(|k| columns[k][*o] != post[k])
                        })
                    })
            })
            .collect();
        let mut record = json!({
            "answer": index,
            "where": core.show(&answer.where_),
            "inputs": inside.len(),
            "laws": core.pin(answer)?.into_iter().map(|(scope, f)| json!({"scope": scope, "formula": f})).collect::<Vec<_>>(),
            "reason_requirement": (answer.class == Class::Reject).then(|| core.reason_name(answer.reason)),
            "status": if differs.is_empty() { "recorded" } else { "disagreement" },
        });
        if let Some(deliveries) = &answer.deliveries {
            record["delivery_requirement"] = match deliveries {
                DeliveryAnswer::Case => json!("case"),
                DeliveryAnswer::Exact(values) => deliveries::shown(values),
            };
            record["delivery_requirement_check"] = json!(
                "declared domains and complete native outbox comparison; not emitted as a law"
            );
        }
        if let Some(first) = differs.first() {
            record["disagreement"] = json!({
                "inputs": differs.len(),
                "example": core.shown(*first),
                "core": core.decisions[*first].map(|d| core.decision_json(d.class, d.reason, Some(d.next))),
                "owner": core.option_json(chosen, None, None),
            });
            record["disagreement"]["core"]["deliveries"] =
                deliveries::shown(core.native_outbox(*first));
            if let Some(wanted) = fixed.get(*first).and_then(Option::as_deref) {
                record["disagreement"]["owner"]["deliveries"] = deliveries::shown(wanted);
            }
            if let Some(columns) = &successor {
                record["disagreement"]["owner"]["sets"] = Value::Object(
                    (0..core.state_width)
                        .map(|k| {
                            (
                                core.names[k].clone(),
                                core.value_json(k, columns[k][*first]),
                            )
                        })
                        .collect(),
                );
            }
        }
        records.push(record);
    }
    let freedom = core.freedom_from(admitted, &reasons, &fixed);
    let mut out = json!({
        "status": if freedom.stats["inconsistent"].as_u64().unwrap_or(0) > 0 { "inconsistent" }
            else if freedom.stats["reference_refused"].as_u64().unwrap_or(0) > 0 { "incomplete" }
            else if freedom.stats["determined"].as_u64() == Some(core.tuples() as u64) && core.tuples() > 0 { "determined" }
            else { "open" },
        "stats": freedom.stats,
        "residual_witnesses": freedom.residual_witnesses,
        "disagreement_witnesses": freedom.disagreement_witnesses,
        "answers": records,
        "determination": determination_scope(core.delivery_universe.is_some(), "declared-laws-and-recorded-answers"),
    });
    if let Some(universe) = &core.delivery_universe {
        out["delivery_universe"] = universe.json();
    }
    if let Some(region) = freedom.regions.first() {
        let mut mask = vec![false; core.tuples()];
        for o in &region.ordinals {
            mask[*o] = true;
        }
        let formula = core.region_formula(&mask, Some(region.case))?;
        let witness = region.ordinals[0];
        let current = core.core_label(witness);
        let options = region
            .labels
            .iter()
            .map(|chosen| core.offered_option(*chosen, region, &freedom.admitted, current))
            .collect::<Result<Vec<_>>>()?;
        out["question"] = json!({
            "case": region.case,
            "tuples": region.ordinals.len(),
            "where": core.show(&formula),
            "where_law": core.law_text(&formula)?,
            "example": core.shown(witness),
            "options": options,
            "core": current.map(|l| core.option_json(l, Some(region.case), current)),
        });
        if let Some(universe) = &core.delivery_universe
            && region.labels.iter().any(|l| class_of(l.0) != Class::Reject)
        {
            let own = core.native_outbox(witness);
            let mut choices = vec![Vec::new(), universe.example()];
            if !choices.iter().any(|v| v == own) {
                choices.push(own.to_vec());
            }
            out["question"]["delivery_choices"] = json!(
                choices
                    .iter()
                    .map(|v| json!({
                        "deliveries": deliveries::shown(v),
                        "current": region.ordinals.iter().all(|o| v == core.native_outbox(*o)),
                    }))
                    .collect::<Vec<_>>()
            );
            out["question"]["delivery_question"] = json!(
                "Which deliveries should this decision emit? Supply a complete delivery array, or case to retain its delivery expressions."
            );
            out["question"]["core"]["deliveries"] = deliveries::shown(own);
            out["question"]["core"]["deliveries_scope"] = json!("example-only");
        }
    }
    Ok(out)
}

// --- the teach-back door --------------------------------------------------------

/// A restated rule: where it applies and what it claims the core does.
#[derive(Clone, Debug)]
pub(super) struct Rule {
    pub(super) id: String,
    pub(super) where_: Expr,
    pub(super) class: Class,
    pub(super) reason: Option<u32>,
    /// The whole successor, when given; unset fields keep their value.
    pub(super) sets: Option<BTreeMap<usize, Expr>>,
}

impl Core {
    /// The rules of a `zeno-fcis/loop-rules/1` document.
    pub(super) fn rules(&self, document: &Value) -> Result<Vec<Rule>> {
        if document["schema"] != "zeno-fcis/loop-rules/1" {
            return invalid("rules-schema");
        }
        let mut out = Vec::new();
        for (index, item) in document["rules"]
            .as_array()
            .ok_or_else(|| Stop::Invalid("rules must be a list".into()))?
            .iter()
            .enumerate()
        {
            let at = |what: &str| Stop::Invalid(format!("rule {index}: {what}"));
            let id = item["id"].as_str().unwrap_or("rule").to_owned();
            let where_ = self.read_checked(item["where"].as_str().ok_or_else(|| at("where"))?)?;
            let class = match item["class"].as_str() {
                Some("Accept") => Class::Accept,
                Some("Reject") => Class::Reject,
                Some("CommittedFailure") => Class::CommittedFailure,
                _ => return Err(at("teach-back-class")),
            };
            let reason = match item["reason"].as_str() {
                Some(_) if class == Class::Accept => {
                    return Err(at("teach-back-accept-has-no-reason"));
                }
                Some(name) => Some(
                    self.reason_id(name)
                        .ok_or_else(|| at("teach-back-unknown-reason"))?,
                ),
                None if class == Class::Accept => None,
                None => return Err(at("teach-back-reason-missing")),
            };
            let sets = match &item["sets"] {
                Value::Null => None,
                Value::Object(fields) => {
                    if class == Class::Reject {
                        return Err(at("teach-back-reject-keeps-the-state"));
                    }
                    let mut sets = BTreeMap::new();
                    for (name, text) in fields {
                        let k = self.names[..self.state_width]
                            .iter()
                            .position(|n| n == name)
                            .ok_or_else(|| at("teach-back-successor-field"))?;
                        sets.insert(
                            k,
                            self.read_checked(text.as_str().ok_or_else(|| at("sets"))?)?,
                        );
                    }
                    Some(sets)
                }
                _ => return Err(at("sets")),
            };
            out.push(Rule {
                id,
                where_,
                class,
                reason,
                sets,
            });
        }
        Ok(out)
    }

    /// The rule's claimed successor, one column per state position, when it
    /// gives one.
    pub(super) fn claimed_successor(&self, rule: &Rule) -> Result<Option<Vec<Vec<i128>>>> {
        rule.sets
            .as_ref()
            .map(|sets| {
                (0..self.state_width)
                    .map(|k| match sets.get(&k) {
                        Some(e) => self.eval(e),
                        None => Ok(self.columns[k].clone()),
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()
    }

    /// Whether `candidate` on `ordinal` is the decision `rule` claims.
    fn satisfies(
        &self,
        rule: &Rule,
        claimed: Option<&Vec<Vec<i128>>>,
        ordinal: usize,
        candidate: &Candidate,
    ) -> bool {
        label(candidate.class, candidate.reason) == label(rule.class, rule.reason)
            && claimed.is_none_or(|columns| {
                let post = self.state(candidate.next);
                (0..self.state_width).all(|k| columns[k][ordinal] == post[k])
            })
    }

    /// Where `rule` applies, and where the core decides what it claims.
    pub(super) fn decides(&self, rule: &Rule) -> Result<(Vec<bool>, Vec<bool>)> {
        if kind_of(&rule.where_, &self.vocab)?.is_some_and(|kind| kind != Kind::Bool) {
            return invalid("teach-back-condition-must-be-boolean");
        }
        let cond: Vec<bool> = self.eval(&rule.where_)?.iter().map(|v| *v == 1).collect();
        let claimed = self.claimed_successor(rule)?;
        let mut same = vec![false; self.tuples()];
        for (ordinal, slot) in same.iter_mut().enumerate() {
            let Some(d) = self.decisions[ordinal] else {
                continue;
            };
            let mut ok = label(d.class, d.reason) == label(rule.class, rule.reason);
            if ok && let Some(claimed) = &claimed {
                let post = self.state(d.next);
                ok = (0..self.state_width).all(|k| claimed[k][ordinal] == post[k]);
            }
            *slot = ok;
        }
        Ok((cond, same))
    }

    /// The rule's meaning: where it applies, what it claims and, when it
    /// gives one, its successor on every tuple. [`Core::decides`] compares
    /// that successor wherever the core's label is the rule's, inside the
    /// rule's region and outside it (the missing inputs), so it is keyed
    /// everywhere: equal keys give equal `decides` results, hence equal
    /// verdicts and counts.
    pub(super) fn rule_key(&self, rule: &Rule) -> Result<String> {
        let (cond, _) = self.decides(rule)?;
        let (code, reason) = label(rule.class, rule.reason);
        let mut parts = vec![
            cond.iter().map(|c| u8::from(*c)).collect::<Vec<u8>>(),
            [
                i128::from(code).to_be_bytes(),
                i128::from(reason.unwrap_or(0)).to_be_bytes(),
                i128::from(rule.sets.is_some()).to_be_bytes(),
            ]
            .concat(),
        ];
        if let Some(sets) = &rule.sets {
            for k in 0..self.state_width {
                let values = match sets.get(&k) {
                    Some(e) => self.eval(e)?,
                    None => self.columns[k].clone(),
                };
                parts.push(values.iter().flat_map(|v| v.to_be_bytes()).collect());
            }
        }
        Ok(meaning_digest("rule", &parts))
    }

    /// The reachable tuples: those whose state is reachable from genesis.
    pub(super) fn reachable_tuples(&self) -> Vec<bool> {
        (0..self.tuples())
            .map(|o| self.reachable[o / self.inputs])
            .collect()
    }

    /// A region as reported: its count, an exact formula when synthesis
    /// finds one, its first input.
    pub(super) fn region_json(&self, mask: &[bool]) -> Result<Value> {
        let count = mask.iter().filter(|m| **m).count();
        let Some(first) = mask.iter().position(|m| *m) else {
            return Ok(Value::Null);
        };
        let formula = self
            .synthesized_region(mask)?
            .filter(|node| size(node) <= MAX_FORMULA)
            .filter(|node| {
                self.eval(node)
                    .is_ok_and(|v| v.iter().zip(mask).all(|(v, m)| (*v == 1) == *m))
            });
        Ok(json!({
            "count": count,
            "where": formula.as_ref().map(|f| self.show(f)),
            "ordinal": first,
            "input": self.shown(first),
        }))
    }
}

/// `loop teach-back`: each rule judged on the whole declared domain, and a
/// summary of the exact and holding rules.
///
/// `required` false leaves the admissibility table out (the door's retry
/// when computing it did not finish in its stage).
pub(super) fn teach_back(
    core: &Core,
    rules: &[Rule],
    answers: &[Answer],
    required: bool,
) -> Result<Value> {
    let reach = core.reachable_tuples();
    let mut judged = Vec::new();
    let mut covered = vec![0usize; core.tuples()];
    let mut exact_rules = 0;
    let mut verdicts = BTreeMap::new();
    // Whether the declared laws and the recorded answers already require
    // each rule where it holds. The verdicts need no admissibility table,
    // so one over its budget only leaves `required` out.
    let (freedom, required_skipped) = if required {
        match core.freedom(answers) {
            Ok(freedom) => (Some(freedom), None),
            Err(Stop::Inconclusive(reason)) => (None, Some(reason)),
            Err(stop) => return Err(stop),
        }
    } else {
        (
            None,
            Some("admissibility-not-finished-in-its-stage".to_owned()),
        )
    };
    for rule in rules {
        let report = match teach_one(core, rule, &reach, freedom.as_ref()) {
            Ok(report) => report,
            Err(Stop::Invalid(reason)) => {
                json!({"id": rule.id, "verdict": "refused", "reason": reason})
            }
            Err(stop) => return Err(stop),
        };
        let verdict = report["verdict"].as_str().unwrap_or("refused").to_owned();
        if verdict == "exact" || verdict == "holds" {
            if verdict == "exact" {
                exact_rules += 1;
            }
            let cond = core.decides(rule)?.0;
            for (o, c) in cond.iter().enumerate() {
                if *c {
                    covered[o] += 1;
                }
            }
        }
        verdicts.insert(rule.id.clone(), verdict);
        judged.push(report);
    }
    let decided = core.decisions.iter().filter(|d| d.is_some()).count();
    let covered_count = covered.iter().filter(|c| **c > 0).count();
    Ok(json!({
        "rules": judged,
        "summary": {
            "rules": rules.len(),
            "verdicts": verdicts,
            "exact": exact_rules,
            "covered": covered_count,
            "domain": decided,
            "overlapping": covered.iter().filter(|c| **c > 1).count(),
            "complete": covered_count == decided && decided > 0,
            "required": match required_skipped {
                None => json!("computed"),
                Some(reason) => json!({"not_computed": reason}),
            },
        },
    }))
}

/// What the core decides instead on an exception: its label and, when it
/// commits, the successor.
type ExceptionKey = (Label, Option<usize>);

fn teach_one(core: &Core, rule: &Rule, reach: &[bool], freedom: Option<&Freedom>) -> Result<Value> {
    if core.decisions.iter().any(Option::is_none) {
        return invalid("teach-back-core-has-authority-refusals");
    }
    let (cond, same) = core.decides(rule)?;
    let n = core.tuples();
    let applies = cond.iter().filter(|c| **c).count();
    let exceptions: Vec<bool> = (0..n).map(|o| cond[o] && !same[o]).collect();
    let missing: Vec<bool> = (0..n).map(|o| !cond[o] && same[o]).collect();
    let count = |mask: &[bool]| mask.iter().filter(|m| **m).count();
    let on_reach = |mask: &[bool]| (0..n).filter(|o| mask[*o] && reach[*o]).count();
    let verdict = if applies == 0 {
        "vacuous"
    } else if count(&exceptions) > 0 {
        "wrong"
    } else if count(&missing) > 0 {
        "holds"
    } else {
        "exact"
    };
    // Exceptions grouped by what the core decides instead, largest first.
    let mut groups: BTreeMap<ExceptionKey, Vec<usize>> = BTreeMap::new();
    for o in (0..n).filter(|o| exceptions[*o]) {
        if let Some(d) = core.decisions[o] {
            let successor = (d.class != Class::Reject).then_some(d.next);
            groups
                .entry((label(d.class, d.reason), successor))
                .or_default()
                .push(o);
        }
    }
    let mut ordered: Vec<(ExceptionKey, Vec<usize>)> = groups.into_iter().collect();
    ordered.sort_by_key(|(_, members)| std::cmp::Reverse(members.len()));
    let mut exception_groups = Vec::new();
    for ((l, successor), members) in ordered.into_iter().take(MAX_GROUPS) {
        let mut mask = vec![false; n];
        for o in &members {
            mask[*o] = true;
        }
        let mut region = core.region_json(&mask)?;
        region["actual"] = core.decision_json(class_of(l.0), l.1, successor);
        exception_groups.push(region);
    }
    let mut out = json!({
        "id": rule.id,
        "where": core.show(&rule.where_),
        "claim": core.decision_json(rule.class, rule.reason, None),
        "domain": n,
        "applies": applies,
        "applies_reachable": on_reach(&cond),
        "holds_on": (0..n).filter(|o| cond[*o] && same[*o]).count(),
        "exceptions": count(&exceptions),
        "missing": count(&missing),
        "exceptions_reachable": on_reach(&exceptions),
        "missing_reachable": on_reach(&missing),
        "verdict": verdict,
        "exact_on_reachable": applies > 0 && on_reach(&exceptions) == 0 && on_reach(&missing) == 0,
        "exception_groups": exception_groups,
        "missing_region": core.region_json(&missing)?,
        "meaning": core.rule_key(rule)?,
    });
    if let Some(freedom) = freedom
        && (verdict == "exact" || verdict == "holds")
    {
        // The requirements force the rule on a tuple only when every
        // decision they admit there is the rule's; they forbid it when none
        // is (an empty admitted set included). Counting admitted decisions
        // is not enough: one forced decision can contradict the rule.
        let region: Vec<usize> = (0..n).filter(|o| cond[*o] && same[*o]).collect();
        let claimed = core.claimed_successor(rule)?;
        let mut open = Vec::new();
        let mut forbidden = Vec::new();
        for o in &region {
            let admitted = &freedom.admitted[*o];
            let good = admitted
                .iter()
                .filter(|c| core.satisfies(rule, claimed.as_ref(), *o, c))
                .count();
            if good == 0 {
                forbidden.push(*o);
            } else if good < admitted.len() {
                open.push(*o);
            }
        }
        let mut alternatives = BTreeSet::new();
        for o in open.iter().take(512) {
            alternatives.extend(
                freedom.admitted[*o]
                    .iter()
                    .map(|c| label(c.class, c.reason)),
            );
        }
        out["required"] = json!({
            "open": open.len(),
            "forbidden": forbidden.len(),
            "forced": region.len() - open.len() - forbidden.len(),
            "of": region.len(),
            "example": open.first().map(|o| core.shown(*o)),
            "alternatives": alternatives.into_iter().map(|l| json!([class_of(l.0).name(), core.reason_name(l.1)])).collect::<Vec<_>>(),
        });
        if let Some(first) = forbidden.first() {
            out["required"]["forbidden_example"] = json!({
                "input": core.shown(*first),
                "admitted": freedom.admitted[*first].iter().take(4)
                    .map(|c| core.decision_json(c.class, c.reason, Some(c.next))).collect::<Vec<_>>(),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/support/cpmm.rs"]
pub(crate) mod cpmm;

#[cfg(test)]
#[path = "loop_judges_arithmetic_tests.rs"]
mod arithmetic_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn genesis_question_loader_refuses_law_500() {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/apps/library-loans");
        let project =
            std::fs::read_to_string(base.join("project.zeno")).unwrap_or_else(|e| panic!("{e}"));
        let mut rules: Value = serde_json::from_str(
            &std::fs::read_to_string(base.join("v2/policy.json")).unwrap_or_else(|e| panic!("{e}")),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        rules["genesis"] = json!({"110": 1, "111": 1, "112": 1});
        let result = Core::from_sources(project, rules.to_string());
        assert!(
            matches!(result, Err(Stop::Invalid(ref reason)) if reason.contains("no admissible starting state") && reason.contains("law 500"))
        );
    }
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn delivery_counter(eventful: bool) -> Core {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
        let mut project = std::fs::read_to_string(folder.join("project.zeno.in"))
            .unwrap_or_else(|e| panic!("{e}"))
            .replace("${C}", "1")
            .replace("${INSTANCE}", "delivery-counter");
        project.push_str("\ntype 103 destination Sink;\ntype 104 payload Notice;\nfield 132 104 count 105;\nchannel 300 notify destination 103 payload 104;\nchannel 301 alternate destination 103 payload 104;\n");
        let rules = std::fs::read_to_string(folder.join("policy.json.in"))
            .unwrap_or_else(|e| panic!("{e}"))
            .replace("${C}", "1")
            .replace("${INSTANCE}", "delivery-counter");
        let mut rules: Value = serde_json::from_str(&rules).unwrap_or_else(|e| panic!("{e}"));
        rules["leaf_bindings"]["103"] = json!(["Text", 1, 32]);
        if eventful {
            rules["cases"][3]["outbox"] = json!([{"ordinal":0,"channel":301,"destination":"sink",
                "payload":{"132":"value+1"},"idempotency_ordinal":1}]);
            rules["cases"][4]["outbox"] = json!([{"ordinal":0,"channel":300,"destination":"sink",
                "payload":{"132":"value"},"idempotency_ordinal":1}]);
        }
        Core::from_sources(project, rules.to_string()).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn dual_approval_answer_laws_share_determination_and_name_uncovered_deliveries() {
        use crate::contract::review::gaps;
        let core = Core::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval"),
            true,
        )
        .unwrap_or_else(|e| panic!("witness: {e:?}"));
        let document: Value = serde_json::from_str(include_str!(
            "../tests/data/dual-approval-case-answers.json"
        ))
        .unwrap_or_else(|e| panic!("witness answers: {e}"));
        let answers = core.answers(&document).unwrap_or_else(|e| panic!("{e:?}"));
        let question = question(&core, &answers).unwrap_or_else(|e| panic!("{e:?}"));
        let (laws, _) = core
            .pinned_lines(&answers)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let judge = |proposals: &str| {
            gaps::judge(
                core.project(),
                core.rules_text(),
                gaps::Options {
                    max_tuples: MAX_TUPLES,
                    max_work: 100_000_000,
                    proposals: Some(proposals),
                    mutants: false,
                },
            )
            .unwrap_or_else(|e| panic!("gaps: {e:?}"))
        };
        let gap = judge(&laws);
        let report = gap.json();
        assert_eq!(report["status"], "measured", "{report}");
        let after = &report["proposals"]["after"];
        assert_eq!(question["stats"]["determined"], 384);
        assert_eq!(question["stats"]["determined_by_laws"], 300);
        assert_eq!(
            after["admissible"]["full"]["determined"],
            question["stats"]["determined_by_laws"]
        );
        for scope in [
            &question["determination"],
            &after["determination"],
            &gap.summary()["with_proposals"]["determination"],
        ] {
            assert_eq!(scope["uncovered_components"], json!(["deliveries"]));
            assert_eq!(scope["uncovered_scope"], "exported-laws");
        }
        // The reporter's direct reason law is expressible and redundant after
        // the shared reason-class repair; it must not change the measure.
        let extended =
            format!("{laws}\nlaw 9998 reject_reason on reject = decision_reason != 202;\n");
        let extra = judge(&extended).json();
        assert_eq!(extra["status"], "measured", "{extra}");
        assert_eq!(
            extra["proposals"]["after"]["admissible"]["full"]["determined"],
            300
        );
        // The reported delivery-law witness binds the payload observation to
        // its channel explicitly; a channel ID is not an outbox type path.
        let project = format!(
            "{}\nlaw 520 only_a_release_delivers on commit = outbox.104 <= 1 && (post.100.110 == 151 -> outbox.104 == 1) && (post.100.110 != 151 -> outbox.104 == 0);\n",
            core.project()
        );
        let mut rules: Value =
            serde_json::from_str(core.rules_text()).unwrap_or_else(|e| panic!("{e}"));
        rules["law_kinds"]["520"] = json!("DebitCreditEffectEquality");
        rules["delivery_laws"] =
            json!({"520":{"max_deliveries":1,"observations":{"outbox.104":{"channel":300}}}});
        let unsupported = gaps::judge(
            &project,
            &rules.to_string(),
            gaps::Options {
                max_tuples: MAX_TUPLES,
                max_work: 100_000_000,
                proposals: None,
                mutants: false,
            },
        )
        .unwrap_or_else(|e| panic!("delivery-law witness: {e:?}"))
        .json();
        assert_eq!(unsupported["status"], "out-of-scope", "{unsupported}");
        assert_eq!(
            unsupported["determination"]["uncovered_components"],
            json!(["deliveries"])
        );
        let question_scope = Core::from_sources(project, rules.to_string())
            .and_then(|prepared| prepared.admissible(&[]));
        assert!(
            matches!(question_scope, Err(Stop::OutOfScope(_))),
            "{question_scope:?}"
        );
    }

    #[test]
    fn gap_and_question_exclude_native_refusals_from_determined() {
        use crate::contract::review::gaps;
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
        let read = |name: &str| {
            std::fs::read_to_string(folder.join(name))
                .unwrap_or_else(|e| panic!("{e}"))
                .replace("${C}", "1")
                .replace("${INSTANCE}", "refusal-counter")
        };
        let project = read("project.zeno.in");
        let mut rules: Value =
            serde_json::from_str(&read("policy.json.in")).unwrap_or_else(|e| panic!("{e}"));
        rules["cases"][0]["when"] = json!("true");
        let core = Core::from_sources(project.clone(), rules.to_string())
            .unwrap_or_else(|e| panic!("{e:?}"));
        let question = question(&core, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        let gap = gaps::judge(
            &project,
            &rules.to_string(),
            gaps::Options {
                max_tuples: MAX_TUPLES,
                max_work: 100_000_000,
                proposals: None,
                mutants: false,
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .json();
        assert_eq!(gap["status"], "measured");
        assert!(question["stats"]["reference_refused"].as_u64().unwrap_or(0) > 0);
        assert_eq!(
            gap["admissible"]["full"]["determined"],
            question["stats"]["determined"]
        );
        assert!(question["stats"]["determined"].as_u64().unwrap_or(0) < core.tuples() as u64);
    }

    #[test]
    fn committing_answers_require_deliveries_and_empty_lists_advance() {
        let core = Core::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval"),
            true,
        )
        .unwrap_or_else(|e| panic!("witness: {e:?}"));
        let mut document: Value = serde_json::from_str(include_str!(
            "../tests/data/dual-approval-literal-answers.json"
        ))
        .unwrap_or_else(|e| panic!("witness answers: {e}"));
        document["answers"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("answers"))
            .truncate(2);
        document["answers"][1]
            .as_object_mut()
            .unwrap_or_else(|| panic!("answer"))
            .remove("deliveries");
        assert!(
            matches!(core.answers(&document), Err(Stop::Invalid(reason))
            if reason == "answer 1: deliveries required for a committing answer; use [] or \"case\""),
            "an omitted delivery requirement must refuse instead of repeating the question"
        );
        document["answers"][1]["deliveries"] = json!([]);
        let answers = core.answers(&document).unwrap_or_else(|e| panic!("{e:?}"));
        let report = question(&core, &answers).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(report["answers"][1]["status"], "recorded");
        assert_eq!(report["stats"]["determined"], 304);
        assert_ne!(report["question"]["where"], document["answers"][1]["where"]);
        let first = question(&core, &answers[..1]).unwrap_or_else(|e| panic!("{e:?}"));
        for option in first["question"]["options"]
            .as_array()
            .unwrap_or_else(|| panic!("options"))
        {
            if option["class"] != "Reject" {
                assert_eq!(
                    option["deliveries"], "case",
                    "offered options must be submittable"
                );
            }
        }
    }

    #[test]
    fn literal_delivery_disagreements_remain_open_on_the_reported_dual_approval_inputs() {
        let core = Core::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval"),
            true,
        )
        .unwrap_or_else(|e| panic!("witness: {e:?}"));
        let document: Value = serde_json::from_str(include_str!(
            "../tests/data/dual-approval-literal-answers.json"
        ))
        .unwrap_or_else(|e| panic!("witness answers: {e}"));
        let answers = core.answers(&document).unwrap_or_else(|e| panic!("{e:?}"));
        let report = question(&core, &answers).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(report["answers"][3]["disagreement"]["inputs"], 20);
        assert_eq!(
            report["answers"][3]["disagreement"]["owner"]["deliveries"],
            document["answers"][3]["deliveries"]
        );
        assert_eq!(report["stats"]["determined"], 364, "{report}");
        assert_eq!(report["stats"]["open"], 20);
        assert_eq!(report["status"], "open");
        let witnesses = report["disagreement_witnesses"]
            .as_array()
            .unwrap_or_else(|| panic!("witnesses"));
        assert_eq!(witnesses.len(), 20);
        for witness in witnesses {
            assert_ne!(witness["required_deliveries"], witness["core_deliveries"]);
            assert_eq!(witness["reason"], "answer-disagreement");
            assert!(witness["row"].is_object());
        }
        let document: Value = serde_json::from_str(include_str!(
            "../tests/data/dual-approval-case-answers.json"
        ))
        .unwrap_or_else(|e| panic!("case answers: {e}"));
        let answers = core.answers(&document).unwrap_or_else(|e| panic!("{e:?}"));
        let report = question(&core, &answers).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(report["stats"]["determined"], 384);
        assert_eq!(report["status"], "determined");
    }

    #[test]
    fn a_missing_delivery_from_the_universe_is_caught_before_determination() {
        let mut core = delivery_counter(false);
        let before = question(&core, &[]).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(before["status"], "open");
        assert_eq!(before["stats"]["determined"], 6);
        core.delivery_universe
            .as_mut()
            .unwrap_or_else(|| panic!("universe"))
            .omit_delivery_channel();
        assert!(matches!(question(&core, &[]), Err(Stop::OutOfScope(_))));
    }

    #[test]
    fn delivery_answers_resolve_a_delivery_only_gap_and_detect_conflicts() {
        let core = delivery_counter(false);
        let answer = json!({"where":"authorized && action == 150 && value == 0", "class":"Accept",
            "sets":{"value":"1"}, "deliveries":[]});
        let checked = core
            .answers(&json!({"schema":"zeno-fcis/loop-answers/1","answers":[answer.clone()]}))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let report = question(&core, &checked).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(report["stats"]["determined"], 7);
        assert_eq!(report["answers"][0]["status"], "recorded");
        let mut conflicting = answer;
        conflicting["deliveries"] = deliveries::shown(
            &core
                .delivery_universe
                .as_ref()
                .unwrap_or_else(|| panic!("universe"))
                .example(),
        );
        let checked = core
            .answers(&json!({"schema":"zeno-fcis/loop-answers/1",
            "answers":[{"where":"authorized && action == 150 && value == 0","class":"Accept",
                "sets":{"value":"1"},"deliveries":[]},conflicting]}))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(
            question(&core, &checked).unwrap_or_else(|e| panic!("{e:?}"))["status"],
            "conflict"
        );
    }

    #[test]
    fn delivery_answers_compare_channel_destination_payload_marker_and_ordinal() {
        let core = delivery_counter(true);
        let ordinal = (0..core.tuples())
            .find(|o| {
                core.native_outbox(*o)
                    .first()
                    .is_some_and(|d| d.channel == 300)
            })
            .unwrap_or_else(|| panic!("delivery witness"));
        let original = deliveries::shown(core.native_outbox(ordinal));
        for changed_field in [
            "unchanged",
            "channel",
            "destination",
            "payload",
            "idempotency",
            "ordinal",
        ] {
            let mut wanted = original.clone();
            match changed_field {
                "channel" => wanted[0]["channel"] = json!(301),
                "destination" => wanted[0]["destination"] = json!("other"),
                "payload" => wanted[0]["payload"]["132"] = json!("0"),
                "idempotency" => wanted[0]["idempotency_ordinal"] = json!("0"),
                "ordinal" => wanted[0]["ordinal"] = json!(1),
                _ => {}
            }
            let checked = core
                .answers(&json!({"schema":"zeno-fcis/loop-answers/1","answers":[{
                "where":"authorized && action == 151 && value == 1", "class":"Accept",
                "sets":{"value":"0"},"deliveries":wanted}]}))
                .unwrap_or_else(|e| panic!("{e:?}"));
            let report = question(&core, &checked).unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(
                report["answers"][0]["status"],
                if changed_field == "unchanged" {
                    "recorded"
                } else {
                    "disagreement"
                },
                "{changed_field}: {report}"
            );
        }
        let mut unknown_channel = original;
        unknown_channel[0]["channel"] = json!(302);
        assert!(
            core.answers(&json!({"schema":"zeno-fcis/loop-answers/1","answers":[{
            "where":"true", "class":"Accept", "deliveries":unknown_channel}]}))
                .is_err()
        );
    }

    #[test]
    fn arithmetic_law_writers_compile_and_check_with_the_native_laws() {
        let core = Core::from_sources(cpmm::project(2), cpmm::policy(2, 30, false).to_string())
            .unwrap_or_else(|error| panic!("core: {error:?}"));
        let baseline = core.admissible(&[]).unwrap_or_else(|e| panic!("{e:?}"));
        for text in [
            "(x + 1) * (y + 2) >= x * y",
            "mul_div_floor(amount, 30, 10000) <= mul_div_ceil(amount, 30, 10000)",
            "products_ge(x, 2, x, 2)",
            "products_ge(x, 2, 3, x, 2, 3)",
        ] {
            let expression = parse(text, 0, Allow::PLAIN).unwrap_or_else(|e| panic!("{e:?}"));
            let law = core
                .law_text(&expression)
                .unwrap_or_else(|e| panic!("{e:?}"));
            let admitted = core
                .admissible(&[("commit".to_owned(), law.clone())])
                .unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(admitted, baseline, "{text}: {law}");
        }
        let constrained = core
            .admissible(&[("commit".to_owned(), cpmm::RELATION.to_owned())])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_ne!(
            constrained, baseline,
            "product comparison admitted every successor"
        );
    }

    fn check_prefixes(family: &str, parameter: &str, value: i128, answers: &str) {
        let root = std::env::temp_dir().join(format!(
            "loop-question-prefixes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("v2"))
            .unwrap_or_else(|error| panic!("scratch: {error:?}"));
        let template = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../core-components")
            .join(family);
        for (from, to) in [
            ("project.zeno.in", "project.zeno"),
            ("policy.json.in", "v2/policy.json"),
        ] {
            let text = std::fs::read_to_string(template.join(from))
                .unwrap_or_else(|error| panic!("template: {error:?}"))
                .replace(&format!("${{{parameter}}}"), &value.to_string())
                .replace("${INSTANCE}", "question-prefixes");
            std::fs::write(root.join(to), text)
                .unwrap_or_else(|error| panic!("contract: {error:?}"));
        }
        let core = Core::load(&root, false).unwrap_or_else(|error| panic!("core: {error:?}"));
        let document: Value =
            serde_json::from_str(answers).unwrap_or_else(|error| panic!("answers: {error:?}"));
        let answers = core
            .answers(&document)
            .unwrap_or_else(|error| panic!("checked answers: {error:?}"));
        for prefix in 0..=answers.len() {
            let mut laws = Vec::new();
            let mut reasons = Vec::new();
            for answer in &answers[..prefix] {
                laws.extend(
                    core.pin(answer)
                        .unwrap_or_else(|error| panic!("pinned laws: {error:?}")),
                );
                if answer.class == Class::Reject
                    && let Some(reason) = answer.reason
                {
                    reasons.push((
                        core.eval(&answer.where_)
                            .unwrap_or_else(|error| panic!("mask: {error:?}")),
                        reason,
                    ));
                }
            }
            // Independent full-domain re-evaluation of every law on every
            // candidate retains the previous algorithm as a test oracle.
            let reference = core.freedom_from(
                core.admissible(&laws)
                    .unwrap_or_else(|error| panic!("full laws: {error:?}")),
                &reasons,
                &[],
            );
            let narrowed = core
                .freedom(&answers[..prefix])
                .unwrap_or_else(|error| panic!("narrowed table: {error:?}"));
            assert_eq!(
                narrowed.admitted, reference.admitted,
                "{family} prefix {prefix}"
            );
            assert_eq!(narrowed.stats, reference.stats, "{family} prefix {prefix}");
            let report = question(&core, &answers[..prefix])
                .unwrap_or_else(|error| panic!("question: {error:?}"));
            // Complete base laws can expose a contradiction before the last
            // submitted answer. The question flow returns that first prefix.
            let expected = if report["status"] == "conflict" {
                let first = report["answer"]
                    .as_u64()
                    .unwrap_or_else(|| panic!("required value missing"))
                    as usize
                    + 1;
                core.freedom(&answers[..first])
                    .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                    .stats
            } else {
                reference.stats
            };
            assert_eq!(report["stats"], expected, "{family} prefix {prefix}");
        }
        std::fs::remove_dir_all(root).unwrap_or_else(|error| panic!("remove scratch: {error:?}"));
    }

    #[test]
    fn question_narrowing_matches_full_law_evaluation_for_every_answer_prefix() {
        check_prefixes(
            "bounded-counter",
            "C",
            3,
            include_str!("../../zeno-fcis-loop-doors/tests/data/bc-c3-explicit-answers.json"),
        );
        check_prefixes(
            "approval-queue",
            "K",
            1,
            include_str!("../../zeno-fcis-loop-doors/tests/data/aq-k1-explicit-answers.json"),
        );
        check_prefixes(
            "bounded-counter",
            "C",
            3,
            r#"{"schema":"zeno-fcis/loop-answers/1","answers":[
            {"where":"!authorized","class":"Reject","reason":"at_zero"},
            {"where":"value == 0","class":"Reject","reason":"at_capacity"}
        ]}"#,
        );
    }
    #[test]
    fn question_and_gap_judge_share_reason_class_decision_space() {
        use crate::contract::review::gaps;
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
        let project = std::fs::read_to_string(base.join("project.zeno"))
            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
        let rules = std::fs::read_to_string(base.join("v2/policy.json"))
            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
        let proposals = std::fs::read_to_string(base.join("proposed-laws.txt"))
            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
        for closed in [false, true] {
            let mut project = project.clone();
            let mut policy: Value = serde_json::from_str(&rules)
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            if closed {
                project.push('\n');
                project.push_str(&proposals);
                for id in 9000..=9004 {
                    policy["law_kinds"][id.to_string()] = json!(ADDED_KIND.name());
                }
            }
            let rules = policy.to_string();
            let core = Core::from_sources(project.clone(), rules.clone())
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            let admitted = core
                .admissible(&[])
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            let report = gaps::judge(
                &project,
                &rules,
                gaps::Options {
                    max_tuples: MAX_TUPLES,
                    max_work: 100_000_000,
                    proposals: None,
                    mutants: false,
                },
            )
            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"))
            .json();
            assert_eq!(report["status"], "measured", "{report}");
            let determined = admitted
                .iter()
                .filter(|decisions| core.outcome_count(decisions, false) == 1)
                .count();
            assert_eq!(determined, if closed { 300 } else { 0 });
            assert_eq!(report["admissible"]["full"]["determined"], determined);
            let lines = admitted
                .iter()
                .enumerate()
                .map(|(ordinal, decisions)| {
                    if core.decisions[ordinal].is_none() {
                        "-\n".to_owned()
                    } else {
                        format!("{}\n", core.outcome_count(decisions, false))
                    }
                })
                .collect::<String>();
            assert_eq!(
                report["tables"]["admissible_sha256"],
                sha256_hex(lines.as_bytes()),
                "closed={closed}"
            );
            // Question completion also requires successful native execution.
            let executable_determined = admitted
                .iter()
                .zip(&core.decisions)
                .filter(|(decisions, native)| {
                    core.outcome_count(decisions, false) == 1 && native.is_some()
                })
                .count();
            assert_eq!(
                question(&core, &[])
                    .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"))["stats"]
                    ["determined"],
                executable_determined
            );
        }
    }

    #[test]
    fn every_offered_question_option_can_be_submitted_without_conflict() {
        let mut omitted_updates = 0;
        for example in ["dual-approval", "guide-counter"] {
            let base = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples")
                .join(example);
            let core = Core::load(&base, true)
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            let report = question(&core, &[])
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            let mut questions = vec![(
                report["question"]["where"].clone(),
                report["question"]["options"]
                    .as_array()
                    .unwrap_or_else(|| panic!("expected fixture value"))
                    .clone(),
            )];
            let admitted = core
                .admissible(&[])
                .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
            let freedom = core.freedom_from(admitted, &[], &[]);
            // Exercise every eligible region, including smaller rejecting
            // branches that the first largest-region question does not show.
            assert!(freedom.regions.len() > 1, "{example}");
            for region in &freedom.regions {
                let mut mask = vec![false; core.tuples()];
                for ordinal in &region.ordinals {
                    mask[*ordinal] = true;
                }
                let formula = core
                    .region_formula(&mask, Some(region.case))
                    .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
                let current = core.core_label(region.ordinals[0]);
                let options = region
                    .labels
                    .iter()
                    .map(|chosen| {
                        core.offered_option(*chosen, region, &freedom.admitted, current)
                            .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"))
                    })
                    .collect();
                questions.push((json!(core.show(&formula)), options));
            }
            for (where_, offered) in questions {
                assert!(!offered.is_empty(), "{example}: {report}");
                for option in offered {
                    omitted_updates +=
                        usize::from(option["class"] != "Reject" && option.get("sets").is_none());
                    let mut answer = option.clone();
                    answer["where"] = where_.clone();
                    answer
                        .as_object_mut()
                        .unwrap_or_else(|| panic!("expected fixture value"))
                        .remove("current");
                    let answers = core
                        .answers(&json!({"schema":"zeno-fcis/loop-answers/1", "answers":[answer]}))
                        .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
                    let submitted = question(&core, &answers)
                        .unwrap_or_else(|error| panic!("experience fixture failed: {error:?}"));
                    assert_ne!(
                        submitted["status"], "conflict",
                        "{example}: offered={option}; submitted={submitted}"
                    );
                }
            }
        }
        assert!(
            omitted_updates > 0,
            "the examples must exercise an unlawful suggested update"
        );
    }

    #[test]
    fn rejection_answers_do_not_bind_failure_only_reasons() {
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../core-components/bounded-counter");
        let mut project = std::fs::read_to_string(base.join("project.zeno.in"))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
            .replace("${C}", "1");
        project.push_str(
            "reason 203 failure_only precedence 3;\nlaw 503 failure_shape on failure = true;\n",
        );
        let mut policy: Value = serde_json::from_str(
            &std::fs::read_to_string(base.join("policy.json.in"))
                .unwrap_or_else(|error| panic!("required result failed: {error:?}"))
                .replace("${C}", "1")
                .replace("${INSTANCE}", "c1"),
        )
        .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        policy["law_kinds"]["503"] = json!("CommittedFailureEffects");
        policy["cases"].as_array_mut().unwrap_or_else(|| panic!("required value missing")).insert(0, json!({"when":"false","class":"CommittedFailure","reason":203,"post":{"110":"value"},"outbox":[]}));
        let core = Core::from_sources(project, policy.to_string())
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let answers=core.answers(&json!({"schema":"zeno-fcis/loop-answers/1","answers":[{"where":"!authorized","class":"Reject","reason":"not_authorized"}]})).unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let laws = core
            .pin(&answers[0])
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        assert!(laws.iter().any(|(scope, _)| scope == "reject:201"));
        assert!(!laws.iter().any(|(scope, _)| scope == "reject:203"));
        core.admissible(&laws)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let failure = core
            .answers(&json!({"schema":"zeno-fcis/loop-answers/1","answers":[{
            "where":"authorized && action == 150 && value == 0", "class":"CommittedFailure",
            "reason":"failure_only", "sets":{"value":"1"}, "deliveries":[]}]}))
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        let report = question(&core, &failure)
            .unwrap_or_else(|error| panic!("required result failed: {error:?}"));
        // The complete counter laws require an accept on this input, so a
        // failure requirement must be a conflict rather than an unsupported answer.
        assert_eq!(report["status"], "conflict", "{report}");
    }
}
