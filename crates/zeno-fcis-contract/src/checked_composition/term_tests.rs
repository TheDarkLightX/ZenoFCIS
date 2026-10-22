//! Each term constructor gives, on every input tried, the value the library
//! evaluator computes for the program it canonicalizes; the identities that
//! need a Boolean operand apply to one and to nothing else.

use std::collections::BTreeSet;

use zeno_fcis_synthesis::finite::{Domain, Op, Program};

use super::term::{self, Arena, Key, Node};
use super::tests::fail;

fn node(op: Op) -> Node {
    let at = usize::from;
    match op {
        Op::Input(a) => Node::Input(at(a)),
        Op::Int(value) => Node::Int(value),
        Op::Bool(value) => Node::Bool(value),
        Op::Add(a, b) => Node::Add(at(a), at(b)),
        Op::Sub(a, b) => Node::Sub(at(a), at(b)),
        Op::Eq(a, b) => Node::Eq(at(a), at(b)),
        Op::Lt(a, b) => Node::Lt(at(a), at(b)),
        Op::And(a, b) => Node::And(at(a), at(b)),
        Op::Not(a) => Node::Not(at(a)),
        Op::Select(c, a, b) => Node::Select(at(c), at(a), at(b)),
        other => fail(format!("unknown operator {other:?}")),
    }
}

/// Every tuple of values within `bounds`.
fn tuples(bounds: &[(i128, i128)]) -> Vec<Vec<i128>> {
    bounds
        .iter()
        .fold(vec![Vec::new()], |prefixes, (min, max)| {
            prefixes
                .into_iter()
                .flat_map(|prefix| {
                    (*min..=*max).map(move |value| {
                        let mut tuple = prefix.clone();
                        tuple.push(value);
                        tuple
                    })
                })
                .collect()
        })
}

/// Canonicalizes `ops` over command inputs `0..`, the Boolean ones marked,
/// and compares every output's term with the library evaluator's output on
/// every input. Returns the arena and each output's term.
fn agrees(inputs: &[Domain], outputs: &[Domain], ops: &[Op], roots: &[u16]) -> (Arena, Vec<u32>) {
    let program = Program::try_new_extended(
        inputs.to_vec(),
        outputs.to_vec(),
        ops.to_vec(),
        roots.to_vec(),
    )
    .unwrap_or_else(fail);
    let key = |index: usize| Key::Command(u16::try_from(index).unwrap_or_else(fail));
    let booleans: BTreeSet<Key> = inputs
        .iter()
        .enumerate()
        .filter(|(_, domain)| **domain == Domain::Bool)
        .map(|(index, _)| key(index))
        .collect();
    let mut arena = Arena::new(booleans);
    let nodes: Vec<Node> = ops.iter().cloned().map(node).collect();
    let ids = term::canonical(&mut arena, &nodes, |arena, index| {
        Some(arena.input(key(index)))
    })
    .unwrap_or_else(|| fail("canonical terms"));
    let outputs: Vec<u32> = roots.iter().map(|root| ids[usize::from(*root)]).collect();
    let bounds: Vec<(i128, i128)> = inputs.iter().map(|domain| domain.bounds()).collect();
    for input in tuples(&bounds) {
        let expected = program.evaluate(&input).unwrap_or_else(fail);
        let value = |at: Key| match at {
            Key::Command(index) => input.get(usize::from(index)).copied(),
            _ => None,
        };
        for (position, id) in outputs.iter().enumerate() {
            assert_eq!(
                arena.evaluate(*id, &value),
                Some(expected[position]),
                "output {position} at {input:?}"
            );
        }
    }
    (arena, outputs)
}

const SMALL: Domain = Domain::Int { min: -3, max: 3 };

#[test]
fn arithmetic_and_comparison_keep_the_evaluators_values() {
    // a, b; 2; a + b, a - b, 2 + 2, a - 2, (a + b) + 2, a < b, a < a, a == a,
    // 1 < 2, a == b.
    let ops = [
        Op::Input(0),
        Op::Input(1),
        Op::Int(2),
        Op::Add(0, 1),
        Op::Sub(0, 1),
        Op::Add(2, 2),
        Op::Sub(0, 2),
        Op::Add(3, 2),
        Op::Lt(0, 1),
        Op::Lt(0, 0),
        Op::Eq(0, 0),
        Op::Int(1),
        Op::Lt(11, 2),
        Op::Eq(0, 1),
    ];
    let wide = Domain::Int { min: -8, max: 8 };
    let (arena, outputs) = agrees(
        &[SMALL, SMALL],
        &[
            wide,
            wide,
            wide,
            wide,
            wide,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
        ],
        &ops,
        &[3, 4, 5, 6, 7, 8, 9, 10, 12, 13],
    );
    // Constants fold; `a < a` and `a == a` are constant.
    assert_eq!(arena.term(outputs[2]), term::Term::Int(4));
    assert_eq!(arena.term(outputs[6]), term::Term::Bool(false));
    assert_eq!(arena.term(outputs[7]), term::Term::Bool(true));
    assert_eq!(arena.term(outputs[8]), term::Term::Bool(true));
}

#[test]
fn boolean_identities_keep_the_evaluators_values() {
    // p, q Boolean, a, b integers; !p, !!p, true && p, p && true, p && p,
    // false && q, choose(p, a, b), choose(true, a, b), choose(p, a, a).
    let ops = [
        Op::Input(0),
        Op::Input(1),
        Op::Input(2),
        Op::Input(3),
        Op::Bool(true),
        Op::Bool(false),
        Op::Not(0),
        Op::Not(6),
        Op::And(4, 0),
        Op::And(0, 4),
        Op::And(0, 0),
        Op::And(5, 1),
        Op::Select(0, 2, 3),
        Op::Select(4, 2, 3),
        Op::Select(0, 2, 2),
        Op::And(0, 1),
    ];
    let (arena, outputs) = agrees(
        &[Domain::Bool, Domain::Bool, SMALL, SMALL],
        &[
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            Domain::Bool,
            SMALL,
            SMALL,
            SMALL,
            Domain::Bool,
        ],
        &ops,
        &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    );
    let p = term::Term::Input(Key::Command(0));
    // For the Boolean p: !!p, true && p, p && true and p && p are p.
    for output in [1, 2, 3, 4] {
        assert_eq!(arena.term(outputs[output]), p, "output {output}");
    }
    assert_eq!(arena.term(outputs[5]), term::Term::Bool(false));
    assert_eq!(arena.term(outputs[7]), term::Term::Input(Key::Command(2)));
    assert_eq!(arena.term(outputs[8]), term::Term::Input(Key::Command(2)));
}

/// The Rust side of the proof's `unguarded_conjunction` control: without
/// the Boolean guard, `true && x`, `x && x` and `!!x` would be `x`, which is
/// wrong for an integer `x`.
#[test]
fn the_boolean_identities_do_not_apply_to_an_integer() {
    let mut arena = Arena::new(BTreeSet::new());
    let x = arena.input(Key::Command(1));
    let truth = arena.constant(true);
    let and = arena.and(truth, x);
    let same = arena.and(x, x);
    let not = arena.not(x);
    let twice = arena.not(not);
    for id in [and, same, twice] {
        assert_ne!(id, x);
    }
    let at_two = |key: Key| (key == Key::Command(1)).then_some(2);
    assert_eq!(arena.evaluate(x, &at_two), Some(2));
    // The evaluator's conjunction is 1 exactly when both operands are 1,
    // and its negation is 1 exactly on 0.
    assert_eq!(arena.evaluate(and, &at_two), Some(0));
    assert_eq!(arena.evaluate(same, &at_two), Some(0));
    assert_eq!(arena.evaluate(twice, &at_two), Some(1));
    // The same constructors on a Boolean input apply the identities.
    let mut boolean = Arena::new(BTreeSet::from([Key::Command(1)]));
    let x = boolean.input(Key::Command(1));
    let truth = boolean.constant(true);
    let not = boolean.not(x);
    assert_eq!(boolean.and(truth, x), x);
    assert_eq!(boolean.and(x, x), x);
    assert_eq!(boolean.not(not), x);
}
