//! Unit, property, determinism and budget tests. The oracle is the library
//! evaluator through `Program::evaluate`, a different entry point from the
//! `execute_v2` path the search uses, over an enumeration written here.

use super::*;
use crate::ir::Node;
use crate::pool::canonical;
use std::vec;
use std::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op};

const I128: Domain = Domain::Int {
    min: i128::MIN,
    max: i128::MAX,
};

#[test]
fn wide_arithmetic_search_keeps_each_proposer_and_exact_checker() {
    let wide = 1i128 << 100;
    for operation in [
        Op::MulDivFloor(0, 1, 1),
        Op::ProductsGe([0, 1, 1], [0, 1, 1]),
    ] {
        let boolean = matches!(operation, Op::ProductsGe(..));
        let output = if boolean { Domain::Bool } else { I128 };
        let original = bytes(
            vec![int(wide, wide + 1)],
            vec![output],
            vec![Op::Input(0), Op::Int(3), operation],
            vec![2],
        );
        let wrong = bytes(
            vec![int(wide, wide + 1)],
            vec![output],
            vec![if boolean { Op::Bool(false) } else { Op::Int(0) }],
            vec![0],
        );
        for settings in [
            only(true, false, false),
            only(false, true, false),
            only(false, false, true),
        ] {
            let outcome = search(&original, &[&wrong], &settings);
            assert!(!matches!(outcome, Outcome::Stopped(_)), "{outcome:?}");
            assert_exact(&original, &outcome);
        }
    }
}

#[test]
fn arithmetic_graphs_and_proposers_keep_all_operands_and_checker_controls() {
    for operation in [
        Op::MulDivFloor(0, 1, 2),
        Op::MulDivCeil(0, 1, 2),
        Op::ProductsGe([0, 1, 2], [2, 1, 1]),
    ] {
        let boolean = matches!(operation, Op::ProductsGe(..));
        let output = if boolean { Domain::Bool } else { I128 };
        let original = bytes(
            vec![int(0, 40)],
            vec![output],
            vec![Op::Input(0), Op::Int(2), Op::Int(3), operation],
            vec![3],
        );
        let admitted = import(&original);
        let graph = Graph::from_program(&admitted).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(graph.to_program(), Some(admitted));
        let wrong = bytes(
            vec![int(0, 40)],
            vec![output],
            if boolean {
                vec![Op::Bool(false)]
            } else {
                vec![Op::Int(0)]
            },
            vec![0],
        );
        let right_table = table(&original);
        let wrong_table = table(&wrong);
        let control = |compare: &dyn Fn(&Vec<Seen>, &Vec<Seen>) -> bool| {
            compare(&right_table, &right_table) && !compare(&right_table, &wrong_table)
        };
        assert!(control(&|a, b| a == b));
        assert!(!control(&|_, _| true), "stubbed comparison must fail");
        for settings in [
            only(true, false, false),
            only(false, true, false),
            only(false, false, true),
        ] {
            let outcome = search(&original, &[&wrong], &settings);
            assert!(!matches!(outcome, Outcome::Stopped(_)), "{outcome:?}");
            assert_exact(&original, &outcome);
        }
    }
}

#[test]
fn arithmetic_probe_uses_d1_refusals_and_full_product_precision() {
    use crate::arithmetic::evaluate;
    let max = [i128::MAX];
    assert_eq!(
        evaluate(Op::MulDivFloor(0, 1, 2), &[&max, &max, &max]),
        Some(vec![i128::MAX])
    );
    assert_eq!(
        evaluate(Op::MulDivCeil(0, 1, 2), &[&max, &max, &max]),
        Some(vec![i128::MAX])
    );
    assert_eq!(
        evaluate(
            Op::ProductsGe([0, 1, 2], [0, 1, 3]),
            &[&max, &max, &max, &[i128::MAX - 1]]
        ),
        Some(vec![1])
    );
    assert_eq!(
        evaluate(Op::MulDivFloor(0, 1, 2), &[&max, &max, &[0]]),
        None
    );
    assert_eq!(evaluate(Op::MulDivCeil(0, 1, 2), &[&max, &max, &[1]]), None);
    assert_eq!(
        evaluate(Op::ProductsGe([0, 0, 0], [1, 1, 1]), &[&[1], &[-1]]),
        None
    );
}

#[test]
fn scaled_cpmm_search_reports_an_exact_improvement_or_none() {
    for bps in [0, 30, 100] {
        let inputs = vec![int(0, 40); 3];
        let outputs = vec![int(0, 40), int(0, 40), int(0, 80), int(0, 40), Domain::Bool];
        let nodes = vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::Int(bps),
            Op::Int(10000),
            Op::MulDivCeil(2, 3, 4),
            Op::Sub(2, 5),
            Op::Add(0, 6),
            Op::Int(0),
            Op::Eq(7, 8),
            Op::Int(1),
            Op::Select(9, 10, 7),
            Op::MulDivFloor(6, 1, 11),
            Op::Add(0, 2),
            Op::Sub(1, 12),
            Op::ProductsGe([13, 14, 10], [0, 1, 10]),
            Op::Add(12, 8),
        ];
        let roots = vec![5, 16, 13, 14, 15];
        let original = bytes(
            inputs.clone(),
            outputs.clone(),
            nodes.clone(),
            roots.clone(),
        );
        let mut wrong_nodes = nodes;
        wrong_nodes[16] = Op::Int(0);
        let wrong = bytes(inputs, outputs, wrong_nodes, roots);
        assert_ne!(table(&original), table(&wrong));
        let mut settings = only(false, true, false);
        settings.budget.rounds = 1;
        settings.budget.resub_rounds = 1;
        settings.shape.resub_choices = 8;
        let outcome = search(&original, &[&wrong], &settings);
        assert!(
            !matches!(outcome, Outcome::Stopped(_)),
            "fee {bps}: {outcome:?}"
        );
        assert_exact(&original, &outcome);
        match outcome {
            Outcome::Improved(candidate) => {
                eprintln!("scaled CPMM fee {bps}: {} nodes", candidate.nodes)
            }
            Outcome::Unimproved(_) => {
                eprintln!("scaled CPMM fee {bps}: none found in declared search budget")
            }
            Outcome::Stopped(_) => {}
        }
    }
}

fn int(min: i128, max: i128) -> Domain {
    Domain::Int { min, max }
}

fn bytes(inputs: Vec<Domain>, outputs: Vec<Domain>, nodes: Vec<Op>, roots: Vec<u16>) -> Vec<u8> {
    let program = match Program::try_new(inputs, outputs, nodes, roots) {
        Ok(program) => program,
        Err(error) => panic!("test program refused: {error:?}"),
    };
    match encode(&program) {
        Some(bytes) => bytes,
        None => panic!("test program did not encode"),
    }
}

fn import(bytes: &[u8]) -> Program {
    match import_program(bytes) {
        Ok(program) => program,
        Err(error) => panic!("import refused: {error:?}"),
    }
}

/// Every tuple, last input fastest, written independently of `Space`.
fn tuples(inputs: &[Domain]) -> Vec<Vec<i128>> {
    let mut all = vec![Vec::new()];
    for domain in inputs {
        let (min, max) = domain.bounds();
        let mut next = Vec::new();
        for prefix in &all {
            let mut value = min;
            loop {
                let mut tuple = prefix.clone();
                tuple.push(value);
                next.push(tuple);
                if value == max {
                    break;
                }
                value += 1;
            }
        }
        all = next;
    }
    all
}

#[derive(Debug, Eq, PartialEq)]
enum Seen {
    Ok(Vec<i128>),
    Arithmetic,
    Refused(&'static str),
}

fn table(bytes: &[u8]) -> Vec<Seen> {
    let program = import(bytes);
    tuples(program.inputs())
        .iter()
        .map(|tuple| match program.evaluate(tuple) {
            Ok(values) => Seen::Ok(values),
            Err(Error::Arithmetic) => Seen::Arithmetic,
            Err(Error::Invalid(code)) => Seen::Refused(code),
            Err(other) => panic!("unexpected evaluation failure {other:?}"),
        })
        .collect()
}

/// A returned candidate has the same ABI and table and is strictly smaller.
fn assert_exact(original: &[u8], outcome: &Outcome) {
    if let Outcome::Improved(candidate) = outcome {
        let (before, after) = (import(original), import(&candidate.bytes));
        assert_eq!(before.inputs(), after.inputs());
        assert_eq!(before.outputs(), after.outputs());
        assert_eq!(table(original), table(&candidate.bytes));
        assert_eq!(candidate.nodes, after.nodes().len());
        assert!(
            (after.nodes().len(), candidate.bytes.len()) < (before.nodes().len(), original.len())
        );
    }
}

fn only(top_down: bool, resub: bool, fold: bool) -> Settings {
    Settings {
        proposers: Proposers {
            top_down,
            resub,
            fold,
            diagram: false,
            ..Proposers::default()
        },
        ..Settings::default()
    }
}

fn diagram_only() -> Settings {
    Settings {
        proposers: Proposers {
            top_down: false,
            resub: false,
            fold: false,
            diagram: true,
            ..Proposers::default()
        },
        ..Settings::default()
    }
}

fn diagram_report(outcome: &Outcome) -> DiagramReport {
    let provenance = match outcome {
        Outcome::Improved(candidate) => &candidate.provenance,
        Outcome::Unimproved(provenance) => provenance,
        Outcome::Stopped(stop) => panic!("unexpected stop {stop:?}"),
    };
    match &provenance.diagram {
        Some(report) => report.clone(),
        None => panic!("no diagram report"),
    }
}

fn improved_nodes(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Improved(candidate) => candidate.nodes,
        other => panic!("expected an improvement, got {other:?}"),
    }
}

/// `x < 4 ? x + 0 : x` over x in 0..=7, y unused.
fn redundant_select() -> Vec<u8> {
    bytes(
        vec![int(0, 7), int(0, 3)],
        vec![int(0, 7)],
        vec![
            Op::Input(0),
            Op::Int(4),
            Op::Lt(0, 1),
            Op::Int(0),
            Op::Add(0, 3),
            Op::Select(2, 4, 0),
        ],
        vec![5],
    )
}

#[test]
fn resubstitution_alone_removes_a_redundant_selection() {
    let original = redundant_select();
    let outcome = search(&original, &[], &only(false, true, false));
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 1);
}

#[test]
fn top_down_alone_rebuilds_an_inverted_expression() {
    // (x - 3) + 3 over x in 0..=7 is x.
    let original = bytes(
        vec![int(0, 7)],
        vec![int(0, 7)],
        vec![Op::Input(0), Op::Int(3), Op::Sub(0, 1), Op::Add(2, 1)],
        vec![3],
    );
    let outcome = search(&original, &[], &only(true, false, false));
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 1);
}

#[test]
fn top_down_synthesizes_a_case_split_and_two_valued_selection() {
    // Three equality tests selecting between two values.
    let original = bytes(
        vec![int(0, 9)],
        vec![int(0, 100)],
        vec![
            Op::Input(0),
            Op::Int(2),
            Op::Eq(0, 1),
            Op::Int(5),
            Op::Eq(0, 3),
            Op::Int(8),
            Op::Eq(0, 5),
            Op::Int(10),
            Op::Int(20),
            Op::Select(6, 7, 8),
            Op::Select(4, 8, 9),
            Op::Select(2, 8, 10),
        ],
        vec![11],
    );
    let outcome = search(&original, &[], &only(true, false, false));
    assert_exact(&original, &outcome);
    assert!(improved_nodes(&outcome) < 12);
}

#[test]
fn folding_alone_drops_the_conjunction_of_a_decision_list() {
    // (a && b) ? x : (a ? y : z)
    let original = bytes(
        vec![Domain::Bool, Domain::Bool, int(0, 3), int(0, 3), int(0, 3)],
        vec![int(0, 3)],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Input(2),
            Op::Input(3),
            Op::Input(4),
            Op::And(0, 1),
            Op::Select(0, 3, 4),
            Op::Select(5, 2, 6),
        ],
        vec![7],
    );
    let outcome = search(&original, &[], &only(false, false, true));
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 7);
}

#[test]
fn an_unselected_trapping_arm_is_kept_and_a_simplification_that_drops_it_is_refused() {
    // true ? x : x + 1 over x in [MAX - 1, MAX]: the unselected addition traps
    // at MAX, so the program refuses there; plain `x` would not.
    let original = bytes(
        vec![int(i128::MAX - 1, i128::MAX)],
        vec![I128],
        vec![
            Op::Input(0),
            Op::Bool(true),
            Op::Int(1),
            Op::Add(0, 2),
            Op::Select(1, 0, 3),
        ],
        vec![4],
    );
    assert_eq!(table(&original)[1], Seen::Arithmetic);
    for settings in [
        Settings::default(),
        only(true, false, false),
        only(false, true, false),
    ] {
        let outcome = search(&original, &[], &settings);
        assert_exact(&original, &outcome);
        if let Outcome::Unimproved(provenance) | Outcome::Improved(Candidate { provenance, .. }) =
            &outcome
        {
            assert_eq!(provenance.trapping_tuples, 1);
        }
    }
    // Resubstitution proposes plain `x` for the selection; the check refuses
    // it at the trapping tuple (ordinal 1). What it keeps, `(x + 1) - 1`,
    // still traps there.
    let outcome = search(&original, &[], &only(false, true, false));
    assert_exact(&original, &outcome);
    let provenance = match &outcome {
        Outcome::Improved(candidate) => &candidate.provenance,
        Outcome::Unimproved(provenance) => provenance,
        Outcome::Stopped(stop) => panic!("unexpected stop {stop:?}"),
    };
    assert!(
        provenance
            .refused
            .iter()
            .any(|refused| refused.refusal == Refusal::Differs { ordinal: 1 }),
        "{provenance:?}"
    );
}

#[test]
fn a_trapping_subtraction_keeps_its_trap() {
    // (x - 1) + 0 over x in [MIN, MIN + 2]: traps at MIN.
    let original = bytes(
        vec![int(i128::MIN, i128::MIN + 2)],
        vec![I128],
        vec![
            Op::Input(0),
            Op::Int(1),
            Op::Sub(0, 1),
            Op::Int(0),
            Op::Add(2, 3),
        ],
        vec![4],
    );
    assert_eq!(table(&original)[0], Seen::Arithmetic);
    let outcome = search(&original, &[], &Settings::default());
    assert_exact(&original, &outcome);
    // `x - 1` keeps the trap and drops the addition.
    assert_eq!(improved_nodes(&outcome), 3);
}

#[test]
fn a_dead_trapping_node_makes_every_tuple_refuse_and_cannot_be_dropped() {
    let original = bytes(
        vec![int(0, 3)],
        vec![int(0, 3)],
        vec![Op::Input(0), Op::Int(i128::MAX), Op::Add(1, 1)],
        vec![0],
    );
    assert!(
        table(&original)
            .iter()
            .all(|seen| *seen == Seen::Arithmetic)
    );
    let outcome = search(&original, &[], &Settings::default());
    assert_exact(&original, &outcome);
    assert!(matches!(outcome, Outcome::Unimproved(_)), "{outcome:?}");
}

#[test]
fn a_dead_safe_node_is_dropped() {
    let original = bytes(
        vec![int(0, 3)],
        vec![int(0, 3)],
        vec![Op::Input(0), Op::Int(7), Op::Add(0, 1)],
        vec![0],
    );
    let outcome = search(&original, &[], &Settings::default());
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 1);
}

#[test]
fn output_domain_refusals_are_preserved_at_the_edge() {
    // x + 0 with output domain 0..=5 over x in 0..=9 refuses for x > 5; `x`
    // refuses on exactly the same tuples.
    let original = bytes(
        vec![int(0, 9)],
        vec![int(0, 5)],
        vec![Op::Input(0), Op::Int(0), Op::Add(0, 1)],
        vec![2],
    );
    let refused = table(&original)
        .iter()
        .filter(|seen| matches!(seen, Seen::Refused("output-domain")))
        .count();
    assert_eq!(refused, 4);
    let outcome = search(&original, &[], &Settings::default());
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 1);
}

#[test]
fn a_boolean_output_reaches_its_predicate() {
    // !(x == 3) == false is x == 3.
    let original = bytes(
        vec![int(0, 7)],
        vec![Domain::Bool],
        vec![
            Op::Input(0),
            Op::Int(3),
            Op::Eq(0, 1),
            Op::Not(2),
            Op::Bool(false),
            Op::Eq(3, 4),
        ],
        vec![5],
    );
    let outcome = search(&original, &[], &Settings::default());
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 3);
}

#[test]
fn counterexample_guided_synthesis_is_exact() {
    // (x == 3 ? 7 : y) + 0 over a 31 x 31 domain, on the sampled path
    // (forced with a small threshold).
    let original = bytes(
        vec![int(0, 30), int(0, 30)],
        vec![I128],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(3),
            Op::Eq(0, 2),
            Op::Int(7),
            Op::Select(3, 4, 1),
            Op::Int(0),
            Op::Add(5, 6),
        ],
        vec![7],
    );
    let mut settings = only(true, false, false);
    settings.shape.cegis_tuples = 64;
    settings.shape.sample = 16;
    let outcome = search(&original, &[], &settings);
    assert_exact(&original, &outcome);
    assert_eq!(improved_nodes(&outcome), 6);
}

#[test]
fn seeds_are_checked_a_wrong_seed_is_refused_and_a_right_one_can_win() {
    let original = redundant_select();
    let wrong = bytes(
        vec![int(0, 7), int(0, 3)],
        vec![int(0, 7)],
        vec![Op::Input(1)],
        vec![0],
    );
    let right = bytes(
        vec![int(0, 7), int(0, 3)],
        vec![int(0, 7)],
        vec![Op::Input(0)],
        vec![0],
    );
    let none = Settings {
        proposers: Proposers {
            top_down: false,
            resub: false,
            fold: false,
            diagram: false,
            ..Proposers::default()
        },
        ..Settings::default()
    };
    let seeds: [&[u8]; 3] = [wrong.as_slice(), b"not a program", right.as_slice()];
    match search(&original, &seeds, &none) {
        Outcome::Improved(candidate) => {
            assert_eq!(candidate.bytes, right);
            assert_eq!(candidate.provenance.winner, "seed");
            assert_eq!(candidate.provenance.refused.len(), 2);
            assert_eq!(
                candidate.provenance.refused[0].refusal,
                Refusal::Differs { ordinal: 1 }
            );
            assert_eq!(
                candidate.provenance.refused[1].refusal,
                Refusal::NotAdmitted
            );
        }
        other => panic!("expected the right seed to win, got {other:?}"),
    }
}

#[test]
fn budgets_stop_with_a_typed_reason_and_no_candidate() {
    let original = redundant_select();
    let mut settings = Settings::default();
    settings.budget.max_tuples = 31;
    assert_eq!(
        search(&original, &[], &settings),
        Outcome::Stopped(Stop::DomainOverBudget {
            tuples: 32,
            limit: 31
        })
    );
    let mut settings = Settings::default();
    settings.budget.work_units = 100;
    assert_eq!(
        search(&original, &[], &settings),
        Outcome::Stopped(Stop::WorkBudget { limit: 100 })
    );
    let mut settings = Settings::default();
    settings.budget.max_cells = 40;
    assert!(matches!(
        search(&original, &[], &settings),
        Outcome::Stopped(Stop::CellsOverBudget { limit: 40, .. })
    ));
    let seeds: Vec<&[u8]> = vec![original.as_slice(); MAX_SEEDS + 1];
    assert_eq!(
        search(&original, &seeds, &Settings::default()),
        Outcome::Stopped(Stop::TooManySeeds { limit: MAX_SEEDS })
    );
    assert!(matches!(
        search(b"\x08", &[], &Settings::default()),
        Outcome::Stopped(Stop::NotAdmitted { .. })
    ));
    // A budget that runs out late still returns nothing: find the smallest
    // budget that completes, then check one unit less stops.
    let complete = match search(&original, &[], &Settings::default()) {
        Outcome::Improved(candidate) => candidate.provenance.work_used,
        other => panic!("expected an improvement, got {other:?}"),
    };
    let mut settings = Settings::default();
    settings.budget.work_units = complete - 1;
    assert_eq!(
        search(&original, &[], &settings),
        Outcome::Stopped(Stop::WorkBudget {
            limit: complete - 1
        })
    );
    settings.budget.work_units = complete;
    assert!(matches!(
        search(&original, &[], &settings),
        Outcome::Improved(_)
    ));
}

#[test]
fn enumeration_order_is_last_input_fastest() {
    let inputs = [int(-1, 1), Domain::Bool, int(5, 6)];
    let space = match Space::new(&inputs, 1 << 10) {
        Ok(space) => space,
        Err(stop) => panic!("{stop:?}"),
    };
    let expected = tuples(&inputs);
    assert_eq!(space.count(), expected.len());
    let mut tuple = Vec::new();
    for (ordinal, want) in expected.iter().enumerate() {
        space.tuple(ordinal, &mut tuple);
        assert_eq!(&tuple, want);
    }
    let ordinals: Vec<u32> = (0..12).collect();
    assert_eq!(
        space.column(1, &ordinals),
        vec![0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1]
    );
}

#[test]
fn canonical_form_merges_commuted_operands_and_drops_dead_nodes() {
    let graph = |nodes: Vec<Node>, roots: Vec<usize>| Graph {
        inputs: vec![int(0, 3), int(0, 3)],
        outputs: vec![I128, I128],
        nodes,
        roots,
    };
    let merged = canonical(&graph(
        vec![
            Node::Input(0),
            Node::Input(1),
            Node::Add(0, 1),
            Node::Add(1, 0),
        ],
        vec![2, 3],
    ));
    assert_eq!(
        merged.nodes,
        vec![Node::Input(0), Node::Input(1), Node::Add(0, 1)]
    );
    assert_eq!(merged.roots, vec![2, 2]);
    let pruned = canonical(&graph(
        vec![
            Node::Input(0),
            Node::Input(1),
            Node::Int(9),
            Node::Add(1, 0),
        ],
        vec![3, 3],
    ));
    assert_eq!(pruned, merged);
    assert_eq!(canonical(&merged), merged);
}

/// SplitMix64 for test-program generation.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

/// A random well-typed program over small domains, sometimes at the i128
/// edges (so additions trap) and sometimes with narrow output domains.
fn random_program(rng: &mut Gen) -> Vec<u8> {
    loop {
        let input_count = 1 + rng.below(3);
        let mut inputs = Vec::new();
        for _ in 0..input_count {
            inputs.push(match rng.below(8) {
                0 | 1 => Domain::Bool,
                2 => int(i128::MAX - 2, i128::MAX),
                3 => int(i128::MIN, i128::MIN + 2),
                4 => int(0, 7),
                5 => int(-2, 2),
                _ => int(0, 3),
            });
        }
        let mut nodes = Vec::new();
        let mut bools = Vec::new();
        let mut ints = Vec::new();
        for (k, domain) in inputs.iter().enumerate() {
            let id = nodes.len() as u16;
            nodes.push(Op::Input(k as u16));
            if *domain == Domain::Bool {
                bools.push(id);
            } else {
                ints.push(id);
            }
        }
        for _ in 0..1 + rng.below(3) {
            let id = nodes.len() as u16;
            if rng.below(3) == 0 {
                nodes.push(Op::Bool(rng.below(2) == 0));
                bools.push(id);
            } else {
                nodes.push(Op::Int(rng.pick(&[0, 1, 2, 3, -1, 5, i128::MAX])));
                ints.push(id);
            }
        }
        for _ in 0..3 + rng.below(14) {
            let id = nodes.len() as u16;
            match rng.below(7) {
                0 | 1 if !ints.is_empty() => {
                    let (a, b) = (rng.pick(&ints), rng.pick(&ints));
                    nodes.push(if rng.below(2) == 0 {
                        Op::Add(a, b)
                    } else {
                        Op::Sub(a, b)
                    });
                    ints.push(id);
                }
                2 if !ints.is_empty() => {
                    let (a, b) = (rng.pick(&ints), rng.pick(&ints));
                    nodes.push(if rng.below(2) == 0 {
                        Op::Eq(a, b)
                    } else {
                        Op::Lt(a, b)
                    });
                    bools.push(id);
                }
                3 if !bools.is_empty() => {
                    nodes.push(if rng.below(2) == 0 {
                        Op::And(rng.pick(&bools), rng.pick(&bools))
                    } else {
                        Op::Not(rng.pick(&bools))
                    });
                    bools.push(id);
                }
                _ if !bools.is_empty() => {
                    let cond = rng.pick(&bools);
                    if !ints.is_empty() && rng.below(3) != 0 {
                        nodes.push(Op::Select(cond, rng.pick(&ints), rng.pick(&ints)));
                        ints.push(id);
                    } else {
                        nodes.push(Op::Select(cond, rng.pick(&bools), rng.pick(&bools)));
                        bools.push(id);
                    }
                }
                _ => {}
            }
        }
        let mut outputs = Vec::new();
        let mut roots = Vec::new();
        for _ in 0..1 + rng.below(2) {
            let root = nodes.len() - 1 - rng.below(nodes.len().min(4));
            roots.push(root as u16);
            outputs.push(if bools.contains(&(root as u16)) {
                Domain::Bool
            } else if rng.below(4) == 0 {
                int(-4, 4)
            } else {
                I128
            });
        }
        if let Ok(program) = Program::try_new(inputs, outputs, nodes, roots)
            && let Some(bytes) = encode(&program)
        {
            return bytes;
        }
    }
}

fn property_settings() -> Settings {
    let mut settings = Settings::default();
    settings.shape.solve_limit = 400;
    settings.budget.rounds = 3;
    settings
}

#[test]
fn every_returned_candidate_has_the_original_table() {
    let mut rng = Gen(0x5eed);
    let settings = property_settings();
    let (mut improved, mut trapping) = (0, 0);
    for _ in 0..120 {
        let original = random_program(&mut rng);
        let outcome = search(&original, &[], &settings);
        assert_exact(&original, &outcome);
        match &outcome {
            Outcome::Improved(candidate) => {
                improved += 1;
                trapping += usize::from(candidate.provenance.trapping_tuples > 0);
            }
            Outcome::Unimproved(_) => {}
            Outcome::Stopped(stop) => panic!("unexpected stop {stop:?}"),
        }
    }
    // Non-vacuous: the generator leaves room for improvement, including on
    // programs that trap somewhere.
    std::println!("120 random programs: {improved} improved, {trapping} of them trap somewhere");
    assert!(improved >= 40, "only {improved} improved");
    assert!(trapping >= 1, "no improved program traps anywhere");
}

#[test]
fn two_runs_give_identical_results() {
    let mut rng = Gen(0xd1ce);
    let settings = property_settings();
    let mut programs: Vec<Vec<u8>> = (0..25).map(|_| random_program(&mut rng)).collect();
    programs.push(redundant_select());
    for original in &programs {
        assert_eq!(
            search(original, &[], &settings),
            search(original, &[], &settings)
        );
    }
    let mut sampled = only(true, false, false);
    sampled.shape.cegis_tuples = 4;
    sampled.shape.sample = 8;
    assert_eq!(
        search(&redundant_select(), &[], &sampled),
        search(&redundant_select(), &[], &sampled)
    );
}

/// `state == 0 && flag ? 0 : 3`, where state 0 with the flag set is an
/// invalid business state that stays in the table.
fn select_table() -> Vec<u8> {
    bytes(
        vec![int(0, 2), Domain::Bool],
        vec![int(0, 3)],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Int(1),
            Op::Int(0),
            Op::Eq(0, 3),
            Op::And(4, 1),
            Op::Int(3),
            Op::Select(5, 3, 6),
        ],
        vec![7],
    )
}

#[test]
fn diagram_resynthesis_keeps_every_declared_tuple_and_shrinks_the_program() {
    let original = select_table();
    let expected: Vec<Seen> = [3, 0, 3, 3, 3, 3]
        .iter()
        .map(|value| Seen::Ok(vec![*value]))
        .collect();
    assert_eq!(table(&original), expected);
    let outcome = search(&original, &[], &diagram_only());
    assert_exact(&original, &outcome);
    match &outcome {
        Outcome::Improved(candidate) => assert_eq!(candidate.provenance.winner, "dd-resynth"),
        other => panic!("expected an improvement, got {other:?}"),
    }
    let report = diagram_report(&outcome);
    assert_eq!((report.orders_tried, report.orders_completed), (2, 2));
    assert_eq!(report.skipped, None);
    assert_eq!(outcome, search(&original, &[], &diagram_only()));
}

#[test]
fn diagram_resynthesis_reads_inputs_and_constants_directly() {
    // A conjunction is already its own diagram; projections and a constant
    // output are terminals.
    let conjunction = bytes(
        vec![Domain::Bool, Domain::Bool],
        vec![Domain::Bool],
        vec![Op::Input(0), Op::Input(1), Op::And(0, 1)],
        vec![2],
    );
    let outcome = search(&conjunction, &[], &diagram_only());
    assert!(matches!(outcome, Outcome::Unimproved(_)), "{outcome:?}");
    assert_eq!(diagram_report(&outcome).orders_completed, 2);
    let projection = bytes(
        vec![int(-2, 2), Domain::Bool],
        vec![int(-2, 2), Domain::Bool, int(7, 7)],
        vec![Op::Input(0), Op::Input(1), Op::Int(7)],
        vec![0, 1, 2],
    );
    let outcome = search(&projection, &[], &diagram_only());
    assert!(matches!(outcome, Outcome::Unimproved(_)), "{outcome:?}");
    assert_eq!(
        diagram_report(&outcome).resynthesized_outputs,
        vec![0, 1, 2]
    );
}

#[test]
fn diagram_resynthesis_copies_a_many_valued_output() {
    let original = bytes(
        vec![int(0, 15), int(0, 1)],
        vec![int(-1, 15), Domain::Bool],
        vec![
            Op::Input(0),
            Op::Input(1),
            Op::Sub(0, 1),
            Op::Int(0),
            Op::Eq(1, 3),
            Op::Int(9),
            Op::Add(1, 5),
        ],
        vec![2, 4],
    );
    let outcome = search(&original, &[], &diagram_only());
    assert_exact(&original, &outcome);
    let report = diagram_report(&outcome);
    assert_eq!(report.copied_outputs, vec![0]);
    assert_eq!(report.resynthesized_outputs, vec![1]);
}

#[test]
fn diagram_resynthesis_is_skipped_or_bounded_by_its_limits() {
    // A trapping original: a diagram never traps, so the proposer is skipped.
    let trapping = bytes(
        vec![int(i128::MAX - 1, i128::MAX)],
        vec![I128],
        vec![Op::Input(0), Op::Int(1), Op::Add(0, 1)],
        vec![2],
    );
    let skipped = |original: &[u8], settings: &Settings| {
        diagram_report(&search(original, &[], settings)).skipped
    };
    assert_eq!(skipped(&trapping, &diagram_only()), Some("original-traps"));
    let wide = bytes(
        vec![int(0, 16)],
        vec![Domain::Bool],
        vec![Op::Bool(false)],
        vec![0],
    );
    assert_eq!(
        skipped(&wide, &diagram_only()),
        Some("input-cardinality-over-diagram-limit")
    );
    let mut small = diagram_only();
    small.shape.diagram_tuples = 5;
    assert_eq!(
        skipped(&select_table(), &small),
        Some("domain-over-diagram-limit")
    );
    // An output-domain refusal is kept: the raw value is reproduced.
    let narrow = bytes(
        vec![int(0, 2)],
        vec![int(0, 1)],
        vec![Op::Input(0)],
        vec![0],
    );
    assert_eq!(skipped(&narrow, &diagram_only()), None);
    let mut one = diagram_only();
    one.shape.diagram_orders = 1;
    let report = diagram_report(&search(&select_table(), &[], &one));
    assert_eq!((report.orders_tried, report.orders_completed), (1, 1));
    let mut tiny = diagram_only();
    tiny.shape.diagram_nodes = 1;
    let outcome = search(&select_table(), &[], &tiny);
    assert!(matches!(outcome, Outcome::Unimproved(_)), "{outcome:?}");
    let report = diagram_report(&outcome);
    assert_eq!((report.orders_tried, report.orders_over_size), (2, 2));
}

#[test]
fn a_huge_depth_is_clamped_and_returns() {
    let original = redundant_select();
    let mut deep = only(true, false, false);
    deep.shape.depth = Some(u32::MAX);
    let mut clamped = only(true, false, false);
    clamped.shape.depth = Some(MAX_DEPTH);
    let outcome = search(&original, &[], &deep);
    assert_exact(&original, &outcome);
    assert_eq!(outcome, search(&original, &[], &clamped));
}

#[test]
fn a_work_budget_is_never_exceeded() {
    // Every budget either completes within itself or stops with that budget.
    let original = select_table();
    let complete = match search(&original, &[], &Settings::default()) {
        Outcome::Improved(candidate) => candidate.provenance.work_used,
        Outcome::Unimproved(provenance) => provenance.work_used,
        Outcome::Stopped(stop) => panic!("unexpected stop {stop:?}"),
    };
    for limit in (0..=complete).step_by(usize::try_from(complete / 97 + 1).unwrap_or(1)) {
        let mut settings = Settings::default();
        settings.budget.work_units = limit;
        match search(&original, &[], &settings) {
            Outcome::Stopped(stop) => assert_eq!(stop, Stop::WorkBudget { limit }),
            Outcome::Improved(candidate) => assert!(candidate.provenance.work_used <= limit),
            Outcome::Unimproved(provenance) => assert!(provenance.work_used <= limit),
        }
    }
}
