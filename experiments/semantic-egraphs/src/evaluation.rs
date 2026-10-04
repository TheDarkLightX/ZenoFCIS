use crate::boolean::{Boolean, admit_total_boolean, direct, lower, parse_outputs};
use egg::RecExpr;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

pub fn observation(program: &Program, input: &[i64]) -> Value {
    match program.evaluate(input) {
        Ok(output) => json!({"ok":output}),
        Err(error) => json!({"error":format!("{error:?}")}),
    }
}

pub fn valid_inputs(nvars: usize) -> Vec<Vec<i64>> {
    (0..1usize << nvars)
        .map(|mask| (0..nvars).map(|i| ((mask >> i) & 1) as i64).collect())
        .collect()
}

pub fn invalid_inputs(nvars: usize) -> Vec<Vec<i64>> {
    let mut inputs = BTreeSet::new();
    if nvars > 0 {
        inputs.insert(vec![0; nvars - 1]);
    }
    inputs.insert(vec![0; nvars + 1]);
    for index in 0..nvars {
        for value in [-1, 2, i64::MIN, i64::MAX] {
            let mut input = vec![0; nvars];
            input[index] = value;
            inputs.insert(input);
        }
    }
    inputs.into_iter().collect()
}

pub fn compare(
    source: &Program,
    candidate: &Program,
    nvars: usize,
    original: Option<&[RecExpr<Boolean>]>,
    candidate_exprs: Option<&[RecExpr<Boolean>]>,
    expected: Option<&[String]>,
) -> Value {
    if nvars > 6 || source.inputs().len() != nvars || candidate.inputs().len() != nvars {
        return json!({"gate_passed":false,"valid_tuples":0,"invalid_tuples":0,"truth_signatures":[],"checks":[],
                      "mismatches":[{"kind":"declared_domain_guard","nvars":nvars,"source_inputs":source.inputs().len(),"candidate_inputs":candidate.inputs().len()}]});
    }
    let mut mismatches = Vec::new();
    let mut checks = Vec::new();
    let inputs = valid_inputs(nvars);
    let mut signatures = vec![String::new(); candidate.outputs().len()];
    if source.inputs() != candidate.inputs() || source.outputs() != candidate.outputs() {
        mismatches.push(json!({"kind":"interface_changed"}));
    }
    for input in &inputs {
        let left = observation(source, input);
        let right = observation(candidate, input);
        if left != right {
            mismatches.push(
                json!({"kind":"evaluation_mismatch","input":input,"source":left,"candidate":right}),
            );
        }
        if let Some(exprs) = original {
            match exprs.iter().map(|e| direct(e,input).map(i64::from)).collect::<Result<Vec<_>,_>>() {
                Ok(values) if left == json!({"ok":values}) => (),
                result => mismatches.push(json!({"kind":"original_direct_interpreter_mismatch","input":input,"direct":format!("{result:?}"),"source":left})),
            }
        }
        if let Some(exprs) = candidate_exprs {
            match exprs.iter().map(|e| direct(e,input).map(i64::from)).collect::<Result<Vec<_>,_>>() {
                Ok(values) if right == json!({"ok":values}) => (),
                result => mismatches.push(json!({"kind":"candidate_direct_interpreter_mismatch","input":input,"direct":format!("{result:?}"),"candidate":right})),
            }
        }
        if let Ok(values) = candidate.evaluate(input) {
            for (signature, value) in signatures.iter_mut().zip(values) {
                signature.push(if value == 1 { '1' } else { '0' });
            }
        }
        checks.push(json!({"input":input,"source":left,"candidate":right,"admitted":true}));
    }
    if let Some(expected) = expected {
        if signatures != expected {
            mismatches.push(json!({"kind":"independent_corpus_truth_mismatch","expected":expected,"actual":signatures}));
        }
    }
    let invalid = invalid_inputs(nvars);
    for input in &invalid {
        let left = observation(source, input);
        let right = observation(candidate, input);
        let rejection = json!({"error":"Invalid(\"input-domain\")"});
        if left != rejection || right != rejection {
            mismatches.push(json!({"kind":"input_rejection_mismatch","input":input,"source":left,"candidate":right}));
        }
        checks.push(json!({"input":input,"source":left,"candidate":right,"admitted":false}));
    }
    json!({"gate_passed":mismatches.is_empty(),"valid_tuples":inputs.len(),"invalid_tuples":invalid.len(),
           "truth_signatures":signatures,"checks":checks,"mismatches":mismatches,
           "invalid_probe_scope":"Both wrong-length boundaries and each position at -1,2,i64::MIN,i64::MAX; not enumeration of infinite invalid tuples."})
}

fn control(
    id: &str,
    source: Program,
    candidate: Program,
    nvars: usize,
    require_scope: bool,
) -> Value {
    let evaluation = compare(&source, &candidate, nvars, None, None, None);
    let rejected = !evaluation["gate_passed"].as_bool().unwrap_or(false);
    let scope = admit_total_boolean(&source).err();
    json!({"id":id,"expected_outcome":"rejected","passed":rejected&&(!require_scope||scope.is_some()),
           "scope_rejected":scope.is_some(),"scope_error":scope,"evaluation_rejected":rejected,
           "checks":evaluation["checks"],"mismatches":evaluation["mismatches"]})
}

pub fn controls() -> Result<Vec<Value>, String> {
    let boolean = |text: &str, nvars: usize| lower(&parse_outputs(&[text.into()], nvars)?, nvars);
    let mut result = vec![control(
        "altered_truth_function",
        boolean("v0", 1)?,
        boolean("(not v0)", 1)?,
        1,
        false,
    )];
    let make = |inputs, outputs, nodes, roots| {
        Program::try_new(inputs, outputs, nodes, roots)
            .map_err(|e| format!("Control construction: {e:?}"))
    };
    let integer = Domain::Int { min: 0, max: 1 };
    let non_boolean = make(vec![integer], vec![integer], vec![Op::Input(0)], vec![0])?;
    let bool_identity = boolean("v0", 1)?;
    result.push(control(
        "non_boolean_region",
        non_boolean,
        bool_identity.clone(),
        1,
        true,
    ));
    let eager_dead = make(
        vec![],
        vec![Domain::Bool],
        vec![Op::Int(i64::MAX), Op::Int(1), Op::Add(0, 1), Op::Bool(true)],
        vec![3],
    )?;
    result.push(control(
        "eager_dead_add_overflow",
        eager_dead,
        boolean("true", 0)?,
        0,
        true,
    ));
    // At c=1 the selected value is safe, but the unused arm's Add still traps
    // because FCIS evaluates every instruction eagerly before Select.
    let eager_arm = make(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(1, 2),
            Op::Int(0),
            Op::Select(0, 4, 3),
            Op::Eq(5, 4),
        ],
        vec![6],
    )?;
    result.push(control(
        "eager_select_arm_overflow",
        eager_arm,
        boolean("v0", 1)?,
        1,
        true,
    ));
    for (id, input) in [
        ("invalid_input_shape", vec![0, 0]),
        ("invalid_input_domain", vec![2]),
    ] {
        let observed = observation(&bool_identity, &input);
        result.push(json!({"id":id,"expected_outcome":"input_rejected","passed":observed==json!({"error":"Invalid(\"input-domain\")"}),
                           "checks":[{"input":input,"source":observed,"candidate":observed}],"mismatches":[]}));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_negative_control_rejects_the_unsafe_change() {
        for control in controls().expect("well formed controls") {
            assert_eq!(control["passed"], true, "{control}");
        }
    }
    #[test]
    fn lowered_or_matches_independent_boolean_evaluation() {
        let outputs = vec!["(or v0 v1)".to_string(), "(ite v0 (not v1) v1)".to_string()];
        let expressions = parse_outputs(&outputs, 2).unwrap();
        let program = lower(&expressions, 2).unwrap();
        let check = compare(
            &program,
            &program,
            2,
            Some(&expressions),
            Some(&expressions),
            None,
        );
        assert_eq!(check["gate_passed"], true);
        assert_eq!(check["valid_tuples"], 4);
        assert_eq!(check["invalid_tuples"], 10);
    }
}
