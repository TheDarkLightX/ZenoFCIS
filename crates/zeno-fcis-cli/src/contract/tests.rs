//! Contract generator checks: the eight templates regenerate byte for byte,
//! their declarations read as the reviewed rules say, and planted errors are
//! refused at the entry that holds them.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use zeno_fcis_spec::LawScope;

use zeno_fcis_synthesis::finite::{Op, Program, canonical_v2::output, v2_composition as c};
use zeno_fcis_synthesis::finite_runtime::import_program;

use super::declarations::{Declarations, Form, Kind, Leaf, ROOTS};
use super::expr::{self, Ast, Binary, Rounding};
use super::model::{self, Contract, InputLeaf};
use super::rules::{self, Adoption, Class, Rules, Usage};
use super::{
    AdoptionSources, ContractSources, StepBound, generate_contract, policy, program_bytes,
    refresh_receipts, schema, schema_commitment, with_adoption,
};
use crate::transform::{self, DEFAULT_MAX_INPUT_TUPLES, DEFAULT_STEP_LIMIT, Limits};

const TEMPLATES: [&str; 8] = [
    "durable-counter",
    "inventory-reservation",
    "order-fulfillment",
    "account-lockout",
    "withdrawal-queue",
    "agent-treasury-guard",
    "prepared-counter",
    "compliance-gateway",
];

struct Template {
    project: String,
    rules: String,
    schema: Vec<u8>,
    origin: String,
    source: String,
    policy: Vec<u8>,
}

fn template(name: &str) -> Template {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(name);
    let read = |path: &str| {
        fs::read(base.join(path)).unwrap_or_else(|error| panic!("read {name}/{path}: {error}"))
    };
    let text = |path: &str| {
        String::from_utf8(read(path)).unwrap_or_else(|error| panic!("{name}/{path}: {error}"))
    };
    Template {
        project: text("project.zeno"),
        rules: text("v2/policy.json"),
        schema: read("v2/schema.zcve"),
        origin: text("v2/schema-origin.json"),
        source: text("src/v2_contract.rs"),
        policy: read("v2/policy.zcve"),
    }
}

impl Template {
    fn sources(&self) -> ContractSources<'_> {
        ContractSources {
            project: &self.project,
            rules: &self.rules,
            schema_origin: Some(&self.origin),
            adoptions: &[],
            replayed: &[],
            evolutions: &[],
        }
    }

    fn rules(&self) -> Rules {
        Rules::read(&self.rules).unwrap_or_else(|error| panic!("{error}"))
    }

    fn declarations(&self) -> Declarations {
        Declarations::read(&self.project, &self.rules().leaf_bindings)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    /// The rules file with `edit` applied to its JSON.
    fn with_rules(&self, edit: impl FnOnce(&mut Value)) -> Self {
        let mut rules: Value =
            serde_json::from_str(&self.rules).unwrap_or_else(|error| panic!("{error}"));
        edit(&mut rules);
        Self {
            rules: rules.to_string(),
            ..self.clone()
        }
    }

    fn with_project(&self, from: &str, to: &str) -> Self {
        assert_eq!(
            self.project.matches(from).count(),
            1,
            "unique anchor {from:?}"
        );
        Self {
            project: self.project.replacen(from, to, 1),
            ..self.clone()
        }
    }

    /// The place and reason of the refusal.
    fn refusal(&self) -> (String, String) {
        let Err(error) = generate_contract(self.sources()) else {
            panic!("planted error must be refused")
        };
        (error.place().to_owned(), error.reason().to_owned())
    }
}

impl Clone for Template {
    fn clone(&self) -> Self {
        Self {
            project: self.project.clone(),
            rules: self.rules.clone(),
            schema: self.schema.clone(),
            origin: self.origin.clone(),
            source: self.source.clone(),
            policy: self.policy.clone(),
        }
    }
}

fn first_difference(expected: &str, actual: &str) -> String {
    let line = expected
        .lines()
        .zip(actual.lines())
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
    let show = |text: &str| {
        text.lines()
            .skip(line.saturating_sub(3))
            .take(8)
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "first difference at line {}:\n--- committed\n{}\n--- generated\n{}",
        line + 1,
        show(expected),
        show(actual)
    )
}

fn name(text: &str) -> Box<Ast> {
    Box::new(Ast::Name(text.to_owned()))
}

fn binary(operator: Binary, left: Box<Ast>, right: Box<Ast>) -> Box<Ast> {
    Box::new(Ast::Binary(operator, left, right))
}

/// Values each program input can take: two booleans, each variant, each integer.
fn input_domain(declarations: &Declarations) -> u128 {
    declarations
        .inputs()
        .unwrap_or_else(|error| panic!("{error}"))
        .iter()
        .map(|input| match declarations.kind(input.type_id) {
            Ok(Kind::Bool) => 2,
            Ok(Kind::Sum(variants)) => variants.len() as u128,
            Ok(Kind::I128 { min, max }) => u128::try_from(max - min + 1).unwrap_or(0),
            other => panic!("input {} has no finite domain: {other:?}", input.name),
        })
        .product()
}

#[test]
fn every_template_regenerates_byte_for_byte() {
    for name in TEMPLATES {
        let files = template(name);
        let generated =
            generate_contract(files.sources()).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            generated.source() == files.source,
            "{name} src/v2_contract.rs: {}",
            first_difference(&files.source, generated.source())
        );
        assert!(
            generated.policy() == files.policy,
            "{name} v2/policy.zcve differs"
        );
        assert!(
            generated.schema() == files.schema,
            "{name} v2/schema.zcve differs"
        );
        assert_eq!(generated.summary().application, name);
        assert_eq!(files.rules().template, name);
    }
}

#[test]
fn generation_is_a_function_of_its_inputs() {
    let files = template("account-lockout");
    let first = generate_contract(files.sources()).unwrap_or_else(|error| panic!("{error}"));
    let second = generate_contract(files.sources()).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(first, second);
}

// Replaces test_precedence_and_non_language_tokens_refuse.
#[test]
fn rule_precedence_and_tokens_outside_the_language_refuse() {
    assert_eq!(
        expr::parse("a || b -> c && d"),
        Ok(Ast::Binary(
            Binary::Implies,
            binary(Binary::Or, name("a"), name("b")),
            binary(Binary::And, name("c"), name("d")),
        ))
    );
    assert_eq!(
        expr::parse("div_ceil(a * 3, 4 * price)"),
        Ok(Ast::Div(
            Rounding::Ceil,
            binary(Binary::Mul, name("a"), Box::new(Ast::Int(3))),
            binary(Binary::Mul, Box::new(Ast::Int(4)), name("price")),
        ))
    );
    assert_eq!(
        expr::parse("a -> b -> c"),
        Ok(Ast::Binary(
            Binary::Implies,
            name("a"),
            binary(Binary::Implies, name("b"), name("c")),
        )),
        "implication associates right"
    );
    assert_eq!(
        expr::parse("a - b - c"),
        Ok(Ast::Binary(
            Binary::Sub,
            binary(Binary::Sub, name("a"), name("b")),
            name("c"),
        )),
        "subtraction associates left"
    );
    assert!(expr::parse("x; side_effect()").is_err());
    assert!(expr::parse("callback(x)").is_err());
    assert!(expr::parse("choose(a, b)").is_err());
    assert!(expr::parse("a /").is_err());
    assert!(expr::parse("(a").is_err());
    assert!(expr::parse("").is_err());
}

// Replaces test_complete_original_definitions_and_scopes.
#[test]
fn templates_declare_complete_types_and_scoped_laws() {
    for (name, count) in TEMPLATES.into_iter().zip([6, 9, 12, 10, 13, 21, 7, 15]) {
        let files = template(name);
        let declarations = files.declarations();
        assert_eq!(declarations.types.len(), count, "{name}");
        let laws: Vec<u32> = declarations.laws.iter().map(|law| law.id).collect();
        let kinds: Vec<u32> = files.rules().law_kinds.keys().copied().collect();
        assert_eq!(laws, kinds, "{name}: every law has exactly one kind");
    }
    let account = template("account-lockout").declarations();
    let leaf = |declarations: &Declarations, id: u32| match &declarations.types[&id].form {
        Form::Leaf(leaf) => *leaf,
        other => panic!("type {id} is not a leaf: {other:?}"),
    };
    assert_eq!(leaf(&account, 105), Leaf::I128 { min: 0, max: 2 });
    assert_eq!(
        leaf(&account, 106),
        Leaf::I128 {
            min: 0,
            max: 4_102_444_800
        }
    );
    assert_eq!(
        leaf(&account, 107),
        Leaf::I128 {
            min: 0,
            max: 4_102_445_700
        }
    );
    assert!(matches!(account.kind(101), Ok(Kind::Sum(_))));
    assert!(matches!(account.kind(109), Ok(Kind::Sum(_))));
    let treasury = template("agent-treasury-guard").declarations();
    let scopes: Vec<(u32, LawScope, bool)> = treasury
        .laws
        .iter()
        .map(|law| (law.id, law.scope, law.genesis))
        .collect();
    assert_eq!(
        scopes,
        [
            (500, LawScope::Committing, true),
            (501, LawScope::Committing, false),
            (502, LawScope::Committing, false),
            (503, LawScope::Accept, false),
            (504, LawScope::Accept, false),
            (505, LawScope::Accept, false),
            (506, LawScope::Accept, false),
            (507, LawScope::Accept, false),
            (508, LawScope::CommittedFailure, false),
        ]
    );
}

// Replaces test_variable_cycles_refuse.
#[test]
fn variables_that_refer_to_themselves_refuse() {
    let mut variables = template("durable-counter").rules().variables;
    variables.insert(
        "count".to_owned(),
        expr::parse("count + 1").unwrap_or_else(|error| panic!("{error}")),
    );
    assert!(expr::expand(&Ast::Name("count".to_owned()), &variables).is_err());
    variables.insert("count".to_owned(), Ast::Name("full".to_owned()));
    variables.insert("full".to_owned(), Ast::Name("count".to_owned()));
    assert!(expr::expand(&Ast::Name("full".to_owned()), &variables).is_err());
}

// Replaces test_original_sum_leaf_not_enum.
#[test]
fn sum_types_keep_their_sum_input_leaves() {
    for name in TEMPLATES {
        let declarations = template(name).declarations();
        for (id, declared) in &declarations.types {
            if matches!(declared.form, Form::Sum(_)) {
                assert!(
                    matches!(
                        model::input_leaf(&declarations, *id, false),
                        Ok(InputLeaf::Sum { type_id, .. }) if type_id == *id
                    ),
                    "{name} type {id}"
                );
            }
        }
    }
}

// Replaces test_original_prepared_shape_bounds_and_all_216_inputs.
#[test]
fn prepared_counter_shape_bounds_and_all_216_inputs() {
    let files = template("prepared-counter");
    let declarations = files.declarations();
    let rules = files.rules();
    assert_eq!(
        declarations.types[&105].form_leaf(),
        Some(Leaf::I128 { min: 0, max: 3 })
    );
    assert_eq!(
        declarations.types[&106].form_leaf(),
        Some(Leaf::I128 { min: -1, max: 1 })
    );
    let command: Vec<(u16, &str, u32)> = declarations
        .fields(101)
        .unwrap_or_else(|error| panic!("{error}"))
        .iter()
        .map(|field| (field.id, field.name.as_str(), field.type_id))
        .collect();
    assert_eq!(
        command,
        [
            (120, "first", 106),
            (121, "second", 106),
            (122, "third", 106)
        ]
    );
    assert_eq!(declarations.types[&102].form_leaf(), Some(Leaf::Bool));
    assert_eq!(input_domain(&declarations), 216);
    let laws: Vec<(u32, LawScope, bool)> = declarations
        .laws
        .iter()
        .map(|law| (law.id, law.scope, law.genesis))
        .collect();
    assert_eq!(
        laws,
        [
            (500, LawScope::Committing, true),
            (501, LawScope::Accept, false),
            (502, LawScope::CommittedFailure, false),
            (503, LawScope::Reject, false),
        ]
    );
    let channels: Vec<(u32, u32, u32)> = declarations
        .channels
        .iter()
        .map(|channel| (channel.id, channel.destination, channel.payload))
        .collect();
    assert_eq!(channels, [(300, 103, 104)]);
    let reasons: Vec<Option<u32>> = rules.cases.iter().map(|case| case.reason).collect();
    assert_eq!(reasons, [Some(200), Some(201), None]);
    assert_eq!(
        rules
            .cases
            .last()
            .map(|case| case.outbox[0].destination.as_str()),
        Some("local-observer")
    );
}

// Replaces test_original_gateway_full_domain_rule_precedence_and_links.
#[test]
fn gateway_full_domain_rule_precedence_and_links() {
    let files = template("compliance-gateway");
    let declarations = files.declarations();
    let rules = files.rules();
    assert_eq!(input_domain(&declarations), 2880);
    let channels: Vec<(u32, u32, u32)> = declarations
        .channels
        .iter()
        .map(|channel| (channel.id, channel.destination, channel.payload))
        .collect();
    assert_eq!(channels, [(300, 103, 104), (301, 105, 106)]);
    let reasons: Vec<u32> = rules.cases.iter().filter_map(|case| case.reason).collect();
    assert_eq!(reasons, [200, 201, 210, 211, 212, 213, 214]);
    assert_eq!(rules.reject_law, 509);
    assert_eq!(
        rules.cases.last().map(|case| case.class),
        Some(Class::Accept)
    );
    for case in &rules.cases {
        let fields: Vec<u16> = case.post.keys().copied().collect();
        let expected: &[u16] = if case.class == Class::Reject {
            &[]
        } else {
            &[120]
        };
        assert_eq!(fields, expected);
    }
}

// Replaces test_original_framework_ids_and_complete_field_sets.
#[test]
fn framework_law_ids_and_complete_field_sets() {
    for name in ["inventory-reservation", "withdrawal-queue"] {
        let rules = template(name).rules();
        assert_eq!((rules.failure_law, rules.reject_law), (508, 509), "{name}");
    }
    for name in [
        "account-lockout",
        "order-fulfillment",
        "agent-treasury-guard",
        "compliance-gateway",
    ] {
        assert_eq!(template(name).rules().reject_law, 509, "{name}");
    }
    let prepared = template("prepared-counter").with_rules(|rules| {
        rules["cases"][2]["post"] = serde_json::json!({});
    });
    let (place, reason) = prepared.refusal();
    assert_eq!(place, "v2/policy.json cases[2].post");
    assert_eq!(reason, "must set every state field");
    let gateway = template("compliance-gateway");
    let rules = gateway.rules();
    let delivering = rules
        .cases
        .iter()
        .position(|case| !case.outbox.is_empty())
        .unwrap_or_else(|| panic!("a delivering case"));
    let gateway = gateway.with_rules(|rules| {
        rules["cases"][delivering]["outbox"][0]["payload"] = serde_json::json!({});
    });
    let (place, reason) = gateway.refusal();
    assert_eq!(
        place,
        format!("v2/policy.json cases[{delivering}].outbox[0].payload")
    );
    assert_eq!(reason, "must set every payload field");
}

#[test]
fn planted_rule_errors_are_refused_where_they_are() {
    let counter = template("durable-counter");
    type Edit = Box<dyn Fn(&mut Value)>;
    let cases: [(&str, Edit, &str); 12] = [
        (
            "v2/policy.json",
            Box::new(|rules| rules["extra"] = Value::Bool(true)),
            "unknown key `extra`",
        ),
        (
            "v2/policy.json cases[3]",
            Box::new(|rules| rules["cases"][3]["class"] = "Reject".into()),
            "a reject sets no state",
        ),
        (
            "v2/policy.json cases[3].when",
            Box::new(|rules| rules["cases"][3]["when"] = "allowed".into()),
            "the last case must be `true`",
        ),
        (
            "v2/policy.json genesis",
            Box::new(|rules| {
                rules["genesis"]
                    .as_object_mut()
                    .map(|genesis| genesis.remove("111"));
            }),
            "must set every state field",
        ),
        (
            "v2/policy.json cases[0].reason",
            Box::new(|rules| rules["cases"][0]["reason"] = 299.into()),
            "reason 299 is not declared",
        ),
        (
            "v2/policy.json cases",
            Box::new(|rules| rules["cases"][1]["reason"] = 200.into()),
            "reason 201 is declared but no case uses it",
        ),
        (
            "v2/policy.json cases",
            Box::new(|rules| rules["cases"][3]["reason"] = 200.into()),
            "reason 200 is used with two classes",
        ),
        (
            "v2/policy.json cases[2].post.110",
            Box::new(|rules| rules["cases"][2]["post"]["110"] = "count * 2".into()),
            "decision rules have no multiplication",
        ),
        (
            "v2/policy.json cases[2].post.110",
            Box::new(|rules| rules["cases"][2]["post"]["110"] = "pre.100.999".into()),
            "`pre.100.999` is not a variable or an input",
        ),
        (
            "v2/policy.json variables.pre.extra",
            Box::new(|rules| rules["variables"]["pre.extra"] = "1".into()),
            "variable names may not start with a root",
        ),
        (
            "v2/policy.json law_kinds",
            Box::new(|rules| {
                rules["law_kinds"]
                    .as_object_mut()
                    .map(|kinds| kinds.remove("503"));
            }),
            "must give a kind to exactly the declared laws",
        ),
        (
            "v2/policy.json cases[2].post.0110",
            Box::new(|rules| {
                let post = &mut rules["cases"][2]["post"];
                let value = post["110"].clone();
                post.as_object_mut().map(|post| post.remove("110"));
                post["0110"] = value;
            }),
            "`0110` is not a decimal ID",
        ),
    ];
    for (expected_place, edit, expected_reason) in cases {
        let (place, reason) = counter.with_rules(|rules| edit(rules)).refusal();
        assert_eq!(place, expected_place, "{expected_reason}");
        assert!(reason.contains(expected_reason), "{place}: {reason}");
    }
    let duplicate = Template {
        rules: counter
            .rules
            .replacen("\"schema\":", "\"schema\": \"x\", \"schema\":", 1),
        ..counter.clone()
    };
    assert!(duplicate.refusal().1.contains("duplicate key `schema`"));
    let duplicate_key = Template {
        rules: counter
            .rules
            .replacen("\"110\": 0,", "\"110\": 0, \"110\": 1,", 1),
        ..counter.clone()
    };
    assert!(duplicate_key.refusal().1.contains("duplicate key `110`"));
}

#[test]
fn planted_declaration_errors_are_refused_where_they_are() {
    let counter = template("durable-counter");
    let cases = [
        (
            counter.with_project(
                "law 503 reject_no_authority on reject = true;",
                "law 503 reject_no_authority = true;",
            ),
            "project.zeno law 503",
            "needs a scope",
        ),
        (
            counter.with_project(
                "law 503 reject_no_authority on reject = true;",
                "law 503 reject_no_authority on reject = forall i in 0..2 { pre.100.110 >= 0 };",
            ),
            "project.zeno law 503",
            "quantifier has no contract form",
        ),
        (
            counter.with_project(
                "type 105 int CounterValue;",
                "type 105 int CounterValue in 0..=2;",
            ),
            "project.zeno type 105",
            "its leaf binding differs from its declared range",
        ),
        (
            counter.with_rules(|rules| rules["leaf_bindings"]["104"] = serde_json::json!(["Bool"])),
            "project.zeno type 104",
            "has more than one of fields, variants and a leaf",
        ),
        (
            counter.with_project(
                "type 101 command CounterCommand;",
                "type 101 data CounterCommand;",
            ),
            "project.zeno",
            "type 101",
        ),
    ];
    for (planted, expected_place, expected_reason) in cases {
        let (place, reason) = planted.refusal();
        assert_eq!(place, expected_place, "{expected_reason}");
        assert!(reason.contains(expected_reason), "{place}: {reason}");
    }
    let unbound = counter.with_rules(|rules| {
        rules["leaf_bindings"]
            .as_object_mut()
            .map(|bindings| bindings.remove("105"));
    });
    assert_eq!(
        unbound.refusal(),
        (
            "project.zeno type 105".to_owned(),
            "needs fields, variants, a declared range or a leaf binding".to_owned()
        )
    );
}

#[test]
fn schema_and_library_disagreements_are_refused() {
    let counter = template("durable-counter");
    let origin = Template {
        origin: counter
            .origin
            .replacen("\"bytes\": 329", "\"bytes\": 330", 1),
        ..counter.clone()
    };
    assert_eq!(
        origin.refusal(),
        (
            "v2/schema-origin.json".to_owned(),
            "`bytes` differs from v2/schema.zcve".to_owned()
        )
    );
    // A delivery longer than its destination type allows.
    let long = counter.with_rules(|rules| {
        rules["cases"][2]["outbox"][0]["destination"] = "x".repeat(33).into();
    });
    assert_eq!(
        long.refusal(),
        (
            "v2/policy.json cases[2].outbox[0].destination".to_owned(),
            "must be ASCII text of 1 to 32 bytes".to_owned()
        )
    );
}

#[test]
fn library_rules_are_refused_where_they_are_written() {
    let counter = template("durable-counter");
    type Edit = Box<dyn Fn(&mut Value)>;
    let cases: [(&str, Edit, &str); 6] = [
        (
            "v2/policy.json cases[2].reason",
            Box::new(|rules| rules["cases"][2]["reason"] = 200.into()),
            "an accept has no reason",
        ),
        (
            "v2/policy.json cases[3].reason",
            Box::new(|rules| rules["cases"][3]["reason"] = Value::Null),
            "a reject or committed failure needs a reason",
        ),
        (
            "v2/policy.json cases[2].post.110",
            Box::new(|rules| rules["cases"][2]["post"]["110"] = 4.into()),
            "4 is outside 0..=3",
        ),
        (
            "v2/policy.json cases[2].post.110",
            Box::new(|rules| rules["cases"][2]["post"]["110"] = "action".into()),
            "`command.101` has type 101, which differs from type 105",
        ),
        (
            "project.zeno law 501",
            Box::new(|rules| rules["law_kinds"]["501"] = "StateInvariant".into()),
            "a StateInvariant law must be declared `on commit, genesis`",
        ),
        (
            "v2/policy.json law_kinds",
            Box::new(|rules| rules["law_kinds"]["500"] = "AssetConservation".into()),
            "needs a StateInvariant law, declared `on commit, genesis`",
        ),
    ];
    for (expected_place, edit, expected_reason) in cases {
        let (place, reason) = counter.with_rules(|rules| edit(rules)).refusal();
        assert_eq!(
            (place.as_str(), reason.as_str()),
            (expected_place, expected_reason)
        );
    }
    for key in ["framework_failure_law", "framework_reject_law"] {
        let zero = counter.with_rules(|rules| rules[key] = 0.into());
        assert_eq!(
            zero.refusal(),
            (
                format!("v2/policy.json {key}"),
                "must be a nonzero law ID".to_owned()
            )
        );
    }
    // The withdrawal queue decides no committed failure, so its framework
    // failure law applies; it may not share the structural reject law's ID.
    let shared = template("withdrawal-queue").with_rules(|rules| {
        rules["framework_failure_law"] = 509.into();
    });
    assert_eq!(
        shared.refusal(),
        (
            "v2/policy.json".to_owned(),
            "the framework CommittedFailureEffects and RejectNoAuthority laws share ID 509"
                .to_owned()
        )
    );

    // Each project edit, with the rules edit that keeps the rest consistent.
    // A changed schema no longer matches the template's schema origin, so
    // these contracts are generated without one.
    let declared: [(&str, &str, Edit, &str, &str); 4] = [
        (
            "variant 121 101 RecordFailure none;",
            "variant 122 101 RecordFailure none;",
            Box::new(|_| {}),
            "project.zeno",
            "sum type 101 needs consecutive variant IDs to be a program input or output",
        ),
        (
            "law 503 reject_no_authority",
            "law 909 reject_no_authority",
            Box::new(|rules| {
                if let Some(kinds) = rules["law_kinds"].as_object_mut()
                    && let Some(kind) = kinds.remove("503")
                {
                    kinds.insert("909".to_owned(), kind);
                }
            }),
            "project.zeno law 909",
            "has the ID of the framework AuthoritySubjectRecipient law",
        ),
        (
            "on commit, genesis = post.100.110 >= 0",
            "on commit, genesis = pre.100.110 >= 0",
            Box::new(|_| {}),
            "project.zeno law 500",
            "applies at genesis, where `pre.100.110` has no value",
        ),
        (
            "on reject = true;",
            "on reject = pre.100 == pre.100;",
            Box::new(|_| {}),
            "project.zeno law 503",
            "`pre.100` is a record; name one of its fields",
        ),
    ];
    for (from, to, edit, expected_place, expected_reason) in declared {
        let edited = counter
            .with_project(from, to)
            .with_rules(|rules| edit(rules));
        let Err(error) = generate_contract(ContractSources {
            schema_origin: None,
            ..edited.sources()
        }) else {
            panic!("planted error must be refused: {to}")
        };
        assert_eq!(
            (error.place(), error.reason()),
            (expected_place, expected_reason)
        );
    }
}

impl super::declarations::Type {
    fn form_leaf(&self) -> Option<Leaf> {
        match self.form {
            Form::Leaf(leaf) => Some(leaf),
            _ => None,
        }
    }
}

#[test]
fn a_declared_range_may_be_restated_but_not_changed() {
    let counter = template("durable-counter");
    let restated = counter.with_project(
        "type 105 int CounterValue;",
        "type 105 int CounterValue in 0..=3;",
    );
    let generated = generate_contract(restated.sources()).unwrap_or_else(|error| panic!("{error}"));
    assert!(generated.policy() == counter.policy);
}

#[test]
fn the_state_root_must_be_a_record() {
    let project = "zeno 1;
project 1 flag;
namespace 2 core;
type 100 state Flag;
type 101 command Toggle;
type 102 context Caller;
type 103 destination Desk;
type 104 payload Note;
type 105 bool Seen;
field 110 104 seen 105;
variant 120 101 Flip none;
reason 200 refused precedence 0;
channel 300 note destination 103 payload 104;
component 400 machine {
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 8;
}
merge [400];
";
    let rules = r#"{
        "schema": "zeno-fcis/template-declarative-policy/2",
        "template": "flag",
        "roots": {"state": 100, "command": 101, "context": 102},
        "leaf_bindings": {"100": ["Bool"], "102": ["Bool"], "103": ["Text", 1, 8], "105": ["Bool"]},
        "variables": {},
        "cases": [{"when": "true", "class": "Reject", "reason": 200, "post": {}, "outbox": []}],
        "genesis": {},
        "law_kinds": {}
    }"#;
    let Err(error) = generate_contract(ContractSources {
        project,
        rules,
        schema_origin: None,
        adoptions: &[],
        replayed: &[],
        evolutions: &[],
    }) else {
        panic!("a scalar state must be refused")
    };
    assert_eq!(error.reason(), "the state root, type 100, must be a record");
}

#[test]
fn rules_files_render_back_byte_for_byte() {
    for name in TEMPLATES {
        let files = template(name);
        assert_eq!(
            rules::reformat(&files.rules).unwrap_or_else(|error| panic!("{name}: {error}")),
            files.rules,
            "{name}"
        );
    }
}

fn adoption(candidate: &[u8], receipt: &[u8], usage: Usage, superseded: &[u8]) -> Adoption {
    Adoption {
        candidate_sha256: transform::sha256_hex(candidate),
        receipt_sha256: transform::sha256_hex(receipt),
        usage,
        superseded_policy_sha256: transform::sha256_hex(superseded),
    }
}

#[test]
fn with_adoption_appends_one_entry_in_the_file_layout() {
    let counter = template("durable-counter");
    let first = adoption(b"candidate", b"receipt", Usage::NewVersion, b"policy 1");
    let once = with_adoption(&counter.rules, &first).unwrap_or_else(|error| panic!("{error}"));
    let head = counter
        .rules
        .strip_suffix("\n}\n")
        .unwrap_or_else(|| panic!("the rules file ends with its closing brace"));
    assert_eq!(
        once,
        format!(
            "{head},\n  \"adoptions\": [\n    {{\n      \"candidate_sha256\": \"{}\",\n      \
             \"receipt_sha256\": \"{}\",\n      \"usage\": \"new-version\",\n      \
             \"superseded_policy_sha256\": \"{}\"\n    }}\n  ]\n}}\n",
            first.candidate_sha256, first.receipt_sha256, first.superseded_policy_sha256
        )
    );
    assert_eq!(
        Rules::read(&once)
            .unwrap_or_else(|error| panic!("{error}"))
            .adoptions,
        std::slice::from_ref(&first)
    );
    let second = adoption(b"other", b"later", Usage::Preserved, b"policy 2");
    let twice = with_adoption(&once, &second).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        Rules::read(&twice)
            .unwrap_or_else(|error| panic!("{error}"))
            .adoptions,
        [first, second]
    );
    let (a, b, c) = ("a".repeat(64), "b".repeat(64), "c".repeat(64));
    let planted = [
        (
            "adoptions[0].candidate_sha256",
            format!(
                r#"{{"candidate_sha256": "abc", "receipt_sha256": "", "usage": "preserved", "superseded_policy_sha256": "{c}"}}"#
            ),
            "must be a lowercase hexadecimal SHA-256",
        ),
        (
            "adoptions[0].usage",
            format!(
                r#"{{"candidate_sha256": "{a}", "receipt_sha256": "{b}", "usage": "same", "superseded_policy_sha256": "{c}"}}"#
            ),
            "`same` is not preserved or new-version",
        ),
        (
            "adoptions[0].superseded_policy_sha256",
            format!(
                r#"{{"candidate_sha256": "{a}", "receipt_sha256": "{b}", "usage": "preserved", "superseded_policy_sha256": "C"}}"#
            ),
            "must be a lowercase hexadecimal SHA-256",
        ),
        (
            "adoptions[0]",
            format!(
                r#"{{"candidate_sha256": "{a}", "receipt_sha256": "{b}", "usage": "preserved"}}"#
            ),
            "missing key `superseded_policy_sha256`",
        ),
        (
            "adoptions[0]",
            format!(
                r#"{{"candidate_sha256": "{a}", "receipt_sha256": "{b}", "usage": "preserved", "superseded_policy_sha256": "{c}", "nodes": 1}}"#
            ),
            "unknown key `nodes`",
        ),
    ];
    for (place, entry, reason) in planted {
        let rules = format!("{head},\n  \"adoptions\": [{entry}]\n}}\n");
        let error = Rules::read(&rules)
            .err()
            .unwrap_or_else(|| panic!("{entry} is refused"));
        assert_eq!(error.place(), format!("v2/policy.json {place}"));
        assert_eq!(error.reason(), reason);
    }
}

/// The rules-compiled program of a template, as the receipts name it.
fn current_program(files: &Template) -> (Program, Vec<u8>) {
    let rules = files.rules();
    let declarations = files.declarations();
    let schema = schema::encode(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|error| panic!("{error}"));
    let contract = Contract::build(&declarations, &rules, commitment)
        .unwrap_or_else(|error| panic!("{error}"));
    let program = contract.program().unwrap_or_else(|error| panic!("{error}"));
    let bytes = program_bytes(&program).unwrap_or_else(|error| panic!("{error}"));
    (program, bytes)
}

/// `program` with `count` unused constants: equal results, and each costs
/// one more Step on every input.
fn padded(program: &Program, count: usize) -> Vec<u8> {
    let mut nodes = program.nodes().to_vec();
    nodes.extend(std::iter::repeat_n(Op::Int(0), count));
    let padded = Program::try_new(
        program.inputs().to_vec(),
        program.outputs().to_vec(),
        nodes,
        program.roots().to_vec(),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    program_bytes(&padded).unwrap_or_else(|error| panic!("{error}"))
}

fn receipt(original: &[u8], candidate: &[u8]) -> Vec<u8> {
    let limits = Limits {
        steps: DEFAULT_STEP_LIMIT,
        input_tuples: DEFAULT_MAX_INPUT_TUPLES,
    };
    match transform::check(original, candidate, limits) {
        Ok(equivalence) => equivalence.receipt(),
        Err(rejection) => panic!("the candidate must be equivalent: {rejection:?}"),
    }
}

#[test]
fn adoptions_replay_their_receipts_in_order_and_emit_every_version() {
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let padded_once = padded(&program, 1);
    let padded_receipt = receipt(&original, &padded_once);
    let claimed = with_adoption(
        &counter.rules,
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::Preserved,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let first = [AdoptionSources {
        candidate: &padded_once,
        receipt: &padded_receipt,
    }];
    let refused = generate_contract(ContractSources {
        rules: &claimed,
        adoptions: &first,
        ..counter.sources()
    })
    .err()
    .unwrap_or_else(|| panic!("a false usage claim is refused"));
    assert_eq!(
        (refused.place(), refused.reason()),
        (
            "v2/policy.json adoptions[0].usage",
            "the receipt reports that Step usage differs; `new-version` is required"
        )
    );

    let adopted = with_adoption(
        &counter.rules,
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::NewVersion,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let generated = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &first,
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    let summary = generated.summary();
    assert_eq!(summary.version, 2);
    assert_eq!(summary.program_nodes, program.nodes().len() + 1);
    assert_eq!(summary.adoptions.len(), 1);
    assert_eq!(
        summary.adoptions[0].program_nodes,
        [program.nodes().len(), program.nodes().len() + 1]
    );
    assert!(!summary.adoptions[0].usage_preserved);
    assert_eq!(summary.adoptions[0].usage, Usage::NewVersion);
    // The superseded version is the committed contract, reading its own policy file.
    assert_eq!(generated.previous().len(), 1);
    assert_eq!(
        generated.previous()[0].source(),
        counter
            .source
            .replace("../v2/policy.zcve", "../v2/policy_v1.zcve")
    );
    assert_eq!(generated.previous()[0].policy(), counter.policy);
    assert_ne!(generated.policy(), counter.policy, "the program changed");
    assert_eq!(generated.program(), padded_once);
    assert!(generated.source().contains("pub const VERSION: u32 = 2;"));
    assert!(
        generated
            .source()
            .contains("#[path = \"v2_contract_v1.rs\"]\npub mod v1;")
    );
    assert!(
        generated
            .source()
            .contains("let catalog_1 = v1::checked_catalog(&descriptor_1)?;")
    );
    // The lineage passes the replayed receipt's digest with the catalogs.
    assert!(generated.source().contains(&format!(
        "pub const ADOPTION_RECEIPTS: &[&str] =\n    &[\"{}\"];",
        transform::sha256_hex(&padded_receipt)
    )));
    assert!(
        generated
            .source()
            .contains("Ok(f((&[&catalog_1, &catalog], ADOPTION_RECEIPTS)))")
    );
    assert!(generated.source().contains("Op::Int(0)"));
    assert_eq!(
        import_program(&padded_once)
            .unwrap_or_else(|error| panic!("{error}"))
            .nodes()
            .len(),
        summary.program_nodes
    );

    // A second adoption replays against the adopted program, not the
    // rules-compiled one, and binds the policy of version 2.
    let padded_twice = padded(&program, 2);
    let twice_receipt = receipt(&padded_once, &padded_twice);
    let chained = with_adoption(
        &adopted,
        &adoption(
            &padded_twice,
            &twice_receipt,
            Usage::NewVersion,
            generated.policy(),
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let both = [
        first[0],
        AdoptionSources {
            candidate: &padded_twice,
            receipt: &twice_receipt,
        },
    ];
    let generated = generate_contract(ContractSources {
        rules: &chained,
        adoptions: &both,
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(generated.summary().version, 3);
    assert!(!generated.summary().adoptions[1].usage_preserved);
    assert_eq!(generated.previous().len(), 2);
    assert_eq!(generated.replayed().len(), 2);
    assert!(
        generated
            .source()
            .contains("Ok(f((&[&catalog_1, &catalog_2, &catalog], ADOPTION_RECEIPTS)))")
    );
    assert!(
        generated.previous()[1]
            .source()
            .contains("include_bytes!(\"../v2/policy_v2.zcve\")")
    );
    assert!(
        generated.previous()[1]
            .source()
            .contains("pub const VERSION: u32 = 2;")
    );
    assert!(!generated.previous()[1].source().contains("pub mod v1;"));
    assert!(
        !generated.previous()[1]
            .source()
            .contains("ADOPTION_RECEIPTS")
    );
    // The same generation reuses its witnesses and enumerates nothing again.
    let again = generate_contract(ContractSources {
        rules: &chained,
        adoptions: &both,
        replayed: generated.replayed(),
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(again, generated);

    // The receipt of the first adoption does not replay against the adopted program.
    let misordered = [both[1], both[0]];
    let misordered_rules = with_adoption(
        &with_adoption(
            &counter.rules,
            &adoption(
                &padded_twice,
                &twice_receipt,
                Usage::NewVersion,
                &counter.policy,
            ),
        )
        .unwrap_or_else(|error| panic!("{error}")),
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::NewVersion,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let refused = generate_contract(ContractSources {
        rules: &misordered_rules,
        adoptions: &misordered,
        ..counter.sources()
    })
    .err()
    .unwrap_or_else(|| panic!("a receipt against another program is refused"));
    assert_eq!(refused.place(), "v2/policy.json adoptions[0]");
    assert!(
        refused.reason().starts_with(
            "v2/adoptions/1/receipt.json does not replay against version 1's program and the candidate: "
        ),
        "{}",
        refused.reason()
    );
}

#[test]
fn an_adoption_must_change_the_program_and_never_repeat_a_version() {
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let refusal = |rules: &str, files: &[AdoptionSources<'_>]| {
        generate_contract(ContractSources {
            rules,
            adoptions: files,
            ..counter.sources()
        })
        .err()
        .map(|error| (error.place().to_owned(), error.reason().to_owned()))
    };
    // The identical candidate would leave the identity unchanged.
    let same_receipt = receipt(&original, &original);
    let unchanged = with_adoption(
        &counter.rules,
        &adoption(&original, &same_receipt, Usage::Preserved, &counter.policy),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        refusal(
            &unchanged,
            &[AdoptionSources {
                candidate: &original,
                receipt: &same_receipt
            }]
        ),
        Some((
            "v2/policy.json adoptions[0]".to_owned(),
            "v2/adoptions/1/program.zcve is version 1's own program; an adoption must change it"
                .to_owned()
        ))
    );
    // Adopting version 1's program back would repeat version 1.
    let padded_once = padded(&program, 1);
    let padded_receipt = receipt(&original, &padded_once);
    let adopted = with_adoption(
        &counter.rules,
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::NewVersion,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let first = AdoptionSources {
        candidate: &padded_once,
        receipt: &padded_receipt,
    };
    let generated = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &[first],
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    let back = receipt(&padded_once, &original);
    let reverted = with_adoption(
        &adopted,
        &adoption(&original, &back, Usage::NewVersion, generated.policy()),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        refusal(
            &reverted,
            &[
                first,
                AdoptionSources {
                    candidate: &original,
                    receipt: &back
                }
            ]
        ),
        Some((
            "v2/policy.json adoptions[1]".to_owned(),
            "version 3 would repeat version 1: a contract's identity binds its policy, so stores \
             could not tell them apart. Adopt a program no earlier version ran"
                .to_owned()
        ))
    );
}

#[test]
fn an_edit_after_adoption_is_refused_and_version_one_is_kept() {
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let padded_once = padded(&program, 1);
    let padded_receipt = receipt(&original, &padded_once);
    let adopted = with_adoption(
        &counter.rules,
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::NewVersion,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let files = [AdoptionSources {
        candidate: &padded_once,
        receipt: &padded_receipt,
    }];
    let generated = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &files,
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(generated.previous()[0].policy(), counter.policy);
    // A law edit leaves the cases, so the program and the receipt, unchanged,
    // but it would change version 1, which stores may run.
    let edited = counter.with_project("post.100.110 <= 3", "post.100.110 <= 2");
    let fresh = generate_contract(edited.sources()).unwrap_or_else(|error| panic!("{error}"));
    assert_ne!(
        fresh.policy(),
        counter.policy,
        "the edit changes the policy"
    );
    assert_eq!(fresh.program(), original, "but not the decision program");
    let refused = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &files,
        ..edited.sources()
    })
    .err()
    .unwrap_or_else(|| panic!("an edit after adoption is refused"));
    assert_eq!(
        refused.place(),
        "v2/policy.json adoptions[0].superseded_policy_sha256"
    );
    assert_eq!(
        refused.reason(),
        "differs from version 1's policy as these declarations and rules produce it: an edit \
         after adoption 1 changes version 1, which existing stores may run. Once a version is \
         superseded, only another adoption changes the contract"
    );
    // The unedited inputs still give version 1 exactly.
    let kept = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &files,
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(kept.previous()[0].policy(), counter.policy);
}

#[test]
fn adoptions_bind_their_retained_files() {
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let padded_once = padded(&program, 1);
    let padded_receipt = receipt(&original, &padded_once);
    let entry = adoption(
        &padded_once,
        &padded_receipt,
        Usage::NewVersion,
        &counter.policy,
    );
    let adopted = with_adoption(&counter.rules, &entry).unwrap_or_else(|error| panic!("{error}"));
    let refusal = |files: &[AdoptionSources<'_>]| {
        generate_contract(ContractSources {
            rules: &adopted,
            adoptions: files,
            ..counter.sources()
        })
        .err()
        .map(|error| (error.place().to_owned(), error.reason().to_owned()))
    };
    assert_eq!(
        refusal(&[]),
        Some((
            "v2/policy.json adoptions".to_owned(),
            "lists 1 adoptions but 0 retained candidate/receipt pairs were supplied".to_owned()
        ))
    );
    let mut other = padded_once.clone();
    other.push(0);
    assert_eq!(
        refusal(&[AdoptionSources {
            candidate: &other,
            receipt: &padded_receipt
        }]),
        Some((
            "v2/policy.json adoptions[0].candidate_sha256".to_owned(),
            "differs from v2/adoptions/1/program.zcve".to_owned()
        ))
    );
    let mut tampered = padded_receipt.clone();
    tampered.extend_from_slice(b" ");
    assert_eq!(
        refusal(&[AdoptionSources {
            candidate: &padded_once,
            receipt: &tampered
        }]),
        Some((
            "v2/policy.json adoptions[0].receipt_sha256".to_owned(),
            "differs from v2/adoptions/1/receipt.json".to_owned()
        ))
    );
    // A receipt with the right digest that names other programs does not replay.
    let foreign = adoption(&padded_once, b"{}", Usage::Preserved, &counter.policy);
    let foreign_rules =
        with_adoption(&counter.rules, &foreign).unwrap_or_else(|error| panic!("{error}"));
    let refused = generate_contract(ContractSources {
        rules: &foreign_rules,
        adoptions: &[AdoptionSources {
            candidate: &padded_once,
            receipt: b"{}",
        }],
        ..counter.sources()
    })
    .err()
    .unwrap_or_else(|| panic!("an unreadable receipt is refused"));
    assert_eq!(
        refused.reason(),
        "v2/adoptions/1/receipt.json does not replay against version 1's program and the candidate: not a transform receipt with readable bindings"
    );
    // A superseded policy digest that is not version 1's is refused before
    // any replay.
    let misbound = adoption(
        &padded_once,
        &padded_receipt,
        Usage::NewVersion,
        b"another policy",
    );
    let misbound_rules =
        with_adoption(&counter.rules, &misbound).unwrap_or_else(|error| panic!("{error}"));
    let refused = generate_contract(ContractSources {
        rules: &misbound_rules,
        adoptions: &[AdoptionSources {
            candidate: &padded_once,
            receipt: &padded_receipt,
        }],
        ..counter.sources()
    })
    .err()
    .unwrap_or_else(|| panic!("a wrong superseded policy is refused"));
    assert_eq!(
        refused.place(),
        "v2/policy.json adoptions[0].superseded_policy_sha256"
    );
}

/// `receipt` as an older checker identity wrote it: the crate version and
/// the checker's source digest instead of its semantics version.
fn older_checker(receipt: &[u8]) -> Vec<u8> {
    let mut value: Value =
        serde_json::from_slice(receipt).unwrap_or_else(|error| panic!("{error}"));
    value["checker"] = serde_json::json!({
        "crate": "zeno-fcis-cli", "version": "1.1.0",
        "source": "crates/zeno-fcis-cli/src/transform.rs",
        "source_sha256": transform::sha256_hex(b"older checker source"),
        "evaluator_identity": value["checker"]["evaluator_identity"]
    });
    transform::canonical_json(&value).into_bytes()
}

#[test]
fn a_refresh_rebinds_receipts_to_this_checker_and_nothing_else() {
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let padded_once = padded(&program, 1);
    let current = receipt(&original, &padded_once);
    let older = older_checker(&current);
    let rules_naming = |receipt: &[u8]| {
        with_adoption(
            &counter.rules,
            &adoption(&padded_once, receipt, Usage::NewVersion, &counter.policy),
        )
        .unwrap_or_else(|error| panic!("{error}"))
    };
    let (old_rules, new_rules) = (rules_naming(&older), rules_naming(&current));
    let with = |rules: &str, receipt: &[u8], refresh: bool| {
        let sources = ContractSources {
            rules,
            adoptions: &[AdoptionSources {
                candidate: &padded_once,
                receipt,
            }],
            ..counter.sources()
        };
        if refresh {
            refresh_receipts(sources).map(|refreshed| {
                (
                    refreshed.rules().to_owned(),
                    refreshed.receipts().to_vec(),
                    refreshed.generated().clone(),
                )
            })
        } else {
            generate_contract(sources).map(|generated| (rules.to_owned(), Vec::new(), generated))
        }
    };
    // The older identity no longer replays.
    let refused = with(&old_rules, &older, false)
        .err()
        .unwrap_or_else(|| panic!("an older checker identity does not replay"));
    assert_eq!(refused.place(), "v2/policy.json adoptions[0]");
    assert!(
        refused
            .reason()
            .ends_with("`checker` differ from the record"),
        "{}",
        refused.reason()
    );
    // A refresh rebinds the receipt and the rules, and the result replays.
    let (_, _, expected) =
        with(&new_rules, &current, false).unwrap_or_else(|error| panic!("{error}"));
    let rebound = with(&old_rules, &older, true).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        rebound,
        (new_rules.clone(), vec![current.clone()], expected.clone())
    );
    // A current receipt is left as it is.
    assert_eq!(
        with(&new_rules, &current, true).unwrap_or_else(|error| panic!("{error}")),
        (new_rules.clone(), vec![current.clone()], expected.clone())
    );
    // An interrupted refresh wrote the receipt but not the rules: running it
    // again finishes it.
    assert_eq!(
        with(&old_rules, &current, true).unwrap_or_else(|error| panic!("{error}")),
        (new_rules.clone(), vec![current.clone()], expected)
    );
    // Anything but the checker identity that would change is refused.
    let mut claimed: Value =
        serde_json::from_slice(&older).unwrap_or_else(|error| panic!("{error}"));
    claimed["usage"]["usage_preserved"] = Value::Bool(true);
    let claimed = transform::canonical_json(&claimed).into_bytes();
    let refused = with(&rules_naming(&claimed), &claimed, true)
        .err()
        .unwrap_or_else(|| panic!("a changed usage report is refused"));
    assert_eq!(
        refused.reason(),
        "v2/adoptions/1/receipt.json does not refresh against version 1's program and the candidate: `usage` differ from the record"
    );
    // A receipt the rules do not name is refused as before.
    let refused = with(&old_rules, &claimed, true)
        .err()
        .unwrap_or_else(|| panic!("an unbound receipt is refused"));
    assert_eq!(refused.place(), "v2/policy.json adoptions[0]");
}

#[test]
fn a_step_limit_must_cover_each_program_and_every_law_node() {
    let fits = StepBound {
        program: [10, 12],
        laws: 4,
        limits: [14, 16],
    };
    assert!(fits.never_binds());
    for limits in [[13, 16], [14, 15]] {
        assert!(!StepBound { limits, ..fits }.never_binds(), "{limits:?}");
    }
    // No overflow: the sum is taken in u128.
    let saturated = StepBound {
        program: [u64::MAX; 2],
        laws: u64::MAX,
        limits: [u64::MAX; 2],
    };
    assert!(!saturated.never_binds());
    // The adopted durable-counter padding reports its bound.
    let counter = template("durable-counter");
    let (program, original) = current_program(&counter);
    let padded_once = padded(&program, 1);
    let padded_receipt = receipt(&original, &padded_once);
    let adopted = with_adoption(
        &counter.rules,
        &adoption(
            &padded_once,
            &padded_receipt,
            Usage::NewVersion,
            &counter.policy,
        ),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let generated = generate_contract(ContractSources {
        rules: &adopted,
        adoptions: &[AdoptionSources {
            candidate: &padded_once,
            receipt: &padded_receipt,
        }],
        ..counter.sources()
    })
    .unwrap_or_else(|error| panic!("{error}"));
    let steps = generated.summary().adoptions[0].steps;
    let nodes = program.nodes().len() as u64;
    assert_eq!(steps.program, [nodes, nodes + 1]);
    assert_eq!(
        steps.limits,
        [steps.laws + nodes + 5, steps.laws + nodes + 1 + 5]
    );
    assert!(steps.never_binds());
}

/// The durable counter with a second delivery, ordinal 1 and idempotency
/// ordinal 2, on its accepting case.
fn counter_with_two_deliveries() -> Template {
    template("durable-counter").with_rules(|rules| {
        let mut second = rules["cases"][2]["outbox"][0].clone();
        second["ordinal"] = 1.into();
        second["idempotency_ordinal"] = 2.into();
        if let Some(outbox) = rules["cases"][2]["outbox"].as_array_mut() {
            outbox.push(second);
        }
    })
}

/// The counter's framed state, command and context.
fn counter_input(commitment: &[u8; 32], count: i128, command: u16) -> [Vec<u8>; 3] {
    let limit = 1 << 16;
    let envelope = |index: usize, payload: Vec<u8>| {
        output::encode_envelope(ROOTS[index].1, commitment, &payload, limit)
            .unwrap_or_else(|error| panic!("{error:?}"))
    };
    let state = output::encode_record(
        &[
            c::Field {
                id: 110,
                value: c::Atom::I128(count),
            },
            c::Field {
                id: 111,
                value: c::Atom::I128(0),
            },
        ],
        limit,
    );
    let command = output::encode_atom(
        c::Atom::Sum {
            type_id: 101,
            variant: command,
        },
        limit,
    );
    let context = output::encode_atom(c::Atom::Bool(true), limit);
    let encoded = |payload: Result<Vec<u8>, _>| payload.unwrap_or_else(|error| panic!("{error:?}"));
    [
        envelope(0, encoded(state)),
        envelope(1, encoded(command)),
        envelope(2, encoded(context)),
    ]
}

/// The class and each delivery's ordinal and idempotency value the library
/// Authority decides for one increment, or its refusal.
fn increment(
    contract: &Contract<'_>,
    schema: &[u8],
) -> Result<(c::Class, Vec<(u32, u128)>), String> {
    let raw = counter_input(&contract.commitment, 0, 120);
    policy::with_authority(contract, schema, |authority| {
        let evaluation = authority.evaluate(c::Raw {
            state: &raw[0],
            command: &raw[1],
            context: &raw[2],
        });
        evaluation
            .result()
            .map(|candidate| {
                let outbox = candidate
                    .outbox()
                    .iter()
                    .map(|delivery| match delivery.idempotency {
                        c::Atom::U128(value) => (delivery.ordinal, value),
                        other => panic!("idempotency {other:?}"),
                    })
                    .collect();
                (candidate.class(), outbox)
            })
            .map_err(|refusal| format!("{refusal:?}"))
    })
    .unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn deliveries_set_the_effect_limit_and_the_idempotency_domain() {
    // One delivery with ordinal 0, as in every template: unchanged.
    let counter = template("durable-counter");
    let generated = generate_contract(counter.sources()).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        generated
            .source()
            .contains(".with_limit(Resource::Effect, 1)")
    );
    assert!(
        generated
            .source()
            .contains("idempotency: c::Domain::U128 { min: 0, max: 0 }")
    );
    // Two deliveries: the limit is 2 and the domain reaches ordinal 2.
    let two = counter_with_two_deliveries();
    let generated = generate_contract(two.sources()).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        generated
            .source()
            .contains(".with_limit(Resource::Effect, 2)")
    );
    assert!(
        generated
            .source()
            .contains("idempotency: c::Domain::U128 { min: 0, max: 2 }")
    );
    // The library Authority makes both deliveries.
    let (rules, declarations) = (two.rules(), two.declarations());
    let schema = schema::encode(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|error| panic!("{error}"));
    let mut contract = Contract::build(&declarations, &rules, commitment)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(contract.budgets.effect, 2);
    assert_eq!(
        increment(&contract, &schema),
        Ok((c::Class::Accept, vec![(0, 0), (1, 2)]))
    );
    // Planted control: the limit the generator used to render refuses it.
    contract.budgets.effect = 1;
    let refused = increment(&contract, &schema).err();
    assert!(
        refused
            .as_deref()
            .is_some_and(|refusal| refusal.contains("resource: Effect, limit: 1, attempted: 2")),
        "{refused:?}"
    );
}

#[test]
fn a_case_delivers_in_increasing_ordinal_order() {
    let repeated = counter_with_two_deliveries().with_rules(|rules| {
        rules["cases"][2]["outbox"][1]["ordinal"] = 0.into();
    });
    assert_eq!(
        repeated.refusal(),
        (
            "v2/policy.json cases[2].outbox[1].ordinal".to_owned(),
            "must exceed the previous delivery's ordinal 0: a case's deliveries are in \
             increasing ordinal order"
                .to_owned()
        )
    );
}

#[test]
fn a_committed_failure_needs_a_committed_failure_law() {
    // Law 502 is the counter's only CommittedFailureEffects law; case 3 is
    // its committed failure.
    let unlawful = template("durable-counter").with_rules(|rules| {
        rules["law_kinds"]["502"] = "AuthoritySubjectRecipient".into();
    });
    let (place, reason) = unlawful.refusal();
    assert_eq!(place, "v2/policy.json cases[3]");
    assert!(
        reason.starts_with(
            "is a committed failure, but no law has kind CommittedFailureEffects, so framework \
             law 908 would refuse every committed failure at run time."
        ),
        "{reason}"
    );
    // Without committed failures, no failure law is needed: framework law 908
    // then never applies.
    let generated = generate_contract(template("withdrawal-queue").sources());
    assert!(generated.is_ok());
    assert!(
        !template("withdrawal-queue")
            .rules()
            .law_kinds
            .values()
            .any(|kind| *kind == super::rules::LawKind::CommittedFailureEffects)
    );
}

#[test]
fn a_catalog_refusal_names_the_entry_it_is_about() {
    let two = counter_with_two_deliveries();
    let (rules, declarations) = (two.rules(), two.declarations());
    let schema = schema::encode(&declarations).unwrap_or_else(|error| panic!("{error}"));
    let commitment = schema_commitment(&schema).unwrap_or_else(|error| panic!("{error}"));
    let build = || {
        Contract::build(&declarations, &rules, commitment).unwrap_or_else(|error| panic!("{error}"))
    };
    let refusal = |contract: &Contract<'_>| {
        let Err(error) = policy::encode(contract, &schema) else {
            panic!("the planted contract must be refused")
        };
        (error.place().to_owned(), error.reason().to_owned())
    };
    // A delivery outside its channel's idempotency domain, as before the
    // domain followed the rules.
    let mut contract = build();
    contract.channels[0].idempotency = 0;
    assert_eq!(
        refusal(&contract),
        (
            "v2/policy.json cases[2].outbox[1]".to_owned(),
            "the library catalog refused the generated contract (Descriptor); it admits the \
             contract with only the deliveries before this one"
                .to_owned()
        )
    );
    // A declared law, then a framework law, whose program has no root node.
    for (index, place) in [
        (0, "project.zeno law 500"),
        (
            contract.laws.len() - 1,
            "v2/policy.json framework DecisionConformance law 991",
        ),
    ] {
        let mut contract = build();
        contract.laws[index].root = contract.laws[index].nodes.len();
        let (refused, reason) = refusal(&contract);
        assert_eq!(refused, place);
        assert_eq!(
            reason,
            "the library catalog refused the generated contract (Descriptor); it admits the \
             contract when this law's formula is replaced by `true`"
        );
    }
    // A channel whose destination domain is empty.
    let mut contract = build();
    contract.channels[0].destination = model::Domain::I128 { min: 1, max: 0 };
    assert_eq!(refusal(&contract).0, "project.zeno channel 300");
    // A refusal no single entry accounts for keeps the library's own words.
    let mut contract = build();
    contract.reasons.clear();
    assert_eq!(
        refusal(&contract),
        (
            "library catalog".to_owned(),
            "refused the generated contract: Descriptor".to_owned()
        )
    );
}

/// The rows of the table after `<!-- policy-keys: NAME -->` in
/// `docs/CONTRACT_RULES.md`: its first cell without backticks, and its
/// second cell.
fn documented(name: &str) -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/CONTRACT_RULES.md");
    let page =
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let marker = format!("<!-- policy-keys: {name} -->\n");
    let start = page
        .find(&marker)
        .unwrap_or_else(|| panic!("no table marked {name}"));
    page[start + marker.len()..]
        .lines()
        .skip(2)
        .take_while(|line| line.starts_with('|'))
        .map(|line| {
            let cells: Vec<String> = line
                .replace("\\|", "\u{0}")
                .split('|')
                .map(|cell| cell.trim().replace('\u{0}', "|"))
                .collect();
            (cells[1].trim_matches('`').to_owned(), cells[2].clone())
        })
        .collect()
}

fn names(rows: &[(String, String)]) -> Vec<&str> {
    rows.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn the_rules_reference_documents_exactly_what_the_generator_reads() {
    use super::expr::{FUNCTIONS, OPERATORS};
    use super::model::required_declaration;
    use super::rules::{
        ADOPTION_KEYS, CASE_KEYS, DELIVERY_KEYS, EVOLUTION_KEYS, FILE_KEYS, LEAVES, LawKind,
        MIGRATION_FIELD_KEYS, MIGRATION_KEYS, ROOT_KEYS, SHORTCUT_KEYS,
    };
    for (table, keys) in [
        ("file", FILE_KEYS.as_slice()),
        ("roots", ROOT_KEYS.as_slice()),
        ("case", CASE_KEYS.as_slice()),
        ("delivery", DELIVERY_KEYS.as_slice()),
        ("adoption", ADOPTION_KEYS.as_slice()),
        ("evolution", EVOLUTION_KEYS.as_slice()),
        ("shortcut", SHORTCUT_KEYS.as_slice()),
        ("migration", MIGRATION_KEYS.as_slice()),
        ("migration-field", MIGRATION_FIELD_KEYS.as_slice()),
        ("leaf-bindings", LEAVES.as_slice()),
    ] {
        assert_eq!(names(&documented(table)), keys, "{table}");
    }
    let classes: Vec<&str> = Class::ALL.iter().map(|class| class.name()).collect();
    assert_eq!(names(&documented("classes")), classes);
    let operators: Vec<&str> = OPERATORS.iter().map(|(written, _)| *written).collect();
    assert_eq!(names(&documented("operators")), operators);
    let functions: Vec<&str> = FUNCTIONS.iter().map(|(name, _)| *name).collect();
    assert_eq!(names(&documented("functions")), functions);
    // Each function's documented form parses with its documented arity.
    for ((name, form), (_, arity)) in documented("functions").iter().zip(FUNCTIONS) {
        let call = form
            .split('`')
            .nth(1)
            .unwrap_or_else(|| panic!("{name} has no written form"));
        assert!(expr::parse(call).is_ok(), "{call}");
        assert_eq!(call.matches(',').count() + 1, arity, "{call}");
    }
    let kinds = documented("law-kinds");
    let expected: Vec<(String, String)> = LawKind::ALL
        .iter()
        .map(|kind| {
            let written = required_declaration(*kind)
                .map_or("any scope".to_owned(), |(_, _, written)| {
                    format!("`{written}`")
                });
            (kind.name().to_owned(), written)
        })
        .collect();
    assert_eq!(kinds, expected);
}
