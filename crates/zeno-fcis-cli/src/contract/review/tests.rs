//! Review checks: enumeration, boundary sets, the mutant catalog,
//! classification, agreement with the owner's examples and determinism.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::super::ContractError;
use super::super::declarations::Declarations;
use super::super::expr;
use super::super::model::Contract;
use super::super::rules::Rules;
use super::super::{policy, schema, schema_commitment};
use super::domain::{self, Construction};
use super::examples::Example;
use super::mutants::{self, OPERATORS};
use super::{DEFAULT_MAX_TUPLES, PACKET_SCHEMA, Review, ReviewSources, review};

/// An application's review inputs.
struct App {
    project: String,
    rules: String,
    examples: Option<String>,
}

impl App {
    fn template(name: &str) -> Self {
        Self::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("templates")
                .join(name),
        )
    }

    fn dual_approval() -> Self {
        Self::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/dual-approval"))
    }

    /// The spend-approval contract of the 2026-10-05 app-building study.
    fn spend_approval() -> Self {
        Self::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spend-approval"))
    }

    fn read(base: PathBuf) -> Self {
        let text = |path: &str| {
            fs::read_to_string(base.join(path))
                .unwrap_or_else(|error| panic!("read {}/{path}: {error}", base.display()))
        };
        Self {
            project: text("project.zeno"),
            rules: text("v2/policy.json"),
            examples: fs::read_to_string(base.join("tests/decision-examples.txt")).ok(),
        }
    }

    fn sources(&self) -> ReviewSources<'_> {
        ReviewSources {
            project: &self.project,
            rules: &self.rules,
            examples: self.examples.as_deref(),
        }
    }

    fn rules(&self) -> Rules {
        Rules::read(&self.rules).unwrap_or_else(|error| panic!("{error}"))
    }

    fn declarations(&self) -> Declarations {
        Declarations::read(&self.project, &self.rules().leaf_bindings)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn review(&self) -> Review {
        review(self.sources(), DEFAULT_MAX_TUPLES).unwrap_or_else(|error| panic!("{error}"))
    }

    /// `text` read as the review reads examples: against the descriptor of
    /// the contract bound through the library.
    fn parse_examples(&self, text: &str) -> Result<Vec<Example>, ContractError> {
        let rules = self.rules();
        let declarations = self.declarations();
        let schema = schema::encode(&declarations)?;
        let contract = Contract::build(&declarations, &rules, schema_commitment(&schema)?)?;
        policy::with_authority(&contract, &schema, |authority| {
            super::examples::parse(text, authority.descriptor())
        })?
    }

    fn example_inputs(&self) -> Vec<Vec<i64>> {
        self.examples
            .as_deref()
            .map(|text| {
                self.parse_examples(text)
                    .unwrap_or_else(|error| panic!("{error}"))
                    .into_iter()
                    .map(|example| example.inputs)
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn packet(review: &Review) -> Value {
    serde_json::from_str(review.packet()).unwrap_or_else(|error| panic!("packet: {error}"))
}

fn mutants(packet: &Value) -> &[Value] {
    packet["mutants"]["list"]
        .as_array()
        .unwrap_or_else(|| panic!("mutant list"))
}

fn find<'p>(packet: &'p Value, site: &str, change: &str) -> &'p Value {
    mutants(packet)
        .iter()
        .find(|mutant| mutant["site"] == site && mutant["change"] == change)
        .unwrap_or_else(|| panic!("mutant {site} {change}"))
}

#[test]
fn a_small_domain_is_enumerated_in_full_in_program_order() {
    let app = App::template("durable-counter");
    let declarations = app.declarations();
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        positions
            .iter()
            .map(|position| position.name.as_str())
            .collect::<Vec<_>>(),
        ["pre.100.110", "pre.100.111", "command.101", "context.102"]
    );
    assert_eq!(domain::domain_size(&positions), Some(64));
    let inputs = domain::input_set(&positions, &app.rules(), &declarations, &[], 64)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(inputs.construction, Construction::FullDomain);
    assert_eq!(inputs.len(), 64);
    let tuples: Vec<&[i64]> = inputs.tuples().collect();
    assert_eq!(tuples[0], [0, 0, 120, 0]);
    assert_eq!(tuples[1], [0, 0, 120, 1]);
    assert_eq!(tuples[2], [0, 0, 121, 0]);
    assert_eq!(tuples[63], [3, 3, 121, 1]);
    // One tuple fewer than the domain falls back to the boundary sets, whose
    // product here is the domain again, so to probes: the genesis base, its
    // 8 single-position variations and its 22 two-position variations.
    let inputs = domain::input_set(&positions, &app.rules(), &declarations, &[], 63)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(inputs.construction, Construction::BoundaryProbes);
    assert_eq!(inputs.len(), 31);
    let boundary = inputs
        .boundary
        .as_ref()
        .unwrap_or_else(|| panic!("boundary"));
    assert_eq!((boundary.singles, boundary.pairs), (8, 22));
    assert!(!boundary.truncated);
    assert_eq!(inputs.tuples().next(), Some(&[0, 0, 120, 0][..]));
    let capped = domain::input_set(&positions, &app.rules(), &declarations, &[], 10)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(capped.len(), 10);
    assert!(
        capped
            .boundary
            .as_ref()
            .is_some_and(|boundary| boundary.truncated)
    );
}

#[test]
fn the_account_lockout_boundary_set_holds_the_lock_constants() {
    let app = App::template("account-lockout");
    let declarations = app.declarations();
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let examples = app.example_inputs();
    let examples: Vec<&[i64]> = examples.iter().map(Vec::as_slice).collect();
    let inputs = domain::input_set(
        &positions,
        &app.rules(),
        &declarations,
        &examples,
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(inputs.construction, Construction::BoundaryProduct);
    let boundary = inputs
        .boundary
        .as_ref()
        .unwrap_or_else(|| panic!("boundary"));
    let values = &boundary.values;
    assert_eq!(values[0], [0, 1, 2], "attempts");
    assert_eq!(
        values[1],
        [0, 1, 899, 900, 901, 4_102_445_700],
        "locked_until"
    );
    assert_eq!(values[2], [0, 1, 899, 900, 901, 4_102_444_800], "last_seen");
    assert_eq!(values[3], [120, 121, 122], "command variants");
    assert_eq!(values[4], values[2], "now shares last_seen's type");
    assert_eq!(values[5], [0, 1], "admin flag");
    assert_eq!(inputs.len(), 3 * 6 * 6 * 3 * 6 * 2);
    // The lock length 900 is a constant of the time it is added to and of
    // the deadline it is assigned to; variant IDs never reach integer leaves.
    assert!(boundary.constants[&106].contains(&900));
    assert!(boundary.constants[&107].contains(&900));
    assert!(!boundary.constants[&106].contains(&121));
}

#[test]
fn a_large_domain_is_probed_from_genesis_and_the_owner_examples() {
    let app = App::template("agent-treasury-guard");
    let declarations = app.declarations();
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let examples = app.example_inputs();
    let examples: Vec<&[i64]> = examples.iter().map(Vec::as_slice).collect();
    let inputs = domain::input_set(
        &positions,
        &app.rules(),
        &declarations,
        &examples,
        DEFAULT_MAX_TUPLES,
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(inputs.construction, Construction::BoundaryProbes);
    let boundary = inputs
        .boundary
        .as_ref()
        .unwrap_or_else(|| panic!("boundary"));
    assert_eq!(boundary.bases[0].0, "genesis");
    assert_eq!(&boundary.bases[0].1[..7], [6, 1, 0, 0, 170, 0, 0]);
    assert!(boundary.bases.len() > 1 && boundary.bases.len() <= examples.len() + 1);
    assert!(!boundary.truncated);
    assert!(boundary.singles > 0 && boundary.pairs > boundary.singles);
    assert!(inputs.len() <= usize::try_from(DEFAULT_MAX_TUPLES).unwrap_or(usize::MAX));
    let distinct: std::collections::BTreeSet<&[i64]> = inputs.tuples().collect();
    assert_eq!(distinct.len(), inputs.len(), "no tuple twice");
    // The limit stops the probes and says so.
    let capped = domain::input_set(&positions, &app.rules(), &declarations, &examples, 100)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(capped.len(), 100);
    assert!(
        capped
            .boundary
            .as_ref()
            .is_some_and(|boundary| boundary.truncated)
    );
}

#[test]
fn the_mutant_catalog_is_fixed_and_numbered_in_catalog_order() {
    let app = App::dual_approval();
    let rules = app.rules();
    let declarations = app.declarations();
    let catalog = mutants::catalog(&rules, &declarations);
    assert_eq!(catalog.len(), 33);
    for (index, mutant) in catalog.iter().enumerate() {
        assert_eq!(mutant.id, format!("m{:04}", index + 1));
    }
    let order: Vec<usize> = catalog
        .iter()
        .map(|mutant| {
            OPERATORS
                .iter()
                .position(|(name, _)| *name == mutant.operator)
                .unwrap_or_else(|| panic!("operator {}", mutant.operator))
        })
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] <= pair[1]));
    for (name, _) in OPERATORS {
        if name == "drop-conjunct" {
            // Dual approval has no `&&`.
            continue;
        }
        assert!(
            catalog.iter().any(|mutant| mutant.operator == name),
            "{name}"
        );
    }
    let again = mutants::catalog(&rules, &declarations);
    let describe = |catalog: &[mutants::Mutant]| {
        catalog
            .iter()
            .map(|mutant| {
                format!(
                    "{} {} {} {}",
                    mutant.id, mutant.operator, mutant.site, mutant.change
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(describe(&catalog), describe(&again));
    let flip = &catalog[0];
    assert_eq!(
        (flip.operator, flip.site.as_str(), flip.change.as_str()),
        (
            "flip-comparison",
            "cases[0].when",
            "`status != 150` -> `status == 150`"
        )
    );
    assert_eq!(
        mutants::render(
            &expr::parse("a && b -> choose(c, 1, 2) >= -d")
                .unwrap_or_else(|error| panic!("{error}"))
        ),
        "(a && b) -> (choose(c, 1, 2) >= -d)"
    );
}

#[test]
fn every_mutant_of_a_fully_enumerated_domain_is_classified() {
    let review = App::dual_approval().review();
    let summary = review.summary();
    assert_eq!(summary.construction, "full-domain");
    assert_eq!(summary.inputs, 384);
    assert_eq!(summary.undistinguished, 0);
    assert_eq!(
        summary.distinguished
            + summary.refused_by_generator
            + summary.refused_by_library
            + summary.equivalent,
        summary.mutants
    );
    let packet = packet(&review);
    assert_eq!(packet["schema"], PACKET_SCHEMA);
    assert_eq!(packet["authority"], "none");
    // Swapping the two disjoint guards `officer == first` and `first == 160`
    // changes no decision anywhere in the domain.
    let swap = find(
        &packet,
        "cases[3],cases[4]",
        "case 4 `first == 160` decides before case 3 `officer == first`",
    );
    assert_eq!(swap["classification"], "equivalent-over-full-domain");
    // A witness not covered by the owner's examples is proposed in their format.
    let swap = find(
        &packet,
        "cases[0],cases[1]",
        "case 1 `officer == 160` decides before case 0 `status != 150`",
    );
    assert_eq!(swap["classification"], "distinguished");
    assert_eq!(swap["witness"]["source"]["kind"], "input-set");
    let proposed = swap["witness"]["proposed_example"]
        .as_str()
        .unwrap_or_else(|| panic!("proposed example"));
    assert!(
        proposed.ends_with("| reject 200 151 160 160 | -"),
        "{proposed}"
    );
    assert!(
        swap["witness"]["mutant_as_example"]
            .as_str()
            .is_some_and(|line| line.contains("| reject 201 ")),
        "{}",
        swap["witness"]
    );
}

#[test]
fn a_mutant_the_library_refuses_is_classified_as_such() {
    let app = App::dual_approval();
    let packet = packet(&app.review());
    // The generated channel's idempotency domain covers every ordinal the
    // rules use, so the library binds this mutant and its decisions judge it.
    let idempotency = find(
        &packet,
        "cases[5].outbox[0].idempotency_ordinal",
        "`0` -> `1`",
    );
    assert_eq!(idempotency["classification"], "distinguished");
    let generator = find(&packet, "cases[2].post.110", "`152` -> `153`");
    assert_eq!(generator["classification"], "refused-by-generator");
    assert_eq!(
        generator["refusal"]["reason"],
        "153 is not a variant of type 105"
    );

    // No mutant of the catalog reaches the library's refusal any more, so the
    // refusals below are constructed from that same mutant.
    let (rules, declarations) = (app.rules(), app.declarations());
    let schema =
        super::super::schema::encode(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let commitment =
        super::super::schema_commitment(&schema).unwrap_or_else(|error| panic!("{error}"));
    let mutant = mutants::catalog(&rules, &declarations)
        .into_iter()
        .find(|mutant| {
            mutant.site == "cases[5].outbox[0].idempotency_ordinal" && mutant.change == "`0` -> `1`"
        })
        .unwrap_or_else(|| panic!("the idempotency mutant"));
    let build = |rules: &Rules| {
        Contract::build(&declarations, rules, commitment).unwrap_or_else(|error| panic!("{error}"))
    };
    // Bound with the channel's idempotency domain fixed at 0, as the generator
    // rendered it before, the library refuses the mutant at its delivery; the
    // refusal names that entry and keeps the library's error.
    let mut fixed = build(&mutant.rules);
    fixed.channels[0].idempotency = 0;
    let Err(refusal) = super::super::policy::with_authority(&fixed, &schema, |_| ()) else {
        panic!("the library must refuse the delivery")
    };
    assert_eq!(refusal.place(), "v2/policy.json cases[5].outbox[0]");
    assert!(
        refusal
            .reason()
            .starts_with("the library catalog refused the generated contract (Descriptor)"),
        "{}",
        refusal.reason()
    );
    // Classification reports the binding's refusal as it is: driven through
    // `classify` with a schema the library refuses, the mutant is
    // refused-by-library with exactly the place and reason of that binding.
    let mut refused_schema = schema.clone();
    refused_schema.push(0);
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let inputs = domain::input_set(&positions, &rules, &declarations, &[], DEFAULT_MAX_TUPLES)
        .unwrap_or_else(|error| panic!("{error}"));
    let original = build(&rules);
    let framer = super::evaluate::Framer::new(&positions, &original);
    let table = super::table::Table::default();
    let context = super::Context {
        declarations: &declarations,
        commitment,
        schema: &refused_schema,
        framer: &framer,
        examples: &[],
        at_examples: &[],
        inputs: &inputs,
        table: &table,
        state_width: declarations
            .state_fields()
            .unwrap_or_else(|error| panic!("{error}"))
            .len(),
    };
    let Err(expected) =
        super::super::policy::with_authority(&build(&mutant.rules), &refused_schema, |_| ())
    else {
        panic!("the library must refuse the schema")
    };
    match super::classify(&mutant, &context) {
        super::Classification::RefusedByLibrary { place, reason } => {
            assert_eq!(
                (place.as_str(), reason.as_str()),
                (expected.place(), expected.reason())
            );
            assert!(reason.contains("Schema("), "{reason}");
        }
        other => panic!("classified {}", other.name()),
    }
}

#[test]
fn a_boundary_set_review_never_claims_equivalence() {
    let review = App::template("account-lockout").review();
    let summary = review.summary();
    assert_eq!(summary.construction, "boundary-product");
    assert_eq!(summary.equivalent, 0);
    assert!(summary.undistinguished > 0);
    let packet = packet(&review);
    for mutant in mutants(&packet) {
        assert_ne!(mutant["classification"], "equivalent-over-full-domain");
    }
    let swap = find(
        &packet,
        "cases[2],cases[3]",
        "case 3 `action == 120` decides before case 2 `(action == 122) && !admin`",
    );
    assert_eq!(
        swap["classification"],
        "not-distinguished-within-boundary-set"
    );
}

#[test]
fn account_lockout_witnesses_sit_at_the_lock_threshold_and_the_deadline() {
    let review = App::template("account-lockout").review();
    assert_eq!(review.summary().disagreements, 0);
    assert_eq!(review.summary().examples, 20);
    let packet = packet(&review);
    for (site, change, example) in [
        // The lock deadline `now + 900`, moved either way, is caught by the
        // owner's example of the third failed login.
        (
            "variables.computed.deadline",
            "`900` -> `901`",
            "2 0 1010 121 1020 0 | failure 203 0 1920 1020 | 300 150 1920",
        ),
        (
            "variables.computed.deadline",
            "`900` -> `899`",
            "2 0 1010 121 1020 0 | failure 203 0 1920 1020 | 300 150 1920",
        ),
        // The lock threshold `failed == 2`, moved to 1 or flipped.
        (
            "cases[4].when",
            "`2` -> `1`",
            "1 0 1000 121 1010 0 | failure 203 2 0 1010 | -",
        ),
        (
            "cases[4].when",
            "`failed == 2` -> `failed != 2`",
            "0 0 0 121 1000 0 | failure 203 1 0 1000 | -",
        ),
        // `now < until` at its boundary: the example where the lock has just expired.
        (
            "cases[1].when",
            "`now < until` -> `now <= until`",
            "0 1920 1020 120 1920 0 | accept - 0 1920 1920 | -",
        ),
    ] {
        let mutant = find(&packet, site, change);
        assert_eq!(mutant["classification"], "distinguished", "{site} {change}");
        let witness = &mutant["witness"];
        assert_eq!(
            witness["source"]["kind"], "owner-example",
            "{site} {change}"
        );
        assert_eq!(witness["proposed_example"], example, "{site} {change}");
        assert_eq!(witness["owner_agrees_with"], "original", "{site} {change}");
    }
}

#[test]
fn a_planted_wrong_constant_contradicts_an_owner_example() {
    let mut app = App::template("account-lockout");
    assert_eq!(app.rules.matches("\"now + 900\"").count(), 1);
    app.rules = app.rules.replace("\"now + 900\"", "\"now + 901\"");
    let review = app.review();
    let summary = review.summary();
    assert!(summary.disagreements > 0);
    let packet = packet(&review);
    let disagreements = packet["examples"]["disagreements"]
        .as_array()
        .unwrap_or_else(|| panic!("disagreements"));
    assert_eq!(disagreements.len(), summary.disagreements);
    // The third failed login now locks until `now + 901`. Law 500, the
    // first law in the contract's order to refuse it, bounds the lock by
    // `last_seen + 900`; at the latest time the deadline also leaves its
    // domain.
    assert_eq!(disagreements[0]["line"], 21);
    assert_eq!(
        disagreements[0]["difference"],
        "the library refused: Core(Law(Violated)) by law 500"
    );
    assert!(disagreements.iter().any(|disagreement| {
        disagreement["line"] == 34
            && disagreement["difference"] == "the library refused: Core(Decision(Domain))"
    }));
    // The mutant that restores 900 is told apart by that example, and the
    // owner's outcome is the mutant's, not the contract's.
    let restored = find(&packet, "variables.computed.deadline", "`901` -> `900`");
    assert_eq!(restored["classification"], "distinguished");
    assert_eq!(restored["witness"]["source"]["line"], 21);
    assert_eq!(restored["witness"]["owner_agrees_with"], "mutant");
    assert!(
        packet["findings"]
            .as_array()
            .unwrap_or_else(|| panic!("findings"))
            .iter()
            .any(|finding| {
                finding["kind"] == "example-agrees-with-mutant"
                    && finding["mutant"] == restored["id"]
                    && finding["line"] == 21
            }),
        "{}",
        packet["findings"]
    );
    // The same lock, from pre-states that satisfy state law 500, is a law
    // refusal finding of its own; the deadline beyond its domain is a domain
    // refusal, which is counted but is not a law refusal.
    assert_eq!(summary.law_refusals, 1);
    let found: Vec<&Value> = packet["findings"]
        .as_array()
        .unwrap_or_else(|| panic!("findings"))
        .iter()
        .filter(|finding| finding["kind"] == "law-refusal-on-law-consistent-state")
        .collect();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["law"], 500);
    assert_eq!(found[0]["refusal"], "Core(Law(Violated))");
    assert_eq!(found[0]["first"]["input"][0], "2", "{}", found[0]);
    let classes: Vec<(&Value, &Value)> = packet["decision_table"]["refusals"]
        .as_array()
        .unwrap_or_else(|| panic!("refusals"))
        .iter()
        .map(|refusal| (&refusal["refusal"], &refusal["class"]))
        .collect();
    assert!(classes.contains(&(&json!("Core(Law(Violated))"), &json!("law"))));
    assert!(classes.contains(&(&json!("Core(Decision(Domain))"), &json!("domain"))));
}

/// The study's planted bug: the CFO check of "execute: tier 1 and above need
/// the CFO" moved from tier 1 to tier 2.
const CFO_CHECK: &str = "\"action == 172 && tier >= 1 && !cfo_ok\"";

#[test]
fn a_law_refusal_on_a_pre_state_that_satisfies_every_state_law_is_a_finding() {
    let review = App::spend_approval().review();
    let summary = review.summary();
    assert_eq!(
        (
            summary.disagreements,
            summary.law_refusals,
            summary.findings
        ),
        (0, 0, 0)
    );
    let unchanged = packet(&review);
    assert_eq!(unchanged["refusals"]["state_laws"], json!([500]));
    assert_eq!(unchanged["refusals"]["counts"]["count"], 0);

    let mut planted = App::spend_approval();
    assert_eq!(planted.rules.matches(CFO_CHECK).count(), 1);
    planted.rules = planted
        .rules
        .replace(CFO_CHECK, &CFO_CHECK.replace("tier >= 1", "tier >= 2"));
    let review = planted.review();
    let summary = review.summary();
    // No owner example covers a tier 1 execution without the CFO.
    assert_eq!(
        (
            summary.disagreements,
            summary.law_refusals,
            summary.findings
        ),
        (0, 1, 1)
    );
    let packet = packet(&review);
    let counts = &packet["refusals"]["counts"];
    assert_eq!(counts["count"], 16);
    assert_eq!(counts["by_class"], json!({"law": 16}));
    assert_eq!(
        counts["on_law_consistent_states"],
        json!({"count": 16, "by_class": {"law": 16}})
    );
    assert_eq!(counts["on_states_the_laws_exclude"]["count"], 0);
    // The library's law diagnostics name the refusing law.
    assert_eq!(
        packet["decision_table"]["refusals"],
        json!([{"refusal": "Core(Law(Violated))", "class": "law", "law": 500}])
    );
    let finding = &packet["findings"][0];
    assert_eq!(finding["kind"], "law-refusal-on-law-consistent-state");
    assert_eq!(finding["law"], 500);
    assert_eq!(finding["inputs"], 16);
    // A pending tier 1 request without the CFO's approval, executed by the
    // clerk within the limit, reaches the payment case; law 500 refuses an
    // executed tier 1 request without the CFO.
    assert_eq!(
        finding["first"]["input"],
        json!(["151", "1", "0", "0", "172", "0", "161", "0", "1"])
    );
    assert_eq!(finding["first"]["case"], 16);
    assert_eq!(finding["first"]["case_when"], "action == 172");
    let refused: Vec<&str> = packet["decision_table"]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows"))
        .iter()
        .filter_map(Value::as_str)
        .filter(|row| row.contains("| refused"))
        .collect();
    assert_eq!(refused.len(), 16);
    assert!(refused.iter().all(|row| row.ends_with("| refused 0")));
    assert_eq!(
        review.law_refusal_lines(),
        [
            "law refusal: law 500 refuses 16 inputs whose pre-state satisfies every state law (Core(Law(Violated))); first 151 1 0 0 172 0 161 0 1, decided by cases[16] `action == 172`"
        ]
    );
}

#[test]
fn refusals_on_pre_states_the_state_laws_exclude_are_reported_apart() {
    let review = App::template("order-fulfillment").review();
    let summary = review.summary();
    assert_eq!((summary.law_refusals, summary.findings), (0, 0));
    assert!(review.law_refusal_lines().is_empty());
    let packet = packet(&review);
    let refusals = &packet["refusals"];
    assert_eq!(refusals["state_laws"], json!([500]));
    assert_eq!(
        refusals["counts"],
        json!({
            "count": 9,
            "by_class": {"law": 9},
            "on_law_consistent_states": {"count": 0, "by_class": {}},
            "on_states_the_laws_exclude": {"count": 9, "by_class": {"law": 9}},
        })
    );
    assert_eq!(refusals["by_unsatisfied_state_law"], json!({"500": 9}));
    // Law 500 asks an order awaiting payment for at least one payment
    // attempt; these pre-states have none, so no committed state is one.
    let group = &refusals["groups"][0];
    assert_eq!(group["unsatisfied_state_law"], 500);
    assert_eq!(group["law"], 500);
    assert_eq!(
        (&group["first"]["input"][0], &group["first"]["input"][1]),
        (&json!("161"), &json!("0"))
    );
    let refused: Vec<&str> = packet["decision_table"]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("rows"))
        .iter()
        .filter_map(Value::as_str)
        .filter(|row| row.contains("| refused"))
        .collect();
    assert_eq!(refused.len(), 9);
    assert!(
        refused
            .iter()
            .all(|row| row.ends_with("| refused 0 unsatisfied 500"))
    );
}

#[test]
fn the_decision_table_agrees_with_every_owner_example() {
    for (name, examples, inputs) in [
        ("durable-counter", 12, 64),
        ("prepared-counter", 0, 216),
        ("inventory-reservation", 20, 864),
        ("order-fulfillment", 23, 1728),
    ] {
        let review = App::template(name).review();
        let summary = review.summary();
        assert_eq!(summary.construction, "full-domain", "{name}");
        assert_eq!(summary.inputs, inputs, "{name}");
        assert_eq!(summary.examples, examples, "{name}");
        assert_eq!(summary.disagreements, 0, "{name}");
        assert_eq!(summary.undistinguished, 0, "{name}");
        assert_eq!(summary.findings, 0, "{name}");
    }
    let review = App::dual_approval().review();
    assert_eq!(review.summary().examples, 12);
    assert_eq!(review.summary().disagreements, 0);
}

#[test]
fn examples_parse_in_both_delivery_forms_and_name_a_bad_line() {
    let app = App::template("agent-treasury-guard");
    let declarations = app.declarations();
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let text = app
        .examples
        .as_deref()
        .unwrap_or_else(|| panic!("examples"));
    let examples = app
        .parse_examples(text)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(examples.len(), 30);
    // Inputs split over three sections; a payload-only delivery on the only channel.
    assert_eq!(examples[0].inputs.len(), positions.len());
    assert_eq!(examples[0].outbox, vec![(300, vec![1, 183, 184, 2, 2, 3])]);
    let app = App::template("compliance-gateway");
    let text = app
        .examples
        .as_deref()
        .unwrap_or_else(|| panic!("examples"));
    let examples = app
        .parse_examples(text)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(examples.len(), 28);
    assert_eq!(examples[1].outbox, vec![(300, vec![176, 2])]);
    assert_eq!(examples[2].outbox, vec![(301, vec![170, 1])]);
    let bad = format!("{text}\n0 150 160 1 165 2 0 | accept - 0 | 300 176\n");
    let error = app
        .parse_examples(&bad)
        .err()
        .unwrap_or_else(|| panic!("a bad delivery is refused"));
    assert!(
        error
            .place()
            .ends_with(&format!("line {}", bad.lines().count()))
    );
    let out = format!("{text}\n9 150 160 1 165 2 0 | accept - 9 | -\n");
    let error = app
        .parse_examples(&out)
        .err()
        .unwrap_or_else(|| panic!("an input outside its domain is refused"));
    assert_eq!(
        error.reason(),
        "`9` is outside the domain of state field 120"
    );
}

/// What an example says, without the text it was written as.
fn meaning(examples: &[Example]) -> Vec<Example> {
    examples
        .iter()
        .map(|example| Example {
            text: String::new(),
            ..example.clone()
        })
        .collect()
}

#[test]
fn the_study_example_forms_parse_alike_and_malformed_lines_are_refused() {
    // Dual approval: three state fields, a scalar command, one context
    // field, and one channel whose payload has two fields.
    let app = App::dual_approval();
    let text = app.examples.clone().unwrap_or_else(|| panic!("examples"));
    let written = app
        .parse_examples(&text)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(written.len(), 12);
    // The three forms the study found read differently by the review and
    // the application: inputs split over sections, payload-only deliveries
    // on the only channel, and indented comments.
    let rewritten: String = text
        .lines()
        .map(|line| {
            if line.starts_with('#') {
                return format!(" \t{line}\n");
            }
            let sections: Vec<&str> = line.split('|').map(str::trim).collect();
            let inputs: Vec<&str> = sections[0].split_whitespace().collect();
            let deliveries = sections[2].strip_prefix("300 ").unwrap_or(sections[2]);
            format!(
                "{} | {} | {} | {} | {deliveries}\n",
                inputs[..3].join(" "),
                inputs[3],
                inputs[4],
                sections[1]
            )
        })
        .collect();
    assert!(rewritten.contains("150 161 160 | 140 | 162 | accept - 151 161 162 | 162 161"));
    let reread = app
        .parse_examples(&rewritten)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(meaning(&reread), meaning(&written));
    // Malformed lines, each refused with the line and the reason.
    for (line, reason) in [
        (
            "150 161 160 140 162 | accept - 151 161 162",
            "expected `inputs | class reason post | deliveries`",
        ),
        (
            "150 161 160 140 | accept - 151 161 162 | -",
            "4 input numbers; the contract reads 5",
        ),
        (
            "150 161 160 142 162 | accept - 151 161 162 | -",
            "`142` is outside the domain of command",
        ),
        (
            "150 161 160 140 162 | approve - 151 161 162 | -",
            "the decision starts with accept, reject or failure",
        ),
        (
            "150 161 160 140 162 | accept x 151 161 162 | -",
            "`x` is not a reason",
        ),
        (
            "150 161 160 140 162 | accept - 151 161 | -",
            "2 post-state numbers; the state has 3 fields",
        ),
        (
            "150 161 160 140 162 | accept - 153 161 162 | -",
            "`153` is outside the domain of post-state field 110",
        ),
        (
            "150 161 160 140 162 | accept - 151 161 162 | 301 162 161",
            "a delivery is a declared channel followed by its payload numbers, or the payload numbers alone when one channel is declared",
        ),
        (
            "150 161 160 140 162 | accept - 151 161 162 | 300 162 161;",
            "a delivery is a declared channel followed by its payload numbers, or the payload numbers alone when one channel is declared",
        ),
        (
            "150 161 160 140 162 | accept - 151 161 162 | 300 162 164",
            "`164` is outside the domain of channel 300 field 131",
        ),
        (
            "150 161 160 140 162 | accept - 151 161 162 # trailing note | -",
            "`#` is not a number",
        ),
    ] {
        let error = app
            .parse_examples(&format!("# a comment\n\n{line}\n"))
            .err()
            .unwrap_or_else(|| panic!("{line} is refused"));
        assert_eq!(
            (error.place(), error.reason()),
            ("tests/decision-examples.txt line 3", reason),
            "{line}"
        );
    }
    // With two channels declared, a delivery names its channel.
    let gateway = App::template("compliance-gateway");
    let error = gateway
        .parse_examples("0 150 161 2 165 2 0 | accept - 0 | 176 2\n")
        .err()
        .unwrap_or_else(|| panic!("a payload-only delivery is refused"));
    assert_eq!(
        error.reason(),
        "a delivery is a declared channel followed by its payload numbers, or the payload numbers alone when one channel is declared"
    );
}

#[test]
fn packets_are_byte_identical_on_repeat() {
    let app = App::template("durable-counter");
    let first = app.review();
    let second = app.review();
    assert_eq!(first.packet(), second.packet());
    assert!(first.packet().ends_with("}\n"));
    let packet = packet(&first);
    assert_eq!(packet["decision_table"]["count"], 64);
    assert_eq!(packet["decision_table"]["rows_omitted"], false);
    assert_eq!(
        packet["decision_table"]["rows"][0],
        "0 0 120 0 | Reject 200 0 0"
    );
    assert_eq!(packet["inputs"]["construction"], "full-domain");
    assert_eq!(packet["examples"]["file"], "tests/decision-examples.txt");
}

#[test]
fn every_template_examples_file_reads_as_it_did_before_the_shared_grammar() {
    // Each count and SHA-256 was recorded with the review's own parser,
    // before the grammar shared with the applications replaced it, over one
    // line per example: `line inputs class reason post outbox`, the lists in
    // Rust's debug form.
    for (name, count, digest) in [
        (
            "account-lockout",
            20,
            "6ba24a33a0af574bcb117cac023ac8a81f456a01c0a852426e4b02aea59e398d",
        ),
        (
            "agent-treasury-guard",
            30,
            "5b44fe7b3375d6c3000c43584c007cad5f03795ddef7f7053c8aef72ce6d5ba8",
        ),
        (
            "compliance-gateway",
            28,
            "7d8023f39df7e26164638e3278b63061d2445aacdb44b6b3fc468c2667590b28",
        ),
        (
            "durable-counter",
            12,
            "969b3741aae0f6d309c132c39aeb3f6b0670a043374f04b79b7b26fcdb7bec44",
        ),
        (
            "inventory-reservation",
            20,
            "ca5707742a88826188424434718bd19c7544f76cc50619f3c5fcb9213c13a7af",
        ),
        (
            "order-fulfillment",
            23,
            "7412c3a48dc7e19640b3a63b4d9b6110418c491e28b565377001752a42163fb7",
        ),
        (
            "withdrawal-queue",
            26,
            "140fcaa2b9bc97fec4962797c4906c3c87b46516666df80ebcf032df7af85249",
        ),
        (
            "dual-approval",
            12,
            "dd2442de6e541755dae1d2fcf14bccf0e52fa2cd16d91b5caae055309406b593",
        ),
    ] {
        let app = if name == "dual-approval" {
            App::dual_approval()
        } else {
            App::template(name)
        };
        let text = app
            .examples
            .as_deref()
            .unwrap_or_else(|| panic!("{name} examples"));
        let examples = app
            .parse_examples(text)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let rendered: String = examples
            .iter()
            .map(|example| {
                format!(
                    "{} {:?} {} {:?} {:?} {:?}\n",
                    example.line,
                    example.inputs,
                    example.class.name(),
                    example.reason,
                    example.post,
                    example.outbox
                )
            })
            .collect();
        assert_eq!(examples.len(), count, "{name}");
        assert_eq!(
            super::evaluate::hex(&super::evaluate::digest(rendered.as_bytes())),
            digest,
            "{name}"
        );
    }
    // The eighth template keeps no examples file.
    assert!(App::template("prepared-counter").examples.is_none());
}
