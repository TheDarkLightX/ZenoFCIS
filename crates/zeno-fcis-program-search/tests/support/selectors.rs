#![allow(dead_code)]
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

pub(crate) const NAMES: [&str; 8] = [
    "max4",
    "min4",
    "lower-median4",
    "upper-median4",
    "min-of-pair-maxima",
    "max-of-pair-minima",
    "guard-maxmin",
    "priority-select",
];

pub(crate) fn reference(name: &str, x: &[i128]) -> i128 {
    let [a, b, c, d] = [x[0], x[1], x[2], x[3]];
    match name {
        "max4" => *x.iter().max().unwrap_or_else(|| panic!("empty input")),
        "min4" => *x.iter().min().unwrap_or_else(|| panic!("empty input")),
        "lower-median4" | "upper-median4" => {
            let mut ordered = [a, b, c, d];
            ordered.sort_unstable();
            ordered[if name == "lower-median4" { 1 } else { 2 }]
        }
        "min-of-pair-maxima" => a.max(b).min(c.max(d)),
        "max-of-pair-minima" => a.min(b).max(c.min(d)),
        "guard-maxmin" => {
            if a < b {
                c.max(d)
            } else {
                c.min(d)
            }
        }
        "priority-select" => {
            if a < b {
                a
            } else if b < c {
                b
            } else if c < d {
                c
            } else {
                d
            }
        }
        _ => panic!("unknown selector"),
    }
}

fn add(nodes: &mut Vec<Op>, op: Op) -> u16 {
    let id = u16::try_from(nodes.len()).unwrap_or_else(|e| panic!("{e:?}"));
    nodes.push(op);
    id
}

fn choose(nodes: &mut Vec<Op>, a: u16, b: u16, minimum: bool) -> u16 {
    let p = add(nodes, Op::Lt(a, b));
    add(
        nodes,
        if minimum {
            Op::Select(p, a, b)
        } else {
            Op::Select(p, b, a)
        },
    )
}

/// Deliberately repeated bubble-sort passes and redundant selections. The
/// independent reference above does not interpret or share this construction.
pub(crate) fn original(name: &str) -> Program {
    let mut nodes = vec![Op::Input(0), Op::Input(1), Op::Input(2), Op::Input(3)];
    let root = match name {
        "max4" | "min4" | "lower-median4" | "upper-median4" => {
            let mut x = [0, 1, 2, 3];
            for _ in 0..4 {
                for j in 0..3 {
                    let a = x[j];
                    let b = x[j + 1];
                    x[j] = choose(&mut nodes, a, b, true);
                    x[j + 1] = choose(&mut nodes, a, b, false);
                }
            }
            x[match name {
                "min4" => 0,
                "lower-median4" => 1,
                "upper-median4" => 2,
                _ => 3,
            }]
        }
        "min-of-pair-maxima" | "max-of-pair-minima" => {
            let inner_min = name == "max-of-pair-minima";
            let a = choose(&mut nodes, 0, 1, inner_min);
            let b = choose(&mut nodes, 2, 3, inner_min);
            choose(&mut nodes, a, b, !inner_min)
        }
        "guard-maxmin" => {
            let a = choose(&mut nodes, 2, 3, false);
            let b = choose(&mut nodes, 2, 3, true);
            let p = add(&mut nodes, Op::Lt(0, 1));
            add(&mut nodes, Op::Select(p, a, b))
        }
        "priority-select" => {
            let mut r = 3;
            for i in (0..3).rev() {
                let p = add(&mut nodes, Op::Lt(i, i + 1));
                r = add(&mut nodes, Op::Select(p, i, r));
            }
            r
        }
        _ => panic!("unknown selector"),
    };
    let p = add(&mut nodes, Op::Eq(0, 1));
    let root = add(&mut nodes, Op::Select(p, root, root));
    Program::try_new(
        vec![Domain::Int { min: 0, max: 7 }; 4],
        vec![Domain::Int { min: 0, max: 7 }],
        nodes,
        vec![root],
    )
    .unwrap_or_else(|e| panic!("{e:?}"))
}

pub(crate) fn bytes(program: &Program) -> Vec<u8> {
    program
        .value()
        .unwrap_or_else(|e| panic!("{e:?}"))
        .canonical_bytes()
        .unwrap_or_else(|e| panic!("{e:?}"))
}

pub(crate) fn tuple(ordinal: usize) -> Vec<i128> {
    (0..4)
        .map(|i| ((ordinal >> (3 * (3 - i))) & 7) as i128)
        .collect()
}

pub(crate) fn core(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/data/{name}.original.zcve",
        env!("CARGO_MANIFEST_DIR")
    );
    let old = std::fs::read(path).unwrap_or_else(|e| panic!("{e:?}"));
    let p = zeno_fcis_synthesis::finite_runtime::import_i64_program(&old)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let len = p.profile().len();
    let mut out = old[..10].to_vec();
    out[9] += 1;
    out.extend_from_slice(p.profile().replace("i64", "i128").as_bytes());
    out.extend_from_slice(&old[10 + len..]);
    out
}

pub(crate) const CORE: [&str; 7] = [
    "reservation-pool-c2-q1",
    "bounded-counter-c8",
    "retry-budget-c8",
    "finite-phase-machine-c1",
    "consumable-budget-c3",
    "rate-limiter-n2-w2",
    "idempotency-slot-c8",
];
