//! Typed scenario replay uses the existing numeric parser, frames and Authority.
use super::super::scenario::{self, Replayed, Scenario};
use super::*;

type Run = Scenario<examples::Line>;

fn starts_at_genesis(contract: &Contract<'_>, rules: &Rules, state: &[Atom<'_>]) -> bool {
    contract.bindings.iter().zip(state).all(|((_, id), value)| {
        match (id.and_then(|id| rules.genesis.get(&id)), value) {
            (Some(Constant::Bool(a)), Atom::Bool(b)) => a == b,
            (Some(Constant::Int(a)), Atom::I128(b)) => a == b,
            (Some(Constant::Int(a)), Atom::Sum { variant, .. }) => *a == i128::from(*variant),
            (Some(Constant::U128(a)), Atom::U128(b)) => a == b,
            _ => false,
        }
    })
}

fn run(
    contract: &Contract<'_>,
    authority: &NumericAuthority<'_>,
    scenario: &Run,
    width: usize,
    rules: &Rules,
) -> Result<Option<Value>, ContractError> {
    let first = &scenario.steps[0];
    let mut state = first.inputs[..width].to_vec();
    let failure = |index: usize, reason: &str, outcome: Option<Value>| {
        Some(json!({"step":index+1,"line":scenario.steps[index].number,
            "reason":reason,"outcome":outcome}))
    };
    if starts_at_genesis(contract, rules, &state) {
        let frames = frame(contract, &first.inputs)?;
        if let Err(refusal) = zeno_fcis_contract::internals::genesis::admit(authority, &frames[0]) {
            return Ok(failure(0, &refusal.reason, None));
        }
    }
    for (index, step) in scenario.steps.iter().enumerate() {
        if step.inputs[..width] != state {
            return Ok(failure(
                index,
                "the step's pre-state is not the replayed state",
                None,
            ));
        }
        let outcome = evaluate(contract, authority, &step.inputs)?;
        if !example_agrees(step, &outcome, width) {
            return Ok(failure(
                index,
                "decision differs from the numeric example",
                Some(scenario::outcome_json(&outcome)),
            ));
        }
        if let Outcome::Decision(decision) = outcome
            && decision.class != Class::Reject
        {
            // Agreement checked the complete Authority successor and deliveries.
            state.clone_from(&step.post);
        }
    }
    Ok(None)
}

fn replay_rules(
    declarations: &Declarations,
    schema: &[u8],
    rules: &Rules,
    runs: &[Run],
    width: usize,
) -> Result<Vec<Option<Value>>, ContractError> {
    let contract = Contract::build(declarations, rules, schema_commitment(schema)?)?;
    policy::with_numeric_authority(&contract, schema, |authority| {
        runs.iter()
            .map(|scenario| run(&contract, authority, scenario, width, rules))
            .collect()
    })?
}

pub(in super::super) fn replay(
    project: &str,
    text_rules: &str,
    text: &str,
    label: &str,
) -> Result<Replayed, ContractError> {
    let rules = Rules::read_for_project(project, text_rules)?;
    let declarations = Declarations::read(project, &rules.leaf_bindings)?;
    let schema = schema::encode(&declarations)?;
    let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
    let width = contract
        .bindings
        .iter()
        .take_while(|(s, _)| *s == Source::State)
        .count();
    let (runs, results) = policy::with_numeric_authority(&contract, &schema, |authority| {
        let parsed = examples::parse(text, authority.descriptor()).map_err(|error| {
            ContractError::new(format!("{label} line {}", error.line), error.reason)
        })?;
        let runs = scenario::group(text, parsed, label, |step| step.number)?;
        let results = runs
            .iter()
            .map(|scenario| run(&contract, authority, scenario, width, &rules))
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, ContractError>((runs, results))
    })??;
    let failed = results.iter().filter(|failure| failure.is_some()).count();
    let list = runs.iter().zip(&results).map(|(scenario, failure)| {
        let mut entry = json!({"name":scenario.name,"line":scenario.line,"steps":scenario.steps.len(),
            "start":if starts_at_genesis(&contract,&rules,&scenario.steps[0].inputs[..width]) {"genesis"} else {"stated"},
            "verdict":if failure.is_some() {"failed"} else {"passed"}});
        if let Some(failure) = failure { entry["failure"]=failure.clone(); }
        entry
    }).collect::<Vec<_>>();
    let mut packet = json!({"scenarios":list,"count":runs.len(),"failed_on_contract":failed});
    if failed > 0 {
        let withheld = json!({"status":"withheld", "reason":"a scenario fails on the contract; kills are credited only when every scenario passes on it"});
        packet["mutants"] = withheld.clone();
        packet["reject_everything"] = withheld;
        return Ok(Replayed { packet, failed });
    }
    let catalog = mutants::extended_catalog(&rules, &declarations);
    let (mut killed, mut refused) = (0, 0);
    let mut survived = Vec::new();
    let mut mutations = Vec::new();
    for mutant in &catalog {
        let entry = if Contract::build(&declarations, &mutant.rules, contract.commitment).is_err() {
            refused += 1;
            json!({"id":mutant.id,"classification":"refused-by-generator"})
        } else {
            match replay_rules(&declarations, &schema, &mutant.rules, &runs, width) {
                Err(_) => {
                    refused += 1;
                    json!({"id":mutant.id,"classification":"refused-by-library"})
                }
                Ok(failures) => match failures.iter().position(Option::is_some) {
                    Some(index) => {
                        killed += 1;
                        json!({"id":mutant.id,"classification":"killed","scenario":runs[index].name,
                            "step":failures[index].as_ref().map(|failure| failure["step"].clone())})
                    }
                    None => {
                        survived.push(mutant.id.clone());
                        json!({"id":mutant.id,"classification":"survived"})
                    }
                },
            }
        };
        mutations.push(entry);
    }
    packet["mutants"] = json!({"catalog":mutants::CATALOG_V2,"count":catalog.len(),"killed":killed,
        "refused":refused,"survived":survived,"list":mutations,"operators":catalog.iter().map(|mutant|
        (mutant.id.clone(),json!([mutant.operator,mutant.site,mutant.change]))).collect::<serde_json::Map<_,_>>()});
    packet["reject_everything"] = match super::super::machine::reject_everything(&rules) {
        None => json!({"status":"not-applicable","reason":"no case rejects"}),
        Some((core, reason)) => match replay_rules(&declarations, &schema, &core, &runs, width) {
            Err(error) => {
                json!({"status":"not-bound","reject_reason":reason,"place":error.place(),"reason":error.reason()})
            }
            Ok(failures) => {
                let killer = failures.iter().position(Option::is_some);
                json!({"status":"bound","reject_reason":reason,"killed":killer.is_some(),
                    "first_failing_scenario":killer.map(|index| runs[index].name.clone())})
            }
        },
    };
    Ok(Replayed { packet, failed })
}
