//! Advisory first-match coverage. Read the generated selection graph without
//! changing the bound policy, decision evaluation, or its refusal accounting.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use zeno_fcis_synthesis::finite::{
    V2NumericAtom as Atom, V2NumericDomain as Domain, V2NumericOp as Op,
    V2NumericProgram as Program, V2Resource as Resource, execute_numeric_v2, v2_zero_limits,
};

use super::super::{model::Contract, policy, rules::Rules};
use super::{ContractError, mutants};

const MEANING: &str = "decided counts first-match case selections before Authority admission; a selected decision may still be refused. Law-consistent means that every state law holds on the pre-state, not that it is reachable.";

#[derive(Clone, Debug, Default)]
pub(super) struct Coverage {
    pub(super) packet: Value,
    pub(super) findings: Vec<Value>,
    pub(super) lines: Vec<String>,
}

impl Coverage {
    pub(super) fn not_checked(reason: &str) -> Self {
        Self {
            packet: json!({"status": "not checked", "reason": reason, "cases": null}),
            ..Self::default()
        }
    }
}

/// Owned, invocation-local tallies; no input or policy reference escapes.
#[derive(Default)]
struct Count {
    guard_true: usize,
    decided: usize,
    law_consistent: usize,
    first: Option<(usize, Option<u32>)>,
    covered_by: BTreeMap<usize, usize>,
}

/// The compiler emits output 0 first, as nested selections. Its prefix
/// contains every guard, including unselected ones, and no assignments.
/// Reuse that exact graph and the library's eager interpreter. Do not infer
/// guard truth from a refused decision or add an expression interpreter.
struct Guards {
    inputs: Vec<Domain<'static>>,
    nodes: Vec<Op>,
    roots: Vec<u16>,
    outputs: Vec<Domain<'static>>,
}

impl Guards {
    fn new(contract: &Contract<'_>, cases: usize) -> Result<Self, ContractError> {
        use super::super::graph::ScalarOp;
        let fail = || ContractError::new("case coverage", "selection graph has no case structure");
        let root = *contract.program_roots.first().ok_or_else(fail)?;
        let small = |n| u16::try_from(n).map_err(|_| fail());
        let mut at = root;
        let mut roots = vec![small(root)?];
        for index in 0..cases.saturating_sub(1) {
            let Some(ScalarOp::Select(guard, yes, no)) = contract.nodes.get(at) else {
                return Err(fail());
            };
            if !matches!(contract.nodes.get(*yes), Some(ScalarOp::Int(n)) if usize::try_from(*n) == Ok(index))
            {
                return Err(fail());
            }
            roots.push(small(*guard)?);
            at = *no;
        }
        if !matches!(contract.nodes.get(at), Some(ScalarOp::Int(n)) if usize::try_from(*n) == Ok(cases - 1))
        {
            return Err(fail());
        }
        let nodes = contract.nodes[..=root]
            .iter()
            .map(policy::numeric_op)
            .collect::<Result<_, _>>()?;
        let mut outputs = vec![Domain::Bool; roots.len()];
        outputs[0] = Domain::I128 {
            min: 0,
            max: (cases - 1) as i128,
        };
        Ok(Self {
            inputs: contract
                .input_domains
                .iter()
                .map(policy::numeric_domain)
                .collect(),
            nodes,
            roots,
            outputs,
        })
    }

    fn evaluate(&self, tuple: &[Atom<'static>]) -> Result<(usize, Vec<bool>), String> {
        let program = Program {
            inputs: &self.inputs,
            outputs: &self.outputs,
            nodes: &self.nodes,
            roots: &self.roots,
        };
        let (result, _) = execute_numeric_v2(
            program,
            tuple,
            v2_zero_limits().with_limit(Resource::Step, self.nodes.len() as u64),
        )
        .into_parts();
        let output = result.map_err(|e| format!("guard evaluation refused: {e:?}"))?;
        let selected = match output.first() {
            Some(Atom::I128(n)) => usize::try_from(*n).map_err(|e| e.to_string())?,
            _ => return Err("selection is not an index".into()),
        };
        let mut holds = output[1..]
            .iter()
            .map(|value| match value {
                Atom::Bool(v) => Ok(*v),
                _ => Err("guard is not Boolean".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        holds.push(true); // The compiler requires a literal final catch-all.
        if holds.iter().position(|v| *v) != Some(selected) {
            return Err("guards disagree with generated selection".into());
        }
        Ok((selected, holds))
    }
}

/// Enumerate only after the caller has established complete domain coverage.
/// `tuple` and `state_law` borrow caller-owned immutable inputs and return
/// owned data. Refusal makes the entire coverage unchecked, never dead.
pub(super) fn measure(
    contract: &Contract<'_>,
    rules: &Rules,
    full: bool,
    tuples: usize,
    tuple: impl Fn(usize) -> Vec<Atom<'static>>,
    state_law: impl Fn(usize) -> Option<u32>,
) -> Coverage {
    if !full {
        return Coverage::not_checked("domain not fully enumerated");
    }
    // A receipt may have changed the graph's structure. Reconstruct the
    // source selection for diagnostics; binding already replays its exact
    // program equivalence receipt, including the selected index output.
    let original;
    let selection = if rules.adoptions.is_empty() {
        contract
    } else {
        let mut source_rules = rules.clone();
        source_rules.adoptions.clear();
        source_rules.evolutions.clear();
        match Contract::build(contract.declarations, &source_rules, contract.commitment) {
            Ok(source) => {
                original = source;
                &original
            }
            Err(e) => return Coverage::not_checked(e.reason()),
        }
    };
    let guards = match Guards::new(selection, rules.cases.len()) {
        Ok(guards) => guards,
        Err(e) => return Coverage::not_checked(e.reason()),
    };
    let mut counts: Vec<Count> = (0..rules.cases.len()).map(|_| Count::default()).collect();
    for ordinal in 0..tuples {
        let (selected, holds) = match guards.evaluate(&tuple(ordinal)) {
            Ok(values) => values,
            Err(reason) => {
                let mut result = Coverage::not_checked(&reason);
                result.packet["first"] = witness(ordinal, &tuple(ordinal));
                return result;
            }
        };
        let unsatisfied = state_law(ordinal);
        let chosen = &mut counts[selected];
        chosen.decided += 1;
        chosen.law_consistent += usize::from(unsatisfied.is_none());
        chosen.first.get_or_insert((ordinal, unsatisfied));
        for (index, (count, holds)) in counts.iter_mut().zip(holds).enumerate() {
            if holds {
                count.guard_true += 1;
                if selected < index {
                    count.covered_by.entry(selected).or_insert(ordinal);
                }
            }
        }
    }
    let mut result = Coverage::default();
    let rows: Vec<_> = counts.iter().enumerate().map(|(index, count)| {
        let case = &rules.cases[index];
        let when = mutants::render(&case.when);
        let covered_by: Vec<_> = count.covered_by.iter().map(|(earlier, ordinal)| json!({
            "case": earlier,
            "when": mutants::render(&rules.cases[*earlier].when),
            "witness": witness(*ordinal, &tuple(*ordinal)),
        })).collect();
        let mut row = json!({
            "case": index, "rule": case.rule, "when": when,
            "guard_true": count.guard_true, "decided": count.decided,
            "law_consistent": count.law_consistent,
        });
        if count.decided == 0 {
            let cause = if count.guard_true == 0 { "guard never true" } else { "covered by earlier cases" };
            let covers = count.covered_by.iter().map(|(earlier, ordinal)| {
                format!("cases[{earlier}] `{}` at {}", mutants::render(&rules.cases[*earlier].when), words(&tuple(*ordinal)).join(" "))
            }).collect::<Vec<_>>().join("; ");
            let text = format!("dead case: cases[{index}] `{when}` decides no tuple: {cause}{}", if covers.is_empty() { String::new() } else { format!("; {covers}") });
            row["cause"] = json!(cause);
            row["covered_by"] = json!(covered_by);
            let mut finding = row.clone();
            finding["kind"] = json!("case-never-decides");
            finding["text"] = json!(text);
            result.lines.push(text);
            result.findings.push(finding);
        } else if count.law_consistent == 0 {
            let (ordinal, law) = count.first.unwrap_or_else(|| unreachable!("positive count has a first tuple"));
            let text = format!("excluded-only case: cases[{index}] `{when}` decides {} tuples, all on pre-states the state laws exclude; first {} (state law {})", count.decided, words(&tuple(ordinal)).join(" "), law.map_or_else(|| "unknown".to_owned(), |id| id.to_string()));
            let mut finding = row.clone();
            finding["kind"] = json!("case-only-on-excluded-pre-states");
            finding["first"] = witness(ordinal, &tuple(ordinal));
            finding["first"]["unsatisfied_state_law"] = json!(law);
            finding["text"] = json!(text);
            result.lines.push(text);
            result.findings.push(finding);
        }
        row
    }).collect();
    result.packet = json!({"status": "checked", "scope": "full-domain", "meaning": MEANING, "tuples": tuples, "cases": rows});
    result
}

fn words(tuple: &[Atom<'_>]) -> Vec<String> {
    tuple
        .iter()
        .map(|v| match v {
            Atom::Bool(v) => u8::from(*v).to_string(),
            Atom::I128(v) => v.to_string(),
            Atom::U128(v) => v.to_string(),
            _ => unreachable!("numeric inputs have numeric atoms"),
        })
        .collect()
}

fn witness(ordinal: usize, tuple: &[Atom<'_>]) -> Value {
    json!({"ordinal": ordinal, "input": words(tuple)})
}
