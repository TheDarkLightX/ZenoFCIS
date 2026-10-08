//! The symbolic check against the library on sampled points, on the app
//! study's contracts and the templates, and the whole check with the
//! reference solver standing in for CVC5 and Z3 where a domain is small
//! enough. Planted solver faults (an unknown, a disagreement, a solver that
//! answers `unsat` to everything) must leave the check inconclusive.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::*;
use crate::symbolic::smt::Sort;
use crate::symbolic::testing::reference;
use crate::symbolic::verdict::Answer;

/// An application's contract files, with optional text replacements.
struct App {
    name: String,
    project: String,
    rules: String,
}

impl App {
    fn read(base: PathBuf) -> Self {
        let text = |path: &str| {
            fs::read_to_string(base.join(path))
                .unwrap_or_else(|error| panic!("read {}/{path}: {error}", base.display()))
        };
        Self {
            name: base
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            project: text("project.zeno"),
            rules: text("v2/policy.json"),
        }
    }

    fn fixture(name: &str) -> Self {
        Self::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
    }

    fn template(name: &str) -> Self {
        Self::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("templates")
                .join(name),
        )
    }

    /// The same contract with `from` replaced by `to` in one file, exactly
    /// once.
    fn replaced(mut self, in_rules: bool, from: &str, to: &str) -> Self {
        let text = if in_rules {
            &mut self.rules
        } else {
            &mut self.project
        };
        assert_eq!(text.matches(from).count(), 1, "{from} must occur once");
        *text = text.replacen(from, to, 1);
        self
    }

    /// The study's escrow with every money amount in 0..=1 and every time
    /// at 0: a domain of 12,544 tuples that enumeration decides.
    fn reduced_escrow() -> Self {
        Self::fixture("escrow")
            .replaced(
                false,
                "type 108 int Money in 0..=100000000;",
                "type 108 int Money in 0..=1;",
            )
            .replaced(
                false,
                "type 109 int UnixTime in 0..=4102444800;",
                "type 109 int UnixTime in 0..=0;",
            )
    }

    /// The study's planted spend-approval bug: the CFO check starts at tier
    /// 2 instead of tier 1.
    fn planted_spend_approval() -> Self {
        Self::fixture("spend-approval").replaced(
            true,
            "\"action == 172 && tier >= 1 && !cfo_ok\"",
            "\"action == 172 && tier >= 2 && !cfo_ok\"",
        )
    }

    fn check(
        &self,
        strengthening: Option<&str>,
        max_tuples: u64,
        solve: &mut Solve<'_>,
    ) -> Checked {
        check(
            SymbolicSources {
                project: &self.project,
                rules: &self.rules,
                strengthening,
            },
            max_tuples,
            solve,
        )
        .unwrap_or_else(|error| panic!("{}: {error:?}", self.name))
    }
}

/// The study's strengthening: a Created escrow has paid nothing out.
const CREATED_PAYS_NOTHING: &str = r#"{
  "schema": "zeno-fcis/strengthening/1",
  "invariants": [
    {"name": "created_pays_nothing", "formula": "post.100.110 == 150 -> post.100.113 == 0 && post.100.114 == 0"}
  ]
}"#;

fn not_configured(_: &Query<'_>) -> Answers {
    Answers {
        cvc5: Answer::NotConfigured,
        z3: Answer::NotConfigured,
        millis: [None, None],
    }
}

/// The bound contract of an application, as `check` builds it.
fn with_bound<R>(
    app: &App,
    body: impl FnOnce(&Authority<'_>, &[Position], &Framer, &Declarations) -> R,
) -> R {
    let name = app.name.as_str();
    let rules = built(name, Rules::read(&app.rules));
    let declarations = built(name, Declarations::read(&app.project, &rules.leaf_bindings));
    let schema = built(name, schema::encode(&declarations));
    let commitment = built(name, schema_commitment(&schema));
    let contract = built(name, Contract::build(&declarations, &rules, commitment));
    let positions = built(name, domain::positions(&declarations));
    let framer = Framer::new(&positions, &contract);
    built(
        name,
        policy::with_authority(&contract, &schema, |authority| {
            body(authority, &positions, &framer, &declarations)
        }),
    )
}

fn built<T>(name: &str, result: Result<T, ContractError>) -> T {
    result.unwrap_or_else(|error| panic!("{name}: {error:?}"))
}

fn encoded<T>(name: &str, result: Result<T, Unsupported>) -> T {
    result.unwrap_or_else(|error| panic!("{name}: {}", error.reason()))
}

/// A fixed pseudo-random sequence (xorshift64*).
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
    }
}

/// Tuples that hit boundaries and equalities often: each integer is a
/// domain bound, a value from a small pool, a value an earlier integer of the
/// same tuple took, or uniform.
fn sampled_tuples(positions: &[Position], count: usize, seed: u64) -> Vec<Vec<i64>> {
    const POOL: [i64; 10] = [
        0, 1, 2, 3, 50, 604_800, 604_801, 1_209_600, 1_209_601, 1_814_401,
    ];
    let mut random = Random(seed);
    (0..count)
        .map(|_| {
            let mut tuple: Vec<i64> = Vec::with_capacity(positions.len());
            let mut integers: Vec<i64> = Vec::new();
            for position in positions {
                let value = match &position.domain {
                    domain::LeafDomain::Bool => i64::from(random.next() % 2 == 1),
                    domain::LeafDomain::Sum { variants, .. } => {
                        i64::from(variants[random.below(variants.len())])
                    }
                    domain::LeafDomain::Int { min, max } => {
                        let earlier: Vec<i64> = integers
                            .iter()
                            .copied()
                            .filter(|value| (*min..=*max).contains(value))
                            .collect();
                        let pooled: Vec<i64> = POOL
                            .iter()
                            .copied()
                            .filter(|value| (*min..=*max).contains(value))
                            .chain([*min, *max, max.saturating_sub(1).max(*min)])
                            .collect();
                        let value = match random.below(5) {
                            0 => {
                                let width = u64::try_from(i128::from(*max) - i128::from(*min))
                                    .unwrap_or(u64::MAX);
                                let offset = random.next() % width.saturating_add(1).max(1);
                                i64::try_from(i128::from(*min) + i128::from(offset)).unwrap_or(*min)
                            }
                            1 if !earlier.is_empty() => earlier[random.below(earlier.len())],
                            _ => pooled[random.below(pooled.len())],
                        };
                        integers.push(value);
                        value
                    }
                };
                tuple.push(value);
            }
            tuple
        })
        .collect()
}

/// Encodes a contract's decision, each committing branch with its successor
/// and the state laws on it, and the state laws on the pre-state, then
/// compares every value with the library's at sampled tuples. Returns how
/// many sampled tuples committed each committing case.
fn encodings_agree_with_the_library(app: &App, samples: usize) -> BTreeMap<usize, usize> {
    with_bound(app, |authority, positions, framer, _| {
        let descriptor = authority.descriptor();
        let state_laws = evaluate::state_laws(descriptor);
        let laws: Vec<LawProgram<'_>> = descriptor
            .laws
            .iter()
            .filter(|law| state_laws.contains(&law.id))
            .map(|law| LawProgram {
                id: law.id,
                nodes: law.program.nodes,
                root: law.program.root,
            })
            .collect();
        let committing: Vec<usize> = descriptor
            .branches
            .iter()
            .enumerate()
            .filter(|(_, branch)| {
                matches!(branch.class, c::Class::Accept | c::Class::CommittedFailure)
            })
            .map(|(index, _)| index)
            .collect();
        let name = app.name.as_str();
        let mut script = Script::default();
        let decision = encoded(name, Decision::encode(&mut script, descriptor, positions));
        for law in &laws {
            let holds = encoded(
                name,
                encode::law(
                    &mut script,
                    &format!("pre_{}_", law.id),
                    law.nodes,
                    law.root,
                    &|field| decision.pre_state(field),
                ),
            );
            script.define(&format!("pre_law_{}", law.id), Sort::Bool, holds);
        }
        for case in &committing {
            let branch = encoded(name, Branch::encode(descriptor, &decision, *case));
            script.define(&format!("commits_{case}"), Sort::Bool, branch.commits);
            for (field, value) in &branch.post {
                if let Some(number) = value.number() {
                    script.define(&format!("post_{case}_{field}"), Sort::Int, number);
                }
            }
            for law in &laws {
                let holds = encoded(
                    name,
                    encode::law(
                        &mut script,
                        &format!("post_{case}_{}_", law.id),
                        law.nodes,
                        law.root,
                        &|field| {
                            branch
                                .post
                                .iter()
                                .find(|(id, _)| *id == field)
                                .map(|(_, value)| value.clone())
                        },
                    ),
                );
                script.define(&format!("post_law_{case}_{}", law.id), Sort::Bool, holds);
            }
        }
        let mut hits: BTreeMap<usize, usize> = committing.iter().map(|case| (*case, 0)).collect();
        for tuple in sampled_tuples(positions, samples, 0x5EED_0000 + samples as u64) {
            let assignment: BTreeMap<String, i128> = tuple
                .iter()
                .enumerate()
                .map(|(index, value)| (format!("x{index}"), i128::from(*value)))
                .collect();
            let evaluation = script.evaluate(&assignment).unwrap_or_else(|| {
                panic!("{}: {tuple:?} is outside the declared domain", app.name)
            });
            let at = |what: &str| format!("{}: {what} at {}", app.name, named(positions, &tuple));
            let state = library::state(framer, &tuple);
            for law in &laws {
                let library =
                    library::verdict(descriptor, *law, &state) == Some(l::Verdict::Satisfied);
                assert_eq!(
                    evaluation.boolean(&format!("pre_law_{}", law.id)),
                    Some(library),
                    "{}",
                    at(&format!("law {} on the pre-state", law.id))
                );
            }
            let decided = library::decide(authority, framer, &tuple);
            let outputs = library::run(descriptor, &tuple);
            let selected = outputs
                .as_ref()
                .and_then(|outputs| library::branch(descriptor, outputs));
            let reaches_laws = matches!(decided, Decided::Commits(_) | Decided::LawRefuses(..));
            for case in &committing {
                let expected = reaches_laws && selected == Some(*case);
                assert_eq!(
                    evaluation.boolean(&format!("commits_{case}")),
                    Some(expected),
                    "{} (the Authority {})",
                    at(&format!("case {case} commits")),
                    decided.text()
                );
                if !expected {
                    continue;
                }
                *hits.entry(*case).or_default() += 1;
                let outputs = outputs.as_deref().unwrap_or_default();
                let post = library::successor(descriptor, positions, &tuple, outputs, *case)
                    .unwrap_or_else(|reason| panic!("{}: {reason}", at("the successor")));
                if let Decided::Commits(committed) = &decided {
                    assert_eq!(committed, &post, "{}", at("the committed successor"));
                }
                for (field, value) in &post {
                    assert_eq!(
                        evaluation.int(&format!("post_{case}_{field}")),
                        Some(*value),
                        "{}",
                        at(&format!("case {case} successor field {field}"))
                    );
                }
                let successor =
                    library::state(framer, &library::with_state(positions, &tuple, &post));
                for law in &laws {
                    let library = library::verdict(descriptor, *law, &successor)
                        == Some(l::Verdict::Satisfied);
                    assert_eq!(
                        evaluation.boolean(&format!("post_law_{case}_{}", law.id)),
                        Some(library),
                        "{}",
                        at(&format!("case {case}, law {} on the successor", law.id))
                    );
                }
            }
        }
        hits
    })
}

#[test]
fn the_contract_encoding_agrees_with_the_library_on_sampled_tuples() {
    let mut apps: Vec<App> = [
        "account-lockout",
        "agent-treasury-guard",
        "compliance-gateway",
        "durable-counter",
        "inventory-reservation",
        "order-fulfillment",
        "prepared-counter",
        "withdrawal-queue",
    ]
    .into_iter()
    .map(App::template)
    .collect();
    apps.extend(["escrow", "spend-approval"].map(App::fixture));
    apps.push(App::planted_spend_approval());
    let mut committed = 0;
    for app in &apps {
        let hits = encodings_agree_with_the_library(app, 3000);
        println!("{}: sampled tuples committing each case {hits:?}", app.name);
        committed += hits.values().sum::<usize>();
        if app.name == "escrow" {
            // Every escrow case is reached at sampled tuples, so each
            // branch's successor and laws were compared.
            assert!(hits.values().all(|count| *count > 0), "escrow: {hits:?}");
        }
    }
    assert!(
        committed > 100,
        "too few sampled tuples committed: {committed}"
    );
}

/// Strengthening clauses in the arithmetic the encoding must follow:
/// floor and ceiling division with negative operands and zero divisors,
/// checked `i128` multiplication, `choose` and negation.
const ARITHMETIC: &str = r#"{
  "schema": "zeno-fcis/strengthening/1",
  "invariants": [
    {"name": "floor", "formula": "div_floor(post.100.111 - post.100.112, post.100.113 - 3) >= -2"},
    {"name": "ceiling", "formula": "div_ceil(post.100.111 - 50, post.100.114 - post.100.113) <= 7"},
    {"name": "product", "formula": "post.100.111 * 10000000000000000000 * 10000000000000000000 >= 0"},
    {"name": "chosen", "formula": "choose(post.100.110 == 150, post.100.111, post.100.112 - post.100.111) >= 0"},
    {"name": "negated", "formula": "-post.100.111 < post.100.112 - post.100.116"},
    {"name": "mixed", "formula": "div_floor(post.100.115 * 3, post.100.111 - post.100.112) <= post.100.116 || post.100.110 == 151"}
  ]
}"#;

#[test]
fn law_arithmetic_agrees_with_the_library_law_evaluator_on_sampled_states() {
    let app = App::fixture("escrow");
    with_bound(&app, |authority, positions, framer, declarations| {
        let descriptor = authority.descriptor();
        let clauses = built("escrow", strengthening::read(ARITHMETIC, declarations));
        let ops: Vec<Vec<l::Op<'_>>> = clauses
            .iter()
            .map(|clause| clause.nodes.iter().map(policy::law_op).collect())
            .collect();
        let mut script = Script::default();
        let decision = encoded(
            "escrow",
            Decision::encode(&mut script, descriptor, positions),
        );
        for (clause, nodes) in clauses.iter().zip(&ops) {
            let holds = encoded(
                &clause.name,
                encode::law(
                    &mut script,
                    &format!("{}_", clause.name),
                    nodes,
                    clause.root,
                    &|field| decision.pre_state(field),
                ),
            );
            script.define(&clause.name, Sort::Bool, holds);
        }
        let mut seen: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for tuple in sampled_tuples(positions, 4000, 0xA817) {
            let assignment: BTreeMap<String, i128> = tuple
                .iter()
                .enumerate()
                .map(|(index, value)| (format!("x{index}"), i128::from(*value)))
                .collect();
            let evaluation = script
                .evaluate(&assignment)
                .unwrap_or_else(|| panic!("{tuple:?} is outside the declared domain"));
            let state = library::state(framer, &tuple);
            for (index, (clause, nodes)) in clauses.iter().zip(&ops).enumerate() {
                let program = LawProgram {
                    id: CLAUSE_IDS + u32::try_from(index).unwrap_or(0),
                    nodes,
                    root: clause.root,
                };
                let verdict = library::verdict(descriptor, program, &state);
                *seen
                    .entry((clause.name.as_str(), library::verdict_name(verdict)))
                    .or_default() += 1;
                assert_eq!(
                    evaluation.boolean(&clause.name),
                    Some(verdict == Some(l::Verdict::Satisfied)),
                    "{} at {} ({})",
                    clause.name,
                    named(positions, &tuple),
                    library::verdict_name(verdict)
                );
            }
        }
        // The samples reach every outcome the arithmetic has: zero divisors
        // and overflow (undefined), and both truth values.
        for (clause, verdict) in [
            ("floor", "undefined"),
            ("floor", "satisfied"),
            ("floor", "violated"),
            ("ceiling", "undefined"),
            ("ceiling", "violated"),
            ("product", "undefined"),
            ("product", "satisfied"),
            ("chosen", "violated"),
            ("negated", "violated"),
            ("mixed", "undefined"),
        ] {
            assert!(
                seen.contains_key(&(clause, verdict)),
                "no sample made {clause} {verdict}: {seen:?}"
            );
        }
    });
}

fn summary(checked: &Checked) -> (u64, u64, u64) {
    let count = |key: &str| checked.report["summary"][key].as_u64().unwrap_or(u64::MAX);
    (count("holds"), count("refuted"), count("inconclusive"))
}

fn case<'r>(report: &'r Value, rule: &str) -> &'r Value {
    report["cases"]
        .as_array()
        .and_then(|cases| cases.iter().find(|case| case["rule"] == rule))
        .unwrap_or_else(|| panic!("no case {rule}"))
}

#[test]
fn spend_approval_holds_on_both_routes_and_its_planted_bug_is_refuted_on_both() {
    let app = App::fixture("spend-approval");
    let checked = app.check(None, DEFAULT_MAX_TUPLES, &mut reference);
    assert_eq!(
        checked.status,
        Status::Proved,
        "{:?}",
        checked.report["causes"]
    );
    assert_eq!(summary(&checked), (6, 0, 0));
    assert_eq!(checked.report["domain"]["route"], "exhaustive-and-symbolic");
    assert_eq!(checked.report["premise"]["status"], "sat");
    for case in checked.report["cases"].as_array().into_iter().flatten() {
        assert_eq!(case["reached"]["status"], "reached", "{case}");
        for target in case["targets"].as_array().into_iter().flatten() {
            assert_eq!(target["exhaustive"]["status"], "holds", "{target}");
            assert_eq!(target["symbolic"]["status"], "unsat", "{target}");
        }
    }

    let planted = App::planted_spend_approval();
    let checked = planted.check(None, DEFAULT_MAX_TUPLES, &mut reference);
    assert_eq!(checked.status, Status::Refuted);
    assert_eq!(summary(&checked), (5, 1, 0));
    assert_eq!(checked.report["summary"]["solver_disagreements"], 0);
    let execute = case(&checked.report, "execute: the payment is sent");
    let target = &execute["targets"][0];
    assert_eq!(target["target"], "law 500");
    assert_eq!(target["status"], "refuted");
    assert_eq!(target["exhaustive"]["status"], "refuted");
    assert_eq!(target["symbolic"]["status"], "sat");
    // Both witnesses replayed: a pending tier-1 request without the CFO's
    // approval executes, and the Authority refuses it by law 500.
    for witness in [
        &target["counterexample"]["witness"],
        &target["symbolic"]["replayed"],
    ] {
        let input = |name: &str| {
            witness["input"]
                .as_array()
                .and_then(|input| input.iter().find(|value| value["name"] == name))
                .map(|value| value["value"].clone())
                .unwrap_or_else(|| panic!("no {name} in {witness}"))
        };
        assert_eq!(input("pre.100.120"), "151");
        assert_eq!(input("pre.100.121"), "1");
        assert_eq!(input("pre.100.122"), "0");
        assert_eq!(input("command.101.130"), "172");
        assert_eq!(witness["target_on_successor"], "violated");
        assert!(
            witness["authority"]
                .as_str()
                .is_some_and(|text| text.contains("by law 500")),
            "{witness}"
        );
    }
}

#[test]
fn the_symbolic_route_alone_attests_and_refutes_where_enumeration_is_not_run() {
    let checked = App::fixture("spend-approval").check(None, 0, &mut reference);
    assert_eq!(
        checked.status,
        Status::Attested,
        "{:?}",
        checked.report["causes"]
    );
    assert_eq!(checked.report["domain"]["route"], "symbolic");
    assert_eq!(checked.report["evidence"], "attested");
    assert_eq!(summary(&checked), (6, 0, 0));
    for control in checked.report["domain_controls"]
        .as_array()
        .into_iter()
        .flatten()
    {
        assert_eq!(control["status"], "substantive", "{control}");
    }

    let checked = App::planted_spend_approval().check(None, 0, &mut reference);
    assert_eq!(checked.status, Status::Refuted);
    let execute = case(&checked.report, "execute: the payment is sent");
    assert_eq!(execute["targets"][0]["status"], "refuted");
    assert_eq!(execute["targets"][0]["evidence"], "checked");
    assert_eq!(execute["targets"][0]["counterexample"]["route"], "symbolic");
}

#[test]
fn the_reduced_escrow_needs_its_strengthening_on_the_exhaustive_route() {
    let app = App::reduced_escrow();
    let alone = app.check(None, DEFAULT_MAX_TUPLES, &mut not_configured);
    assert_eq!(alone.status, Status::Refuted);
    let fund = case(&alone.report, "fund: the buyer funds the escrow");
    assert_eq!(fund["targets"][0]["target"], "law 500");
    assert_eq!(fund["targets"][0]["status"], "refuted");
    assert_eq!(fund["targets"][0]["evidence"], "checked");
    let witness = &fund["targets"][0]["counterexample"]["witness"];
    assert_eq!(witness["input"][0]["name"], "pre.100.110");
    assert_eq!(witness["input"][0]["value"], "150");
    // Only Fund breaks law 500, from a Created state that paid something out.
    assert_eq!(summary(&alone), (17, 1, 0));

    let strengthened = app.check(
        Some(CREATED_PAYS_NOTHING),
        DEFAULT_MAX_TUPLES,
        &mut not_configured,
    );
    assert_eq!(
        strengthened.status,
        Status::Proved,
        "{:?}",
        strengthened.report["causes"]
    );
    assert_eq!(summary(&strengthened), (27, 0, 0));
    assert_eq!(strengthened.report["genesis"][2]["genesis"], "satisfied");
}

#[test]
fn an_unknown_is_inconclusive_and_never_holds() {
    // The escrow's domain is far beyond the reference solver's search, so
    // it answers unknown to every query.
    let checked = App::fixture("escrow").check(
        Some(CREATED_PAYS_NOTHING),
        DEFAULT_MAX_TUPLES,
        &mut reference,
    );
    assert_eq!(checked.status, Status::Inconclusive);
    assert_eq!(summary(&checked), (0, 0, 27));
    assert!(
        checked
            .lines
            .iter()
            .any(|line| line.contains("CVC5 answered unknown")),
        "{:?}",
        checked.lines
    );
    let checked = App::fixture("escrow").check(None, DEFAULT_MAX_TUPLES, &mut not_configured);
    assert_eq!(checked.status, Status::Inconclusive);
    assert_eq!(checked.report["evidence"], "none");
}

#[test]
fn a_planted_solver_disagreement_is_inconclusive() {
    // Z3 claims a model that does not replay wherever CVC5 answers unsat.
    let mut disagree = |query: &Query<'_>| {
        let reference = reference(query);
        match reference.cvc5 {
            Answer::Unsat { .. } => Answers {
                z3: Answer::Sat(BTreeMap::from([("x0".to_owned(), 0)])),
                ..reference
            },
            _ => reference,
        }
    };
    let checked = App::fixture("spend-approval").check(None, 0, &mut disagree);
    assert_eq!(checked.status, Status::Inconclusive);
    assert_eq!(summary(&checked), (0, 0, 6));
    assert_eq!(checked.report["summary"]["solver_disagreements"], 6);
    assert!(
        checked
            .lines
            .iter()
            .any(|line| line.contains("the solvers disagree")),
        "{:?}",
        checked.lines
    );
    // Enumeration, where it runs, decides; the disagreement stays reported.
    let checked = App::fixture("spend-approval").check(None, DEFAULT_MAX_TUPLES, &mut disagree);
    assert_eq!(checked.status, Status::Proved);
    assert_eq!(checked.report["summary"]["solver_disagreements"], 6);
}

#[test]
fn a_solver_that_answers_unsat_to_everything_fails_the_planted_controls() {
    let mut always_unsat = |_: &Query<'_>| Answers {
        cvc5: Answer::Unsat { proof_output: true },
        z3: Answer::Unsat {
            proof_output: false,
        },
        millis: [None, None],
    };
    let checked = App::fixture("spend-approval").check(None, 0, &mut always_unsat);
    assert_eq!(checked.status, Status::Inconclusive);
    assert!(
        checked
            .lines
            .iter()
            .any(|line| line.contains("the planted premise control was not refuted")),
        "{:?}",
        checked.lines
    );
    for case in checked.report["cases"].as_array().into_iter().flatten() {
        assert_eq!(case["reached"]["status"], "vacuous", "{case}");
    }
    assert_eq!(
        checked.report["domain_controls"][0]["status"],
        "implied-by-domain"
    );
}

#[test]
fn strengthening_files_are_read_strictly_and_checked_at_genesis() {
    let app = App::fixture("escrow");
    let read = |text: &str| {
        check(
            SymbolicSources {
                project: &app.project,
                rules: &app.rules,
                strengthening: Some(text),
            },
            0,
            &mut not_configured,
        )
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
    };
    let clause = |name: &str, formula: &str| {
        format!(
            r#"{{"schema": "zeno-fcis/strengthening/1", "invariants": [{{"name": "{name}", "formula": "{formula}"}}]}}"#
        )
    };
    assert!(read("[]").is_err());
    assert!(read(r#"{"schema": "zeno-fcis/strengthening/2", "invariants": []}"#).is_err());
    assert!(
        read(r#"{"schema": "zeno-fcis/strengthening/1", "invariants": [], "extra": 1}"#).is_err()
    );
    assert!(read(&clause("Upper", "post.100.111 >= 0")).is_err());
    assert!(read(&clause("bad", "post.100.111 >=")).is_err());
    let command = read(&clause("reads_command", "command.101.121 >= 0"));
    assert!(
        command
            .as_ref()
            .is_err_and(|error| error.contains("reads only the state")),
        "{command:?}"
    );
    let repeated = r#"{"schema": "zeno-fcis/strengthening/1", "invariants": [
        {"name": "a", "formula": "post.100.111 >= 0"}, {"name": "a", "formula": "post.100.111 >= 0"}]}"#;
    assert!(read(repeated).is_err());
    assert!(read(&clause("held_is_small", "post.100.111 <= 5")).is_ok());

    // A clause genesis breaks is no base for an induction.
    let checked = app.check(
        Some(&clause("never_created", "post.100.110 != 150")),
        0,
        &mut not_configured,
    );
    assert_eq!(checked.status, Status::Inconclusive);
    assert!(
        checked.lines.iter().any(|line| line
            .contains("strengthening never_created does not hold on the genesis state (violated)")),
        "{:?}",
        checked.lines
    );
}
