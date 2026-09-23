#![allow(clippy::unwrap_used)]
use super::*;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
use zeno_fcis_synthesis::system::{Property, SystemCheck, SystemLimits, check_system_property};

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
    assert_eq!(failure.code, "tool-version", "{}", failure.message);
}

#[test]
fn runner_retries_a_briefly_busy_executable_then_fails_closed() {
    use std::os::unix::fs::PermissionsExt;
    let temp = runner::Temp::new().unwrap_or_else(|error| panic!("{}", error.message));
    let stand_in = temp.path().join("node");
    fs::write(&stand_in, "#!/bin/sh\necho v18.20.8\n").unwrap();
    fs::set_permissions(&stand_in, fs::Permissions::from_mode(0o700)).unwrap();
    let hold = || fs::OpenOptions::new().write(true).open(&stand_in).unwrap();
    let run = || {
        target("javascript")
            .unwrap()
            .runner
            .run("", &[], Some(&stand_in))
    };

    // Released soon, as an inherited descriptor is at its child's exec: the
    // stand-in starts and its unqualified version is refused as usual.
    let writer = hold();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(5));
        drop(writer);
    });
    let Err(failure) = run() else {
        panic!("unqualified runtime was admitted")
    };
    release.join().unwrap();
    assert_eq!(failure.code, "tool-version", "{}", failure.message);

    // Held throughout: the start fails closed.
    let writer = hold();
    let Err(failure) = run() else {
        panic!("busy runtime was admitted")
    };
    drop(writer);
    assert_eq!(failure.code, "tool-start", "{}", failure.message);
}

const COUNTER_PROGRAM: &[u8] =
    include_bytes!("../../templates/durable-counter/synthesized/program.zcve");

/// A property over the counter's four inputs followed by its six outputs.
fn counter_property(problem: &problem::Problem, nodes: Vec<Op>) -> Property {
    let inputs = problem.contract.inputs().to_vec();
    let outputs = problem.contract.outputs().to_vec();
    let root = u16::try_from(nodes.len() - 1).unwrap();
    let relation = Program::try_new(
        [inputs.clone(), outputs.clone()].concat(),
        vec![Domain::Bool],
        nodes,
        vec![root],
    )
    .unwrap();
    Property::try_new(inputs, outputs, relation).unwrap()
}

#[test]
fn counter_system_properties_are_checked_against_the_exact_program() {
    // Inputs: 0 pre.count, 1 pre.failures, 2 command.record_failure,
    // 3 context.allowed. Outputs: 4 decision, 5 post.count, 6 post.failures,
    // 7 outbox.notify, 8 outbox.count, 9 outbox.failures.
    let problem = problem::parse(COUNTER.to_vec()).unwrap();
    let Outcome::Selected { program, .. } =
        synthesize(&problem.contract, &problem.sketch, Budget::default()).unwrap()
    else {
        panic!("counter must be expressible")
    };
    // The obligations bind to the exact shipped program bytes.
    assert_eq!(program.value().canonical_bytes().unwrap(), COUNTER_PROGRAM);
    let limits = SystemLimits::default();
    let check = |property: &Contract| check_system_property(&program, property, limits).unwrap();
    let checked = |property: &Property| check(property.contract());

    // Refinement: the program satisfies the reviewed relation, and the relation
    // is not implied by the output domains alone.
    assert_eq!(
        check(&problem.contract),
        SystemCheck::SystemProperty { inputs: 64 }
    );

    // Positive controls: transition-dependent properties.
    let failures_monotone = counter_property(
        &problem,
        vec![Op::Input(6), Op::Input(1), Op::Lt(0, 1), Op::Not(2)],
    );
    let reject_keeps_state = counter_property(
        &problem,
        vec![
            Op::Input(4),
            Op::Int(2),
            Op::Lt(0, 1),
            Op::Input(5),
            Op::Input(0),
            Op::Eq(3, 4),
            Op::Input(6),
            Op::Input(1),
            Op::Eq(6, 7),
            Op::And(5, 8),
            Op::Not(9),
            Op::And(2, 10),
            Op::Not(11),
        ],
    );
    let notification_matches_state = counter_property(
        &problem,
        vec![
            Op::Input(7),
            Op::Input(8),
            Op::Input(5),
            Op::Eq(1, 2),
            Op::Input(9),
            Op::Input(6),
            Op::Eq(4, 5),
            Op::And(3, 6),
            Op::Not(7),
            Op::And(0, 8),
            Op::Not(9),
        ],
    );
    for property in [
        &failures_monotone,
        &reject_keeps_state,
        &notification_matches_state,
    ] {
        assert_eq!(
            checked(property),
            SystemCheck::SystemProperty { inputs: 64 }
        );
    }

    // Domain-only control: `post.count <= 3` is implied by the output domain,
    // so it cannot satisfy the positive control.
    let count_bounded = counter_property(
        &problem,
        vec![Op::Int(3), Op::Input(5), Op::Lt(0, 1), Op::Not(2)],
    );
    assert_eq!(
        checked(&count_bounded),
        SystemCheck::DomainImplied {
            inputs: 64,
            pairs: 64 * 2048
        }
    );

    // Negative control: raise the capacity guard's bound from 3 to 4. The
    // first admitted input that now leaves the output domain is a failure
    // recorded at pre.failures = 3.
    let nodes = program.nodes();
    let guard = (0..nodes.len())
        .find(|&index| {
            nodes[index] == Op::Int(3)
                && nodes[index + 1..]
                    .iter()
                    .any(|op| matches!(op, Op::Lt(_, bound) if usize::from(*bound) == index))
        })
        .unwrap();
    let mut mutated = nodes.to_vec();
    mutated[guard] = Op::Int(4);
    let buggy = Program::try_new(
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        mutated,
        program.roots().to_vec(),
    )
    .unwrap();
    let first = vec![0, 3, 1, 1];
    assert_eq!(
        check_system_property(&buggy, failures_monotone.contract(), limits).unwrap(),
        SystemCheck::NotTotal {
            input: first.clone()
        }
    );
    assert!(buggy.evaluate(&first).is_err());
    assert!(program.evaluate(&first).is_ok());
}

#[test]
#[ignore = "requires the workflow-pinned CVC5 executable"]
fn pinned_counter_system_smt_agrees_with_exhaustive_check() {
    use std::io::Write as _;
    use std::process::Stdio;
    use zeno_fcis_formal_tools::{parse_system_answer, system_verdict};

    let cvc5 = std::path::PathBuf::from(std::env::var_os("ZENO_FCIS_CVC5").unwrap());
    let problem = problem::parse(COUNTER.to_vec()).unwrap();
    let Outcome::Selected { program, .. } =
        synthesize(&problem.contract, &problem.sketch, Budget::default()).unwrap()
    else {
        panic!("counter must be expressible")
    };
    assert_eq!(program.value().canonical_bytes().unwrap(), COUNTER_PROGRAM);
    let solve = |kind, script: &[u8]| {
        let mut child = std::process::Command::new(&cvc5)
            .args(["--lang", "smt2"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(script).unwrap();
        parse_system_answer(kind, &child.wait_with_output().unwrap().stdout, 4, 6)
    };
    let properties = [
        vec![Op::Input(6), Op::Input(1), Op::Lt(0, 1), Op::Not(2)],
        vec![
            Op::Input(4),
            Op::Int(2),
            Op::Lt(0, 1),
            Op::Input(5),
            Op::Input(0),
            Op::Eq(3, 4),
            Op::Input(6),
            Op::Input(1),
            Op::Eq(6, 7),
            Op::And(5, 8),
            Op::Not(9),
            Op::And(2, 10),
            Op::Not(11),
        ],
        vec![Op::Int(3), Op::Input(5), Op::Lt(0, 1), Op::Not(2)],
    ];
    for nodes in properties {
        let property = counter_property(&problem, nodes);
        let exhaustive =
            check_system_property(&program, property.contract(), SystemLimits::default()).unwrap();
        let verdict = system_verdict(&program, &property, solve).unwrap();
        assert_eq!(verdict.code(), exhaustive.code());
    }
}
