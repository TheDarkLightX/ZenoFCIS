//! Checks of the judges over the whole declared domain: the specification
//! gaps, the state profile, lemmas, scenarios and probes, on core-component
//! instances and on contracts with planted faults. Expected values agree
//! with the separately written reference `tools/contract_judges_reference.py`.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::super::declarations::Declarations;
use super::super::rules::Rules;
use super::gaps::{self, Gaps};
use super::lemma::{self, Lemmas};
use super::mutants;
use super::probe::{self, Answer, Question};
use super::profile::{self, Profile};
use super::scenario;
use super::{DEFAULT_MAX_TUPLES, PACKET_SCHEMA_GAPS, ReviewSources, review_with_gaps};

/// One core-component instance: the family's templates with `parameters`
/// substituted, as `tools/instantiate_core.py` substitutes them.
struct Core {
    project: String,
    rules: String,
}

impl Core {
    fn new(family: &str, parameters: &[(&str, i128)]) -> Self {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../core-components")
            .join(family);
        let instance = parameters
            .iter()
            .map(|(name, value)| format!("{}{value}", name.to_lowercase()))
            .collect::<Vec<_>>()
            .join("-");
        let substitute = |file: &str| {
            let mut text = fs::read_to_string(base.join(file))
                .unwrap_or_else(|error| panic!("read {family}/{file}: {error}"));
            for (name, value) in parameters {
                text = text.replace(&format!("${{{name}}}"), &value.to_string());
            }
            text.replace("${INSTANCE}", &instance)
        };
        Self {
            project: substitute("project.zeno.in"),
            rules: substitute("policy.json.in"),
        }
    }

    fn with_rules(&self, edit: impl FnOnce(&mut Value)) -> Self {
        let mut rules: Value =
            serde_json::from_str(&self.rules).unwrap_or_else(|error| panic!("{error}"));
        edit(&mut rules);
        Self {
            project: self.project.clone(),
            rules: rules.to_string(),
        }
    }

    fn options(proposals: Option<&str>) -> gaps::Options<'_> {
        gaps::Options {
            max_tuples: DEFAULT_MAX_TUPLES,
            max_work: gaps::DEFAULT_MAX_WORK,
            proposals,
            mutants: true,
        }
    }

    fn gaps(&self, proposals: Option<&str>) -> Gaps {
        gaps::judge(&self.project, &self.rules, Self::options(proposals))
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn profile(&self) -> Value {
        match profile::profile(&self.project, &self.rules, DEFAULT_MAX_TUPLES)
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Profile::Measured(value) => value,
            Profile::Inconclusive(inconclusive) => panic!("inconclusive {inconclusive:?}"),
        }
    }

    fn lemmas(&self, formulas: &[&str]) -> Vec<Value> {
        let formulas: Vec<String> = formulas.iter().map(|text| (*text).to_owned()).collect();
        match lemma::judge(&self.project, &self.rules, &formulas, DEFAULT_MAX_TUPLES)
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Lemmas::Judged(verdicts) => verdicts,
            Lemmas::Inconclusive(inconclusive) => panic!("inconclusive {inconclusive:?}"),
        }
    }

    fn probe(&self, question: &Question) -> Value {
        match probe::probe(&self.project, &self.rules, question, DEFAULT_MAX_TUPLES)
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Answer::Answered(value) => value,
            Answer::Inconclusive(inconclusive) => panic!("inconclusive {inconclusive:?}"),
        }
    }
}

fn measured(gaps: &Gaps) -> &Value {
    match gaps {
        Gaps::Measured(value) => value,
        other => panic!("not measured: {}", other.json()),
    }
}

fn count(value: &Value) -> u64 {
    value
        .as_u64()
        .unwrap_or_else(|| panic!("{value} is not a count"))
}

#[test]
fn the_declared_laws_of_a_bounded_counter_determine_no_tuple_and_admit_rejecting_everything() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let gaps = counter.gaps(None);
    let value = measured(&gaps);
    let full = &value["admissible"]["full"];
    assert_eq!(full["tuples"], 12);
    assert_eq!(full["reference_refused"], 0);
    assert_eq!(full["determined"], 0);
    // Every tuple admits all three reject reasons; four admit an accept too.
    assert_eq!(full["freedom"]["reject_reason"], 12);
    assert_eq!(full["freedom"]["class"], 4);
    assert_eq!(full["freedom"]["successor"], 0);
    assert_eq!(value["frame_check"]["mismatches"], 0);
    assert_eq!(value["frame_check"]["reference_decisions_rechecked"], 12);
    for law in ["500", "501", "502"] {
        assert_eq!(value["activation"]["full"][law], 4, "law {law}");
    }
    assert_eq!(value["activation"]["inactive"], json!([]));
    assert_eq!(
        value["reject_everything"]["admitted_where_the_contract_decides"],
        true
    );
    assert_eq!(value["reject_everything"]["reject_reason"], 200);
    assert_eq!(
        value["tables"]["admissible_sha256"],
        "eef4ca197f8167da61ffe23ccf6610324d598f3483913581839881c7eb9aa88a"
    );
    let mutation = &value["mutation"];
    assert_eq!(mutation["catalog"], mutants::CATALOG_V2);
    let full = &mutation["full"];
    assert_eq!(
        count(&full["caught"])
            + count(&full["admitted"])
            + count(&full["equivalent"])
            + count(&mutation["refused_by_generator"])
            + count(&mutation["refused_by_library"]),
        count(&mutation["count"])
    );
    assert!(count(&full["admitted"]) > 0);
    // Every admitted mutant has a witness where its decision differs.
    for mutant in mutation["list"].as_array().into_iter().flatten() {
        if mutant["classification"] == "admitted" {
            assert!(mutant["admitted_witness"].is_object(), "{mutant}");
        }
    }
    assert_eq!(gaps.summary()["full"]["determined"], 0);
}

#[test]
fn a_contract_that_delivers_is_outside_the_gap_judge() {
    let app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval");
    let read = |file: &str| {
        fs::read_to_string(app.join(file)).unwrap_or_else(|error| panic!("{file}: {error}"))
    };
    let gaps = gaps::judge(
        &read("project.zeno"),
        &read("v2/policy.json"),
        Core::options(None),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(gaps.status(), "out-of-scope");
}

#[test]
fn over_a_limit_the_gap_judge_is_inconclusive() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    for (max_tuples, max_work, reason) in [
        (11, gaps::DEFAULT_MAX_WORK, "domain-over-limit"),
        (DEFAULT_MAX_TUPLES, 100, "work-over-limit"),
    ] {
        let options = gaps::Options {
            max_tuples,
            max_work,
            proposals: None,
            mutants: true,
        };
        match gaps::judge(&counter.project, &counter.rules, options)
            .unwrap_or_else(|error| panic!("{error}"))
        {
            Gaps::Inconclusive(inconclusive) => assert_eq!(inconclusive.reason, reason),
            other => panic!("not inconclusive: {}", other.json()),
        }
    }
}

#[test]
fn a_proposed_law_is_checked_against_the_contract_before_it_is_measured() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let alarm = counter.gaps(Some("law 9000 never on any = false;\n"));
    assert_eq!(alarm.status(), "proposal-false-alarm");
    assert_eq!(alarm.json()["false_alarm"]["law"], 9000);
    assert_eq!(alarm.json()["false_alarm"]["input"], json!([0, 150, 0]));
    let held = counter.gaps(Some(
        "law 9000 kept on reject = context.102.130 == 0 || context.102.130 == 1;\nlaw 9001 harmless on any = true;\n",
    ));
    let proposals = &measured(&held)["proposals"];
    assert_eq!(proposals["laws"][0]["id"], 9000);
    assert_eq!(proposals["laws"][0]["applies_on"], 8);
    assert_eq!(proposals["laws"][1]["applies_on"], 12);
    assert_eq!(proposals["inactive"], json!([]));
    assert_eq!(proposals["determined"]["full"], json!([0, 0]));
    for line in [
        "type 9000 int Extra in 0..=1;",
        "law 9000 a on any = true; law 9001 b on any = true;",
        "law 500 again on any = true;",
        "law 9000 commented on any = true; // trailing",
    ] {
        let gaps = gaps::judge(&counter.project, &counter.rules, Core::options(Some(line)))
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(gaps.status(), "invalid-proposals", "{line}");
    }
}

#[test]
fn without_mutants_the_gap_judge_binds_none_and_measures_the_rest_alike() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let proposals = "law 9000 kept on reject = context.102.130 == 0 || context.102.130 == 1;\nlaw 9001 harmless on any = true;\n";
    let full = counter.gaps(Some(proposals));
    let options = gaps::Options {
        mutants: false,
        ..Core::options(Some(proposals))
    };
    let lean = gaps::judge(&counter.project, &counter.rules, options)
        .unwrap_or_else(|error| panic!("{error}"));
    let (full, lean) = (measured(&full), measured(&lean));
    assert!(count(&full["mutation"]["count"]) > 0);
    assert_eq!(full["reject_everything"]["status"], "bound");
    // The mutant and core sections are the only ones that differ, before and
    // after the proposals.
    let mut expected = full.clone();
    for section in ["mutation", "reject_everything"] {
        expected[section] = Value::Null;
        expected["proposals"]["after"][section] = Value::Null;
    }
    for section in ["mutants_admitted", "now_caught"] {
        expected["proposals"][section] = Value::Null;
    }
    assert_eq!(*lean, expected);
}

#[test]
fn proposed_laws_count_against_the_work_limit() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let declared = measured(&counter.gaps(None))["work"]
        .as_str()
        .and_then(|work| work.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("work"));
    let proposals = "law 9000 a on any = true;\nlaw 9001 b on any = true;\n";
    let options = gaps::Options {
        max_tuples: DEFAULT_MAX_TUPLES,
        max_work: declared,
        proposals: Some(proposals),
        mutants: true,
    };
    match gaps::judge(&counter.project, &counter.rules, options)
        .unwrap_or_else(|error| panic!("{error}"))
    {
        Gaps::Inconclusive(inconclusive) => {
            assert_eq!(inconclusive.reason, "work-with-proposals-over-limit");
            assert!(inconclusive.size > Some(u128::from(declared)));
        }
        other => panic!("not inconclusive: {}", other.json()),
    }
    let options = gaps::Options {
        max_work: declared * 2,
        ..options
    };
    let held = gaps::judge(&counter.project, &counter.rules, options)
        .unwrap_or_else(|error| panic!("{error}"));
    let after = &measured(&held)["proposals"]["after"];
    let after_work = after["work"]
        .as_str()
        .and_then(|work| work.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("work after"));
    assert!(after_work > declared, "{after_work} <= {declared}");
}

#[test]
fn a_false_guard_does_not_supply_a_rejected_frames_missing_successor() {
    let counter = Core::new("bounded-counter", &[("C", 1)]);
    let formula = "(context.102.130 == 1 && command.101.120 == 150 && pre.100.110 < 1) -> post.100.110 == pre.100.110 + 1";
    let alarm = counter.gaps(Some(&format!("law 9000 increment on any = {formula};\n")));
    assert_eq!(alarm.status(), "proposal-false-alarm");
    assert_eq!(alarm.json()["false_alarm"]["input"], json!([0, 150, 0]));
    let probe = counter.probe(&Question::Count {
        condition: formula.to_owned(),
    });
    assert_eq!(probe["answer"], 2);
    assert_eq!(probe["undefined"], 6);
    assert_eq!(probe["of"], 8);
    let accepted = counter.gaps(Some(&format!(
        "law 9000 increment on accept = {formula};\n"
    )));
    assert_eq!(accepted.status(), "measured");
    assert_eq!(
        measured(&accepted)["proposals"]["determined"]["full"],
        json!([0, 0])
    );
    assert_eq!(
        measured(&accepted)["reject_everything"]["admitted_where_the_contract_decides"],
        true
    );
}

#[test]
fn the_extended_catalog_keeps_every_first_catalog_mutant_and_its_number() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let rules = Rules::read(&counter.rules).unwrap_or_else(|error| panic!("{error}"));
    let declarations = Declarations::read(&counter.project, &rules.leaf_bindings)
        .unwrap_or_else(|error| panic!("{error}"));
    let first = mutants::catalog(&rules, &declarations);
    let extended = mutants::extended_catalog(&rules, &declarations);
    let describe = |mutant: &mutants::Mutant| {
        format!(
            "{} {} {} {}",
            mutant.id, mutant.operator, mutant.site, mutant.change
        )
    };
    assert!(extended.len() > first.len());
    for (before, after) in first.iter().zip(&extended) {
        assert_eq!(describe(before), describe(after));
    }
    for (index, mutant) in extended.iter().enumerate() {
        assert_eq!(mutant.id, format!("m{:04}", index + 1));
    }
    let added: Vec<&str> = extended[first.len()..]
        .iter()
        .map(|mutant| mutant.operator)
        .collect();
    for (name, _) in mutants::ADDED {
        assert!(added.contains(&name), "{name}");
    }
    let flip = extended
        .iter()
        .find(|mutant| mutant.operator == "flip-class" && mutant.site == "cases[0].class")
        .unwrap_or_else(|| panic!("flip-class of case 0"));
    assert_eq!(flip.change, "Reject -> Accept keeping every state field");
    let perturbed: Vec<&str> = extended
        .iter()
        .filter(|mutant| mutant.operator == "perturb-successor")
        .map(|mutant| mutant.change.as_str())
        .collect();
    assert_eq!(
        perturbed,
        [
            "`value + 1` -> `(value + 1) + 1`",
            "`value + 1` -> `(value + 1) - 1`",
            "`value - 1` -> `(value - 1) + 1`",
            "`value - 1` -> `(value - 1) - 1`"
        ]
    );
}

#[test]
fn review_with_gaps_writes_packet_version_three_with_the_extended_catalog() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let sources = ReviewSources {
        project: &counter.project,
        rules: &counter.rules,
        examples: None,
    };
    let first =
        review_with_gaps(sources, Core::options(None)).unwrap_or_else(|error| panic!("{error}"));
    let again =
        review_with_gaps(sources, Core::options(None)).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(first.packet(), again.packet());
    let packet: Value =
        serde_json::from_str(first.packet()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(packet["schema"], PACKET_SCHEMA_GAPS);
    assert_eq!(packet["authority"], "none");
    assert_eq!(packet["tool"]["mutant_catalog"], mutants::CATALOG_V2);
    assert_eq!(packet["mutants"]["catalog"], mutants::CATALOG_V2);
    assert_eq!(
        packet["mutants"]["operators"].as_array().map(Vec::len),
        Some(mutants::OPERATORS.len() + mutants::ADDED.len())
    );
    assert_eq!(packet["gaps"]["status"], "measured");
    assert_eq!(packet["summary"]["gaps"]["full"]["tuples"], 12);
}

#[test]
fn a_profile_counts_valid_reachable_and_minimal_states() {
    let retry = Core::new("retry-budget", &[("C", 1)]).profile();
    assert_eq!(
        retry["states"],
        json!({"schema": 4, "valid": 4, "reachable": 4, "minimal": 3})
    );
    assert_eq!(
        retry["memory_bits"],
        json!({"declared": 2, "reachable": 2, "minimal": 2})
    );
    let register = Core::new("versioned-register", &[("C", 1)]).profile();
    assert_eq!(
        register["states"],
        json!({"schema": 4, "valid": 4, "reachable": 3, "minimal": 2})
    );
    assert_eq!(register["robustness"]["refused_tuples"]["all"], 0);
    let counter = Core::new("bounded-counter", &[("C", 2)]).profile();
    assert_eq!(counter["genesis"], json!([0]));
    assert_eq!(
        counter["robustness"]["genesis_reachable_from_every_reachable_state"],
        true
    );
}

#[test]
fn a_lemma_is_inductive_false_or_not_inductive() {
    let register = Core::new("versioned-register", &[("C", 1)]);
    let verdicts = register.lemmas(&[
        "0 <= post.100.111 && post.100.111 <= 1",
        "post.100.111 == 0",
        "!(post.100.110 == 0 && post.100.111 == 0)",
    ]);
    assert_eq!(verdicts[0]["verdict"], "inductive");
    assert_eq!(verdicts[0]["exact"], false);
    assert_eq!(verdicts[0]["unreachable_admitted"], 1);
    assert_eq!(verdicts[0]["implies_state_laws"], true);
    assert_eq!(verdicts[1]["verdict"], "false");
    assert_eq!(
        verdicts[1]["reason"],
        "a reachable state lies outside the lemma"
    );
    assert_eq!(verdicts[1]["trace_from_genesis"], json!([[0, 0, 1]]));
    assert_eq!(
        verdicts[1]["counterexample_to_induction"]["state"],
        json!([0, 0])
    );
    assert_eq!(verdicts[2]["verdict"], "false");
    assert_eq!(verdicts[2]["reason"], "genesis does not satisfy the lemma");
    // In a logical deadline of capacity 1 only genesis (1, 0, 0) and
    // (1, 1, 1) are reachable; the unreachable (0, 0, 0) commits into
    // (0, 0, 1), which the lemma excludes.
    let deadline = Core::new("logical-deadline", &[("C", 1)]);
    let weak = deadline.lemmas(&["!(post.100.110 == 0 && post.100.111 == 0 && post.100.112 == 1)"]);
    assert_eq!(weak[0]["verdict"], "not-inductive", "{}", weak[0]);
    assert_eq!(
        weak[0]["counterexample_to_induction"],
        json!({"state": [0, 0, 0], "input": [0, 1], "successor": [0, 0, 1], "count": 1})
    );
}

#[test]
fn a_lemma_must_be_one_state_formula() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    for formula in [
        "true; law 9 x on any = false",
        "post.100.110 == 0 // comment",
        "pre.100.110 == 0",
        "command.101.120 == 150",
        "",
    ] {
        let result = lemma::judge(
            &counter.project,
            &counter.rules,
            &[formula.to_owned()],
            DEFAULT_MAX_TUPLES,
        );
        assert!(result.is_err(), "{formula:?} was judged");
    }
}

const COUNTER_SCENARIOS: &str = "# Bounded counter runs.\n\
# scenario count-up-and-down\n\
0 | 150 | 1 | accept - 1 | -\n\
1 | 150 | 1 | accept - 2 | -\n\
2 | 150 | 1 | reject 201 2 | -\n\
2 | 151 | 1 | accept - 1 | -\n\
# scenario unauthorized\n\
0 | 151 | 0 | reject 200 0 | -\n";

#[test]
fn scenarios_replay_through_the_contract_its_mutants_and_the_reject_everything_core() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let replayed = scenario::replay(&counter.project, &counter.rules, COUNTER_SCENARIOS, "s")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replayed.failed, 0);
    let packet = &replayed.packet;
    assert_eq!(packet["count"], 2);
    assert_eq!(packet["scenarios"][0]["start"], "genesis");
    assert_eq!(packet["scenarios"][0]["steps"], 4);
    assert_eq!(packet["reject_everything"]["killed"], true);
    assert_eq!(
        packet["reject_everything"]["first_failing_scenario"],
        "count-up-and-down"
    );
    assert!(count(&packet["mutants"]["killed"]) > 0);
    // A step that writes a pre-state the replay did not reach fails there.
    let broken = COUNTER_SCENARIOS.replace("2 | 151 | 1 | accept - 1", "1 | 151 | 1 | accept - 0");
    let replayed = scenario::replay(&counter.project, &counter.rules, &broken, "s")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replayed.failed, 1);
    assert_eq!(replayed.packet["scenarios"][0]["failure"]["step"], 4);
    assert_eq!(replayed.packet["scenarios"][0]["failure"]["line"], 6);
}

#[test]
fn a_scenario_that_fails_on_the_contract_credits_no_kill() {
    // The review's case: one wrong scenario once credited 36 mutant kills and
    // the reject-everything core's.
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    let wrong = "# scenario wrong\n0 | 150 | 1 | reject 201 0 | -\n";
    let replayed = scenario::replay(&counter.project, &counter.rules, wrong, "s")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replayed.failed, 1);
    assert_eq!(replayed.packet["mutants"]["status"], "withheld");
    assert_eq!(replayed.packet["reject_everything"]["status"], "withheld");
    assert!(replayed.packet["mutants"].get("killed").is_none());
}

#[test]
fn a_scenario_catches_a_fault_planted_on_its_path() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    // A first case that rejects exactly the second step's input, 1 150 1.
    let faulted = counter.with_rules(|rules| {
        let cases = rules["cases"]
            .as_array_mut()
            .unwrap_or_else(|| panic!("cases"));
        cases.insert(
            0,
            json!({
                "when": "value == 1 && action == 150 && authorized",
                "class": "Reject", "reason": 201, "post": {}, "outbox": []
            }),
        );
    });
    let replayed = scenario::replay(&faulted.project, &faulted.rules, COUNTER_SCENARIOS, "s")
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(replayed.failed, 1);
    let failure = &replayed.packet["scenarios"][0]["failure"];
    assert_eq!(failure["step"], 2);
    assert_eq!(failure["case"], 0);
    assert_eq!(replayed.packet["scenarios"][1]["verdict"], "passed");
}

#[test]
fn a_scenario_file_names_its_runs() {
    let counter = Core::new("bounded-counter", &[("C", 2)]);
    for (text, place) in [
        ("0 | 150 | 1 | accept - 1 | -\n", "s line 1"),
        ("# scenario\n0 | 150 | 1 | accept - 1 | -\n", "s line 1"),
        (
            "# scenario a\n# scenario b\n0 | 150 | 1 | accept - 1 | -\n",
            "s line 1",
        ),
        (
            "# scenario a\n0 | 150 | 1 | accept - 1 | -\n# scenario a\n0 | 150 | 1 | accept - 1 | -\n",
            "s line 3",
        ),
        ("# scenarios follow\n", "s"),
    ] {
        let error = scenario::replay(&counter.project, &counter.rules, text, "s")
            .err()
            .unwrap_or_else(|| panic!("{text:?} was replayed"));
        assert_eq!(error.place(), place, "{text:?}: {error}");
    }
}

#[test]
fn probes_answer_exactly_over_the_declared_domain() {
    let pool = Core::new("reservation-pool", &[("C", 3), ("Q", 2)]);
    let explain = pool.probe(&Question::Explain {
        input: vec![1, 2, 152, 1, 1],
    });
    assert_eq!(explain["case"], 7);
    assert_eq!(explain["cases_tried"].as_array().map(Vec::len), Some(8));
    assert_eq!(
        explain["authority"]["example"],
        "1 2 152 1 1 | accept - 1 1 | -"
    );
    let reachable = pool.probe(&Question::Reachable {
        state: "post.100.111 == 3".to_owned(),
    });
    assert_eq!(reachable["answer"], "yes");
    assert_eq!(
        reachable["trace_from_genesis"],
        json!([[150, 1, 1], [150, 2, 1]])
    );
    let unreachable = pool.probe(&Question::Reachable {
        state: "post.100.110 + post.100.111 == 4".to_owned(),
    });
    assert_eq!(unreachable["answer"], "no");
    let counted = pool.probe(&Question::Count {
        condition: "post.100.111 > pre.100.111".to_owned(),
    });
    assert_eq!(counted["answer"], 9);
    assert_eq!(counted["of"], 256);
    let implies = pool.probe(&Question::Implies {
        premise: "command.101.120 == 151 && context.102.130 == 1".to_owned(),
        conclusion: "pre.100.111 > 0".to_owned(),
    });
    assert_eq!(implies["answer"], "no");
    let decisions = pool.probe(&Question::Decisions {
        condition: Some("context.102.130 == 0".to_owned()),
    });
    assert_eq!(decisions["inputs"], 128);
    // A reject keeps whatever pre-state it had: no difference.
    let independent = pool.probe(&Question::Depends {
        position: "pre.100.110".to_owned(),
        condition: Some("context.102.130 == 0 && pre.100.110 + pre.100.111 <= 3".to_owned()),
    });
    assert_eq!(independent["answer"], "no", "{independent}");
    let dependent = pool.probe(&Question::Depends {
        position: "command.101.121".to_owned(),
        condition: None,
    });
    assert_eq!(dependent["answer"], "yes");
    for question in [
        Question::Explain { input: vec![1, 2] },
        Question::Explain {
            input: vec![9, 2, 152, 1, 1],
        },
        Question::Depends {
            position: "pre.100.999".to_owned(),
            condition: None,
        },
    ] {
        assert!(
            probe::probe(&pool.project, &pool.rules, &question, DEFAULT_MAX_TUPLES).is_err(),
            "{question:?}"
        );
    }
    // Nothing to compare is no answer.
    let vacuous = pool.probe(&Question::Implies {
        premise: "false".to_owned(),
        conclusion: "true".to_owned(),
    });
    assert_eq!(vacuous["answer"], "vacuous");
    let alone = pool.probe(&Question::Depends {
        position: "pre.100.110".to_owned(),
        condition: Some("pre.100.110 == 0".to_owned()),
    });
    assert_eq!(alone["answer"], "vacuous", "{alone}");
    let over = probe::probe(
        &pool.project,
        &pool.rules,
        &Question::Decisions { condition: None },
        255,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(matches!(over, Answer::Inconclusive(_)));
}
