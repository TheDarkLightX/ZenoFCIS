//! Native usage model-to-runtime checks; no external private fixtures.
use super::*;
pub(super) fn validations(source: &str) -> Vec<Validation> {
    let mut w = Work {
        used: 0,
        limits: Limits::default(),
        rows: 0,
        states: 0,
    };
    let mut p = Parser {
        tokens: tokenize(source, &mut w).unwrap_or_else(|s| panic!("{s:?}")),
        i: 0,
    };
    let (facts, _) = p.program(&mut w).unwrap_or_else(|s| panic!("{s:?}"));
    let get = |t: &Term| match t {
        Term::Const(Value::Text(s)) => s.clone(),
        _ => panic!("text"),
    };
    facts
        .iter()
        .filter(|a| a.pred == "record")
        .map(|a| {
            let id = get(&a.args[0]);
            let e = facts
                .iter()
                .find(|e| e.pred == "evidence" && e.args[0] == a.args[0])
                .unwrap_or_else(|| panic!("evidence"));
            Validation {
                statement: lesson_statement(source, &id, Limits::default())
                    .unwrap_or_else(|s| panic!("{s:?}"))
                    .0,
                lesson: id,
                recipe_sha256: get(&e.args[2]),
                dependency_sha256: get(&a.args[3]),
                validation_sha256: "0".repeat(64),
                mode: get(&a.args[4]),
                family: get(&a.args[5]),
                last_validation: 1,
                status: EvidenceStatus::Validated,
            }
        })
        .collect()
}
fn checked(source: &str, request: Request) -> Report {
    run_with_evidence(&[source], &request, Limits::default(), &validations(source))
        .unwrap_or_else(|s| panic!("{s:?}"))
}
fn advice(task: &str) -> Advice {
    let Outcome::Advised(a) = checked(EXAMPLE, Request::Advise(task.into())).outcome else {
        panic!("advice");
    };
    a
}
#[test]
fn kb_usage_three_exact_tasks_unknowns_inheritance_and_traces() {
    for (task, expected) in [
        ("task(subject,check-program).", vec![text("check-inputs")]),
        (
            "task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).",
            vec![text("containment"), text("diagnostic-order")],
        ),
        (
            "task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4097).",
            vec![text("learner-bound")],
        ),
    ] {
        let a = advice(task);
        assert_eq!(a.lessons, expected, "advice-exact-control");
        for e in a.explanations {
            assert!(!e.trace.is_empty(), "provenance-control");
            let mut seen = BTreeSet::new();
            for d in &e.trace {
                assert!(d.premises.iter().all(|p| seen.contains(p)));
                seen.insert(d.fact.clone());
            }
            assert!(seen.contains(&e.conclusion));
        }
    }
    let a = advice("task(subject,learn-terms).");
    assert!(a.lessons.is_empty(), "scope-unknown-control");
    assert!(!a.unknowns.is_empty());
    assert!(a.unknowns.iter().all(|f| {
        a.explanations
            .iter()
            .any(|e| &e.conclusion == f && !e.trace.is_empty())
    }));
    assert!(advice("task(subject,learn-terms).task(tool,zeno-fcis).task(version,v240).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).").lessons.is_empty());
}
#[test]
fn kb_usage_conflict_and_missing_evidence_controls() {
    let source = EXAMPLE.to_owned()
        + "lesson(planted,check-program).strength(planted,observed).effect(planted,hurts).evidence(planted,r,\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\").record(planted,promoted,usage-v1,\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",observation,f).";
    let Outcome::Checked(p) = checked(&source, Request::Check).outcome else {
        panic!("check");
    };
    assert_eq!(p, vec!["kb-unresolved-conflict"], "conflict-control");
    let missing = EXAMPLE.to_owned()
        + "lesson(unevidenced,sample).strength(unevidenced,observed).effect(unevidenced,neutral).record(unevidenced,proposed,usage-v1,\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",observation,f).";
    let Outcome::Checked(p) = run_with_evidence(
        &[&missing],
        &Request::Check,
        Limits::default(),
        &validations(EXAMPLE),
    )
    .unwrap_or_else(|s| panic!("{s:?}"))
    .outcome
    else {
        panic!("check");
    };
    assert_eq!(p, vec!["kb-lesson-without-evidence"], "evidence-control");
    let source = EXAMPLE.to_owned() + "isa(learner,learn-terms).";
    let Outcome::Checked(p) = run(&source, &Request::Check, Limits::default())
        .unwrap_or_else(|s| panic!("{s:?}"))
        .outcome
    else {
        panic!("check");
    };
    assert!(p.contains(&"kb-isa-cycle"));
    let source = EXAMPLE.to_owned() + "supersedes(missing,check-inputs).";
    let Outcome::Checked(p) = run(&source, &Request::Check, Limits::default())
        .unwrap_or_else(|s| panic!("{s:?}"))
        .outcome
    else {
        panic!("check");
    };
    assert!(p.contains(&"kb-unknown-lesson"));
}
#[test]
fn kb_usage_numeric_boundaries_units_and_stale_records() {
    let r = run(
        "n(1).n(2).p(N):-n(N),N>1.",
        &Request::Query("p(N)".into()),
        Limits::default(),
    )
    .unwrap_or_else(|s| panic!("{s:?}"));
    assert_eq!(
        r.outcome,
        Outcome::Query(vec![Binding::from([("N".into(), Value::Integer(2))])])
    );
    assert!(
        run(
            "n(a).p(N):-n(N),N>1.",
            &Request::Query("p(N)".into()),
            Limits::default()
        )
        .is_err()
    );
    let task = "task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).";
    for (n, excluded) in [(4095, false), (4096, false), (4097, true)] {
        let a = advice(&alloc::format!("{task}task(tuples-count,{n})."));
        assert_eq!(a.exclusions.iter().any(|e| e.enforced), excluded);
    }
    let a = advice(&alloc::format!("{task}task(tuples-bytes,4097)."));
    assert!(a.exclusions.iter().all(|e| !e.enforced));
    let mut inputs = validations(EXAMPLE);
    for v in &mut inputs {
        v.status = EvidenceStatus::Stale;
    }
    let r = run_with_evidence(
        &[EXAMPLE],
        &Request::Advise(task.into()),
        Limits::default(),
        &inputs,
    )
    .unwrap_or_else(|s| panic!("{s:?}"));
    let Outcome::Advised(a) = r.outcome else {
        panic!("advice");
    };
    assert!(a.lessons.is_empty());
}
#[test]
fn kb_usage_planner_skips_blocker_and_replans_after_actual_observation() {
    let task =
        "task(subject,observe).task(tool,zeno-fcis).task(version,v241).task(environment,sandbox).";
    let r = checked(
        EXAMPLE,
        Request::PlanFor {
            have: vec!["model".into(), "observations".into()],
            want: vec!["answer".into()],
            task: task.into(),
        },
    );
    let Outcome::Planned(p) = r.outcome else {
        panic!("plan");
    };
    assert_eq!(
        p.steps.iter().map(|s| s.name()).collect::<Vec<_>>(),
        vec!["explain", "isolate"]
    );
    assert!(
        p.exclusions
            .iter()
            .any(|e| e.step == "observe" && e.enforced)
    );
    for (produced, next) in [(vec![], "explain"), (vec!["diagnoses".into()], "isolate")] {
        let r = checked(
            EXAMPLE,
            Request::Replan {
                have: vec!["model".into(), "observations".into()],
                want: vec!["answer".into()],
                task: task.into(),
                produced,
            },
        );
        let Outcome::Planned(p) = r.outcome else {
            panic!("plan");
        };
        assert_eq!(p.steps[0].name(), next);
    }
}
#[test]
fn kb_usage_layer_conflict_retains_both_assertions_and_is_not_advice() {
    let second = EXAMPLE.replace(
        "limit(learner-bound,tuples,4096,count).",
        "limit(learner-bound,tuples,3,count).",
    );
    let r = run_layered(&[EXAMPLE, &second], &Request::Check, Limits::default())
        .unwrap_or_else(|s| panic!("{s:?}"));
    assert_eq!(r.layer_conflicts, 1);
    assert_eq!(r.overrides.len(), 1);
    assert_ne!(r.overrides[0].earlier, r.overrides[0].later);
    let Outcome::Checked(p) = r.outcome else {
        panic!("check");
    };
    assert!(p.contains(&"kb-layer-conflict"));
}
