#![allow(clippy::unwrap_used)]
use super::*;

const COUNTER: &[u8] = include_bytes!("../../templates/durable-counter/synthesis.json");
fn selected() -> (problem::Problem, Vec<Case>) {
    let problem = problem::parse(COUNTER.to_vec()).unwrap();
    let Outcome::Selected { cases, .. } =
        synthesize(&problem.contract, &problem.sketch, Budget::default()).unwrap()
    else {
        panic!("counter must be expressible")
    };
    (problem, cases)
}
#[test]
fn counter_satisfies_independent_full_decision_table_and_mutations() {
    let (problem, cases) = selected();
    assert_eq!(cases.len(), 64);
    for case in cases {
        let [count, failures, command, allowed]: [i64; 4] = case.input.clone().try_into().unwrap();
        let selected = if command == 0 { count } else { failures };
        let expected = if allowed == 0 {
            vec![0, count, failures, 0, 0, 0]
        } else if selected == 3 {
            vec![1, count, failures, 0, 0, 0]
        } else if command == 0 {
            vec![2, count + 1, failures, 1, count + 1, failures]
        } else {
            vec![3, count, failures + 1, 1, count, failures + 1]
        };
        assert_eq!(case.output, expected);
        for index in 0..expected.len() {
            let mut mutant = expected.clone();
            mutant[index] = if mutant[index] == 0 { 1 } else { 0 };
            assert!(!problem.contract.holds(&case.input, &mutant).unwrap());
        }
    }
}
#[test]
fn source_admission_rejects_duplicates_unknown_fields_and_noninteger_atoms() {
    let original = String::from_utf8(COUNTER.to_vec()).unwrap();
    for malformed in [
        original.replacen("{", "{\"extra\":0,", 1),
        original.replacen("{", "{\"schema\":\"duplicate\",", 1),
    ] {
        assert!(problem::parse(malformed.into_bytes()).is_err());
    }
    let mut source: Value = serde_json::from_slice(COUNTER).unwrap();
    source["sketch"]["nodes"][0] = json!(["input", true]);
    assert!(problem::parse(serde_json::to_vec(&source).unwrap()).is_err());
    source["sketch"]["nodes"][0] = json!(["read_file", "secret"]);
    assert!(problem::parse(serde_json::to_vec(&source).unwrap()).is_err());
    source["profile"] = json!("temporal/1");
    assert!(problem::parse(serde_json::to_vec(&source).unwrap()).is_err());
}
#[test]
fn runner_verdicts_require_exact_outputs_and_complete_coverage() {
    let (problem, cases) = selected();
    let wire = cases
        .iter()
        .map(|c| {
            format!(
                "{}\n",
                c.output
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<String>();
    assert!(runner::validate(&problem.contract, &cases, wire.as_bytes()).is_ok());
    assert!(runner::validate(&problem.contract, &cases, b"passed\n").is_err());
    assert!(
        runner::validate(
            &problem.contract,
            &cases[..63],
            wire.lines()
                .take(63)
                .map(|line| format!("{line}\n"))
                .collect::<String>()
                .as_bytes()
        )
        .is_err()
    );
    let mut changed = cases.clone();
    changed[0].output[0] = 2;
    let bad = changed
        .iter()
        .map(|c| {
            format!(
                "{}\n",
                c.output
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        })
        .collect::<String>();
    assert!(runner::validate(&problem.contract, &changed, bad.as_bytes()).is_err());
}
#[test]
fn discovery_has_distinct_language_adapters_and_missing_tools_stay_unknown() {
    assert_eq!(target("rust").unwrap().emitter.target().language, "rust");
    let python = target("python").unwrap();
    assert_eq!(python.emitter.target().language, "python");
    let javascript = target("javascript").unwrap();
    assert_eq!(javascript.emitter.target().extension, "mjs");
    // Discovery must state the exact call and harness, never a bare language.
    assert!(javascript.emitter.abi().contains("primitive string"));
    assert!(javascript.runner.tool_requirement().contains("Node.js 22"));
    assert!(javascript.runner.invocation().contains("node fixture.mjs"));
    assert!(target("unsupported").is_none());
    assert!(target("js").is_none());
    // A registered adapter that kept the uninformative default ABI, or whose
    // published invocation disagreed with its own artifact, would force an
    // agent to guess the harness from the language name.
    for registered in targets() {
        let id = registered.emitter.target();
        let harness = format!("fixture.{}", id.extension);
        let stated = !registered.emitter.abi().starts_with("unspecified")
            && registered.runner.invocation().contains(&harness);
        assert!(stated, "{} publishes no usable harness", id.language);
    }
    for adapter in [python, javascript] {
        let language = adapter.emitter.target().language;
        let Err(failure) =
            adapter
                .runner
                .run("", &[], Some(Path::new("/zeno-fcis-missing-interpreter")))
        else {
            panic!("{language} admitted a missing tool")
        };
        assert_eq!(failure.code, "tool-missing");
    }
}
#[cfg(all(target_os = "linux", not(target_env = "uclibc")))]
#[test]
fn javascript_runner_refuses_an_unqualified_runtime_version() {
    use std::os::unix::fs::PermissionsExt;
    let temp = runner::Temp::new().unwrap_or_else(|error| panic!("{}", error.message));
    let stand_in = temp.path().join("node");
    fs::write(&stand_in, "#!/bin/sh\necho v18.20.8\n").unwrap();
    fs::set_permissions(&stand_in, fs::Permissions::from_mode(0o700)).unwrap();
    // An interpreter that exits successfully while reporting an unqualified
    // version cannot produce evidence, and never reaches the emitted source.
    let Err(failure) = target("javascript")
        .unwrap()
        .runner
        .run("", &[], Some(&stand_in))
    else {
        panic!("unqualified runtime was admitted")
    };
    assert_eq!(failure.code, "tool-version");
}
