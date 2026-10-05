//! Review checks: enumeration, boundary sets, the mutant catalog,
//! classification, agreement with the owner's examples and determinism.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::super::declarations::Declarations;
use super::super::expr;
use super::super::rules::Rules;
use super::domain::{self, Construction};
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

    fn example_inputs(&self) -> Vec<Vec<i64>> {
        let declarations = self.declarations();
        let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
        self.examples
            .as_deref()
            .map(|text| {
                super::examples::parse(text, &positions, &declarations)
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
    let packet = packet(&App::dual_approval().review());
    let refused = find(
        &packet,
        "cases[5].outbox[0].idempotency_ordinal",
        "`0` -> `1`",
    );
    assert_eq!(refused["classification"], "refused-by-library");
    assert_eq!(refused["refusal"]["place"], "library catalog");
    let generator = find(&packet, "cases[2].post.110", "`152` -> `153`");
    assert_eq!(generator["classification"], "refused-by-generator");
    assert_eq!(
        generator["refusal"]["reason"],
        "153 is not a variant of type 105"
    );
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
    // The third failed login now locks until `now + 901`, which law 503
    // refuses; at the latest time the deadline also leaves its domain.
    assert_eq!(disagreements[0]["line"], 21);
    assert_eq!(
        disagreements[0]["difference"],
        "the library refused: Core(Law(Violated))"
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
    let examples = super::examples::parse(text, &positions, &declarations)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(examples.len(), 30);
    // Inputs split over three sections; a payload-only delivery on the only channel.
    assert_eq!(examples[0].inputs.len(), positions.len());
    assert_eq!(examples[0].outbox, vec![(300, vec![1, 183, 184, 2, 2, 3])]);
    let app = App::template("compliance-gateway");
    let declarations = app.declarations();
    let positions = domain::positions(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let text = app
        .examples
        .as_deref()
        .unwrap_or_else(|| panic!("examples"));
    let examples = super::examples::parse(text, &positions, &declarations)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(examples.len(), 28);
    assert_eq!(examples[1].outbox, vec![(300, vec![176, 2])]);
    assert_eq!(examples[2].outbox, vec![(301, vec![170, 1])]);
    let bad = format!("{text}\n0 150 160 1 165 2 0 | accept - 0 | 300 176\n");
    let error = super::examples::parse(&bad, &positions, &declarations)
        .err()
        .unwrap_or_else(|| panic!("a bad delivery is refused"));
    assert!(
        error
            .place()
            .ends_with(&format!("line {}", bad.lines().count()))
    );
    let out = format!("{text}\n9 150 160 1 165 2 0 | accept - 9 | -\n");
    let error = super::examples::parse(&out, &positions, &declarations)
        .err()
        .unwrap_or_else(|| panic!("an input outside its domain is refused"));
    assert_eq!(error.reason(), "`9` is outside the domain of `pre.100.120`");
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
