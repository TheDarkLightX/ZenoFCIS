//! Unit tests: the expression language, lowering, inverse semantics, and the
//! repair, sketch and family doors on exported core-component programs
//! (`tests/data`, exported by this source tree's `contract export-program`).
//!
//! The door cases translate the prototype's documented behaviour: the same
//! subjects, proposals and expected completions, regions and repairs.

#![allow(clippy::needless_range_loop)]

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op as LibOp, Program};
use zeno_fcis_synthesis::finite_runtime::import_program;

use crate::Refusal;
use crate::expr::{
    Allow, Env, Expr, Fill, Hole, HoleValue, Kind, Op, Vocabulary, bind_params, evaluate, fold,
    holes_of, parse, print_symbolic, show,
};
use crate::family::{self, Form, TransferStatus};
use crate::inverse::{
    self, MAX, MIN, Need, Synthesis, Synthesizer, derive, describe_region, positions,
    solve_constant, unsatisfiable,
};
use crate::lower::{from_program, lower_outputs};
use crate::repair::{self, SiteKind, Status as RepairStatus, first_difference};
use crate::sketch::{self, Status};
use crate::subject::{DEFAULT_MAX_TUPLES, Subject};

const COUNTER_C8: &str = "authorized ? action - (value == (action == Inc ? 8 : 0) ? 149 : 147) : 0";
const POOL: &str = "(?c < available + reserved) ? 0 : (authorized ? (((action < 151) ? (available < quantity) : \
((action == 153) ? (?c < available + reserved + quantity) : (reserved < quantity))) ? \
((action < 151) ? 2 : ((action == 153) ? 4 : 3)) : action - ?c) : 1)";

fn data(name: &str) -> &'static [u8] {
    match name {
        "bounded-counter-c1" => include_bytes!("../tests/data/bounded-counter-c1.original.zcve"),
        "bounded-counter-c2" => include_bytes!("../tests/data/bounded-counter-c2.original.zcve"),
        "bounded-counter-c3" => include_bytes!("../tests/data/bounded-counter-c3.original.zcve"),
        "bounded-counter-c4" => include_bytes!("../tests/data/bounded-counter-c4.original.zcve"),
        "bounded-counter-c5" => include_bytes!("../tests/data/bounded-counter-c5.original.zcve"),
        "bounded-counter-c6" => include_bytes!("../tests/data/bounded-counter-c6.original.zcve"),
        "bounded-counter-c7" => include_bytes!("../tests/data/bounded-counter-c7.original.zcve"),
        "bounded-counter-c8" => include_bytes!("../tests/data/bounded-counter-c8.original.zcve"),
        "reservation-pool-c2-q2" => {
            include_bytes!("../tests/data/reservation-pool-c2-q2.original.zcve")
        }
        "approval-queue-k2" => include_bytes!("../tests/data/approval-queue-k2.original.zcve"),
        "approval-queue-k1" => include_bytes!("../tests/data/approval-queue-k1.original.zcve"),
        "rate-limiter-n2-w2" => include_bytes!("../tests/data/rate-limiter-n2-w2.original.zcve"),
        "rate-limiter-n1-w2" => include_bytes!("../tests/data/rate-limiter-n1-w2.original.zcve"),
        "consumable-budget-c6" => {
            include_bytes!("../tests/data/consumable-budget-c6.original.zcve")
        }
        "reservation-pool-c2-q2.optimized" => {
            include_bytes!("../tests/data/reservation-pool-c2-q2.optimized.zcve")
        }
        _ => panic!("no test program {name}"),
    }
}

type Variants = Vec<(&'static str, &'static str, i64)>;

/// Field names, variants, case count and parameters of one family's
/// instance, as the shell writes them from `project.zeno` and the policy.
fn vocabulary_of(name: &str) -> (Vec<&'static str>, Variants, i64, Vec<(&'static str, i64)>) {
    let digits = |prefix: char| -> i64 {
        name.split('-')
            .find_map(|part| part.strip_prefix(prefix).and_then(|n| n.parse().ok()))
            .unwrap_or(0)
    };
    if name.starts_with("bounded-counter") {
        return (
            vec!["value", "action", "authorized"],
            vec![("action", "Inc", 150), ("action", "Dec", 151)],
            5,
            vec![("C", digits('c'))],
        );
    }
    if name.starts_with("reservation-pool") {
        return (
            vec!["available", "reserved", "action", "quantity", "authorized"],
            vec![
                ("action", "Reserve", 150),
                ("action", "Release", 151),
                ("action", "Consume", 152),
                ("action", "Replenish", 153),
            ],
            9,
            vec![("C", digits('c')), ("Q", digits('q'))],
        );
    }
    if name.starts_with("approval-queue") {
        return (
            vec![
                "phase",
                "vote0",
                "vote1",
                "vote2",
                "action",
                "principal",
                "authorized",
            ],
            vec![
                ("phase", "Empty", 150),
                ("phase", "Pending", 151),
                ("phase", "Executed", 152),
                ("action", "Enqueue", 160),
                ("action", "Vote", 161),
                ("action", "Execute", 162),
            ],
            8,
            vec![("K", digits('k'))],
        );
    }
    if name.starts_with("rate-limiter") {
        return (
            vec!["start", "used", "command", "now", "authorized"],
            vec![("command", "Acquire", 150)],
            5,
            vec![("N", digits('n')), ("W", digits('w'))],
        );
    }
    if name.starts_with("consumable-budget") {
        return (
            vec!["remaining", "amount", "authorized"],
            Vec::new(),
            3,
            vec![("C", digits('c'))],
        );
    }
    panic!("no vocabulary for {name}")
}

fn program(name: &str) -> Program {
    match import_program(data(name)) {
        Ok(program) => program,
        Err(error) => panic!("{name}: {error:?}"),
    }
}

fn subject(name: &str) -> Subject {
    let original = program(name);
    let (names, variants, cases, parameters) = vocabulary_of(name);
    let mut table: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for (field, variant, value) in variants {
        table
            .entry(field.to_string())
            .or_default()
            .insert(variant.to_string(), value);
    }
    let vocab = Vocabulary::new(
        names.into_iter().map(ToString::to_string).collect(),
        original.inputs().to_vec(),
        original.outputs().to_vec(),
        &table,
    );
    let vocab = match vocab {
        Ok(vocab) => vocab,
        Err(refusal) => panic!("{name}: {refusal}"),
    };
    let parameters = parameters
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    match Subject::new(original, vocab, parameters, Some(cases), DEFAULT_MAX_TUPLES) {
        Ok(subject) => subject,
        Err(refusal) => panic!("{name}: {refusal}"),
    }
}

fn ok<T>(result: Result<T, Refusal>) -> T {
    match result {
        Ok(value) => value,
        Err(refusal) => panic!("refused: {refusal}"),
    }
}

fn expr(text: &str) -> Expr {
    ok(parse(text, 0, Allow::ALL))
}

fn outputs(texts: &[&str]) -> Vec<Expr> {
    texts.iter().map(|text| expr(text)).collect()
}

fn resolved(s: &Subject, texts: &[&str]) -> Vec<Expr> {
    outputs(texts)
        .iter()
        .map(|node| ok(s.vocab.resolve_variants(node)))
        .collect()
}

// --- the expression language -------------------------------------------------

#[test]
fn printing_round_trips_and_holes_do_not_swallow_names() {
    for text in [
        COUNTER_C8,
        POOL,
        "x - -3",
        "!(a && b) || c -> d",
        "?c[0..8] + value",
        "?{value | 0} == ?e",
        "c ? (a ? b : d) : e ? f : g",
        "(action == 161) == (161 < action)",
        "a?count:0",
        "a -> b -> c",
        "(a -> b) -> c",
        "a - (b - c)",
        "a - b - c",
        "!!x",
        "!(c ? a : b)",
        "?c[-3..-1] < x",
        "$C + $K - 1",
        "0 - x",
        "a <= b && b >= c || a != c",
        "x * 2 + 1",
    ] {
        let ast = expr(text);
        assert_eq!(expr(&print_symbolic(&ast)), ast, "{text}");
    }
    assert_eq!(
        expr("a?count:0"),
        Expr::select(Expr::var("a"), Expr::var("count"), Expr::Int(0))
    );
}

#[test]
fn every_operator_has_its_precedence_and_associativity() {
    let b = |op, x, y| Expr::bin(op, x, y);
    let v = Expr::var;
    assert_eq!(
        expr("a + b * c"),
        b(Op::Add, v("a"), b(Op::Mul, v("b"), v("c")))
    );
    assert_eq!(
        expr("a + b < c"),
        b(Op::Lt, b(Op::Add, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a < b == c"),
        b(Op::Eq, b(Op::Lt, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a == b && c"),
        b(Op::And, b(Op::Eq, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a && b || c"),
        b(Op::Or, b(Op::And, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a || b -> c"),
        b(Op::Implies, b(Op::Or, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a -> b ? c : d"),
        Expr::select(b(Op::Implies, v("a"), v("b")), v("c"), v("d"))
    );
    assert_eq!(
        expr("a - b - c"),
        b(Op::Sub, b(Op::Sub, v("a"), v("b")), v("c"))
    );
    assert_eq!(
        expr("a -> b -> c"),
        b(Op::Implies, v("a"), b(Op::Implies, v("b"), v("c")))
    );
    assert_eq!(
        expr("a ? b : c ? d : e"),
        Expr::select(v("a"), v("b"), Expr::select(v("c"), v("d"), v("e")))
    );
    for (text, op) in [
        ("a != b", Op::Ne),
        ("a <= b", Op::Le),
        ("a > b", Op::Gt),
        ("a >= b", Op::Ge),
    ] {
        assert_eq!(expr(text), b(op, v("a"), v("b")));
    }
    assert_eq!(expr("!a && b"), b(Op::And, Expr::negation(v("a")), v("b")));
    assert_eq!(
        expr("-a + b"),
        b(Op::Add, b(Op::Sub, Expr::Int(0), v("a")), v("b"))
    );
    assert_eq!(expr("-3"), Expr::Int(-3));
    assert_eq!(expr("true"), Expr::Int(1));
    assert_eq!(expr("false"), Expr::Int(0));
}

#[test]
fn every_hole_form_parameters_and_keep() {
    assert_eq!(expr("?c"), Expr::Hole(0, Hole::Const(None)));
    assert_eq!(expr("?c[2..5]"), Expr::Hole(0, Hole::Const(Some((2, 5)))));
    assert_eq!(
        expr("?c[-2..-1]"),
        Expr::Hole(0, Hole::Const(Some((-2, -1))))
    );
    assert_eq!(expr("?e"), Expr::Hole(0, Hole::Expr));
    assert_eq!(
        expr("?{a | b + 1}"),
        Expr::Hole(
            0,
            Hole::Choice(vec![
                Expr::var("a"),
                Expr::bin(Op::Add, Expr::var("b"), Expr::Int(1))
            ])
        )
    );
    let numbered = ok(parse("?c + ?e < ?c[0..1]", 3, Allow::ALL));
    let ids: Vec<usize> = holes_of(&numbered)
        .iter()
        .map(|h| match h {
            Expr::Hole(k, _) => *k,
            _ => usize::MAX,
        })
        .collect();
    assert_eq!(ids, vec![3, 4, 5]);
    // `?count` is a conditional, not a hole followed by a name
    assert_eq!(
        expr("x ?count:0"),
        Expr::select(Expr::var("x"), Expr::var("count"), Expr::Int(0))
    );
    assert_eq!(expr("$C"), Expr::Param(String::from("C")));
    assert_eq!(expr("keep"), Expr::Keep);
    let bound = ok(bind_params(
        &expr("$C + 1 < value"),
        &[(String::from("C"), 8)].into_iter().collect(),
    ));
    assert_eq!(
        ok(fold(&bound)),
        Expr::bin(Op::Lt, Expr::Int(9), Expr::var("value"))
    );
    assert!(bind_params(&expr("$K"), &BTreeMap::new()).is_err());
}

#[test]
fn malformed_and_unsupported_text_is_refused() {
    let long = "x + ".repeat(3000) + "x";
    let many = (0..17).map(|_| "?c").collect::<Vec<_>>().join(" + ");
    let wide = (0..520).map(|_| "x").collect::<Vec<_>>().join(" + ");
    for (text, allow) in [
        ("keep + 1", Allow::ALL),
        ("keep", Allow::PLAIN),
        ("?c", Allow::PLAIN),
        ("$C", Allow::PLAIN),
        ("x \u{e9} 1", Allow::ALL),
        (long.as_str(), Allow::ALL),
        (many.as_str(), Allow::ALL),
        (wide.as_str(), Allow::ALL),
        ("f(x)", Allow::ALL),
        ("x + then", Allow::ALL),
        ("x +", Allow::ALL),
        ("x y", Allow::ALL),
        ("(x", Allow::ALL),
        ("x # y", Allow::ALL),
        ("?{a}", Allow::ALL),
        ("?{a | ?c}", Allow::ALL),
        ("?c[5..2]", Allow::ALL),
        ("?c[0..70000]", Allow::ALL),
        ("?c[a..2]", Allow::ALL),
        ("99999999999999999999", Allow::ALL),
        ("a.b.c", Allow::ALL),
        ("", Allow::ALL),
    ] {
        assert!(parse(text, 0, allow).is_err(), "{text}");
    }
}

#[test]
fn deep_nesting_is_refused_before_the_parser_descends() {
    // On a default-sized thread stack (2 MiB): without a bound before
    // descent, these overflow it, debug frames included.
    let worker = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let forms = [
                "(".repeat(2000) + "x" + &")".repeat(2000),
                "!".repeat(4000) + "x",
                "-".repeat(4000) + "x",
                ["x"; 1000].join(" -> "),
                "x ? ".repeat(1000) + "1" + &" : 0".repeat(1000),
                ["1"; 600].join(" + "),
            ];
            forms
                .iter()
                .map(|text| parse(text, 0, Allow::ALL).map_err(|refusal| refusal.0))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|error| panic!("spawn: {error}"));
    let results = worker
        .join()
        .unwrap_or_else(|_| panic!("the parser overflowed its stack"));
    for result in results {
        let reason = result.err().unwrap_or_default();
        assert!(reason.starts_with("expression-too-deep"), "{reason}");
    }
    // a shallow nested expression still parses
    assert!(parse("((x)) == 0 && !(!y)", 0, Allow::ALL).is_ok());
}

#[test]
fn unknown_names_and_clashing_variants_are_refused() {
    let s = subject("bounded-counter-c8");
    assert!(s.vocab.check_names(&expr("value + nosuch")).is_err());
    assert!(s.vocab.check_names(&expr("action == Inc")).is_ok());
    let domains = vec![Domain::Int { min: 0, max: 3 }, Domain::Bool];
    let clash: BTreeMap<String, BTreeMap<String, i64>> = [(
        String::from("x"),
        [(String::from("y"), 1)].into_iter().collect(),
    )]
    .into_iter()
    .collect();
    for (names, table) in [
        (vec!["x", "y"], clash),
        (vec!["x", "x"], BTreeMap::new()),
        (vec!["x", "out0"], BTreeMap::new()),
        (vec!["x", "then"], BTreeMap::new()),
        (vec!["x"], BTreeMap::new()),
    ] {
        let names = names.into_iter().map(ToString::to_string).collect();
        assert!(Vocabulary::new(names, domains.clone(), vec![Domain::Bool], &table).is_err());
    }
}

#[test]
fn vectorised_evaluation_matches_the_lowered_program() {
    let s = subject("reservation-pool-c2-q2");
    let text = POOL
        .replacen("?c < available + reserved)", "2 < available + reserved)", 1)
        .replace(
            "?c < available + reserved + quantity",
            "2 < available + reserved + quantity",
        )
        .replace("action - ?c", "action - 145");
    let node = ok(s.vocab.resolve_variants(&expr(&text)));
    let mut all = vec![node.clone()];
    all.extend((1..5).map(|_| Expr::Keep));
    let lowered = ok(lower_outputs(&all, &s.vocab, &s.original));
    assert_eq!(first_difference(&s, &lowered), None);
    assert_eq!(
        ok(evaluate(&node, &s.env(None), &BTreeMap::new())),
        s.targets[0]
    );
}

#[test]
fn lowering_keeps_the_exact_abi_and_round_trips_a_program() {
    for name in [
        "bounded-counter-c8",
        "approval-queue-k2",
        "rate-limiter-n2-w2",
    ] {
        let s = subject(name);
        let exprs = ok(from_program(&s.original, &s.vocab, 4096));
        let lowered = ok(lower_outputs(&exprs, &s.vocab, &s.original));
        assert_eq!(lowered.inputs(), s.original.inputs(), "{name}");
        assert_eq!(lowered.outputs(), s.original.outputs(), "{name}");
        assert!(lowered.nodes().len() <= s.original.nodes().len(), "{name}");
        assert_eq!(first_difference(&s, &lowered), None, "{name}");
        let kept = ok(lower_outputs(
            &vec![Expr::Keep; s.original.outputs().len()],
            &s.vocab,
            &s.original,
        ));
        assert_eq!(kept.nodes().len(), s.original.nodes().len(), "{name}");
        assert_eq!(first_difference(&s, &kept), None, "{name}");
    }
    let s = subject("bounded-counter-c8");
    assert!(lower_outputs(&outputs(&["value", "keep", "keep"]), &s.vocab, &s.original).is_ok());
    for texts in [
        vec!["value * 2", "keep", "keep"],
        vec!["?c", "keep", "keep"],
        vec!["$C", "keep", "keep"],
        vec!["nosuch", "keep", "keep"],
        vec!["value < authorized", "keep", "keep"],
        vec!["keep", "keep"],
        vec!["keep", "value + 1", "2 == 3"],
    ] {
        assert!(
            lower_outputs(&outputs(&texts), &s.vocab, &s.original).is_err(),
            "{texts:?}"
        );
    }
}

// --- inverse semantics -------------------------------------------------------

/// x in 0..=3 and a flag, one Int output `flag ? x + 1 : x`.
fn tiny() -> Subject {
    let inputs = vec![Domain::Int { min: 0, max: 3 }, Domain::Bool];
    let outputs = vec![Domain::Int { min: -10, max: 10 }];
    let nodes = vec![
        LibOp::Input(0),
        LibOp::Input(1),
        LibOp::Int(1),
        LibOp::Add(0, 2),
        LibOp::Select(1, 3, 0),
    ];
    let original = match Program::try_new(inputs.clone(), outputs.clone(), nodes, vec![4]) {
        Ok(program) => program,
        Err(error) => panic!("{error:?}"),
    };
    let vocab = ok(Vocabulary::new(
        vec![String::from("x"), String::from("flag")],
        inputs,
        outputs,
        &BTreeMap::new(),
    ));
    ok(Subject::new(original, vocab, BTreeMap::new(), None, 64))
}

fn derived(s: &Subject, root: &str, path: &[u8], kind: Kind, target: &[i64]) -> Need {
    let all = vec![true; s.size];
    ok(derive(
        &expr(root),
        path,
        &Need::exact(kind, target, &all),
        &s.env(None),
        &BTreeMap::new(),
    ))
}

#[test]
fn inverse_rules_for_each_operator() {
    let s = tiny();
    let n = s.size;
    let x = s.columns[0].clone();
    let flag = s.columns[1].clone();
    // `+`: shifted by the sibling
    let plus: Vec<i64> = x.iter().map(|v| v + 2).collect();
    let d = derived(&s, "x + ?c", &[1], Kind::Int, &plus);
    assert!(d.has_eq.iter().all(|h| *h) && d.eq.iter().all(|e| *e == 2));
    // `k - x`: the requirement on x is k - target
    let minus: Vec<i64> = x.iter().map(|v| 5 - v).collect();
    let wide: Vec<i128> = x.iter().map(|v| i128::from(*v)).collect();
    assert_eq!(derived(&s, "5 - x", &[1], Kind::Int, &minus).eq, wide);
    // a select's branch: kept where taken, dropped elsewhere
    let target: Vec<i64> = x
        .iter()
        .zip(&flag)
        .map(|(v, f)| if *f == 1 { v + 1 } else { *v })
        .collect();
    let d = derived(&s, "flag ? ?c : x", &[1], Kind::Int, &target);
    for t in 0..n {
        assert_eq!(d.has_eq[t], flag[t] == 1);
    }
    // a select's test: pick the branch that is right
    let d = derived(&s, "?c ? x + 1 : x", &[0], Kind::Int, &target);
    assert_eq!(d.kind, Kind::Bool);
    for t in 0..n {
        assert_eq!((d.has_eq[t], d.eq[t]), (true, i128::from(flag[t])));
    }
    // `==` on integers: equality where true, inequality where false
    let two: Vec<i64> = x.iter().map(|v| i64::from(*v == 2)).collect();
    assert_eq!(
        solve_constant(&derived(&s, "?c == x", &[0], Kind::Bool, &two), &[]),
        Some(2)
    );
    // order comparisons: per-tuple bounds
    let below: Vec<i64> = x.iter().map(|v| i64::from(*v < 2)).collect();
    for (root, path, value) in [
        ("x < ?c", 1u8, 2),
        ("?c > x", 0, 2),
        ("x <= ?c", 1, 1),
        ("?c >= x", 0, 1),
    ] {
        let need = derived(&s, root, &[path], Kind::Bool, &below);
        assert_eq!(solve_constant(&need, &[]), Some(value), "{root}");
    }
    // `&&`: free where the sibling is false, impossible where it forces a
    // wrong result
    let both: Vec<i64> = x
        .iter()
        .zip(&flag)
        .map(|(v, f)| i64::from(*v > 1 && *f == 1))
        .collect();
    let and = derived(&s, "flag && ?c", &[1], Kind::Bool, &both);
    for t in 0..n {
        assert_eq!(and.has_eq[t], flag[t] == 1);
    }
    let ones = vec![1; n];
    let wrong = derived(&s, "flag && ?c", &[1], Kind::Bool, &ones);
    assert!(
        unsatisfiable(&wrong)
            .iter()
            .zip(&flag)
            .all(|(bad, f)| *bad == (*f == 0))
    );
    // `||`: free where the sibling is true
    let or = derived(&s, "flag || ?c", &[1], Kind::Bool, &both);
    for t in 0..n {
        assert_eq!(or.has_eq[t], flag[t] == 0);
    }
    // `->`: the antecedent is required negated where the consequent is false
    let implies = derived(&s, "?c -> flag", &[0], Kind::Bool, &both);
    for t in 0..n {
        if flag[t] == 0 {
            assert_eq!(
                (implies.has_eq[t], implies.eq[t]),
                (true, i128::from(1 - both[t]))
            );
        }
    }
    // `!`: negated
    let not = derived(&s, "!?c", &[0], Kind::Bool, &both);
    for t in 0..n {
        assert_eq!(not.eq[t], i128::from(1 - both[t]));
    }
    // a constant's feasible set: an interval minus finitely many values
    let mut need = Need::exact(Kind::Int, &vec![0; n], &vec![false; n]);
    need.lo = vec![MIN; n];
    need.hi = vec![3; n];
    need.neq = vec![(vec![0; n], vec![true; n]), (vec![1; n], vec![true; n])];
    // nearest zero, avoiding 0 and 1: -1 comes before 2
    assert_eq!(solve_constant(&need, &[]), Some(-1));
    assert_eq!(solve_constant(&need, &[3]), Some(3));
    need.lo = vec![0; n];
    assert_eq!(solve_constant(&need, &[]), Some(2));
    need.hi = vec![1; n];
    assert_eq!(solve_constant(&need, &[]), None);
}

#[test]
fn inverse_semantics_proves_positions_unable() {
    let s = subject("bounded-counter-c8");
    let node = ok(s
        .vocab
        .resolve_variants(&expr(&COUNTER_C8.replace("? 8 :", "? 7 :"))));
    let need = Need::exact(Kind::Int, &s.targets[0], &vec![true; s.size]);
    let env = s.env(Some(0));
    let mut blocked: Vec<Vec<u8>> = Vec::new();
    for (path, _) in positions(&node) {
        if path.is_empty() {
            continue;
        }
        let derived = ok(derive(&node, &path, &need, &env, &BTreeMap::new()));
        if unsatisfiable(&derived).iter().any(|b| *b) {
            blocked.push(path);
        }
    }
    assert!(
        blocked.contains(&vec![0]),
        "the authorization test cannot fix it"
    );
    assert!(
        !blocked.contains(&vec![1, 1, 0, 1, 1]),
        "the constant 7 can"
    );
}

// --- the sketch door ---------------------------------------------------------

fn settings() -> sketch::Settings {
    sketch::Settings::default()
}

fn sketch_of(name: &str, texts: &[&str]) -> (Subject, sketch::SketchResult) {
    let s = subject(name);
    let r = sketch::solve(&s, &resolved(&s, texts), &settings());
    (s, r)
}

#[test]
fn several_holes_complete_to_the_known_constants() {
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &[
            "authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0",
            "keep",
            "keep",
        ],
    );
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    let holes: Vec<(usize, Fill)> = r.holes.clone().into_iter().collect();
    assert_eq!(
        holes,
        vec![
            (0, Fill::Const(8)),
            (1, Fill::Const(149)),
            (2, Fill::Const(147))
        ]
    );
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| first_difference(&s, p).is_none())
    );
    // Meanings preserves the frozen joint order: hole 0 is the pivot,
    // both branch constants are searched, then the pivot is derived.
    assert_eq!(r.outputs[0].search, "meanings");
    assert_eq!(r.outputs[0].pivot, Some(0));
    assert_eq!(
        r.outputs[0]
            .searched
            .iter()
            .map(|space| space.hole)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(r.outputs[0].solved_alone >= 1);
    // the joint search finds the same completion with hole 0 as the pivot
    let joint = sketch::Settings {
        search: sketch::Search::Joint,
        ..settings()
    };
    let texts = [
        "authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0",
        "keep",
        "keep",
    ];
    let j = sketch::solve(&s, &resolved(&s, &texts), &joint);
    assert_eq!(j.status, Status::Completed);
    assert_eq!(j.holes, r.holes);
    assert_eq!(j.outputs[0].pivot, Some(0));
}

#[test]
fn comparison_holes_get_complete_ranges_so_failure_is_a_proof() {
    let (_, r) = sketch_of(
        "bounded-counter-c8",
        &[
            "authorized ? action - (value == ?c ? 149 : 147) : 0",
            "keep",
            "keep",
        ],
    );
    assert_eq!(r.status, Status::Unrealizable);
    // no single tuple rules every value out here (the required values
    // conflict across tuples), so the claim rests on the exact derivation
    // over all integers, with nothing enumerated
    assert!(r.outputs[0].witness.is_none());
    assert!(r.outputs[0].searched.is_empty());
    let s = subject("bounded-counter-c8");
    let texts = [
        "authorized ? action - (value == ?c ? ?c : 147) : 0",
        "keep",
        "keep",
    ];
    let joint = sketch::Settings {
        search: sketch::Search::Joint,
        ..settings()
    };
    let r = sketch::solve(&s, &resolved(&s, &texts), &joint);
    assert_eq!(r.status, Status::Unrealizable);
    let search = &r.outputs[0];
    assert!(search.searched.iter().all(|x| x.complete), "{search:?}");
    assert_eq!(search.tried as u128, search.assignments);
    // over meanings: the compared hole has one class per value of
    // `value == ?c` (-1 and 9 make the same program), each class failed
    // exactly, so the same complete space is exhausted with fewer tries
    let r = sketch::solve(&s, &resolved(&s, &texts), &settings());
    assert_eq!(r.status, Status::Unrealizable);
    let search = &r.outputs[0];
    assert_eq!(search.search, "meanings");
    assert!(search.searched.iter().all(|x| x.complete), "{search:?}");
    assert_eq!(search.searched[0].values.len(), 10, "{search:?}");
    assert!(search.witness.is_none());
}

#[test]
fn single_hole_unrealizable_carries_a_witness() {
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &["authorized ? action - 149 : ?c", "keep", "keep"],
    );
    assert_eq!(r.status, Status::Unrealizable);
    let Some(witness) = r.outputs[0].witness else {
        panic!("a witness");
    };
    // there the hole's branch is not taken and the output is wrong
    assert_eq!(s.point(witness.ordinal)[2], 1);
    let text = COUNTER_C8.replace("? 149 :", "? ?c :");
    let (_, r) = sketch_of("bounded-counter-c8", &[&text, "keep", "keep"]);
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.holes.get(&0), Some(&Fill::Const(149)));
}

#[test]
fn a_choice_hole_is_decided_by_trying_each_alternative() {
    let text = COUNTER_C8.replace("value ==", "?{value + 1 | value} ==");
    let (_, r) = sketch_of("bounded-counter-c8", &[&text, "keep", "keep"]);
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.holes.get(&0), Some(&Fill::Choice(1)));
    let text = COUNTER_C8.replace("value ==", "?{value + 1 | value - 1} ==");
    let (_, r) = sketch_of("bounded-counter-c8", &[&text, "keep", "keep"]);
    assert_eq!(r.status, Status::Unrealizable);
}

#[test]
fn an_expression_hole_is_synthesised_and_compared() {
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &["authorized ? ?e : 0", "keep", "keep"],
    );
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| first_difference(&s, p).is_none())
    );
}

#[test]
fn reservation_pool_sketch_beats_the_optimizer() {
    let mut texts = vec![POOL];
    texts.extend(["keep"; 4]);
    let (_, r) = sketch_of("reservation-pool-c2-q2", &texts);
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    let holes: Vec<(usize, Fill)> = r.holes.clone().into_iter().collect();
    assert_eq!(
        holes,
        vec![
            (0, Fill::Const(2)),
            (1, Fill::Const(2)),
            (2, Fill::Const(145))
        ]
    );
    let optimized = program("reservation-pool-c2-q2.optimized");
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| p.nodes().len() < optimized.nodes().len())
    );
}

#[test]
fn an_exhausted_enumeration_budget_is_inconclusive() {
    let s = subject("bounded-counter-c8");
    let tight = sketch::Settings {
        max_assignments: 3,
        ..settings()
    };
    let texts = [
        "authorized ? action - (value == (action == Inc ? ?c : 0) ? ?c : ?c) : 0",
        "keep",
        "keep",
    ];
    let r = sketch::solve(&s, &resolved(&s, &texts), &tight);
    assert_eq!(r.status, Status::Inconclusive);
    assert!(r.program.is_none());
    // so is an exhausted evaluation-work budget
    let starved = sketch::Settings {
        max_work: 100,
        ..settings()
    };
    let r = sketch::solve(&s, &resolved(&s, &texts), &starved);
    assert_eq!(r.status, Status::Inconclusive);
    assert!(r.outputs[0].tried < 20);
    // a domain over the door's tuple budget is refused before any work
    let original = program("reservation-pool-c2-q2");
    let pool = subject("reservation-pool-c2-q2");
    assert!(Subject::new(original, pool.vocab, BTreeMap::new(), None, 10).is_err());
}

#[test]
fn malformed_sketches_are_refused() {
    let s = subject("bounded-counter-c8");
    for texts in [
        vec!["?e + ?e", "keep", "keep"],
        vec!["$C", "keep", "keep"],
        vec!["nosuch ? 1 : 0", "keep", "keep"],
        vec!["?c", "keep"],
    ] {
        let r = sketch::solve(&s, &outputs(&texts), &settings());
        assert_eq!(r.status, Status::Refused, "{texts:?}");
    }
}

// --- the repair door ---------------------------------------------------------

fn explain(s: &Subject, texts: &[&str]) -> repair::Explanation {
    let nodes = resolved(s, texts);
    let lowered = ok(lower_outputs(&nodes, &s.vocab, &s.original));
    ok(repair::explain(
        s,
        &nodes,
        &lowered,
        true,
        &repair::Settings::default(),
    ))
}

#[test]
fn localisation_finds_the_one_wrong_constant() {
    let s = subject("bounded-counter-c8");
    let wrong = COUNTER_C8.replace("? 8 :", "? 7 :");
    let e = explain(&s, &[&wrong, "value + 1", "value - 1"]);
    let top = &e.localised[0].1.sites[0];
    assert_eq!(top.subexpression, Expr::Int(7));
    assert_eq!(top.replacement, Some(Expr::Int(8)));
    assert_eq!(e.region.tuples, 2);
    assert!(e.repair.is_some_and(|r| r.status == RepairStatus::Repaired));
}

#[test]
fn exact_failing_region_for_a_wrong_quorum() {
    let s = subject("approval-queue-k2");
    let any = "(vote0 ? vote0 : (vote1 ? vote1 : vote2))";
    let quorum = "(vote0 ? (vote1 ? vote1 : vote2) : (vote1 && vote2))";
    let (p152, p151) = ("(151 < phase)", "(phase == 151)");
    let p150 = alloc::format!("({p152} == {p151})");
    let (a161, a162) = ("(action == 161)", "(161 < action)");
    let okplace = alloc::format!("(({a161} == {a162}) ? {p150} : {p151})");
    let voted = "(principal == 2 ? vote2 : (principal == 0 ? vote0 : vote1))";
    let index = alloc::format!(
        "({any} ? {p150} : {p152}) ? 0 : (authorized ? ({okplace} ? (({a161} && {voted}) ? 3 : \
         ({quorum} ? (action - 155) : ({a162} ? 4 : (action - 155)))) : 2) : 1)"
    );
    let e = explain(
        &s,
        &[
            &index,
            "vote0 ? vote0 : principal == 0",
            "vote1 ? vote1 : principal == 1",
            "vote2 ? vote2 : principal == 2",
        ],
    );
    assert_eq!(e.region.tuples, 54);
    let Some(formula) = &e.region.formula else {
        panic!("a region formula");
    };
    assert!(show(formula, &s.vocab, Some(Kind::Bool)).contains("phase == Executed"));
    assert!(e.repair.is_some_and(|r| r.status == RepairStatus::Repaired));
}

#[test]
fn one_step_edits_fix_swapped_branches_and_flipped_conditions() {
    let s = subject("rate-limiter-n2-w2");
    let good = "authorized ? (now < start ? 1 : (now < start + 2 ? (used == 2 ? 2 : 4) : 3)) : 0";
    let good = resolved(&s, &[good, "used + 1"]);
    for bad in [
        "authorized ? (now < start ? 1 : (now < start + 2 ? (used == 2 ? 4 : 2) : 3)) : 0",
        "authorized ? (start < now ? 1 : (now < start + 2 ? (used == 2 ? 2 : 4) : 3)) : 0",
    ] {
        let e = explain(&s, &[bad, "used + 1"]);
        let Some(repaired) = e.repair else {
            panic!("a repair for {bad}");
        };
        assert_eq!(repaired.status, RepairStatus::Repaired, "{bad}");
        assert!(
            repaired
                .program
                .as_ref()
                .is_some_and(|p| first_difference(&s, p).is_none())
        );
        // P1 prefers the smaller source edit even when lowering a negated
        // equality needs an extra IR node. Keep the one-edit guarantee.
        let bad_nodes = resolved(&s, &[bad, "used + 1"]);
        assert!(
            inverse::positions(&bad_nodes[0])
                .into_iter()
                .any(|(path, sub)| {
                    repair::local_edits(sub).into_iter().any(|edit| {
                        inverse::replace_at(&bad_nodes[0], &path, &edit)
                            .is_ok_and(|expression| expression == repaired.expressions[0])
                    })
                }),
            "{bad}: {repaired:?}"
        );
        assert!(
            repaired.expressions[1] == good[1],
            "the unrelated output must stay intact"
        );
    }
}

#[test]
fn a_fault_at_the_root_of_an_output_gets_its_one_step_edit() {
    let s = subject("consumable-budget-c6");
    let e = explain(
        &s,
        &[
            "authorized ? (remaining < amount ? 1 : 2) : 0",
            "remaining + amount",
        ],
    );
    assert!(
        e.repair
            .as_ref()
            .is_some_and(|r| r.status == RepairStatus::Repaired)
    );
    let top = &e.localised[0].1.sites[0];
    assert_eq!(top.path, Vec::<u8>::new());
    assert_eq!(top.found_by, Some(SiteKind::Edit));
    assert_eq!(top.replacement, Some(expr("remaining - amount")));
}

#[test]
fn greedy_repair_fixes_faults_in_two_branches() {
    let s = subject("bounded-counter-c8");
    let two = COUNTER_C8
        .replace("? 8 :", "? 7 :")
        .replace(": 147) : 0", ": 147) : 1");
    let e = explain(&s, &[&two, "value + 1", "value - 1"]);
    let Some(repaired) = e.repair else {
        panic!("a repair");
    };
    assert_eq!(repaired.status, RepairStatus::Repaired);
    assert_eq!(repaired.steps.len(), 2);
    let mut replacements: Vec<Expr> = repaired
        .steps
        .iter()
        .map(|s| s.replacement.clone())
        .collect();
    replacements.sort();
    assert_eq!(replacements, vec![Expr::Int(0), Expr::Int(8)]);
}

#[test]
fn a_region_formula_is_exact_on_the_mask() {
    let s = subject("bounded-counter-c8");
    let synthesizer = Synthesizer::new(&s, Synthesis::default());
    let mask: Vec<bool> = (0..s.size)
        .map(|t| s.columns[0][t] >= 7 && s.columns[2][t] == 1)
        .collect();
    let Some(formula) = ok(describe_region(&mask, &synthesizer, 3)) else {
        panic!("a formula");
    };
    let values = ok(evaluate(&formula, &s.env(None), &BTreeMap::new()));
    assert!(values.iter().zip(&mask).all(|(v, m)| (*v == 1) == *m));
}

// --- the family door ---------------------------------------------------------

fn counters() -> Vec<Subject> {
    (1..=8)
        .map(|c| subject(&alloc::format!("bounded-counter-c{c}")))
        .collect()
}

#[test]
fn one_parametric_program_for_every_bounded_counter() {
    let text = COUNTER_C8.replace("? 8 :", "? $C :");
    let family_program = outputs(&[&text, "value + 1", "value - 1"]);
    for s in counters() {
        let prepared = family::prepare(&family_program, &s, &settings());
        let Some(candidate) = prepared.program else {
            panic!("{:?}: {:?}", s.parameters, prepared.refusal);
        };
        assert_eq!(first_difference(&s, &candidate), None, "{:?}", s.parameters);
        assert!(candidate.nodes().len() < s.original.nodes().len());
    }
}

#[test]
fn hole_values_are_fitted_to_the_parameter_and_lifted() {
    let text = COUNTER_C8.replace("? 8 :", "? ?c :");
    let sketch_program = outputs(&[&text, "value + 1", "value - 1"]);
    let all = counters();
    let mut samples = Vec::new();
    for s in all
        .iter()
        .filter(|s| matches!(s.parameters.get("C"), Some(2 | 5 | 8)))
    {
        let prepared = family::prepare(&sketch_program, s, &settings());
        let Some(result) = prepared.sketch else {
            panic!("a sketch");
        };
        assert_eq!(result.status, Status::Completed);
        samples.push((s.parameters.clone(), result.holes));
    }
    let forms = family::fit(&samples);
    let Some(Some(form)) = forms.get(&0) else {
        panic!("a form for hole 0: {forms:?}");
    };
    assert_eq!(form.text(), "$C");
    assert_eq!(
        *form,
        Form::Linear {
            parameter: String::from("C"),
            a: 1,
            b: 0
        }
    );
    // the lifted program holds on every instance, including the five it was
    // not fitted on
    let lifted: Vec<Expr> = sketch_program
        .iter()
        .map(|n| ok(family::apply_forms(n, &forms)))
        .collect();
    for s in &all {
        let prepared = family::prepare(&lifted, s, &settings());
        assert!(prepared.sketch.is_none(), "no holes remain");
        assert!(
            prepared
                .program
                .is_some_and(|p| first_difference(s, &p).is_none())
        );
    }
    let p = |c: i64| -> BTreeMap<String, i64> { [(String::from("C"), c)].into_iter().collect() };
    let h = |v: i64| -> BTreeMap<usize, Fill> { [(0, Fill::Const(v))].into_iter().collect() };
    let text_of =
        |forms: BTreeMap<usize, Option<Form>>| forms.get(&0).cloned().flatten().map(|f| f.text());
    assert_eq!(
        family::fit(&[(p(1), h(4)), (p(3), h(4))]).get(&0),
        Some(&Some(Form::Constant(4)))
    );
    assert_eq!(
        text_of(family::fit(&[(p(1), h(9)), (p(3), h(7))])),
        Some(String::from("0 - $C + 10"))
    );
    assert_eq!(
        text_of(family::fit(&[(p(1), h(1)), (p(3), h(5))])),
        Some(String::from("$C + $C - 1"))
    );
    assert_eq!(
        family::fit(&[(p(1), h(1)), (p(2), h(5)), (p(3), h(6))]).get(&0),
        Some(&None)
    );
}

#[test]
fn transfer_across_parameters_by_sketch() {
    let source = subject("bounded-counter-c3");
    let target = subject("bounded-counter-c7");
    let text = COUNTER_C8.replace("? 8 :", "? 3 :");
    let moved = outputs(&[&text, "value + 1", "value - 1"]);
    let t = family::transfer(
        &moved,
        &source.parameters,
        &target,
        &settings(),
        &repair::Settings::default(),
    );
    assert_eq!(t.status, TransferStatus::Sketch);
    assert!(matches!(t.direct, family::Direct::Differs(_)));
    assert!(t.choices.iter().any(|(_, from, to)| (*from, *to) == (3, 7)));
    assert!(
        t.program
            .is_some_and(|p| first_difference(&target, &p).is_none())
    );
    let same = family::transfer(
        &moved,
        &source.parameters,
        &source,
        &settings(),
        &repair::Settings::default(),
    );
    assert_eq!(same.status, TransferStatus::Direct);
}

// --- determinism and budgets -------------------------------------------------

#[test]
fn every_door_is_deterministic() {
    let s = subject("approval-queue-k2");
    let texts = ["phase == Executed ? 0 : ?e", "keep", "keep", "keep"];
    let a = sketch::solve(&s, &resolved(&s, &texts), &settings());
    let b = sketch::solve(&s, &resolved(&s, &texts), &settings());
    assert_eq!(
        alloc::format!("{:?}", (&a.status, &a.holes, &a.expressions, &a.outputs)),
        alloc::format!("{:?}", (&b.status, &b.holes, &b.expressions, &b.outputs))
    );
    let s = subject("bounded-counter-c8");
    let wrong = COUNTER_C8
        .replace("? 8 :", "? 7 :")
        .replace(": 147) : 0", ": 147) : 1");
    let texts = [wrong.as_str(), "value + 1", "value - 1"];
    let render = |e: repair::Explanation| {
        alloc::format!(
            "{:?}",
            (
                e.region,
                e.localised,
                e.repair.map(|r| (r.status, r.expressions, r.steps))
            )
        )
    };
    assert_eq!(render(explain(&s, &texts)), render(explain(&s, &texts)));
}

#[test]
fn synthesis_over_its_budget_finds_nothing_and_claims_nothing() {
    let s = subject("bounded-counter-c8");
    let starved = Synthesis {
        budget: zeno_fcis_program_search::target::TargetBudget {
            work_units: 10,
            ..Synthesis::default().budget
        },
        ..Synthesis::default()
    };
    let synthesizer = Synthesizer::new(&s, starved);
    let mask: Vec<bool> = (0..s.size).map(|t| s.columns[0][t] >= 7).collect();
    assert_eq!(ok(describe_region(&mask, &synthesizer, 3)), None);
    let tight = sketch::Settings {
        synthesis: starved,
        ..settings()
    };
    let r = sketch::solve(
        &s,
        &resolved(&s, &["authorized ? ?e : 0", "keep", "keep"]),
        &tight,
    );
    assert_eq!(r.status, Status::NotFound);
}

#[test]
fn evaluation_follows_the_lowered_semantics() {
    let s = tiny();
    let env: Env<'_> = s.env(None);
    let none: BTreeMap<usize, HoleValue> = BTreeMap::new();
    let x = &s.columns[0];
    let flag = &s.columns[1];
    let eval = |text: &str| ok(evaluate(&expr(text), &env, &none));
    let or: Vec<i64> = x
        .iter()
        .zip(flag)
        .map(|(v, f)| if *f == 1 { 1 } else { i64::from(*v == 1) })
        .collect();
    assert_eq!(eval("flag || x == 1"), or);
    let implies: Vec<i64> = x
        .iter()
        .zip(flag)
        .map(|(v, f)| if *f == 1 { i64::from(*v == 1) } else { 1 })
        .collect();
    assert_eq!(eval("flag -> x == 1"), implies);
    let not: Vec<i64> = x.iter().map(|v| i64::from(*v == 0)).collect();
    assert_eq!(eval("!x"), not);
    assert!(evaluate(&expr("x * 2"), &env, &none).is_err());
    assert!(evaluate(&expr("?c"), &env, &none).is_err());
    let holes: BTreeMap<usize, HoleValue> = [(0, HoleValue::Choice(1))].into_iter().collect();
    let plus: Vec<i64> = x.iter().map(|v| v + 1).collect();
    assert_eq!(ok(evaluate(&expr("?{x | x + 1}"), &env, &holes)), plus);
}

#[test]
fn one_shared_synthesis_budget_bounds_every_solve_of_a_door_run() {
    let s = subject("bounded-counter-c8");
    let small = Synthesis {
        total_work: 5_000,
        ..Synthesis::default()
    };
    let synthesizer = Synthesizer::new(&s, small);
    let mask: Vec<bool> = (0..s.size).map(|t| s.columns[0][t] >= 7).collect();
    // the first solve may use what is left; afterwards too little remains to start another
    let _ = ok(describe_region(&mask, &synthesizer, 3));
    assert!(synthesizer.remaining() < 5_000);
    let left = synthesizer.remaining();
    assert_eq!(ok(describe_region(&mask, &synthesizer, 3)), None);
    assert_eq!(
        synthesizer.remaining(),
        left,
        "a solve that cannot start uses nothing"
    );
}

// --- signed 64-bit arithmetic (the source's post-review gate fixes) ----------

#[test]
fn evaluation_refuses_eager_overflow() {
    let s = tiny();
    let env: Env<'_> = s.env(None);
    let none: BTreeMap<usize, HoleValue> = BTreeMap::new();
    for text in [
        "9223372036854775807 + 1 < 0",
        "-9223372036854775807 - 2 > 0",
        "flag && !flag ? 9223372036854775807 + 1 : 0",
        "x + 9223372036854775807",
    ] {
        assert_eq!(
            evaluate(&expr(text), &env, &none),
            Err(Refusal::new(crate::expr::OVERFLOW)),
            "{text}"
        );
    }
}

#[test]
fn inverse_comparisons_at_the_64_bit_ends_do_not_wrap() {
    let s = subject("bounded-counter-c8");
    let ones = vec![1; s.size];
    for (text, impossible) in [
        ("?c < -9223372036854775808", true),
        ("?c > 9223372036854775807", true),
        ("?c <= 9223372036854775807", false),
        ("?c >= -9223372036854775808", false),
    ] {
        let need = derived(&s, text, &[0], Kind::Bool, &ones);
        assert_eq!(
            unsatisfiable(&need).iter().all(|bad| *bad),
            impossible,
            "{text}"
        );
        assert!(need.lo.iter().all(|lo| *lo >= MIN) && need.hi.iter().all(|hi| *hi <= MAX));
    }
}

#[test]
fn a_large_constant_sketch_completes_exactly() {
    for (text, constant) in [
        (
            "value + ?c - 5000000000000000000",
            5_000_000_000_000_000_001,
        ),
        (
            "value + ?c + 5000000000000000000",
            -4_999_999_999_999_999_999,
        ),
    ] {
        let (s, r) = sketch_of("bounded-counter-c8", &["keep", text, "keep"]);
        assert_eq!(r.status, Status::Completed, "{text}: {:?}", r.reason);
        assert_eq!(r.holes.get(&0), Some(&Fill::Const(constant)), "{text}");
        assert!(
            r.program
                .as_ref()
                .is_some_and(|p| first_difference(&s, p).is_none())
        );
    }
}

#[test]
fn an_overflowing_choice_does_not_hide_a_valid_completion() {
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &["keep", "value + ?{9223372036854775807 + 1 | 1}", "keep"],
    );
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    assert_eq!(r.holes.get(&0), Some(&Fill::Choice(1)));
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| first_difference(&s, p).is_none())
    );
}

// --- the search over meanings --------------------------------------------------

#[test]
fn holes_alone_on_their_route_are_solved_by_themselves() {
    // the two branch holes serve disjoint tuples: neither is enumerated
    let text = "authorized ? (action == Inc ? value + ?c : value - ?c) : value + ?c";
    let (s, r) = sketch_of("bounded-counter-c8", &["keep", text, "keep"]);
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    let search = &r.outputs[1];
    assert_eq!(search.search, "meanings");
    assert_eq!(search.solved_alone, 3);
    assert!(search.searched.is_empty());
    assert_eq!(search.tried, 1);
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| first_difference(&s, p).is_none())
    );
}

#[test]
fn a_witness_from_a_hole_alone_proves_the_whole_output_unrealizable() {
    // both holes sit alone; on an unauthorized tuple the output is 5,
    // whatever they hold, and should be 0
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &["authorized ? (action == Inc ? ?c : ?c) : 5", "keep", "keep"],
    );
    assert_eq!(r.status, Status::Unrealizable);
    let Some(witness) = r.outputs[0].witness else {
        panic!("a witness: {:?}", r.outputs[0]);
    };
    assert_eq!(s.point(witness.ordinal)[2], 0);
}

#[test]
fn meanings_and_joint_search_agree_and_meanings_try_fewer() {
    let s = subject("reservation-pool-c2-q2");
    let mut texts = vec![POOL];
    texts.extend(["keep"; 4]);
    let joint = sketch::Settings {
        search: sketch::Search::Joint,
        ..settings()
    };
    let m = sketch::solve(&s, &resolved(&s, &texts), &settings());
    let j = sketch::solve(&s, &resolved(&s, &texts), &joint);
    assert_eq!(
        (m.status.clone(), j.status.clone()),
        (Status::Completed, Status::Completed)
    );
    for r in [&m, &j] {
        assert!(
            r.program
                .as_ref()
                .is_some_and(|p| first_difference(&s, p).is_none())
        );
    }
    assert_eq!(m.outputs[0].search, "meanings");
    assert_eq!(j.outputs[0].search, "joint");
}

// BEGIN version-space frozen-order regression
#[test]
fn the_frozen_joint_order_branches_before_deriving_the_pivot() {
    // Hole 1 is widest and becomes the frozen pivot. Hole 0's values -1
    // and 1 have the same effect; retain -1, the first representative,
    // before deriving the pivot. Preserve this order as well as completion.
    let s = subject("bounded-counter-c8");
    assert!(s.original_constants().contains(&1));
    let (s, r) = sketch_of(
        "bounded-counter-c8",
        &["keep", "keep", "(?c == 0) ? 5 : value - ?c"],
    );
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
    assert_eq!(r.holes.get(&0), Some(&Fill::Const(-1)));
    assert_eq!(r.holes.get(&1), Some(&Fill::Const(1)));
    assert_eq!(r.outputs[2].pivot, Some(1));
    assert_eq!(r.outputs[2].searched.len(), 1);
    assert_eq!(r.outputs[2].searched[0].hole, 0);
    assert_eq!(r.outputs[2].searched[0].values, vec![0, -1]);
    assert!(
        r.program
            .as_ref()
            .is_some_and(|p| first_difference(&s, p).is_none())
    );
}
// END version-space frozen-order regression

#[test]
fn a_search_over_meanings_stopped_by_its_budget_is_inconclusive_never_unrealizable() {
    // A completion exists (?c = 8; the last two holes may be anything), but
    // a tight work budget stops the class lists short: that is no exhausted
    // space, so nothing may be called unrealizable.
    let s = subject("bounded-counter-c8");
    let texts = [
        "authorized ? action - (value == (action == Inc ? ?c : 0) ? 149 : 147) : (((?c < value) == (?c < value)) ? 0 : 0)",
        "keep",
        "keep",
    ];
    for max_work in [1, 100, 400] {
        let tight = sketch::Settings {
            max_work,
            ..settings()
        };
        let r = sketch::solve(&s, &resolved(&s, &texts), &tight);
        assert_ne!(
            r.status,
            Status::Unrealizable,
            "max_work {max_work}: {:?}",
            r.outputs
        );
        assert!(r.program.is_none() || r.status == Status::Completed);
    }
    let r = sketch::solve(&s, &resolved(&s, &texts), &settings());
    assert_eq!(r.status, Status::Completed, "{:?}", r.reason);
}

#[test]
fn repairs_prefer_constant_edits_and_greedy_steps_to_rewrites() {
    let s = subject("bounded-counter-c3");
    let one = "!authorized ? 0 : action == Inc && value == 2 ? 1 : action == Dec && value == 0 ? 2 : action == Inc ? 3 : 4";
    let e = explain(&s, &[one, "value + 1", "value - 1"]);
    let repaired = e.repair.as_ref().unwrap_or_else(|| panic!("repair"));
    assert_eq!(repaired.status, RepairStatus::Repaired);
    assert!(show(&repaired.expressions[0], &s.vocab, None).contains("value == 3"));
    assert_eq!(e.localised[0].1.sites[0].found_by, Some(SiteKind::Constant));
    assert_eq!(
        first_difference(
            &s,
            repaired
                .program
                .as_ref()
                .unwrap_or_else(|| panic!("program"))
        ),
        None
    );
    let two = one.replace("value == 0", "value == 1");
    let e = explain(&s, &[&two, "value + 2", "value - 1"]);
    let repaired = e.repair.as_ref().unwrap_or_else(|| panic!("greedy repair"));
    assert_eq!(repaired.status, RepairStatus::Repaired);
    assert_eq!(repaired.steps.len(), 2);
    let mut replacements: Vec<Expr> = repaired
        .steps
        .iter()
        .map(|step| step.replacement.clone())
        .collect();
    replacements.sort();
    assert_eq!(replacements, vec![Expr::Int(0), Expr::Int(3)]);
    assert_eq!(
        first_difference(
            &s,
            repaired
                .program
                .as_ref()
                .unwrap_or_else(|| panic!("program"))
        ),
        None
    );
}

#[test]
fn affine_comparison_holes_find_the_shifted_complete_range() {
    for threshold in ["start + (?c - 4998)", "(?c - 4998) + start"] {
        let text = format!(
            "!authorized ? 0 : now < start ? 1 : now < {threshold} && used == 1 ? 2 : !(now < {threshold}) ? 3 : 4"
        );
        let (s, r) = sketch_of("rate-limiter-n1-w2", &[&text, "keep"]);
        assert_eq!(r.status, Status::Completed, "{r:?}");
        assert_eq!(r.holes.get(&0), Some(&Fill::Const(5000)));
        assert_eq!(r.holes.get(&1), Some(&Fill::Const(5000)));
        assert_eq!(
            first_difference(
                &s,
                r.program.as_ref().unwrap_or_else(|| panic!("completion"))
            ),
            None
        );
        assert!(r.outputs[0].searched.iter().all(|space| space.complete));
    }
}

#[test]
fn boolean_sums_lower_like_explicit_integer_selects() {
    let s = subject("approval-queue-k1");
    let bad = "vote0 + vote1 + vote2";
    let explicit = "(vote0 ? 1 : 0) + (vote1 ? 1 : 0) + (vote2 ? 1 : 0)";
    let a = resolved(&s, &[bad, "keep", "keep", "keep"]);
    let b = resolved(&s, &[explicit, "keep", "keep", "keep"]);
    let pa = ok(lower_outputs(&a, &s.vocab, &s.original));
    let pb = ok(lower_outputs(&b, &s.vocab, &s.original));
    assert_eq!(ok(repair::observe(&s, &pa)), ok(repair::observe(&s, &pb)));
    for ordinal in 0..s.size {
        let row = s.point(ordinal);
        assert_eq!(
            pa.evaluate(&row)
                .unwrap_or_else(|error| panic!("bool sum: {error:?}"))[0],
            row[1] + row[2] + row[3]
        );
    }
}

#[test]
fn disjoint_output_faults_keep_separate_exact_regions() {
    let s = subject("bounded-counter-c3");
    let e = explain(
        &s,
        &[
            "keep",
            "value == 0 ? value + 2 : value + 1",
            "value == 3 ? value - 2 : value - 1",
        ],
    );
    assert_eq!(e.output_regions.len(), 2);
    for (j, region) in &e.output_regions {
        assert_eq!(region.mask, e.failing[*j]);
        assert_eq!(region.tuples, 4);
        let value = if *j == 1 { 0 } else { 3 };
        assert!(
            region
                .mask
                .iter()
                .enumerate()
                .all(|(ordinal, wrong)| *wrong == (s.point(ordinal)[0] == value))
        );
        if let Some(formula) = &region.formula {
            let values = ok(evaluate(formula, &s.env(None), &BTreeMap::new()));
            assert!(
                values
                    .iter()
                    .zip(&region.mask)
                    .all(|(v, wrong)| (*v == 1) == *wrong)
            );
        } else {
            assert!(region.shared.contains(&(0, value)));
        }
    }
    assert!(
        !e.output_regions[0]
            .1
            .mask
            .iter()
            .zip(&e.output_regions[1].1.mask)
            .any(|(a, b)| *a && *b)
    );
}

#[test]
fn shared_constant_exhaustion_names_incompatible_inputs_without_a_false_witness() {
    let s = tiny();
    let r = sketch::solve(&s, &outputs(&["?c"]), &settings());
    assert_eq!(r.status, Status::Unrealizable);
    assert!(r.outputs[0].witness.is_none());
    let pair = &r.outputs[0].conflicting_inputs;
    assert_eq!(pair.len(), 2);
    assert_ne!(s.targets[0][pair[0]], s.targets[0][pair[1]]);
}
