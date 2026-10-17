//! Fixed unsigned graph descriptions for arithmetic characterization.
//! These are input data for the existing numeric evaluator, never decision callbacks.
use zeno_fcis_synthesis::finite::{
    V2IntegerWidth as Width, V2NumericDomain as Domain, V2NumericOp as Op,
};

pub const LIMIT: u128 = 3_000_000_000;
pub const U: Width = Width::Unsigned128;
pub const UNSIGNED: Domain<'static> = Domain::U128 {
    min: 0,
    max: u128::MAX,
};
pub const SWAP_INPUTS: [Domain<'static>; 4] = [
    Domain::U128 { min: 0, max: LIMIT },
    Domain::U128 {
        min: 0,
        max: 10_000,
    },
    Domain::U128 { min: 1, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
];
pub const SWAP_NODES: [Op; 12] = [
    Op::Input(0),
    Op::Input(1),
    Op::Input(2),
    Op::Input(3),
    Op::U128(10_000),
    Op::U128(1),
    Op::MulDivCeil(0, 1, 4),
    Op::Sub(U, 0, 6),
    Op::Add(U, 2, 7),
    Op::MulDivFloor(3, 7, 8),
    Op::Sub(U, 3, 9),
    Op::ProductsGe([8, 10, 5], [2, 3, 5]),
];
pub const SWAP_ROOTS: [u16; 6] = [6, 7, 9, 8, 10, 11];
pub const SWAP_OUTPUTS: [Domain<'static>; 6] = [
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    UNSIGNED,
    Domain::Bool,
];

pub const BURN_INPUTS: [Domain<'static>; 3] = [
    Domain::U128 { min: 0, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
];
pub const BURN_NODES: [Op; 4] = [
    Op::Input(0),
    Op::Input(1),
    Op::Input(2),
    Op::MulDivFloor(0, 1, 2),
];

pub const MINT_INPUTS: [Domain<'static>; 5] = [
    Domain::U128 { min: 0, max: LIMIT },
    Domain::U128 { min: 0, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
    Domain::U128 { min: 1, max: LIMIT },
];
pub const MINT_NODES: [Op; 9] = [
    Op::Input(0),
    Op::Input(1),
    Op::Input(2),
    Op::Input(3),
    Op::Input(4),
    Op::MulDivFloor(0, 4, 2),
    Op::MulDivFloor(1, 4, 3),
    Op::Lt(5, 6),
    Op::Select(7, 5, 6),
];

#[derive(Clone, Copy, Debug)]
pub enum Graph {
    Swap,
    Burn,
    Mint,
}

fn same_domain(a: Domain<'_>, b: Domain<'_>) -> bool {
    match (a, b) {
        (Domain::U128 { min: a, max: b }, Domain::U128 { min: c, max: d }) => a == c && b == d,
        (Domain::Bool, Domain::Bool) => true,
        _ => false,
    }
}

/// Exact closed-profile admission followed by the candidate range calculation.
/// None means unsupported graph/domain, never a proof or a business rejection.
/// Graph admission and its link to the numeric driver have a tested bridge;
/// the quantified bounds theorems are in spot_bounds.rs.
pub fn analyse_graph(
    kind: Graph,
    inputs: &[Domain<'_>],
    nodes: &[Op],
) -> Option<Vec<Domain<'static>>> {
    let (expected_inputs, expected_nodes): (&[Domain<'_>], &[Op]) = match kind {
        Graph::Swap => (&SWAP_INPUTS, &SWAP_NODES),
        Graph::Burn => (&BURN_INPUTS, &BURN_NODES),
        Graph::Mint => (&MINT_INPUTS, &MINT_NODES),
    };
    if nodes != expected_nodes
        || inputs.len() != expected_inputs.len()
        || !inputs
            .iter()
            .zip(expected_inputs)
            .all(|(a, b)| same_domain(*a, *b))
    {
        return None;
    }
    if let Graph::Mint = kind {
        let (Op::Lt(a, b), Op::Select(c, yes, no)) = (nodes[7], nodes[8]) else {
            return None;
        };
        if c != 7 || !crate::spot::is_minimum(5, 6, a, b, yes, no) {
            return None;
        }
    }
    let b = crate::spot_bounds::analyse(LIMIT, LIMIT, LIMIT)?;
    let domain = |r: crate::spot::Range| Domain::U128 {
        min: r.lower,
        max: r.upper,
    };
    Some(match kind {
        Graph::Swap => vec![
            domain(b.amount),
            domain(b.rate),
            domain(b.reserve),
            domain(b.reserve),
            Domain::U128 {
                min: 10_000,
                max: 10_000,
            },
            Domain::U128 { min: 1, max: 1 },
            domain(b.fee),
            domain(b.net),
            domain(b.denominator),
            domain(b.swap_output),
            domain(b.remaining_reserve),
            Domain::Bool,
        ],
        Graph::Burn => vec![
            domain(b.amount),
            domain(b.reserve),
            domain(b.supply),
            domain(b.burn_output),
        ],
        Graph::Mint => vec![
            domain(b.amount),
            domain(b.amount),
            domain(b.reserve),
            domain(b.reserve),
            domain(b.supply),
            domain(b.mint_side),
            domain(b.mint_side),
            Domain::Bool,
            domain(b.minted),
        ],
    })
}
