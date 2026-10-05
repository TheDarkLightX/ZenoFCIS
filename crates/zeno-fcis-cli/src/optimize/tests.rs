//! Optimizer tests: the e-graph core, signatures, strategy grammar, extraction,
//! the withdrawal-queue artifacts, the Boolean benchmark seeds, planted defects
//! and determinism.

use super::egraph::{
    AddError, Caps, ClassId, EGraph, ENode, Inconsistency, Limit, Merge, MergeReason,
};
use super::extract::{self, Cost, ExtractError};
use super::rules::{self, Rewrite, Rule, Term};
use super::semantics::{DomainInfo, Interval, Kind, Signature, TypeError, op_bytes};
use super::strategy::{
    DEFAULT_STRATEGY_JSON, Extractor, Limits, MAX_LIMIT, MAX_PHASES, MAX_ROUNDS, Phase, PhaseSpec,
    STRATEGY_SCHEMA, Strategy,
};
use super::{Outcome, Refused, Search, Verdict, optimize};
use crate::transform::{self, DEFAULT_MAX_INPUT_TUPLES, Rejection, Replay};
use serde_json::{Value, json};
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program, V2ExecutionFailure};
use zeno_fcis_synthesis::finite_runtime::import_program;

const FULL: Domain = Domain::Int {
    min: i64::MIN,
    max: i64::MAX,
};
const NEAR_MAX: Domain = Domain::Int {
    min: i64::MAX - 1,
    max: i64::MAX,
};
const GENEROUS: transform::Limits = transform::Limits {
    steps: transform::DEFAULT_STEP_LIMIT,
    input_tuples: DEFAULT_MAX_INPUT_TUPLES,
};

fn artifact(name: &str) -> &'static [u8] {
    macro_rules! artifacts {
        ($($name:literal),+) => {
            match name {
                $($name => include_bytes!(concat!(
                    "../../../../docs/benchmarks/withdrawal-queue/artifacts/", $name, ".zcve"
                )),)+
                _ => panic!("unknown artifact {name}"),
            }
        };
    }
    artifacts!(
        "boolean-kernel-original",
        "boolean-kernel-candidate",
        "retained-controller-original",
        "retained-controller-candidate",
        "current-decision-scalars-original",
        "current-decision-scalars-candidate"
    )
}

fn program(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Program {
    Program::try_new(inputs, outputs, nodes, roots)
        .unwrap_or_else(|error| panic!("test program must be admitted: {error}"))
}

fn encode(program: &Program) -> Vec<u8> {
    program
        .value()
        .and_then(|value| value.canonical_bytes())
        .unwrap_or_else(|error| panic!("encode test program: {error}"))
}

fn import(bytes: &[u8]) -> Program {
    import_program(bytes).unwrap_or_else(|error| panic!("import: {error}"))
}

fn default_strategy() -> Strategy {
    Strategy::parse(DEFAULT_STRATEGY_JSON.as_bytes()).unwrap_or_else(|error| panic!("{error:?}"))
}

fn searched(outcome: Outcome) -> Search {
    match outcome {
        Outcome::Searched(search) => *search,
        other => panic!("expected a search, found {other:?}"),
    }
}

fn run(original: &[u8], strategy: &Strategy) -> Search {
    searched(optimize(original, strategy, DEFAULT_MAX_INPUT_TUPLES))
}

/// The default strategy without its semantic-merge phases.
fn without_semantic_merge() -> Strategy {
    let mut strategy = default_strategy();
    strategy
        .phases
        .retain(|phase| phase.phase != Phase::SemanticMerge);
    strategy
}

fn single(phase: Phase, rounds: u32) -> Strategy {
    Strategy {
        phases: vec![PhaseSpec { phase, rounds }],
        limits: Limits::default(),
        extractor: Extractor::DagGreedy,
    }
}

/// Builds the e-graph of a program with the default caps.
fn egraph_of(program: &Program) -> (EGraph, Vec<ClassId>) {
    let domain = DomainInfo::new(program.inputs()).unwrap_or_else(|| panic!("domain"));
    EGraph::from_program(program, domain, Limits::default().caps())
        .unwrap_or_else(|error| panic!("build e-graph: {error:?}"))
}

fn extract(egraph: &EGraph, roots: &[ClassId], base: &Program, extractor: Extractor) -> Program {
    extract::extract(egraph, roots, base.inputs(), base.outputs(), extractor, 8)
        .unwrap_or_else(|error| panic!("extract: {error:?}"))
        .program
}

fn best_nodes(search: &Search) -> Option<usize> {
    search.best.as_ref().map(|best| best.nodes)
}

fn verdicts(search: &Search) -> Vec<&'static str> {
    search
        .candidates
        .iter()
        .map(|candidate| candidate.verdict.name())
        .collect()
}

fn screened(verdict: &Verdict) -> bool {
    verdict.accepted()
        || matches!(
            verdict,
            Verdict::Duplicate | Verdict::NotSmaller | Verdict::NotBetterThanIncumbent
        )
}

/// Reads a `cases.json` decimal string.
fn decimal(value: &Value) -> i64 {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .unwrap_or_else(|| panic!("decimal string expected: {value}"))
}

fn fixture_domains(value: &Value) -> Vec<Domain> {
    value
        .as_array()
        .unwrap_or_else(|| panic!("domain list"))
        .iter()
        .map(|domain| match domain["kind"].as_str() {
            Some("Bool") => Domain::Bool,
            Some("Int") => Domain::Int {
                min: decimal(&domain["min"]),
                max: decimal(&domain["max"]),
            },
            _ => panic!("unknown fixture domain {domain}"),
        })
        .collect()
}

fn fixture_op(node: &Value) -> Op {
    let items = node
        .as_array()
        .unwrap_or_else(|| panic!("fixture node {node}"));
    let id = |index: usize| {
        items[index]
            .as_u64()
            .and_then(|id| u16::try_from(id).ok())
            .unwrap_or_else(|| panic!("fixture reference {node}"))
    };
    match (items[0].as_str(), items.len()) {
        (Some("Input"), 2) => Op::Input(id(1)),
        (Some("Int"), 2) => Op::Int(decimal(&items[1])),
        (Some("Bool"), 2) => Op::Bool(
            items[1]
                .as_bool()
                .unwrap_or_else(|| panic!("fixture literal {node}")),
        ),
        (Some("Add"), 3) => Op::Add(id(1), id(2)),
        (Some("Sub"), 3) => Op::Sub(id(1), id(2)),
        (Some("Eq"), 3) => Op::Eq(id(1), id(2)),
        (Some("Lt"), 3) => Op::Lt(id(1), id(2)),
        (Some("And"), 3) => Op::And(id(1), id(2)),
        (Some("Not"), 2) => Op::Not(id(1)),
        (Some("Select"), 4) => Op::Select(id(1), id(2), id(3)),
        _ => panic!("unknown fixture node {node}"),
    }
}

fn fixture_program(inputs: &[Domain], outputs: &[Domain], graph: &Value) -> Vec<u8> {
    let nodes = graph["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("fixture nodes"))
        .iter()
        .map(fixture_op)
        .collect();
    let roots = graph["roots"]
        .as_array()
        .unwrap_or_else(|| panic!("fixture roots"))
        .iter()
        .map(|root| {
            root.as_u64()
                .and_then(|root| u16::try_from(root).ok())
                .unwrap_or_else(|| panic!("fixture root {root}"))
        })
        .collect();
    encode(&program(inputs.to_vec(), outputs.to_vec(), nodes, roots))
}

/// One benchmark seed: id, original bytes, recorded candidate bytes.
struct Seed {
    id: String,
    original: Vec<u8>,
    candidate: Vec<u8>,
}

fn seeds(prefix: char) -> Vec<Seed> {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../docs/benchmarks/cases.json"))
            .unwrap_or_else(|error| panic!("cases.json: {error}"));
    fixtures["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("cases"))
        .iter()
        .filter(|case| case["id"].as_str().is_some_and(|id| id.starts_with(prefix)))
        .map(|case| {
            let inputs = fixture_domains(&case["input_domains"]);
            let outputs = fixture_domains(&case["output_domains"]);
            Seed {
                id: case["id"].as_str().unwrap_or_default().to_owned(),
                original: fixture_program(&inputs, &outputs, &case["original"]),
                candidate: fixture_program(&inputs, &outputs, &case["candidate"]),
            }
        })
        .collect()
}

/// `a`, `b` Boolean inputs with And(a, b) stored twice and Int(1) twice.
fn duplicated() -> Program {
    program(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool, FULL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::And(0, 1),
            Op::And(0, 1),
            Op::Int(1),
            Op::Int(1),
            Op::Add(4, 5),
        ],
        vec![3, 6],
    )
}

/// `x` in `[MAX - 1, MAX]`; the output is `x`, and an unused `x + 1` traps at `MAX`.
fn unused_trap() -> Program {
    program(
        vec![NEAR_MAX],
        vec![FULL],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![0],
    )
}

/// Seven Boolean inputs, 128 tuples: beyond exact signatures.
fn seven_inputs(tail: Vec<Op>, root: u16) -> Program {
    let nodes: Vec<Op> = (0..7).map(Op::Input).chain(tail).collect();
    program(vec![Domain::Bool; 7], vec![Domain::Bool], nodes, vec![root])
}

// ---- e-graph core -------------------------------------------------------------

#[test]
fn hash_consing_shares_identical_instructions() {
    let base = duplicated();
    let (egraph, roots) = egraph_of(&base);
    assert_eq!(egraph.enode_count(), 5, "two duplicates are shared");
    assert_eq!(egraph.class_count(), 5);
    assert_eq!(egraph.lookup(ENode::And(0, 1)), Some(2));
    assert_eq!(egraph.lookup(ENode::Int(1)), Some(3));
    assert_eq!(egraph.lookup(ENode::Int(2)), None);
    assert_eq!(roots, vec![2, 4]);
    // The shared instruction remembers its earliest original position.
    assert_eq!(egraph.node_origin(2), Some(2));
    assert_eq!(egraph.node_origin(3), Some(4));
    let extracted = extract(&egraph, &roots, &base, Extractor::Tree);
    assert_eq!(extracted.nodes().len(), 5);
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
}

#[test]
fn union_find_merges_into_the_lowest_representative() {
    let base = program(
        vec![Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Not(0), Op::Not(1)],
        vec![2],
    );
    let (mut egraph, _) = egraph_of(&base);
    // Not(Not(a)) has a's signature; a and Not(a) differ.
    assert_eq!(egraph.union(2, 0, MergeReason::Rule), Ok(Merge::Merged));
    assert_eq!(egraph.find(2), 0);
    assert_eq!(egraph.find(0), 0);
    assert_eq!(
        egraph.union(0, 2, MergeReason::Rule),
        Ok(Merge::AlreadyEqual)
    );
    assert_eq!(egraph.merges(), 1);
    assert_eq!(egraph.class_count(), 2);
    assert_eq!(egraph.class_nodes(0), vec![0, 2]);
    assert_eq!(egraph.union(0, 1, MergeReason::Rule), Ok(Merge::Refused));
    assert_eq!(egraph.refused_merges(), 1);
    assert_eq!(egraph.find(1), 1);
    // A structural merge of unequal computations is an inconsistency.
    assert_eq!(
        egraph.union(0, 1, MergeReason::Structural),
        Err(Inconsistency)
    );
}

#[test]
fn rebuild_restores_congruence_and_retires_duplicates() {
    // And(Not a, Not b) and And(Not b, Not a) are the same computation.
    let base = program(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool, Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Not(0),
            Op::Not(1),
            Op::And(2, 3),
            Op::And(3, 2),
        ],
        vec![4, 5],
    );
    let (mut egraph, roots) = egraph_of(&base);
    assert_eq!(egraph.class_count(), 6);
    let caps = Caps {
        max_enodes: 100,
        max_classes: 100,
    };
    assert_eq!(egraph.add(ENode::And(3, 2), caps), Ok(5), "already present");
    assert_eq!(
        egraph.union(4, 5, MergeReason::Structural),
        Ok(Merge::Merged)
    );
    assert!(!egraph.clean());
    egraph.rebuild().unwrap_or_else(|error| panic!("{error:?}"));
    assert!(egraph.clean());
    assert_eq!(egraph.find(5), 4);
    let extracted = extract(&egraph, &roots, &base, Extractor::Tree);
    assert_eq!(extracted.nodes().len(), 5);
    assert_eq!(extracted.roots(), &[4, 4]);
    // Congruence: merging a with Not(Not(a)) makes And(a, b) and
    // And(Not(Not(a)), b) the same e-node; the duplicate is retired.
    let base = program(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool, Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Not(0),
            Op::Not(2),
            Op::And(0, 1),
            Op::And(3, 1),
        ],
        vec![4, 5],
    );
    let (mut egraph, roots) = egraph_of(&base);
    assert_eq!(egraph.union(0, 3, MergeReason::Rule), Ok(Merge::Merged));
    egraph.rebuild().unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(egraph.find(4), egraph.find(5));
    assert_eq!(egraph.live_node_count(), 5);
    assert!(!egraph.node_alive(5));
    assert_eq!(egraph.class_nodes(4), vec![4]);
    assert_eq!(
        egraph.enode_count(),
        6,
        "retired nodes still count for the cap"
    );
    let extracted = extract(&egraph, &roots, &base, Extractor::Tree);
    assert_eq!(extracted.nodes().len(), 3);
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
}

#[test]
fn caps_stop_growth_before_anything_is_created() {
    let base = duplicated();
    let domain = DomainInfo::new(base.inputs()).unwrap_or_else(|| panic!("domain"));
    let tight = Caps {
        max_enodes: 4,
        max_classes: 100,
    };
    assert!(matches!(
        EGraph::from_program(&base, domain, tight),
        Err(AddError::Limit(Limit::ENodes))
    ));
    let (mut egraph, _) = egraph_of(&base);
    let full = Caps {
        max_enodes: 5,
        max_classes: 5,
    };
    // Existing nodes are still shared at the cap; new ones are refused.
    assert_eq!(egraph.add(ENode::Int(1), full), Ok(3));
    assert_eq!(
        egraph.add(ENode::Int(2), full),
        Err(AddError::Limit(Limit::ENodes))
    );
    let classes = Caps {
        max_enodes: 100,
        max_classes: 5,
    };
    assert_eq!(
        egraph.add(ENode::Int(2), classes),
        Err(AddError::Limit(Limit::Classes))
    );
    let open = Caps {
        max_enodes: 100,
        max_classes: 100,
    };
    assert_eq!(
        egraph.add(ENode::And(3, 3), open),
        Err(AddError::Type(TypeError::Mismatch))
    );
    assert_eq!(
        egraph.add(ENode::Input(7), open),
        Err(AddError::Type(TypeError::InputReference))
    );
    assert_eq!(egraph.enode_count(), 5, "nothing was created");
    assert_eq!(Limit::ENodes.name(), "max_enodes");
    assert_eq!(Limit::Classes.name(), "max_classes");
    assert_eq!(Limit::Rewrites.name(), "max_rewrites_per_round");
}

// ---- signatures and trap analysis --------------------------------------------

#[test]
fn signatures_follow_the_checker_enumeration_and_carry_poison() {
    let base = program(
        vec![Domain::Bool, NEAR_MAX],
        vec![Domain::Bool, FULL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(1),
            Op::Add(1, 2),
            Op::Not(0),
            Op::And(0, 4),
            Op::Eq(3, 1),
        ],
        vec![0, 3],
    );
    let (egraph, _) = egraph_of(&base);
    assert!(egraph.domain().exact());
    assert_eq!(egraph.domain().size(), Some(4));
    assert_eq!(egraph.domain().full_mask(), 0b1111);
    let signature = |class: ClassId| {
        egraph
            .data(class)
            .signature
            .clone()
            .unwrap_or_else(|| panic!("exact"))
    };
    // Last input fastest: (0, MAX-1), (0, MAX), (1, MAX-1), (1, MAX).
    assert_eq!(signature(0).values(), &[Some(0), Some(0), Some(1), Some(1)]);
    assert_eq!(signature(0).bits(), Some(0b1100));
    assert_eq!(
        signature(1).values(),
        &[
            Some(i64::MAX - 1),
            Some(i64::MAX),
            Some(i64::MAX - 1),
            Some(i64::MAX)
        ]
    );
    // The addition traps where x = MAX; its users inherit the poison.
    assert_eq!(
        signature(3).values(),
        &[Some(i64::MAX), None, Some(i64::MAX), None]
    );
    assert!(signature(3).poisoned());
    assert_eq!(signature(3).bits(), None);
    assert_eq!(signature(6).values(), &[Some(0), None, Some(0), None]);
    assert!(egraph.node_may_trap(3));
    assert_eq!(egraph.pinned(), vec![3]);
    // And(a, Not a) is constantly false; Not(a) complements a.
    assert_eq!(signature(5).constant_value(), Some(0));
    assert_eq!(egraph.data(5).constant(), Some(0));
    assert_eq!(signature(4).bits(), Some(0b0011));
    assert!(!egraph.node_may_trap(5));
    assert_eq!(egraph.data(3).kind, Kind::Int);
    assert_eq!(egraph.data(6).kind, Kind::Bool);
    assert!(signature(0).agrees_on(&signature(5), 0b0011));
    assert!(!signature(0).agrees_on(&signature(5), 0b1100));
    // Signatures over 64 tuples use every bit.
    let six = program(
        vec![Domain::Bool; 6],
        vec![Domain::Bool],
        vec![Op::Input(5)],
        vec![0],
    );
    let (egraph, _) = egraph_of(&six);
    assert_eq!(egraph.domain().full_mask(), u64::MAX);
    assert_eq!(egraph.domain().tuple_count(), 64);
    assert_eq!(
        egraph.data(0).signature.as_ref().and_then(Signature::bits),
        Some(0xaaaa_aaaa_aaaa_aaaa)
    );
}

#[test]
fn interval_analysis_is_conservative_without_exact_signatures() {
    let small = Domain::Int { min: 0, max: 2 };
    let wide = Domain::Int { min: 0, max: 64 };
    let base = program(
        vec![Domain::Bool, small, wide, NEAR_MAX],
        vec![FULL, Domain::Bool, FULL, FULL],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::Input(3),
            Op::Add(1, 2),       // 4: [0, 66], total
            Op::Int(100),        // 5
            Op::Lt(4, 5),        // 6: always true
            Op::Int(1),          // 7
            Op::Add(3, 7),       // 8: may trap
            Op::Select(0, 4, 8), // 9: hull, may be poisoned
            Op::Sub(1, 4),       // 10: [-66, 2]
            Op::Eq(1, 5),        // 11: always false
        ],
        vec![4, 6, 9, 10],
    );
    let (mut egraph, _) = egraph_of(&base);
    assert!(!egraph.domain().exact());
    assert_eq!(egraph.domain().size(), Some(2 * 3 * 65 * 2));
    assert_eq!(egraph.domain().tuple_count(), 0);
    assert_eq!(egraph.data(4).interval, Interval { min: 0, max: 66 });
    assert!(!egraph.node_may_trap(4));
    assert_eq!(egraph.data(6).constant(), Some(1));
    assert_eq!(egraph.data(11).constant(), Some(0));
    assert!(egraph.node_may_trap(8));
    assert!(egraph.data(8).may_poison);
    assert_eq!(egraph.data(8).constant(), None);
    assert!(egraph.data(9).may_poison);
    assert_eq!(
        egraph.data(9).interval,
        Interval {
            min: 0,
            max: i64::MAX
        }
    );
    assert_eq!(egraph.data(10).interval, Interval { min: -66, max: 2 });
    assert_eq!(egraph.pinned(), vec![8]);
    // Poisoned classes never merge by rule without exact signatures.
    assert_eq!(egraph.union(9, 4, MergeReason::Rule), Ok(Merge::Refused));
    // Unpoisoned classes of one kind may merge by rule, but disjoint bounds
    // are still caught: the two constants cannot be the same value.
    assert_eq!(egraph.union(6, 11, MergeReason::Rule), Err(Inconsistency));
    assert_eq!(egraph.union(6, 0, MergeReason::Rule), Ok(Merge::Merged));
    assert_eq!(
        egraph.data(0).interval,
        Interval::point(1),
        "the meet of [0, 1] and [1, 1]"
    );
    assert_eq!(Interval::point(3).constant(), Some(3));
    assert_eq!(
        Interval { min: 1, max: 2 }.meet(Interval { min: 3, max: 4 }),
        None
    );
}

#[test]
fn op_bytes_match_the_canonical_encoding() {
    let base = |nodes: Vec<Op>, root: u16| {
        encode(&program(
            vec![Domain::Bool, FULL],
            vec![Domain::Bool],
            nodes,
            vec![root],
        ))
        .len() as u64
    };
    let leaf = base(vec![Op::Input(0)], 0);
    assert_eq!(
        base(vec![Op::Input(0), Op::Not(0)], 1) - leaf,
        op_bytes(&Op::Not(0))
    );
    assert_eq!(
        base(vec![Op::Input(0), Op::And(0, 0)], 1) - leaf,
        op_bytes(&Op::And(0, 0))
    );
    assert_eq!(
        base(vec![Op::Input(0), Op::Select(0, 0, 0)], 1) - leaf,
        op_bytes(&Op::Select(0, 0, 0))
    );
    assert_eq!(
        base(vec![Op::Input(0), Op::Bool(true)], 1) - leaf,
        op_bytes(&Op::Bool(true))
    );
    assert_eq!(op_bytes(&Op::Int(i64::MIN)), 39);
    assert_eq!(op_bytes(&Op::Eq(0, 1)), 56);
    assert_eq!(ENode::Select(0, 0, 0).bytes(), 73);
    // Whole programs: the kernel's 16 and 7 instructions.
    assert_eq!(artifact("boolean-kernel-original").len(), 1063);
    assert_eq!(artifact("boolean-kernel-candidate").len(), 763);
}

// ---- strategy grammar ---------------------------------------------------------

#[test]
fn default_strategy_is_fixed_and_versioned() {
    let parsed = default_strategy();
    assert_eq!(STRATEGY_SCHEMA, "zeno-fcis/optimize-strategy/1");
    assert_eq!(parsed.phases.len(), 8);
    assert_eq!(parsed.extractor, Extractor::DagGreedy);
    assert_eq!(parsed.limits, Limits::default());
    let reparsed = Strategy::parse(parsed.json().to_string().as_bytes())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(reparsed, parsed);
    assert_eq!(
        parsed
            .phases
            .iter()
            .map(|phase| phase.phase.name())
            .collect::<Vec<_>>(),
        [
            "fold",
            "share",
            "boolean",
            "select",
            "semantic-merge",
            "boolean",
            "select",
            "fold"
        ]
    );
    assert_eq!(Extractor::Tree.name(), "tree");
}

#[test]
fn strategy_grammar_is_closed() {
    let valid = json!({
        "schema": STRATEGY_SCHEMA,
        "phases": [{"phase": "boolean", "rounds": 2}],
        "limits": {"max_enodes": 500},
        "extractor": "tree"
    });
    let strategy =
        Strategy::parse(valid.to_string().as_bytes()).unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(strategy.limits.max_enodes, 500);
    assert_eq!(
        strategy.limits.max_classes, 10_000,
        "unspecified limits default"
    );
    assert_eq!(strategy.extractor, Extractor::Tree);
    let minimal = json!({"schema": STRATEGY_SCHEMA, "phases": [{"phase": "fold", "rounds": 1}]});
    assert!(Strategy::parse(minimal.to_string().as_bytes()).is_ok());
    let refused = |document: Value, expected: &str| {
        let error = match Strategy::parse(document.to_string().as_bytes()) {
            Err(error) => error,
            Ok(strategy) => panic!("{document} must be refused, parsed {strategy:?}"),
        };
        assert!(
            error.message.contains(expected),
            "{document}: {}",
            error.message
        );
    };
    let mut document = valid.clone();
    document["extra"] = json!(1);
    refused(document, "unknown field");
    let mut document = valid.clone();
    document["phases"][0]["weight"] = json!(1);
    refused(document, "unknown field");
    let mut document = valid.clone();
    document["limits"]["max_time"] = json!(1);
    refused(document, "unknown field");
    let mut document = valid.clone();
    document["phases"][0]["phase"] = json!("egg");
    refused(document, "unknown variant");
    let mut document = valid.clone();
    document["extractor"] = json!("ilp");
    refused(document, "unknown variant");
    let mut document = valid.clone();
    document["phases"][0]["rounds"] = json!("2");
    refused(document, "invalid type");
    let mut document = valid.clone();
    document["phases"][0]["rounds"] = json!(2.5);
    refused(document, "invalid type");
    let mut document = valid.clone();
    document["phases"][0]["rounds"] = json!(-1);
    refused(document, "invalid value");
    let mut document = valid.clone();
    document["phases"][0]["rounds"] = json!(0);
    refused(document, "rounds must be between 1 and 8");
    let mut document = valid.clone();
    document["phases"][0]["rounds"] = json!(MAX_ROUNDS + 1);
    refused(document, "rounds must be between");
    let mut document = valid.clone();
    document["phases"] = json!([]);
    refused(document, "phases must not be empty");
    let mut document = valid.clone();
    document["phases"] = json!(vec![json!({"phase": "fold", "rounds": 1}); MAX_PHASES + 1]);
    refused(document, "at most 16 phases");
    let mut document = valid.clone();
    document["limits"]["max_enodes"] = json!(0);
    refused(document, "limits.max_enodes must be between 1 and 100000");
    let mut document = valid.clone();
    document["limits"]["max_classes"] = json!(MAX_LIMIT + 1);
    refused(document, "limits.max_classes must be between");
    let mut document = valid.clone();
    document["schema"] = json!("zeno-fcis/optimize-strategy/2");
    refused(document, "schema must be");
    let mut document = valid.clone();
    if let Some(object) = document.as_object_mut() {
        object.remove("schema");
    }
    refused(document, "missing field");
    let mut document = valid.clone();
    if let Some(object) = document.as_object_mut() {
        object.remove("phases");
    }
    refused(document, "missing field");
    refused(json!([1, 2]), "not a strategy document");
    refused(json!("boolean"), "not a strategy document");
    // Duplicate keys are refused rather than resolved silently.
    let duplicate = format!(
        "{{\"schema\": {STRATEGY_SCHEMA:?}, \"phases\": [{{\"phase\": \"fold\", \"rounds\": 1}}], \"phases\": []}}"
    );
    assert!(
        Strategy::parse(duplicate.as_bytes())
            .is_err_and(|error| error.message.contains("duplicate field"))
    );
    assert!(Strategy::parse(b"not json").is_err());
    assert!(Strategy::parse(b"").is_err());
}

// ---- phases ------------------------------------------------------------------

#[test]
fn fold_phase_folds_constants_but_never_a_possible_trap() {
    // Not(false) and And(true, Not(false)) fold; Add(MAX, 1) always traps and stays.
    let base = program(
        vec![],
        vec![Domain::Bool, Domain::Bool],
        vec![
            Op::Bool(true),
            Op::Bool(false),
            Op::Not(1),
            Op::And(0, 2),
            Op::Int(i64::MAX),
            Op::Int(1),
            Op::Add(4, 5),
            Op::Int(0),
            Op::Lt(6, 7),
        ],
        vec![3, 8],
    );
    let (mut egraph, roots) = egraph_of(&base);
    let report = rules::run_phase(&mut egraph, Phase::Fold, 3, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.phase, Phase::Fold);
    assert!(report.merges >= 2, "{report:?}");
    assert_eq!(report.refused, 0);
    assert!(report.saturated);
    assert_eq!(egraph.find(2), egraph.find(0), "Not(false) = true");
    assert_eq!(egraph.find(3), egraph.find(0), "And(true, true) = true");
    assert_eq!(egraph.pinned(), vec![6]);
    assert_eq!(egraph.data(6).constant(), None);
    assert_eq!(
        egraph.data(8).constant(),
        None,
        "a poisoned comparison is not constant"
    );
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    // true, MAX, 1, Add, 0, Lt: the trapping addition is kept.
    assert_eq!(extracted.nodes().len(), 6);
    assert!(extracted.nodes().iter().any(|op| matches!(op, Op::Add(..))));
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
    let search = run(&encode(&base), &default_strategy());
    assert_eq!(best_nodes(&search), Some(6));
    assert_eq!(search.pinned_nodes, 1);
}

#[test]
fn share_phase_merges_commuted_conjunctions() {
    let base = program(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool, Domain::Bool, Domain::Bool, Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::And(0, 1),
            Op::And(1, 0),
            Op::Eq(0, 1),
            Op::Eq(1, 0),
        ],
        vec![2, 3, 4, 5],
    );
    let (mut egraph, roots) = egraph_of(&base);
    let report = rules::run_phase(&mut egraph, Phase::Share, 2, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.merges, 2, "{report:?}");
    assert_eq!(egraph.find(2), egraph.find(3));
    assert_eq!(egraph.find(4), egraph.find(5));
    let extracted = extract(&egraph, &roots, &base, Extractor::Tree);
    assert_eq!(extracted.nodes().len(), 4);
    let search = run(&encode(&base), &single(Phase::Share, 1));
    assert_eq!(best_nodes(&search), Some(4));
    // Commutation is structural, so it also applies beyond exact signatures.
    // Unused inputs are instructions too, and the candidate drops them.
    let base = seven_inputs(vec![Op::And(0, 6), Op::And(6, 0), Op::And(7, 8)], 9);
    let search = run(&encode(&base), &single(Phase::Share, 1));
    assert_eq!(best_nodes(&search), Some(4));
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert_eq!(
        import(&best.bytes).nodes(),
        &[Op::Input(0), Op::Input(6), Op::And(0, 1), Op::And(2, 2)]
    );
}

#[test]
fn semantic_merge_needs_exact_signatures() {
    let base = seven_inputs(vec![Op::Not(0), Op::Not(7)], 8);
    let (mut egraph, _) = egraph_of(&base);
    assert!(!egraph.domain().exact());
    let report = rules::run_phase(&mut egraph, Phase::SemanticMerge, 2, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.rounds_run, 0);
    assert!(report.skipped.is_some());
    assert_eq!(report.merges, 0);
    // The Boolean phase still removes the double negation by rule.
    let search = run(&encode(&base), &default_strategy());
    assert_eq!(best_nodes(&search), Some(1));
    assert!(
        search
            .phases
            .iter()
            .any(|phase| phase.phase == Phase::SemanticMerge && phase.skipped.is_some())
    );
    assert!(!search.exact_signatures);
}

#[test]
fn semantic_merge_alone_finds_the_select_or_form() {
    let search = run(
        artifact("boolean-kernel-original"),
        &single(Phase::SemanticMerge, 1),
    );
    assert_eq!(best_nodes(&search), Some(7));
    let search = run(
        artifact("boolean-kernel-original"),
        &single(Phase::Boolean, 1),
    );
    assert_eq!(
        best_nodes(&search),
        Some(7),
        "the or-select rule finds it too"
    );
}

// ---- extraction --------------------------------------------------------------

#[test]
fn extraction_is_deterministic_and_dag_greedy_never_loses_to_tree() {
    let base = import(artifact("retained-controller-original"));
    let (mut egraph, roots) = egraph_of(&base);
    for spec in &default_strategy().phases {
        rules::run_phase(&mut egraph, spec.phase, spec.rounds, &Limits::default())
            .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let run_extractor = |extractor, rounds| {
        extract::extract(
            &egraph,
            &roots,
            base.inputs(),
            base.outputs(),
            extractor,
            rounds,
        )
        .unwrap_or_else(|error| panic!("{error:?}"))
    };
    let tree = run_extractor(Extractor::Tree, 8);
    let dag = run_extractor(Extractor::DagGreedy, 8);
    let again = run_extractor(Extractor::DagGreedy, 8);
    assert_eq!(dag, again);
    assert_eq!(tree.rounds_run, 0);
    assert!(dag.cost <= tree.cost, "{:?} vs {:?}", dag.cost, tree.cost);
    assert_eq!(dag.cost.nodes, dag.program.nodes().len() as u64);
    assert_eq!(
        dag.cost.bytes,
        dag.program.nodes().iter().map(op_bytes).sum::<u64>()
    );
    assert!(
        Cost {
            nodes: 2,
            bytes: 1000
        } < Cost { nodes: 3, bytes: 1 }
    );
    assert!(Cost { nodes: 2, bytes: 1 } < Cost { nodes: 2, bytes: 2 });
    // A round limit of zero reports the limit; a generous one does not.
    let capped = run_extractor(Extractor::DagGreedy, 0);
    assert!(capped.limit_hit);
    assert_eq!(capped.rounds_run, 0);
    assert_eq!(capped.program, tree.program);
    assert!(!dag.limit_hit);
    // Both extractions are judged equivalent to the original.
    for extraction in [&tree, &dag] {
        assert!(transform::check(&encode(&base), &encode(&extraction.program), GENEROUS).is_ok());
    }
}

#[test]
fn extraction_keeps_every_pinned_node_and_dropping_one_is_refuted() {
    let base = unused_trap();
    let (egraph, roots) = egraph_of(&base);
    assert_eq!(egraph.pinned(), vec![2]);
    let kept = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    assert_eq!(kept.nodes().len(), 3);
    assert!(transform::check(&encode(&base), &encode(&kept), GENEROUS).is_ok());
    // Planted defect: an extractor that forgets the pinned node.
    let dropped = extract::extract_with(
        &egraph,
        &roots,
        base.inputs(),
        base.outputs(),
        Extractor::DagGreedy,
        8,
        &[],
    )
    .unwrap_or_else(|error| panic!("{error:?}"))
    .program;
    assert_eq!(dropped.nodes().len(), 1);
    let Err(Rejection::Counterexample(witness)) =
        transform::check(&encode(&base), &encode(&dropped), GENEROUS)
    else {
        panic!("the judge must refute the dropped trap");
    };
    assert_eq!(witness.input, vec![i64::MAX]);
    assert_eq!(witness.original.result, Err(V2ExecutionFailure::Arithmetic));
    assert_eq!(witness.candidate.result, Ok(vec![i64::MAX]));
    // The optimizer itself finds nothing to improve: the trap must stay.
    let search = run(&encode(&base), &default_strategy());
    assert_eq!(best_nodes(&search), None);
    assert!(
        verdicts(&search)
            .iter()
            .all(|verdict| *verdict == "not-smaller")
    );
    assert_eq!(search.pinned_nodes, 1);
}

// ---- acceptance: withdrawal artifacts ----------------------------------------

#[test]
fn withdrawal_kernel_reaches_seven_nodes_with_a_replayable_receipt() {
    let original = artifact("boolean-kernel-original");
    let search = run(original, &default_strategy());
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert_eq!(best.nodes, 7);
    assert_eq!(best.max_steps, 7);
    assert_eq!(best.bytes.len(), 763);
    assert_eq!(import(&best.bytes).nodes().len(), 7);
    let receipt = best.equivalence.receipt();
    assert_eq!(
        transform::replay(&receipt, original, &best.bytes, DEFAULT_MAX_INPUT_TUPLES),
        Replay::Matched
    );
    assert_eq!(best.equivalence.receipt_value()["inputs_checked"], 16);
    assert!(search.exact_signatures);
    assert_eq!(search.domain_size, 16);
    assert_eq!(search.pinned_nodes, 0);
    assert!(!search.any_limit_hit(), "{:?}", search.json()["search"]);
    assert_eq!(
        import(artifact("boolean-kernel-candidate")).nodes().len(),
        7
    );
}

#[test]
fn withdrawal_controller_reaches_at_most_sixty_nodes_with_a_replayable_receipt() {
    let original = artifact("retained-controller-original");
    let search = run(original, &default_strategy());
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert!(best.nodes <= 60, "{}", best.nodes);
    assert_eq!(
        best.nodes, 46,
        "the current engine's result; change deliberately"
    );
    assert_eq!(best.max_steps, 46);
    assert_eq!(
        transform::replay(
            &best.equivalence.receipt(),
            original,
            &best.bytes,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
    assert_eq!(best.equivalence.receipt_value()["inputs_checked"], 384);
    assert!(
        !search.exact_signatures,
        "384 tuples: conservative annotations"
    );
    assert_eq!(
        search.pinned_nodes, 0,
        "the controller's arithmetic cannot overflow"
    );
    assert_eq!(
        import(artifact("retained-controller-candidate"))
            .nodes()
            .len(),
        60
    );
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict))
    );
}

/// The current decision graph has 1,296,000 tuples; each judged candidate is a
/// full enumeration, so this runs only on request.
#[test]
#[ignore = "about a minute per judged candidate in a debug build"]
fn withdrawal_current_decision_graph_stretch() {
    let original = artifact("current-decision-scalars-original");
    let search = run(original, &default_strategy());
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert!(best.nodes <= 100, "{}", best.nodes);
    assert_eq!(
        transform::replay(
            &best.equivalence.receipt(),
            original,
            &best.bytes,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
}

// ---- acceptance: Boolean benchmark seeds -------------------------------------

/// The sixteen Boolean seeds: original size, recorded candidate size and the
/// size the default strategy reaches. B11 to B14 carry deliberately wrong
/// candidates; the optimizer starts from their originals, which are minimal.
const BOOLEAN_EXPECTED: [(&str, usize, usize, Option<usize>); 16] = [
    ("B01", 3, 1, Some(1)),
    ("B02", 7, 1, Some(1)),
    ("B03", 9, 8, Some(5)),
    ("B04", 4, 1, Some(1)),
    ("B05", 6, 5, Some(5)),
    ("B06", 10, 4, Some(4)),
    ("B07", 14, 9, Some(6)),
    ("B08", 7, 6, Some(6)),
    ("B09", 13, 11, Some(11)),
    ("B10", 4, 1, Some(1)),
    ("B11", 4, 5, None),
    ("B12", 4, 4, None),
    ("B13", 4, 8, None),
    ("B14", 2, 2, None),
    ("B15", 1, 1, None),
    ("B16", 3, 3, None),
];

#[test]
fn boolean_seeds_are_judged_and_never_grow() {
    let seeds = seeds('B');
    assert_eq!(seeds.len(), 16);
    for (seed, expected) in seeds.iter().zip(BOOLEAN_EXPECTED) {
        assert_eq!(seed.id, expected.0);
        let original = import(&seed.original);
        assert_eq!(original.nodes().len(), expected.1, "{}", seed.id);
        assert_eq!(
            import(&seed.candidate).nodes().len(),
            expected.2,
            "{}",
            seed.id
        );
        let search = run(&seed.original, &default_strategy());
        assert_eq!(
            best_nodes(&search),
            expected.3,
            "{}: {:?}",
            seed.id,
            verdicts(&search)
        );
        if let Some(best) = &search.best {
            assert!(best.nodes < original.nodes().len());
            assert!(best.bytes.len() < seed.original.len());
            assert_eq!(
                transform::replay(
                    &best.equivalence.receipt(),
                    &seed.original,
                    &best.bytes,
                    DEFAULT_MAX_INPUT_TUPLES
                ),
                Replay::Matched,
                "{}",
                seed.id
            );
        }
        // Every judged candidate passed the checker; the rest were screened
        // out by cost before judging. None was rejected by the checker.
        for candidate in &search.candidates {
            assert!(
                screened(&candidate.verdict),
                "{}: {:?}",
                seed.id,
                candidate.verdict
            );
        }
        assert!(!search.any_limit_hit(), "{}", seed.id);
    }
}

#[test]
fn semantic_merge_never_worsens_the_boolean_aggregate() {
    let mut with = 0;
    let mut without = 0;
    let mut table = Vec::new();
    for seed in seeds('B') {
        let original = import(&seed.original).nodes().len();
        let on = best_nodes(&run(&seed.original, &default_strategy())).unwrap_or(original);
        let off = best_nodes(&run(&seed.original, &without_semantic_merge())).unwrap_or(original);
        assert!(on <= original && off <= original);
        with += on;
        without += off;
        table.push((seed.id.clone(), original, off, on));
    }
    assert!(
        with <= without,
        "with semantic merge {with}, without {without}: {table:?}"
    );
    // Current figures over all sixteen originals (unchanged originals count
    // at their own size); B07 is the case semantic merge improves (7 -> 6).
    assert_eq!((with, without), (59, 60), "{table:?}");
}

#[test]
fn integer_seeds_keep_every_trap() {
    // I08 to I14 carry deliberately wrong candidates that drop traps or
    // change results; the optimizer never reproduces them.
    let expected: [(&str, Option<usize>); 16] = [
        ("I01", Some(1)),
        ("I02", Some(1)),
        ("I03", Some(1)),
        ("I04", None),
        ("I05", Some(1)),
        ("I06", Some(3)),
        ("I07", Some(3)),
        ("I08", None),
        ("I09", None),
        ("I10", None),
        ("I11", Some(3)),
        ("I12", None),
        ("I13", None),
        ("I14", Some(3)),
        ("I15", None),
        ("I16", None),
    ];
    for (seed, (id, best)) in seeds('I').iter().zip(expected) {
        assert_eq!(seed.id, id);
        let search = run(&seed.original, &default_strategy());
        assert_eq!(best_nodes(&search), best, "{id}: {:?}", verdicts(&search));
        assert!(
            search
                .candidates
                .iter()
                .all(|candidate| screened(&candidate.verdict)),
            "{id}"
        );
        if let Some(best) = &search.best {
            assert_eq!(
                transform::replay(
                    &best.equivalence.receipt(),
                    &seed.original,
                    &best.bytes,
                    DEFAULT_MAX_INPUT_TUPLES
                ),
                Replay::Matched,
                "{id}"
            );
        }
    }
}

// ---- planted defects ---------------------------------------------------------

/// An unsound rule: `And(a, b) = Or(a, b)`.
fn planted_and_is_or(_: &EGraph, class: ClassId, node: ENode, out: &mut Vec<Rewrite>) {
    if let ENode::And(a, b) = node {
        out.push(Rewrite {
            lhs: class,
            rhs: Term::Select(
                Box::new(Term::Class(a)),
                Box::new(Term::Class(a)),
                Box::new(Term::Class(b)),
            ),
            rule: "planted-and-is-or",
            reason: MergeReason::Rule,
        });
    }
}

#[test]
fn planted_unsound_rule_is_refused_by_the_guard_or_the_judge() {
    let rules: Vec<Rule> = vec![planted_and_is_or];
    // With exact signatures the merge guard refuses every planted merge.
    let base = import(artifact("boolean-kernel-original"));
    let (mut egraph, roots) = egraph_of(&base);
    let report = rules::run_rules(&mut egraph, &rules, 2, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.merges, 0);
    assert!(report.refused >= 3, "{report:?}");
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
    // Beyond 64 tuples the conservative guard cannot see values, so the
    // planted merge goes through and only the judge stands between it and
    // the output: it refutes the wrong form on the first distinguishing tuple.
    let base = seven_inputs(vec![Op::And(0, 6)], 7);
    let (mut egraph, roots) = egraph_of(&base);
    let report = rules::run_rules(&mut egraph, &rules, 1, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.merges, 1, "{report:?}");
    let class = egraph.find(roots[0]);
    assert_eq!(
        egraph.class_nodes(class).len(),
        2,
        "And and the planted Or share a class"
    );
    let planted = seven_inputs(vec![Op::Select(0, 0, 6)], 7);
    let Err(Rejection::Counterexample(witness)) =
        transform::check(&encode(&base), &encode(&planted), GENEROUS)
    else {
        panic!("the judge must refute Or for And");
    };
    assert_eq!(witness.ordinal, 1);
    assert_eq!(witness.input, vec![0, 0, 0, 0, 0, 0, 1]);
    // Whatever the extractor picks from the corrupted class, the judge
    // decides: the result is accepted only when it is still the And.
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    let judged = transform::check(&encode(&base), &encode(&extracted), GENEROUS);
    let uses_or = extracted
        .nodes()
        .iter()
        .any(|op| matches!(op, Op::Select(..)));
    assert_eq!(judged.is_ok(), !uses_or);
}

#[test]
fn candidates_the_judge_rejects_are_never_the_result() {
    // Drive the optimizer's judge with a wrong extraction (the pinned trap
    // dropped) and confirm it is recorded as a counterexample, never accepted.
    let base = unused_trap();
    let (egraph, roots) = egraph_of(&base);
    let wrong = extract::extract_with(
        &egraph,
        &roots,
        base.inputs(),
        base.outputs(),
        Extractor::DagGreedy,
        8,
        &[],
    );
    let search = run(&encode(&base), &single(Phase::Fold, 1));
    let (candidate, accepted) = super::judge(
        &search,
        &encode(&base),
        &base,
        0,
        Phase::Fold,
        wrong,
        GENEROUS,
    );
    assert!(accepted.is_none());
    assert_eq!(candidate.verdict.name(), "counterexample");
    assert_eq!(candidate.nodes, Some(1));
    let detail = super::candidate_json(&candidate);
    assert_eq!(detail["verdict"], "counterexample");
    assert_eq!(detail["detail"]["ordinal"], 1);
    assert_eq!(detail["detail"]["input"], json!(["9223372036854775807"]));
    assert_eq!(detail["detail"]["original"]["error"], "Arithmetic");
    assert_eq!(
        detail["detail"]["candidate"]["ok"],
        json!(["9223372036854775807"])
    );
    // The other rejection shapes render in the checker's vocabulary.
    let json = super::rejection_json(&Rejection::Inconclusive(
        transform::Inconclusive::DomainTooLarge {
            size: Some(16),
            limit: 15,
        },
    ));
    assert_eq!(json["cause"], "domain-too-large");
    assert_eq!(json["domain_size"], "16");
    let json = super::rejection_json(&Rejection::Refused(transform::Refusal::EmptyInputDomain {
        position: 2,
    }));
    assert_eq!(json["reason"], "empty-input-domain");
}

// ---- determinism, screening and refusals --------------------------------------

#[test]
fn repeated_runs_are_byte_identical() {
    for original in [
        artifact("retained-controller-original"),
        artifact("boolean-kernel-original"),
    ] {
        let first = run(original, &default_strategy());
        let second = run(original, &default_strategy());
        assert_eq!(first.json(), second.json());
        let (first, second) = (
            first.best.as_ref().unwrap_or_else(|| panic!("improvement")),
            second
                .best
                .as_ref()
                .unwrap_or_else(|| panic!("improvement")),
        );
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.equivalence.receipt(), second.equivalence.receipt());
    }
}

#[test]
fn search_report_states_its_bounds() {
    let search = run(artifact("boolean-kernel-original"), &default_strategy());
    let report = search.json();
    assert_eq!(report["original"]["nodes"], 16);
    assert_eq!(report["original"]["bytes"], 1063);
    assert_eq!(
        report["domain"],
        json!({"size": 16, "exact_signatures": true, "max_input_tuples": DEFAULT_MAX_INPUT_TUPLES})
    );
    assert_eq!(report["strategy"], default_strategy().json());
    assert_eq!(report["search"]["phases_requested"], 8);
    assert_eq!(report["search"]["phases_run"], 8);
    assert_eq!(report["search"]["any_limit_hit"], false);
    assert_eq!(report["search"]["stopped"], Value::Null);
    assert_eq!(report["search"]["pinned_nodes"], 0);
    let phases = report["search"]["phases"]
        .as_array()
        .unwrap_or_else(|| panic!("phases"));
    assert_eq!(phases.len(), 8);
    assert_eq!(phases[0]["phase"], "fold");
    assert_eq!(phases[0]["rounds_requested"], 1);
    assert_eq!(phases[0]["rounds_run"], 1);
    assert_eq!(phases[4]["phase"], "semantic-merge");
    assert_eq!(phases[4]["skipped"], Value::Null);
    for phase in phases {
        assert!(phase["enodes"].as_u64().is_some_and(|enodes| enodes >= 16));
        assert!(phase["classes"].as_u64().is_some());
        assert_eq!(phase["limit_hit"], Value::Null);
        assert!(phase["merges"].as_u64().is_some());
        assert!(phase["refused_merges"].as_u64().is_some());
    }
    let candidates = report["candidates"]
        .as_array()
        .unwrap_or_else(|| panic!("candidates"));
    assert_eq!(candidates.len(), 8);
    assert_eq!(candidates[0]["verdict"], "not-smaller");
    assert_eq!(candidates[0]["nodes"], 16);
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate["verdict"] == "accepted")
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate["verdict"] != "counterexample")
    );
    assert_eq!(report["best"]["nodes"], 7);
    assert_eq!(
        report["best"]["receipt"]["schema"],
        "zeno-fcis/transform-receipt/1"
    );
    assert_eq!(
        report["best"]["receipt"]["candidate"]["sha256"],
        report["best"]["sha256"]
    );
    assert_eq!(report["best"]["max_steps"], 7);
}

#[test]
fn a_domain_above_the_cap_is_inconclusive_before_any_search() {
    match optimize(artifact("boolean-kernel-original"), &default_strategy(), 15) {
        Outcome::Inconclusive(stop) => assert_eq!((stop.size, stop.limit), (Some(16), 15)),
        other => panic!("{other:?}"),
    }
    let one = encode(&program(
        vec![FULL],
        vec![FULL],
        vec![Op::Input(0)],
        vec![0],
    ));
    match optimize(&one, &default_strategy(), u64::MAX) {
        Outcome::Inconclusive(stop) => assert_eq!(stop.size, None),
        other => panic!("{other:?}"),
    }
    let searched = run(artifact("boolean-kernel-original"), &default_strategy());
    assert_eq!(searched.max_input_tuples, DEFAULT_MAX_INPUT_TUPLES);
}

#[test]
fn the_original_must_be_admitted() {
    let mut trailing = artifact("boolean-kernel-original").to_vec();
    trailing.push(0);
    match optimize(&trailing, &default_strategy(), DEFAULT_MAX_INPUT_TUPLES) {
        Outcome::Refused(Refused::NotAdmitted { code }) => assert_eq!(code, "program-encoding"),
        other => panic!("{other:?}"),
    }
    match optimize(b"", &default_strategy(), DEFAULT_MAX_INPUT_TUPLES) {
        Outcome::Refused(Refused::NotAdmitted { code }) => assert_eq!(code, "program-encoding"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn tight_limits_are_reported_and_the_original_is_never_lost() {
    let mut strategy = default_strategy();
    strategy.limits.max_rewrites_per_round = 1;
    strategy.limits.max_enodes = 40;
    let search = run(artifact("boolean-kernel-original"), &strategy);
    assert!(search.any_limit_hit());
    assert!(search.phases.iter().any(|phase| phase.limit_hit.is_some()));
    for phase in &search.phases {
        assert!(phase.enodes <= 40);
    }
    if let Some(best) = &search.best {
        assert!(best.nodes < 16);
    }
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict))
    );
    // A cap below the original's own size stops before any phase.
    let mut strategy = default_strategy();
    strategy.limits.max_enodes = 10;
    let search = run(artifact("boolean-kernel-original"), &strategy);
    assert_eq!(search.stopped, Some("original-exceeds-max-enodes"));
    assert!(search.phases.is_empty());
    assert!(search.best.is_none());
    assert_eq!(
        search.json()["search"]["stopped"],
        "original-exceeds-max-enodes"
    );
}

#[test]
fn verdicts_and_signatures_are_named() {
    assert_eq!(Verdict::NotSmaller.name(), "not-smaller");
    assert_eq!(Verdict::Duplicate.name(), "duplicate");
    assert_eq!(
        Verdict::NotBetterThanIncumbent.name(),
        "not-better-than-incumbent"
    );
    assert_eq!(
        Verdict::Unextractable(ExtractError::Unextractable(3)).name(),
        "unextractable"
    );
    assert_eq!(
        Verdict::Unextractable(ExtractError::NotAdmitted("program-shape")).name(),
        "unextractable"
    );
    assert!(Verdict::Accepted.accepted() && Verdict::AcceptedNotBetter.accepted());
    assert!(!Verdict::Duplicate.accepted());
    let signature = Signature::constant(1, 3);
    assert_eq!(signature.constant_value(), Some(1));
    assert_eq!(signature.bits(), Some(0b111));
    assert_eq!(Signature::from_values(vec![Some(2), Some(2)]).bits(), None);
    assert_eq!(Signature::from_values(vec![None]).constant_value(), None);
}
