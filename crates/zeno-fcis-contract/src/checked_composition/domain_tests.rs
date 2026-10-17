//! The composed-domain manifest's closed form and scopes: every refusal of
//! the profile, and the stateless-component answer for `/1` and `/3`.

use std::fs;
use std::path::PathBuf;

use serde_json::{Value, json};

use super::leaf::{self, sha256};
use super::manifest::Manifest;
use super::{DomainSources, JointComposition, LeafSources, Refusal, Verdict, check_domain, hex};
use crate::internals::declarations::Declarations;
use crate::internals::model::Contract;
use crate::internals::policy;
use crate::internals::rules::Rules;
use crate::internals::{schema, schema_commitment};

pub(super) fn fail<T>(error: impl std::fmt::Debug) -> T {
    panic!("test operation failed: {error:?}")
}

pub(super) fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/checked-composition-3")
}

/// A component directory's two files, relative to the fixtures.
pub(super) fn files(directory: &str) -> LeafSources {
    let read =
        |name: &str| fs::read_to_string(fixtures().join(directory).join(name)).unwrap_or_else(fail);
    LeafSources {
        project: read("project.zeno"),
        rules: read("v2/policy.json"),
    }
}

/// The three pins of a component's files, through the checker's own route.
pub(super) fn pins(sources: &LeafSources) -> [String; 3] {
    let rules = Rules::read(&sources.rules).unwrap_or_else(fail);
    let declarations =
        Declarations::read(&sources.project, &rules.leaf_bindings).unwrap_or_else(fail);
    let schema = schema::encode(&declarations).unwrap_or_else(fail);
    let commitment = schema_commitment(&schema).unwrap_or_else(fail);
    let contract = Contract::build(&declarations, &rules, commitment).unwrap_or_else(fail);
    let program =
        policy::with_authority(&contract, &schema, |authority| sha256(authority.identity()))
            .unwrap_or_else(fail);
    [
        hex(&sha256(sources.project.as_bytes())),
        hex(&sha256(sources.rules.as_bytes())),
        hex(&program),
    ]
}

/// One component of a test manifest: its entry and its files.
pub(super) struct Part {
    pub(super) entry: Value,
    pub(super) sources: LeafSources,
}

/// A component named `name` from the fixture directory `directory`,
/// owning `owns` (bare field names; empty for read-only).
pub(super) fn part(
    name: &str,
    directory: &str,
    owns: &[&str],
    participates: &str,
    bind: Value,
    assume: &str,
    invariant: &str,
) -> Part {
    let sources = files(directory);
    let [project, policy, program] = pins(&sources);
    Part {
        entry: json!({
            "name": name, "path": format!("{name}-{}", directory.replace('/', "-")),
            "project_sha256": project, "policy_sha256": policy, "program_sha256": program,
            "owns": owns.iter().map(|field| format!("{name}.{field}")).collect::<Vec<_>>(),
            "participates": participates, "bind": bind, "assume": assume, "invariant": invariant,
        }),
        sources,
    }
}

/// The counter (bounded counter, C = 2) incrementing when `op == 0`.
pub(super) fn counter() -> Part {
    part(
        "counter",
        "faults/counter",
        &["value"],
        "op == 0",
        json!({"action": "150", "authorized": "authorized"}),
        "true",
        "true",
    )
}

/// The budget (consumable budget, C = 2) spending `qty` when `op != 2`.
pub(super) fn budget() -> Part {
    part(
        "budget",
        "faults/budget",
        &["remaining"],
        "op != 2",
        json!({"amount": "qty", "authorized": "authorized"}),
        "amount >= 1",
        "true",
    )
}

pub(super) fn request() -> Value {
    json!([
        {"name": "op", "domain": {"kind": "int", "min": 0, "max": 2}},
        {"name": "qty", "domain": {"kind": "int", "min": 1, "max": 2}},
        {"name": "authorized", "domain": {"kind": "bool"}}
    ])
}

/// A whole manifest and its sources.
pub(super) struct Built {
    pub(super) manifest: Value,
    pub(super) sources: Vec<LeafSources>,
}

impl Built {
    pub(super) fn new(parts: Vec<Part>, couplings: Value) -> Self {
        let (entries, sources): (Vec<Value>, Vec<LeafSources>) = parts
            .into_iter()
            .map(|part| (part.entry, part.sources))
            .unzip();
        Self {
            manifest: json!({
                "schema": "zeno-fcis/checked-composition/3",
                "checker": "zeno-fcis/composed-domain-checker/1",
                "name": "test",
                "request": request(),
                "components": entries,
                "couplings": couplings,
            }),
            sources,
        }
    }

    pub(super) fn text(&self) -> String {
        serde_json::to_string_pretty(&self.manifest).unwrap_or_else(fail)
    }

    pub(super) fn check(&self) -> Result<super::DomainResult, Refusal> {
        let text = self.text();
        check_domain(&DomainSources {
            manifest: &text,
            components: &self.sources,
        })
    }

    pub(super) fn refusal(&self) -> Refusal {
        match self.check() {
            Ok(result) => panic!("expected a refusal, got {}", result.json()),
            Err(refusal) => refusal,
        }
    }
}

fn refused_with(built: &Built, obligation: &str, words: &str) {
    let refusal = built.refusal();
    let json = refusal.json();
    assert_eq!(json["obligation"], obligation, "{refusal}");
    assert!(refusal.to_string().contains(words), "{refusal}");
}

fn pair() -> Built {
    Built::new(vec![counter(), budget()], json!([]))
}

#[test]
fn a_well_formed_pair_is_checked() {
    let result = pair().check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
}

#[test]
fn duplicate_component_names_are_refused() {
    let mut built = pair();
    built.manifest["components"][1]["name"] = json!("counter");
    built.manifest["components"][1]["owns"] = json!(["counter.remaining"]);
    refused_with(
        &built,
        "closed manifest form",
        "two components are named `counter`",
    );
}

#[test]
fn a_binding_that_reads_a_later_component_is_refused() {
    let mut built = pair();
    built.manifest["components"][0]["bind"]["action"] =
        json!("choose(budget.remaining == 0, 151, 150)");
    refused_with(
        &built,
        "closed manifest form",
        "never of a later one or of the component itself",
    );
}

#[test]
fn a_binding_that_reads_the_component_itself_is_refused() {
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("budget.remaining");
    refused_with(
        &built,
        "closed manifest form",
        "never of a later one or of the component itself",
    );
}

#[test]
fn a_participation_formula_that_reads_a_later_component_is_refused() {
    let mut built = pair();
    built.manifest["components"][0]["participates"] = json!("budget.remaining > 0");
    refused_with(&built, "closed manifest form", "earlier components only");
}

#[test]
fn unknown_fields_are_refused() {
    let mut built = pair();
    built.manifest["components"][1]["bind"]["price"] = json!("1");
    refused_with(
        &built,
        "closed manifest form",
        "`price` is not one of its command or context fields",
    );
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("counter.volume");
    refused_with(
        &built,
        "closed manifest form",
        "not a state field of `counter`",
    );
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("quantity");
    refused_with(
        &built,
        "closed manifest form",
        "neither a request field nor",
    );
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("ledger.balance");
    refused_with(
        &built,
        "closed manifest form",
        "no component is named `ledger`",
    );
    let mut built = pair();
    built.manifest["components"][1]["bind"]
        .as_object_mut()
        .unwrap_or_else(|| fail("bind"))
        .remove("amount");
    refused_with(
        &built,
        "closed manifest form",
        "binds no expression to its field `amount`",
    );
}

#[test]
fn assumptions_and_invariants_out_of_scope_are_refused() {
    let mut built = pair();
    built.manifest["components"][1]["assume"] = json!("remaining >= 1");
    refused_with(
        &built,
        "closed manifest form",
        "not one of its own command or context fields",
    );
    let mut built = pair();
    built.manifest["components"][1]["invariant"] = json!("amount <= 2");
    refused_with(
        &built,
        "closed manifest form",
        "not one of its own state fields",
    );
    let mut built = pair();
    built.manifest["components"][1]["invariant"] = json!("counter.value <= 2");
    refused_with(
        &built,
        "closed manifest form",
        "not one of its own state fields",
    );
}

#[test]
fn a_coupling_that_reads_a_non_member_is_refused() {
    let built = Built::new(
        vec![counter(), budget()],
        json!([{"name": "k", "members": ["counter"], "invariant": "counter.value + budget.remaining <= 4"}]),
    );
    refused_with(
        &built,
        "closed manifest form",
        "not a state field of one of its members",
    );
    let built = Built::new(
        vec![counter(), budget()],
        json!([{"name": "k", "members": ["budget", "counter"], "invariant": "true"}]),
    );
    refused_with(&built, "closed manifest form", "in merge order");
}

#[test]
fn two_components_owning_one_field_are_refused() {
    let mut built = pair();
    built.manifest["components"][1]["owns"] = json!(["budget.remaining", "counter.value"]);
    refused_with(
        &built,
        "disjoint ownership",
        "two components own the field `counter.value`",
    );
    let mut built = pair();
    built.manifest["components"][1]["owns"] = json!(["counter.value"]);
    refused_with(&built, "disjoint ownership", "two components own the field");
}

#[test]
fn a_component_that_owns_another_components_field_is_refused() {
    let mut built = pair();
    built.manifest["components"][0]["owns"] = json!([]);
    built.manifest["components"][0]["bind"]["value"] = json!("0");
    built.manifest["components"][1]["owns"] = json!(["counter.value"]);
    refused_with(&built, "disjoint ownership", "not one of its own fields");
}

#[test]
fn unknown_keys_duplicate_keys_and_non_canonical_values_are_refused() {
    let mut built = pair();
    built.manifest["groups"] = json!([]);
    refused_with(&built, "closed manifest form", "unknown field `groups`");
    let mut built = pair();
    built.manifest["components"][0]["footprint"] = json!({});
    refused_with(&built, "closed manifest form", "unknown field `footprint`");
    // A repeated key, in a component and in a binding object.
    let original = pair();
    let text = original.text();
    let check = |text: &str| {
        check_domain(&DomainSources {
            manifest: text,
            components: &original.sources,
        })
        .err()
        .unwrap_or_else(|| fail("a repeated key must be refused"))
    };
    let repeated = text.replacen(
        "\"assume\": \"true\"",
        "\"assume\": \"true\", \"assume\": \"true\"",
        1,
    );
    let refusal = check(&repeated);
    assert!(
        refusal.to_string().contains("duplicate field `assume`"),
        "{refusal}"
    );
    let repeated = text.replacen(
        "\"action\": \"150\"",
        "\"action\": \"150\", \"action\": \"151\"",
        1,
    );
    let refusal = check(&repeated);
    assert!(
        refusal.to_string().contains("duplicate key `action`"),
        "{refusal}"
    );
    // Non-canonical values.
    let mut built = pair();
    built.manifest["components"][0]["invariant"] = json!("true ");
    refused_with(
        &built,
        "closed manifest form",
        "without leading or trailing space",
    );
    let mut built = pair();
    let pin = built.manifest["components"][0]["project_sha256"]
        .as_str()
        .unwrap_or_else(|| fail("pin"))
        .to_uppercase();
    built.manifest["components"][0]["project_sha256"] = json!(pin);
    refused_with(
        &built,
        "closed manifest form",
        "64 lowercase hexadecimal digits",
    );
    let mut built = Built::new(
        vec![counter(), budget()],
        json!([
            {"name": "m", "members": ["counter"], "invariant": "true"},
            {"name": "k", "members": ["budget"], "invariant": "true"}
        ]),
    );
    refused_with(&built, "closed manifest form", "increasing order");
    built.manifest["couplings"] = json!([]);
    built.manifest["limits"] = json!({"max_obligation": 1u64 << 23, "max_total": 10});
    refused_with(
        &built,
        "closed manifest form",
        "may only lower the ceilings",
    );
    let mut built = pair();
    built.manifest["request"][0]["domain"] = json!({"kind": "int", "min": 3, "max": 1});
    refused_with(&built, "closed manifest form", "a <= b");
}

fn guard(mark: &str) -> Part {
    part(
        "guard",
        "faults/guard",
        &[],
        "true",
        json!({"mark": mark, "request": "0", "authorized": "authorized"}),
        "true",
        "mark == 0",
    )
}

#[test]
fn read_only_bindings_read_earlier_owned_state_only() {
    let built = Built::new(vec![counter(), guard("op - op")], json!([]));
    refused_with(
        &built,
        "closed manifest form",
        "a read-only state binding reads earlier components' state only",
    );
    // A later component may not read a read-only component.
    let mut late = budget();
    late.entry["bind"]["amount"] = json!("choose(guard.mark == 0, 1, 2)");
    let built = Built::new(vec![guard("0"), late], json!([]));
    refused_with(
        &built,
        "closed manifest form",
        "owns no state; only owned state can be read",
    );
    // Owning part of the state is refused.
    let partial = part(
        "wire",
        "faults/tripwire",
        &["tripped"],
        "true",
        json!({"fire": "false", "authorized": "authorized"}),
        "true",
        "true",
    );
    let built = Built::new(vec![partial], json!([]));
    refused_with(
        &built,
        "complete footprint",
        "every one of its state fields, or none",
    );
}

/// `/3` accepts a read-only guard: one whose single state field is bound to
/// a constant and never changes.
#[test]
fn version_three_accepts_a_read_only_guard() {
    let result = Built::new(vec![guard("0"), budget()], json!([]))
        .check()
        .unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
    assert_eq!(result.json()["components"][0]["state"], "read-only");
}

/// `/1` and `/2` refuse a leaf that owns no state: their manifests require
/// at least one owned field. This pins that refusal; `/3` differs.
#[test]
fn version_one_refuses_a_leaf_that_owns_no_state() {
    let text = fs::read_to_string(fixtures().join("guarded/v1-guard.json")).unwrap_or_else(fail);
    let refusal = Manifest::read(&text)
        .err()
        .unwrap_or_else(|| fail("a /1 leaf without state must be refused"));
    assert_eq!(refusal.json()["obligation"], "closed manifest form");
    assert_eq!(refusal.place(), "leaves[0].owns");
    assert_eq!(
        refusal.reason(),
        "must list each owned state field once, and at least one"
    );
}

/// A contract with zero state fields has no route to an Authority: the
/// contract compiler refuses an empty state record.
#[test]
fn a_contract_with_zero_state_fields_is_refused_by_the_contract_route() {
    let guard = files("contracts/guard");
    let no_field = guard
        .project
        .replace("field 110 100 mark 105;\n", "")
        .replace(
            "law 500 unmarked on commit, genesis = post.100.110 == 0;\n",
            "",
        );
    let rules = guard
        .rules
        .replace("\"genesis\": {\"110\": 0}", "\"genesis\": {}")
        .replace(
            "\"law_kinds\": {\"500\": \"StateInvariant\"}",
            "\"law_kinds\": {}",
        );
    let pinned = |text: &str| sha256(text.as_bytes());
    let refusal = leaf::bind(
        "component `guard`",
        false,
        &pinned(&no_field),
        &pinned(&rules),
        &no_field,
        &rules,
    )
    .err()
    .unwrap_or_else(|| fail("an empty state record must be refused"));
    assert_eq!(refusal.json()["obligation"], "ordinary contract route");
    assert_eq!(
        refusal.reason(),
        "needs fields, variants, a declared range or a leaf binding"
    );
    assert_eq!(refusal.place(), "component `guard` project.zeno type 100");
}

/// A binding is a rule expression: no multiplication or division.
#[test]
fn bindings_without_multiplication_or_division() {
    for expression in [
        "qty * 1",
        "div_floor(qty, 1)",
        "div_ceil(qty, 1)",
        "mul_div_floor(qty, 1, 1)",
        "mul_div_ceil(qty, 1, 1)",
        "products_ge(qty, 1, qty, 1)",
        "products_ge(qty, 1, 1, qty, 1, 1)",
        "choose(true, qty, qty * 1)",
    ] {
        let mut built = Built::new(vec![counter(), budget()], json!([]));
        built.manifest["components"][1]["bind"]["amount"] = json!(expression);
        refused_with(
            &built,
            "defined predicate",
            "composed-domain bindings do not support",
        );
    }
}

/// A flat chain of 200,000 conjuncts is refused, typed, at intake: before
/// a deep tree exists or anything is planned.
#[test]
fn a_long_flat_formula_is_refused_at_intake() {
    let mut chain = "true && ".repeat(199_999);
    chain.push_str("true");
    let mut built = pair();
    built.manifest["components"][0]["participates"] = json!(chain);
    refused_with(
        &built,
        "closed manifest form",
        "bytes long; a formula has at most 4096",
    );
    let text = built.text();
    let refusal = JointComposition::from_sources(&DomainSources {
        manifest: &text,
        components: &built.sources,
    })
    .err()
    .unwrap_or_else(|| fail("refused"));
    assert!(refusal.reason().contains("bytes long"), "{refusal}");
}

/// Within the byte limit, a chain deeper than the depth limit is refused,
/// and so is nesting deeper than the parser reads.
#[test]
fn a_deep_formula_within_the_byte_limit_is_refused() {
    let mut chain = "qty && ".repeat(300);
    chain.push_str("qty");
    let mut built = pair();
    built.manifest["components"][0]["participates"] = json!(chain);
    refused_with(
        &built,
        "closed manifest form",
        "has 601 nodes nested 301 deep",
    );
    let nested = format!("{}true{}", "(".repeat(100), ")".repeat(100));
    let mut built = pair();
    built.manifest["components"][0]["participates"] = json!(nested);
    refused_with(&built, "defined predicate", "nests too deeply");
}

fn boolean_guard(directory: &str) -> Built {
    let mut built = Built::new(
        vec![part(
            "guard",
            directory,
            &[],
            "true",
            json!({"mark": "false", "request": "false", "authorized": "authorized"}),
            "true",
            "mark == 0",
        )],
        json!([]),
    );
    built.manifest["request"] = json!([{"name": "authorized", "domain": {"kind": "bool"}}]);
    built
}

/// A component whose fields are all boolean binds constants and variables
/// without any integer declaration of its own; one that also declares an
/// unused integer behaves the same.
#[test]
fn a_boolean_only_component_is_checked_without_an_integer_declaration() {
    for directory in ["boolean/only/guard", "boolean/unused-int/guard"] {
        let result = boolean_guard(directory).check().unwrap_or_else(fail);
        assert_eq!(
            result.verdict(),
            Verdict::Proved,
            "{directory}: {}",
            result.json()
        );
    }
}

/// Numeric bindings keep their numbers: a binding that adds request
/// values is not read as a boolean.
#[test]
fn numeric_bindings_are_not_read_as_booleans() {
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("qty + qty - qty");
    let result = built.check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Proved, "{}", result.json());
    let mut built = pair();
    built.manifest["components"][1]["bind"]["amount"] = json!("qty + qty");
    let result = built.check().unwrap_or_else(fail);
    assert_eq!(result.verdict(), Verdict::Violated, "{}", result.json());
}
