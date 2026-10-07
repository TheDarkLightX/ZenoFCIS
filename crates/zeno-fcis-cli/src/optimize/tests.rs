//! Optimizer tests: the e-graph core, signatures, strategy grammar, extraction,
//! the withdrawal-queue artifacts, the Boolean benchmark seeds, planted defects
//! and determinism.

use super::cut_table::{WITH_EQ, WITHOUT_EQ};
use super::cuts;
use super::egraph::{
    AddError, Caps, ClassId, EGraph, ENode, Inconsistency, Limit, Merge, MergeReason,
};
use super::extract::{self, Cost, ExtractError};
use super::rules::{self, Rewrite, Rule, Term};
use super::semantics::{DomainInfo, Interval, Kind, TypeError, op_bytes};
use super::signature::{Budget, Samples, TABLE_BYTES, TABLE_WORK};
use super::strategy::{
    DEFAULT_STRATEGY_JSON, Extractor, Limits, MAX_LIMIT, MAX_PHASES, MAX_ROUNDS, MAX_STRATEGIES,
    PORTFOLIO_SCHEMA, Phase, PhaseSpec, Plan, STRATEGY_SCHEMA, Strategy,
};
use super::{Outcome, Refused, Search, Verdict, optimize};
use crate::neural_loop::profiles::Profile;
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
        profile: None,
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

/// Seven Boolean inputs, 128 tuples: more tuples than samples.
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

// ---- tables, samples and trap analysis ----------------------------------------

/// Each node's value on one whole-domain tuple, `None` where eager
/// evaluation has trapped at or before it: a reference written separately
/// from the table code.
fn reference_values(program: &Program, tuple: &[i64]) -> Vec<Option<i64>> {
    let mut values: Vec<Option<i64>> = Vec::with_capacity(program.nodes().len());
    for op in program.nodes() {
        let at = |id: u16| values[usize::from(id)];
        let both = |a: u16, b: u16| at(a).zip(at(b));
        values.push(match *op {
            Op::Input(index) => Some(tuple[usize::from(index)]),
            Op::Int(value) => Some(value),
            Op::Bool(value) => Some(i64::from(value)),
            Op::Add(a, b) => both(a, b).and_then(|(a, b)| a.checked_add(b)),
            Op::Sub(a, b) => both(a, b).and_then(|(a, b)| a.checked_sub(b)),
            Op::Eq(a, b) => both(a, b).map(|(a, b)| i64::from(a == b)),
            Op::Lt(a, b) => both(a, b).map(|(a, b)| i64::from(a < b)),
            Op::And(a, b) => both(a, b).map(|(a, b)| i64::from(a == 1 && b == 1)),
            Op::Not(a) => at(a).map(|a| i64::from(a == 0)),
            Op::Select(c, a, b) => {
                both(a, b).and_then(|(a, b)| at(c).map(|c| if c == 1 { a } else { b }))
            }
            _ => panic!("unknown instruction"),
        });
    }
    values
}

/// Every tuple of a program's declared domain, in enumeration order.
fn every_tuple(program: &Program) -> Vec<Vec<i64>> {
    let mut tuples = Vec::new();
    let mut tuple = transform::first_tuple(program.inputs());
    loop {
        tuples.push(tuple.clone());
        if !transform::advance(program.inputs(), &mut tuple) {
            return tuples;
        }
    }
}

/// Checks every original node's class against the reference on every tuple:
/// the table's value and poison, the minimality of its support, the samples
/// at the domain's sample tuples, and the exact trap flag.
fn assert_tables_match_reference(program: &Program) {
    let (egraph, _) = egraph_of(program);
    let domain = egraph.domain();
    let tuples = every_tuple(program);
    let reference: Vec<Vec<Option<i64>>> = tuples
        .iter()
        .map(|tuple| reference_values(program, tuple))
        .collect();
    // The class of each original node; hash-consing shares identical ones.
    let mut classes: Vec<ClassId> = Vec::new();
    for op in program.nodes() {
        let node = ENode::from_op(op, |id| classes.get(usize::from(id)).copied())
            .unwrap_or_else(|| panic!("node"));
        classes.push(egraph.lookup(node).unwrap_or_else(|| panic!("present")));
    }
    for (node, class) in classes.iter().enumerate() {
        let data = egraph.data(*class);
        let table = data
            .table
            .as_ref()
            .unwrap_or_else(|| panic!("node {node} has no table"));
        for (tuple, values) in tuples.iter().zip(&reference) {
            let local = table.index_of(tuple, domain.minima(), domain.widths());
            assert_eq!(table.get(local), values[node], "node {node} at {tuple:?}");
        }
        // Minimal support: changing each support input alone can change
        // the value or the poison somewhere. Tuples are in enumeration
        // order, so a tuple's ordinal is its mixed-radix value.
        for input in table.support() {
            let position = usize::from(*input);
            let stride: usize = domain.widths()[position + 1..]
                .iter()
                .map(|width| *width as usize)
                .product();
            let essential = tuples.iter().enumerate().any(|(ordinal, tuple)| {
                let digit = tuple[position].abs_diff(domain.minima()[position]) as usize;
                (0..domain.widths()[position] as usize).any(|other| {
                    let neighbour = ordinal + other * stride - digit * stride;
                    reference[neighbour][node] != reference[ordinal][node]
                })
            });
            assert!(essential, "node {node}: input {input} is not essential");
        }
        assert_eq!(data.may_poison, table.poisoned(), "node {node}");
        let poisoned = reference.iter().any(|values| values[node].is_none());
        assert_eq!(table.poisoned(), poisoned, "node {node}");
    }
    // Samples are the values at the fixed sample tuples, 0 where poisoned.
    if domain.exhaustive() {
        for (node, class) in classes.iter().enumerate() {
            let expected: Vec<i64> = reference
                .iter()
                .map(|values| values[node].unwrap_or(0))
                .collect();
            assert_eq!(
                egraph.data(*class).samples.values(),
                expected,
                "node {node}"
            );
        }
    }
    // A node may trap exactly when it fails on a tuple whose operands are
    // defined.
    for (node, op) in program.nodes().iter().enumerate() {
        let traps = matches!(op, Op::Add(..) | Op::Sub(..))
            && reference.iter().any(|values| {
                let (Op::Add(a, b) | Op::Sub(a, b)) = *op else {
                    return false;
                };
                values[usize::from(a)].is_some()
                    && values[usize::from(b)].is_some()
                    && values[node].is_none()
            });
        let id = egraph
            .class_nodes(classes[node])
            .into_iter()
            .find(|id| egraph.node_origin(*id) == Some(node as u16));
        if let Some(id) = id {
            assert_eq!(egraph.node_may_trap(id), traps, "node {node}");
        }
    }
}

#[test]
fn tables_follow_the_checker_enumeration_and_carry_poison() {
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
    assert!(egraph.domain().exhaustive());
    assert_eq!(egraph.domain().size(), Some(4));
    assert_eq!(egraph.domain().sample_mask(), [0b1111]);
    let table = |class: ClassId| {
        egraph
            .data(class)
            .table
            .clone()
            .unwrap_or_else(|| panic!("exact"))
    };
    let values = |class: ClassId| {
        let table = table(class);
        (0..table.len())
            .map(|index| table.get(index))
            .collect::<Vec<_>>()
    };
    // Each table covers only the inputs its class depends on.
    assert_eq!(table(0).support(), &[0]);
    assert_eq!(values(0), [Some(0), Some(1)]);
    assert_eq!(table(1).support(), &[1]);
    assert_eq!(values(1), [Some(i64::MAX - 1), Some(i64::MAX)]);
    // The addition traps where x = MAX; its users inherit the poison.
    assert_eq!(table(3).support(), &[1]);
    assert_eq!(values(3), [Some(i64::MAX), None]);
    assert!(table(3).poisoned());
    assert_eq!(values(6), [Some(0), None]);
    assert!(egraph.node_may_trap(3));
    assert_eq!(egraph.pinned(), vec![3]);
    // And(a, Not a) is constantly false, so its support is empty.
    assert!(table(5).support().is_empty());
    assert_eq!(egraph.data(5).constant(), Some(0));
    assert!(!egraph.node_may_trap(5));
    assert_eq!(egraph.data(3).kind, Kind::Int);
    assert_eq!(egraph.data(6).kind, Kind::Bool);
    // The samples are the whole domain, last input fastest:
    // (0, MAX-1), (0, MAX), (1, MAX-1), (1, MAX).
    assert_eq!(egraph.data(0).samples.bits(), [0b1100]);
    assert_eq!(egraph.data(4).samples.bits(), [0b0011]);
    assert_eq!(
        egraph.data(3).samples.values(),
        &[i64::MAX, 0, i64::MAX, 0],
        "poisoned samples hold 0"
    );
    assert!(egraph.data(3).samples.poisoned());
    assert!(
        egraph
            .data(0)
            .samples
            .agrees_on(&egraph.data(5).samples, &[0b0011])
    );
    assert!(
        !egraph
            .data(0)
            .samples
            .agrees_on(&egraph.data(5).samples, &[0b1100])
    );
    assert_tables_match_reference(&base);
    // Six Boolean inputs give 64 samples, every bit used; the input's own
    // table has two entries.
    let six = program(
        vec![Domain::Bool; 6],
        vec![Domain::Bool],
        vec![Op::Input(5)],
        vec![0],
    );
    let (egraph, _) = egraph_of(&six);
    assert_eq!(egraph.domain().sample_mask(), [u64::MAX]);
    assert_eq!(egraph.domain().sample_count(), 64);
    assert_eq!(egraph.data(0).samples.bits(), [0xaaaa_aaaa_aaaa_aaaa]);
    assert_eq!(table_len(&egraph, 0), 2);
}

fn table_len(egraph: &EGraph, class: ClassId) -> usize {
    egraph
        .data(class)
        .table
        .as_ref()
        .map_or(0, |table| table.len())
}

#[test]
fn tables_match_a_reference_evaluation_on_every_tuple() {
    // Mixed kinds, a trapping subtraction, Select over poisoned arms, unused
    // inputs and single-value domains.
    let small = Domain::Int { min: -2, max: 2 };
    let near_min = Domain::Int {
        min: i64::MIN,
        max: i64::MIN + 2,
    };
    let point = Domain::Int { min: 7, max: 7 };
    let base = program(
        vec![Domain::Bool, small, near_min, point, Domain::Bool],
        vec![FULL, Domain::Bool, FULL],
        vec![
            Op::Input(0),        // 0
            Op::Input(1),        // 1
            Op::Input(2),        // 2
            Op::Input(3),        // 3
            Op::Int(1),          // 4
            Op::Sub(2, 4),       // 5: traps at MIN
            Op::Add(1, 3),       // 6: total, support {1} only
            Op::Lt(1, 4),        // 7
            Op::Select(0, 5, 6), // 8: poisoned wherever 5 traps
            Op::Select(7, 6, 6), // 9: equal arms, support {1}
            Op::Eq(0, 7),        // 10
            Op::Not(10),         // 11
            Op::And(11, 0),      // 12
            Op::Sub(6, 3),       // 13: x1, support {1}
            Op::Input(4),        // 14: unused input
        ],
        vec![8, 12, 13],
    );
    assert_tables_match_reference(&base);
    for name in ["boolean-kernel-original", "retained-controller-original"] {
        assert_tables_match_reference(&import(artifact(name)));
    }
    for seed in seeds('I').into_iter().chain(seeds('B')) {
        let program = import(&seed.original);
        if transform::domain_size(program.inputs())
            .is_ok_and(|size| size.is_some_and(|size| size <= 4096))
        {
            assert_tables_match_reference(&program);
        }
    }
}

#[test]
fn semantic_merge_joins_one_function_written_over_different_inputs() {
    // x in [0, 2]: Not(Lt(0, x)) and Eq(x, 0) are one function, and
    // Select(y, Eq(x, 0), Not(Lt(0, x))) reads y but does not depend on it.
    let small = Domain::Int { min: 0, max: 2 };
    let base = program(
        vec![small, Domain::Bool],
        vec![Domain::Bool, Domain::Bool, Domain::Bool],
        vec![
            Op::Input(0),        // 0
            Op::Input(1),        // 1
            Op::Int(0),          // 2
            Op::Lt(2, 0),        // 3
            Op::Not(3),          // 4
            Op::Eq(0, 2),        // 5
            Op::Select(1, 5, 4), // 6
        ],
        vec![4, 5, 6],
    );
    let (mut egraph, roots) = egraph_of(&base);
    assert_eq!(
        egraph.data(6).support,
        vec![0],
        "the minimal support drops y"
    );
    assert_eq!(egraph.data(4).table, egraph.data(5).table);
    let report = rules::run_phase(&mut egraph, Phase::SemanticMerge, 1, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(report.merges >= 2, "{report:?}");
    assert_eq!(egraph.find(roots[0]), egraph.find(roots[1]));
    assert_eq!(egraph.find(roots[0]), egraph.find(roots[2]));
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    assert_eq!(extracted.nodes().len(), 3, "{:?}", extracted.nodes());
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
}

#[test]
fn an_exhausted_table_budget_falls_back_to_bounds_and_stays_checked() {
    let base = import(artifact("retained-controller-original"));
    let domain = DomainInfo::new(base.inputs()).unwrap_or_else(|| panic!("domain"));
    // Enough work for the first few leaves only.
    let (mut egraph, roots) = EGraph::from_program_with_budget(
        &base,
        domain,
        Limits::default().caps(),
        Budget::new(64, TABLE_BYTES),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(egraph.budget().exhausted);
    let (exact, inexact) = egraph.exactness();
    assert!(exact > 0 && inexact > 0, "{exact} exact, {inexact} without");
    for spec in &default_strategy().phases {
        rules::run_phase(&mut egraph, spec.phase, spec.rounds, &Limits::default())
            .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    assert!(extracted.nodes().len() < base.nodes().len());
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
    // Memory runs out the same way: classes keep their bounds instead.
    let domain = DomainInfo::new(base.inputs()).unwrap_or_else(|| panic!("domain"));
    let (egraph, _) = EGraph::from_program_with_budget(
        &base,
        domain,
        Limits::default().caps(),
        Budget::new(TABLE_WORK, 1_000),
    )
    .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(egraph.budget().exhausted);
    assert!(egraph.exactness().1 > 0);
}

#[test]
fn bounds_stay_conservative_beyond_the_table_cap() {
    let small = Domain::Int { min: 0, max: 2 };
    let wide = Domain::Int {
        min: 0,
        max: 99_999,
    };
    let base = program(
        vec![Domain::Bool, small, wide, NEAR_MAX],
        vec![FULL, Domain::Bool, FULL, FULL, Domain::Bool, Domain::Bool],
        vec![
            Op::Input(0),        // 0
            Op::Input(1),        // 1
            Op::Input(2),        // 2: 100,000 values, no table
            Op::Input(3),        // 3
            Op::Add(1, 2),       // 4: [0, 100002], no table, total
            Op::Int(200_000),    // 5
            Op::Lt(4, 5),        // 6: always true, from the bounds
            Op::Int(1),          // 7
            Op::Add(3, 7),       // 8: may trap, exact
            Op::Select(0, 4, 8), // 9: hull, may be poisoned
            Op::Sub(1, 4),       // 10: [-100002, 2]
            Op::Eq(1, 5),        // 11: never true, exact
            Op::Lt(2, 7),        // 12: x2 = 0 only, no table
            Op::Bool(true),      // 13
        ],
        vec![4, 6, 9, 10, 11, 12],
    );
    let (mut egraph, _) = egraph_of(&base);
    assert!(!egraph.domain().exhaustive());
    assert_eq!(egraph.domain().size(), Some(2 * 3 * 100_000 * 2));
    assert_eq!(egraph.domain().sample_count(), 256);
    assert!(egraph.data(2).table.is_none());
    assert_eq!(egraph.data(2).support, vec![2]);
    assert!(egraph.data(4).table.is_none());
    assert_eq!(egraph.data(4).support, vec![1, 2]);
    assert_eq!(
        egraph.data(4).interval,
        Interval {
            min: 0,
            max: 100_001
        }
    );
    assert!(!egraph.node_may_trap(4));
    assert_eq!(egraph.data(6).constant(), Some(1));
    assert_eq!(egraph.data(11).constant(), Some(0));
    assert!(egraph.node_may_trap(8));
    assert!(egraph.data(8).may_poison);
    assert_eq!(egraph.data(8).constant(), None);
    assert!(egraph.data(9).may_poison);
    assert!(egraph.data(9).table.is_none());
    assert_eq!(
        egraph.data(9).interval,
        Interval {
            min: 0,
            max: i64::MAX
        }
    );
    assert_eq!(
        egraph.data(10).interval,
        Interval {
            min: -100_001,
            max: 2
        }
    );
    assert_eq!(egraph.pinned(), vec![8]);
    // Poisoned classes never merge by rule without tables.
    assert_eq!(egraph.union(9, 4, MergeReason::Rule), Ok(Merge::Refused));
    // Different sample values refuse a merge that the bounds alone allow.
    assert_eq!(egraph.union(6, 12, MergeReason::Rule), Ok(Merge::Refused));
    assert_eq!(egraph.refused_merges(), 2);
    // An unpoisoned class without a table merges with an equal exact one and
    // takes its table; the bounds meet.
    assert_eq!(egraph.union(6, 13, MergeReason::Rule), Ok(Merge::Merged));
    assert_eq!(
        egraph.data(6).table.as_ref().map(|table| table.len()),
        Some(1)
    );
    assert_eq!(egraph.data(6).interval, Interval::point(1));
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
    // Commutation is structural, so it applies whatever the tables say.
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
fn semantic_merge_needs_exact_tables() {
    // One input with 100,000 values and no constant: no class has a table.
    let wide = Domain::Int {
        min: 0,
        max: 99_999,
    };
    let base = program(
        vec![wide],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Lt(0, 0), Op::Not(1), Op::Not(2)],
        vec![3],
    );
    let (mut egraph, _) = egraph_of(&base);
    assert_eq!(egraph.exactness(), (0, 4));
    let report = rules::run_phase(&mut egraph, Phase::SemanticMerge, 2, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.rounds_run, 0);
    assert!(report.skipped.is_some());
    assert_eq!(report.merges, 0);
    // The Boolean phase still removes the double negation and the
    // irreflexive comparison by rule.
    let search = run(&encode(&base), &default_strategy());
    assert_eq!(best_nodes(&search), Some(1));
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
    // Exact tables over each class's own inputs let semantic merging and
    // completion work on all 384 tuples (46 nodes before them).
    assert_eq!(
        best.nodes, 36,
        "the current engine's result; change deliberately"
    );
    assert_eq!(best.max_steps, 36);
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
        search.exact_signatures,
        "every class of the original has a table over its own inputs"
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
    // Support-local tables keep 97 of the original's classes exact (the
    // three widest exceed the table cap), so semantic merging runs here too.
    assert_eq!(best.nodes, 88, "the current engine's result");
    assert!(!search.exact_signatures);
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
        ("I04", Some(5)),
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
    // Beyond the table cap the guard sees only bounds and samples. A planted
    // merge that the fixed samples cannot tell apart goes through, and only
    // the judge stands between it and the output: it refutes the wrong form
    // on the first distinguishing tuple.
    let wide = Domain::Int {
        min: 0,
        max: 99_999,
    };
    let probe = program(vec![wide], vec![FULL], vec![Op::Input(0)], vec![0]);
    let (egraph, _) = egraph_of(&probe);
    let sampled = egraph.data(0).samples.values().to_vec();
    let mut unsampled = (1..).filter(|value| !sampled.contains(value));
    let (first, second) = (
        unsampled.next().unwrap_or_default(),
        unsampled.next().unwrap_or_default(),
    );
    let base = program(
        vec![wide],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Int(first),
            Op::Int(second),
            Op::Eq(0, 1),
            Op::Eq(0, 2),
            Op::And(3, 4),
        ],
        vec![5],
    );
    let (mut egraph, roots) = egraph_of(&base);
    assert!(egraph.data(3).table.is_none());
    let report = rules::run_rules(&mut egraph, &rules, 1, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.merges, 1, "{report:?}");
    let class = egraph.find(roots[0]);
    assert_eq!(
        egraph.class_nodes(class).len(),
        2,
        "And and the planted Or share a class"
    );
    let planted = program(
        vec![wide],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Int(first),
            Op::Int(second),
            Op::Eq(0, 1),
            Op::Eq(0, 2),
            Op::Select(3, 3, 4),
        ],
        vec![5],
    );
    let Err(Rejection::Counterexample(witness)) =
        transform::check(&encode(&base), &encode(&planted), GENEROUS)
    else {
        panic!("the judge must refute Or for And");
    };
    assert_eq!(witness.ordinal, first as u64);
    assert_eq!(witness.input, vec![first]);
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
    let encoded = encode(&base);
    let judging = super::Judging {
        original: &encoded,
        program: &base,
        limits: GENEROUS,
    };
    let (candidate, accepted) = super::judge(&search, &judging, (0, 0, Phase::Fold), wrong, None);
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
    let samples = Samples::constant(Kind::Bool, 1, 3);
    assert_eq!(samples.bits(), [0b111]);
    assert!(!samples.poisoned());
    assert_eq!(Samples::from_values(Kind::Int, vec![2, 2]).bits(), [0]);
    // Up to 1,024 tuples the samples are the whole domain, in words.
    let ten = program(
        vec![Domain::Bool; 10],
        vec![Domain::Bool],
        vec![Op::Input(9)],
        vec![0],
    );
    let (egraph, _) = egraph_of(&ten);
    assert!(egraph.domain().exhaustive());
    assert_eq!(egraph.domain().sample_count(), 1_024);
    assert_eq!(egraph.data(0).samples.bits(), [0xaaaa_aaaa_aaaa_aaaa; 16]);
}

// ---- exact cut rewriting -----------------------------------------------------

/// Evaluates a table circuit over its inputs' functions of `vars` inputs.
fn evaluate_circuit(circuit: &str, vars: usize) -> u8 {
    let mask = ((1_u16 << (1 << vars)) - 1) as u8;
    let mut operands: Vec<u8> = (0..vars)
        .map(|input| {
            (0..1_usize << vars)
                .filter(|assignment| assignment >> input & 1 == 1)
                .fold(0, |function, assignment| function | 1 << assignment)
        })
        .collect();
    let mut output = None;
    for token in circuit.split(' ').filter(|token| !token.is_empty()) {
        let digits: Vec<usize> = token[1..]
            .chars()
            .map(|digit| digit.to_digit(10).unwrap_or_else(|| panic!("{circuit}")) as usize)
            .collect();
        let at = |index: usize| operands[digits[index]];
        let value = match &token[..1] {
            "T" => mask,
            "F" => 0,
            "N" => !at(0) & mask,
            "A" => at(0) & at(1),
            "E" => !(at(0) ^ at(1)) & mask,
            "S" => (at(0) & at(1)) | (!at(0) & at(2) & mask),
            "@" => {
                output = Some(at(0));
                continue;
            }
            other => panic!("unknown gate {other} in {circuit}"),
        };
        operands.push(value);
    }
    output.unwrap_or_else(|| *operands.last().unwrap_or_else(|| panic!("{circuit}")))
}

#[test]
fn every_cut_table_entry_computes_its_function() {
    for with_eq in [true, false] {
        let mut by_cost = [0_usize; 5];
        for vars in 0..=3 {
            for function in 0..1_usize << (1 << vars) {
                let circuit = cuts::circuit(with_eq, vars, function as u8);
                assert_eq!(
                    usize::from(evaluate_circuit(circuit, vars)),
                    function,
                    "{vars} inputs, function {function:#x}: {circuit}"
                );
                assert!(with_eq || !circuit.contains('E'), "{circuit}");
                if vars == 3 {
                    by_cost[cuts::cost(circuit) as usize] += 1;
                }
            }
        }
        // Every function of three inputs needs at most four gates; without
        // Eq, 3, 17, 72, 138 and 26 functions need 0 to 4 (Section 2.6 of
        // the 2026-10-05 literature review reports the same counts).
        let expected = if with_eq {
            [3, 20, 91, 130, 12]
        } else {
            [3, 17, 72, 138, 26]
        };
        assert_eq!(by_cost, expected, "with Eq: {with_eq}");
    }
    // Majority of three needs two gates either way.
    assert_eq!(cuts::circuit(false, 3, 0xe8), "S012 S312");
    assert_eq!(cuts::circuit(true, 3, 0xe8), "E01 S302");
}

#[test]
fn cut_tables_are_their_generators_output() {
    let [with_eq, without_eq] = cuts::generator::tables();
    assert_eq!(with_eq, WITH_EQ);
    assert_eq!(without_eq, WITHOUT_EQ);
}

#[test]
#[ignore = "prints the cut tables for cut_table.rs"]
fn print_cut_tables() {
    let [with_eq, without_eq] = cuts::generator::tables();
    for (name, table) in [("WITH_EQ", with_eq), ("WITHOUT_EQ", without_eq)] {
        println!("pub(crate) static {name}: [&str; {}] = [", table.len());
        for entry in table {
            println!("    {entry:?},");
        }
        println!("];");
    }
}

#[test]
fn cut_rewrite_finds_the_two_gate_majority() {
    let seeds = seeds('B');
    let majority = seeds
        .iter()
        .find(|seed| seed.id == "B07")
        .unwrap_or_else(|| panic!("B07"));
    // The default strategy stops at six nodes: three inputs and three gates.
    let search = run(&majority.original, &default_strategy());
    assert_eq!(best_nodes(&search), Some(6));
    let mut strategy = default_strategy();
    strategy.phases.push(PhaseSpec {
        phase: Phase::CutRewrite,
        rounds: 2,
    });
    let search = run(&majority.original, &strategy);
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert_eq!(best.nodes, 5, "{:?}", import(&best.bytes).nodes());
    assert_eq!(
        transform::replay(
            &best.equivalence.receipt(),
            &majority.original,
            &best.bytes,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
    let phase = search.phases.last().unwrap_or_else(|| panic!("phases"));
    assert_eq!(phase.phase, Phase::CutRewrite);
    assert!(phase.cut_evaluations > 0 && phase.rewrites > 0, "{phase:?}");
    assert_eq!(phase.refused, 0, "every cut proposal is a true equation");
}

#[test]
fn cut_rewrite_never_crosses_a_possible_trap() {
    // An overflowing Add feeds the Boolean logic: cuts over its poisoned
    // comparison are never proposed, and the trap is kept.
    let base = program(
        vec![Domain::Bool, NEAR_MAX],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(1),
            Op::Add(1, 2),
            Op::Lt(1, 3),
            Op::Not(0),
            Op::And(4, 5),
            Op::Not(6),
            Op::Not(7),
        ],
        vec![8],
    );
    let (mut egraph, roots) = egraph_of(&base);
    let report = rules::run_phase(&mut egraph, Phase::CutRewrite, 2, &Limits::default())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(report.refused, 0, "{report:?}");
    let extracted = extract(&egraph, &roots, &base, Extractor::DagGreedy);
    assert!(extracted.nodes().iter().any(|op| matches!(op, Op::Add(..))));
    assert!(transform::check(&encode(&base), &encode(&extracted), GENEROUS).is_ok());
}

// ---- profiles ----------------------------------------------------------------

#[test]
fn the_optimizer_profile_matches_the_loop_gate_for_every_opcode() {
    // The smallest well-typed program ending in each opcode, over two Bool
    // inputs; integer instructions read the constant at node 2. No opcode is
    // admitted by one side and excluded by the other.
    let ops = [
        (Op::Input(0), false),
        (Op::Bool(true), false),
        (Op::Int(1), false),
        (Op::Add(2, 2), true),
        (Op::Sub(2, 2), true),
        (Op::Eq(0, 1), false),
        (Op::Lt(2, 2), true),
        (Op::And(0, 1), false),
        (Op::Not(0), false),
        (Op::Select(0, 0, 1), false),
    ];
    for (op, integer) in ops {
        let mut nodes = vec![Op::Input(0), Op::Input(1)];
        if integer {
            nodes.push(Op::Int(1));
        }
        nodes.push(op.clone());
        let base = program(
            vec![Domain::Bool, Domain::Bool],
            vec![Domain::Bool],
            nodes,
            vec![0],
        );
        let gate = Profile::FunctionalBoolV1.admit(&base).is_ok();
        let node =
            ENode::from_op(&op, |id| Some(u32::from(id))).unwrap_or_else(|| panic!("{op:?}"));
        assert_eq!(node.in_profile(Profile::FunctionalBoolV1), gate, "{op:?}");
        assert!(node.in_profile(Profile::CheckedI64V1));
        assert!(Profile::CheckedI64V1.admit(&base).is_ok());
    }
}

#[test]
fn a_profile_keeps_every_proposal_and_candidate_inside_it() {
    let seeds = seeds('B');
    let parity = seeds
        .iter()
        .find(|seed| seed.id == "B06")
        .unwrap_or_else(|| panic!("B06"));
    // Without a profile the parity seed ends as Not(Eq(x0, x1)).
    let free = run(&parity.original, &default_strategy());
    let free = import(
        &free
            .best
            .as_ref()
            .unwrap_or_else(|| panic!("improvement"))
            .bytes,
    );
    assert!(
        free.nodes().iter().any(|op| matches!(op, Op::Eq(..))),
        "{:?}",
        free.nodes()
    );
    // Within functional-bool-v1 it is a four-node Select form instead.
    let mut strategy = default_strategy();
    strategy.profile = Some(Profile::FunctionalBoolV1);
    let search = run(&parity.original, &strategy);
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    let bound = import(&best.bytes);
    assert_eq!(bound.nodes().len(), 4, "{:?}", bound.nodes());
    assert!(bound.nodes().iter().any(|op| matches!(op, Op::Select(..))));
    assert!(Profile::FunctionalBoolV1.admit(&bound).is_ok());
    // Every seed's output passes the gate, and the total is no worse.
    let mut total = 0;
    for seed in &seeds {
        let search = run(&seed.original, &strategy);
        let output = search
            .best
            .as_ref()
            .map_or_else(|| import(&seed.original), |best| import(&best.bytes));
        assert!(
            Profile::FunctionalBoolV1.admit(&output).is_ok(),
            "{}",
            seed.id
        );
        total += output.nodes().len();
    }
    assert!(total <= 59, "{total}");
    // An original whose ABI the profile cannot hold is refused before any
    // search; a stored instruction outside the profile is not.
    let wide = program(vec![FULL], vec![FULL], vec![Op::Input(0)], vec![0]);
    assert!(matches!(
        optimize(&encode(&wide), &strategy, DEFAULT_MAX_INPUT_TUPLES),
        Outcome::Refused(Refused::OutsideProfile(_))
    ));
    let xnor = program(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Eq(0, 1),
            Op::Not(2),
            Op::Not(3),
        ],
        vec![4],
    );
    // Cut rewriting derives the root's function through the Eq and rebuilds
    // it from the table without Eq.
    let mut translating = strategy.clone();
    translating.phases.push(PhaseSpec {
        phase: Phase::CutRewrite,
        rounds: 1,
    });
    let search = run(&encode(&xnor), &translating);
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    let translated = import(&best.bytes);
    assert_eq!(translated.nodes().len(), 4, "{:?}", translated.nodes());
    assert!(Profile::FunctionalBoolV1.admit(&translated).is_ok());
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict)
                || matches!(candidate.verdict, Verdict::Unextractable(_)))
    );
}

#[test]
fn the_strategy_profile_is_closed_and_round_trips() {
    let document = json!({
        "schema": STRATEGY_SCHEMA,
        "phases": [{"phase": "cut-rewrite", "rounds": 1}],
        "profile": "functional-bool-v1"
    });
    let strategy = Strategy::parse(document.to_string().as_bytes())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(strategy.profile, Some(Profile::FunctionalBoolV1));
    assert_eq!(strategy.json()["profile"], "functional-bool-v1");
    assert_eq!(
        Strategy::parse(strategy.json().to_string().as_bytes()),
        Ok(strategy)
    );
    assert!(default_strategy().json().get("profile").is_none());
    for (profile, expected) in [
        (json!("functional-bool-v2"), "profile must be"),
        (json!("FunctionalBoolV1"), "profile must be"),
        (json!(1), "invalid type"),
        (json!(null), "invalid type"),
    ] {
        let mut bad = document.clone();
        bad["profile"] = profile;
        let error = Strategy::parse(bad.to_string().as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{bad} must be refused"));
        assert!(error.message.contains(expected), "{bad}: {}", error.message);
    }
}

// ---- portfolio and work budget -------------------------------------------------

fn default_portfolio() -> Plan {
    Plan::default_portfolio().unwrap_or_else(|error| panic!("{error:?}"))
}

#[test]
fn the_default_portfolio_is_fixed_and_versioned() {
    let plan = default_portfolio();
    assert!(plan.portfolio);
    assert_eq!(PORTFOLIO_SCHEMA, "zeno-fcis/optimize-portfolio/1");
    assert_eq!(plan.strategies.len(), 3);
    let reparsed = Plan::parse_portfolio(plan.json().to_string().as_bytes())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(reparsed, plan);
    let names = |strategy: &Strategy| {
        strategy
            .phases
            .iter()
            .map(|phase| phase.phase.name())
            .collect::<Vec<_>>()
    };
    // The default strategy, then select before boolean, then three cycles;
    // each ends with merging, cut rewriting and a last merge.
    assert_eq!(
        names(&plan.strategies[0])[..8],
        names(&default_strategy())[..]
    );
    assert_eq!(names(&plan.strategies[1])[2..4], ["select", "boolean"]);
    assert_eq!(
        names(&plan.strategies[2])
            .iter()
            .filter(|name| **name == "semantic-merge")
            .count(),
        5
    );
    for strategy in &plan.strategies {
        assert_eq!(
            names(strategy)[strategy.phases.len() - 3..],
            ["semantic-merge", "cut-rewrite", "semantic-merge"]
        );
        // Each run has its own size and work budgets.
        assert_eq!(strategy.limits.max_enodes, 20_000);
        assert_eq!(strategy.limits.max_work, 400);
        assert_eq!(strategy.profile, None);
    }
    // A single strategy reports itself; the portfolio its document.
    assert_eq!(
        Plan::single(default_strategy()).json(),
        default_strategy().json()
    );
    assert_eq!(plan.json()["schema"], PORTFOLIO_SCHEMA);
}

#[test]
fn the_portfolio_grammar_is_closed() {
    let member = json!({"schema": STRATEGY_SCHEMA, "phases": [{"phase": "fold", "rounds": 1}]});
    let valid = json!({"schema": PORTFOLIO_SCHEMA, "strategies": [member.clone()]});
    assert!(Plan::parse_portfolio(valid.to_string().as_bytes()).is_ok());
    let refused = |document: Value, expected: &str| {
        let error = Plan::parse_portfolio(document.to_string().as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{document} must be refused"));
        assert!(
            error.message.contains(expected),
            "{document}: {}",
            error.message
        );
    };
    let mut document = valid.clone();
    document["extra"] = json!(1);
    refused(document, "exactly schema and strategies");
    let mut document = valid.clone();
    document["schema"] = json!("zeno-fcis/optimize-portfolio/2");
    refused(document, "schema must be");
    let mut document = valid.clone();
    document["strategies"] = json!([]);
    refused(document, "strategies must hold 1 to 8");
    let mut document = valid.clone();
    document["strategies"] = json!(vec![member.clone(); MAX_STRATEGIES + 1]);
    refused(document, "strategies must hold 1 to 8");
    let mut document = valid.clone();
    document["strategies"][0]["phases"][0]["phase"] = json!("egg");
    refused(document, "strategies[0]: not a strategy document");
    refused(json!([member]), "exactly schema and strategies");
    // The profile applies to every member, and a member naming another one
    // is refused.
    let mut plan = default_portfolio();
    assert!(plan.restrict(Profile::FunctionalBoolV1).is_ok());
    assert!(
        plan.strategies
            .iter()
            .all(|strategy| strategy.profile == Some(Profile::FunctionalBoolV1))
    );
    assert!(plan.restrict(Profile::CheckedI64V1).is_err());
}

#[test]
fn a_portfolio_judges_every_run_under_one_incumbent() {
    let seeds = seeds('B');
    let majority = seeds
        .iter()
        .find(|seed| seed.id == "B07")
        .unwrap_or_else(|| panic!("B07"));
    let plan = default_portfolio();
    let search = searched(super::optimize_plan(
        &majority.original,
        &plan,
        &[],
        DEFAULT_MAX_INPUT_TUPLES,
    ));
    assert_eq!(search.runs.len(), 3);
    let per_run = |run: usize| {
        search
            .phases
            .iter()
            .filter(|phase| phase.run == run)
            .count()
    };
    assert_eq!(
        (per_run(0), per_run(1), per_run(2)),
        (11, 11, 14),
        "every phase is tagged with its run"
    );
    assert_eq!(search.candidates.len(), 36);
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    assert_eq!(best.nodes, 5);
    // One incumbent: no later candidate was accepted as better than it, and
    // a later run's equal candidates are duplicates or not better.
    let accepted: Vec<(usize, usize)> = search
        .candidates
        .iter()
        .filter(|candidate| matches!(candidate.verdict, Verdict::Accepted))
        .map(|candidate| (candidate.run, candidate.nodes.unwrap_or(usize::MAX)))
        .collect();
    assert!(
        accepted.windows(2).all(|pair| pair[1].1 <= pair[0].1),
        "{accepted:?}"
    );
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict))
    );
    assert_eq!(
        transform::replay(
            &best.equivalence.receipt(),
            &majority.original,
            &best.bytes,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
    let report = search.json();
    assert_eq!(report["strategy"]["schema"], PORTFOLIO_SCHEMA);
    assert_eq!(report["search"]["phases_requested"], 36);
    assert_eq!(report["search"]["runs"][2]["run"], 2);
    let super::Source::Phase { run, .. } = best.source else {
        panic!("an extracted candidate");
    };
    assert_eq!(report["best"]["run"], run);
    assert_eq!(report["best"]["supplied"], Value::Null);
    let again = searched(super::optimize_plan(
        &majority.original,
        &plan,
        &[],
        DEFAULT_MAX_INPUT_TUPLES,
    ));
    assert_eq!(again.json(), report);
}

#[test]
fn a_work_budget_stops_a_run_at_a_fixed_point() {
    let original = artifact("retained-controller-original");
    let mut strategy = default_strategy();
    strategy.limits.max_work = 1;
    let search = run(original, &strategy);
    let limited: Vec<&str> = search
        .phases
        .iter()
        .filter_map(|phase| phase.limit_hit.map(Limit::name))
        .collect();
    assert!(limited.contains(&"max_work"), "{limited:?}");
    // The work counter is deterministic, so the stop point is too.
    assert_eq!(run(original, &strategy).json(), search.json());
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict))
    );
    if let Some(best) = &search.best {
        assert!(best.nodes < 69);
    }
    let first_stop = search
        .phases
        .iter()
        .position(|phase| phase.limit_hit == Some(Limit::Work))
        .unwrap_or_else(|| panic!("a phase stops"));
    assert!(
        search.phases[first_stop..]
            .iter()
            .all(|phase| phase.rounds_run == 0 || phase.limit_hit.is_some())
    );
    assert!(search.runs[0].work >= 1_000_000);
    assert_eq!(Limit::Work.name(), "max_work");
}

// ---- choice fusion -----------------------------------------------------------

/// x in [0, 99_999] (beyond the table cap) with outputs (x - 7) + 7 and
/// (x - 9) + 9: no rule or table shows either equals x. Each supplied
/// program simplifies one output.
fn fusion_case() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let wide = Domain::Int {
        min: 0,
        max: 99_999,
    };
    let original = program(
        vec![wide],
        vec![FULL, FULL],
        vec![
            Op::Input(0),
            Op::Int(7),
            Op::Sub(0, 1),
            Op::Add(2, 1),
            Op::Int(9),
            Op::Sub(0, 4),
            Op::Add(5, 4),
        ],
        vec![3, 6],
    );
    let first = program(
        vec![wide],
        vec![FULL, FULL],
        vec![Op::Input(0), Op::Int(9), Op::Sub(0, 1), Op::Add(2, 1)],
        vec![0, 3],
    );
    let second = program(
        vec![wide],
        vec![FULL, FULL],
        vec![Op::Input(0), Op::Int(7), Op::Sub(0, 1), Op::Add(2, 1)],
        vec![3, 0],
    );
    (encode(&original), encode(&first), encode(&second))
}

#[test]
fn fusing_checked_candidates_combines_their_best_outputs() {
    let (original, first, second) = fusion_case();
    let plan = Plan::single(default_strategy());
    // Alone, the optimizer finds nothing: no rule rewrites x - c + c.
    assert_eq!(best_nodes(&run(&original, &default_strategy())), None);
    // Each supplied program is checked and fused; the roots merge by the
    // checker's verdict because the original can never fail.
    let search = searched(super::optimize_plan(
        &original,
        &plan,
        &[first.clone(), second.clone()],
        DEFAULT_MAX_INPUT_TUPLES,
    ));
    assert_eq!(search.pinned_nodes, 0);
    assert!(
        search
            .supplied
            .iter()
            .all(|supplied| supplied.verdict.accepted()),
        "{:?}",
        search.supplied
    );
    assert_eq!(search.runs[0].fused_roots, (4, 0));
    let best = search
        .best
        .as_ref()
        .unwrap_or_else(|| panic!("improvement"));
    // Both outputs are x: better than either supplied program (4 nodes).
    assert_eq!(import(&best.bytes).nodes(), &[Op::Input(0)]);
    assert!(matches!(best.source, super::Source::Phase { .. }));
    assert_eq!(
        transform::replay(
            &best.equivalence.receipt(),
            &original,
            &best.bytes,
            DEFAULT_MAX_INPUT_TUPLES
        ),
        Replay::Matched
    );
    let report = search.json();
    assert_eq!(report["supplied"][0]["fused"], true);
    assert_eq!(report["search"]["runs"][0]["fused_roots"]["merged"], 4);
    // A wrong program is refuted by the checker and never fused.
    let wrong = encode(&program(
        vec![Domain::Int {
            min: 0,
            max: 99_999,
        }],
        vec![FULL, FULL],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![2, 0],
    ));
    let search = searched(super::optimize_plan(
        &original,
        &plan,
        &[wrong, b"not a program".to_vec()],
        DEFAULT_MAX_INPUT_TUPLES,
    ));
    assert_eq!(search.supplied[0].verdict.name(), "counterexample");
    assert_eq!(search.supplied[1].verdict.name(), "unextractable");
    assert_eq!(search.runs[0].fused_roots, (0, 0));
    assert!(search.best.is_none());
}

#[test]
fn fusion_merges_roots_of_a_trapping_original_only_by_exact_tables() {
    // The original traps at x = MAX; its roots may merge with a supplied
    // program's only where both have identical exact tables.
    let base = unused_trap();
    let supplied = encode(&program(
        vec![NEAR_MAX],
        vec![FULL],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![0],
    ));
    let search = searched(super::optimize_plan(
        &encode(&base),
        &Plan::single(default_strategy()),
        &[supplied],
        DEFAULT_MAX_INPUT_TUPLES,
    ));
    assert_eq!(search.pinned_nodes, 1);
    assert_eq!(search.supplied[0].verdict.name(), "accepted-not-better");
    assert_eq!(search.runs[0].fused_roots, (1, 0), "the tables agree");
    // The supplied Add is not pinned again: the candidates keep one trap.
    assert!(
        search
            .candidates
            .iter()
            .all(|candidate| screened(&candidate.verdict))
    );
    assert!(search.best.is_none());
}
