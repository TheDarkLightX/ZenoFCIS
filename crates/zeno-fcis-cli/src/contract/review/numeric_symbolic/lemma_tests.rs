use super::*;

const PROJECT: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/repair-pre-lemma/project.zeno"
));
const RULES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/repair-pre-lemma/v2/policy.json"
));

/// Count the actual emitted obligation on all six inputs of this regression
/// fixture. This is test evidence, never a counting claim for wide domains.
fn count_inputs(solvers: &crate::symbolic_command::Solvers, script: &Script) -> u64 {
    let mut count = 0;
    for state in 0..=2 {
        for context in 0..=1 {
            let mut fixed = script.clone();
            for (name, value) in [("x0", state), ("x1", 0), ("x2", context)] {
                fixed.assert(Term::eq(Term::Name(name.into()), Term::Int(value)));
            }
            let source = fixed.source();
            let answers = solvers.solve(&Query {
                label: "repair-scope-count",
                source: &source,
                script: &fixed,
            });
            match (&answers.z3, &answers.cvc5) {
                (Answer::Sat(_), Answer::Sat(_)) => count += 1,
                (Answer::Unsat { .. }, Answer::Unsat { .. }) => {}
                _ => panic!("both solvers must agree on every fixture input: {answers:?}"),
            }
        }
    }
    count
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_transition_lemma_repairs_match_exhaustive_counts() {
    let formula = "pre.100.110 >= 1".to_owned();
    let native = match super::super::super::lemma::judge_transitions(
        PROJECT,
        RULES,
        std::slice::from_ref(&formula),
        6,
    )
    .unwrap()
    {
        super::super::super::lemma::Lemmas::Judged(mut results) => results.remove(0),
        other => panic!("full native fixture must run: {other:?}"),
    };
    assert_eq!(native["verdict"], "false");
    assert_eq!(native["tuples"], 6);
    assert_eq!(native["admitted"], 6);
    assert_eq!(native["failed"], 2);
    assert_eq!(native["checked"], 4);
    assert_eq!(native["undefined"], 0);
    let solvers = pinned();
    let mut queries = BTreeMap::new();
    let symbolic = lemma::check(PROJECT, RULES, &[formula], true, &mut |q| {
        if q.label == "lemma-0-nonvacuity" || q.label == "lemma-0-case-1" {
            queries.insert(q.label.to_owned(), q.script.clone());
        }
        solvers.solve(q)
    })
    .unwrap();
    assert_eq!(symbolic["status"], "refuted", "{symbolic}");
    assert_eq!(
        symbolic["lemmas"][0]["obligations"][0]["counterexample"]["values"][0],
        json!({"kind":"i128", "value":"0"})
    );
    let admitted = count_inputs(&solvers, &queries["lemma-0-nonvacuity"]);
    let failed = count_inputs(&solvers, &queries["lemma-0-case-1"]);
    assert_eq!(native["admitted"], admitted, "transition pre-state scope");
    assert_eq!(native["failed"], failed, "transition failure count");
    assert_eq!(native["checked"], admitted - failed);
    eprintln!(
        "repair-pre-lemma: native and dual-solver queries admit {admitted}, refute {failed}, check {}",
        admitted - failed
    );
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_transition_nonvacuity_includes_only_unlawful_repairs() {
    let mut rules: Json = serde_json::from_str(RULES).unwrap();
    rules["cases"][0]["when"] = json!("pre.100.110 >= 1");
    let rules = rules.to_string();
    let solvers = pinned();
    let mut scope = None;
    let result = lemma::check(PROJECT, &rules, &["true".into()], true, &mut |q| {
        if q.label == "lemma-0-nonvacuity" {
            scope = Some(q.script.clone());
        }
        solvers.solve(q)
    })
    .unwrap();
    assert_eq!(result["status"], "solver-checked", "{result}");
    assert_eq!(count_inputs(&solvers, &scope.unwrap()), 2);
    assert_eq!(
        result["lemmas"][0]["nonvacuity"]["counterexample"]["values"][0],
        json!({"kind":"i128", "value":"0"})
    );
}

#[test]
#[ignore = "requires both qualification solver binaries"]
fn pinned_state_induction_keeps_lawful_and_lemma_premises() {
    let mut rules: Json = serde_json::from_str(RULES).unwrap();
    let mut repair = rules["cases"][1].clone();
    repair["when"] = json!("pre.100.110 == 0");
    repair["post"]["110"] = json!("2");
    rules["cases"][1]["post"]["110"] = json!("pre.100.110");
    rules["cases"].as_array_mut().unwrap().insert(1, repair);
    let solvers = pinned();
    // pre=0 satisfies the lemma but is unlawful; pre=2 is lawful but does
    // not satisfy the lemma. Neither belongs to this induction premise.
    let result = lemma::check(
        PROJECT,
        &rules.to_string(),
        &["post.100.110 <= 1".into()],
        false,
        &mut |q| solvers.solve(q),
    )
    .unwrap();
    assert_eq!(result["status"], "solver-checked", "{result}");
}
