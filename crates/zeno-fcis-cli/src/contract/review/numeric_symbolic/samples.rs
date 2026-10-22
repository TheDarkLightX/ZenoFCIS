//! Reproducible guard inputs; sampling never proves a law.
use super::*;
use crate::contract::model::InputLeaf;
pub(super) const SEED: u128 = 0x98e527a4d63b190f;
pub(super) const RANDOM: usize = 64;
fn add(v: &mut Vec<Atom<'static>>, a: Atom<'static>) {
    if !v.contains(&a) {
        v.push(a)
    }
}
fn boundaries(d: &ScalarDomain) -> Vec<Atom<'static>> {
    let mut out = Vec::new();
    match d {
        ScalarDomain::Bool => out.extend([Atom::Bool(false), Atom::Bool(true)]),
        ScalarDomain::Int { min, max } => {
            for n in [
                *min,
                min.saturating_add(1),
                *max,
                max.saturating_sub(1),
                -1,
                0,
                1,
            ] {
                if *min <= n && n <= *max {
                    add(&mut out, Atom::I128(n));
                }
            }
            for p in [63, 64, 100, 126] {
                for delta in [-1, 0, 1] {
                    for n in [(1i128 << p) + delta, -(1i128 << p) + delta] {
                        if *min <= n && n <= *max {
                            add(&mut out, Atom::I128(n));
                        }
                    }
                }
            }
        }
        ScalarDomain::Unsigned { min, max } => {
            for n in [
                *min,
                min.saturating_add(1),
                *max,
                max.saturating_sub(1),
                0,
                1,
            ] {
                if *min <= n && n <= *max {
                    add(&mut out, Atom::U128(n));
                }
            }
            for p in [63, 64, 100, 127] {
                for n in [(1u128 << p) - 1, 1u128 << p, (1u128 << p) + 1] {
                    if *min <= n && n <= *max {
                        add(&mut out, Atom::U128(n));
                    }
                }
            }
        }
    }
    out
}
fn next(seed: &mut u128) -> u128 {
    *seed ^= *seed << 23;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 26;
    *seed
}
fn random(d: &ScalarDomain, seed: &mut u128) -> Atom<'static> {
    let r = next(seed);
    match d {
        ScalarDomain::Bool => Atom::Bool(r & 1 != 0),
        ScalarDomain::Int { min, max } => {
            let width = min.abs_diff(*max);
            let off = if width == u128::MAX {
                r
            } else {
                r % (width + 1)
            };
            Atom::I128(min.wrapping_add(off as i128))
        }
        ScalarDomain::Unsigned { min, max } => {
            let width = max - min;
            Atom::U128(
                min + if width == u128::MAX {
                    r
                } else {
                    r % (width + 1)
                },
            )
        }
    }
}
pub(super) fn rows(c: &Contract<'_>) -> Result<Vec<Vec<Atom<'static>>>, ContractError> {
    let mut cols: Vec<Vec<Atom<'static>>> = c.input_domains.iter().map(boundaries).collect();
    for (i, col) in cols.iter_mut().enumerate() {
        if let InputLeaf::Sum { variants, .. } = unsigned::input_leaf(c, i) {
            *col = variants.iter().map(|v| Atom::I128((*v).into())).collect();
        }
    }
    if cols.iter().any(Vec::is_empty) {
        return Err(error("no guard values for input"));
    }
    let mut commands = vec![Vec::<(usize, Atom<'static>)>::new()];
    for (i, (s, _)) in c.bindings.iter().enumerate() {
        if *s != Source::Command || !matches!(unsigned::input_leaf(c, i), InputLeaf::Sum { .. }) {
            continue;
        }
        let mut next = Vec::new();
        for row in commands {
            for v in &cols[i] {
                let mut row = row.clone();
                row.push((i, *v));
                next.push(row);
            }
        }
        if next.len() > 256 {
            return Err(error("more than 256 command variants in encoding guard"));
        }
        commands = next;
    }
    let mut rows = Vec::new();
    let mut seed = SEED;
    for command in commands {
        let mut base: Vec<_> = cols.iter().map(|v| v[0]).collect();
        for (i, v) in &command {
            base[*i] = *v;
        }
        rows.push(base.clone());
        // Every edge individually, plus paired extrema to expose overflow of
        // two variable operands rather than only constant arithmetic.
        for (i, col) in cols.iter().enumerate() {
            if command.iter().any(|(at, _)| *at == i) {
                continue;
            }
            for v in col {
                let mut row = base.clone();
                row[i] = *v;
                rows.push(row);
            }
            for j in 0..i {
                if command.iter().any(|(at, _)| *at == j) {
                    continue;
                }
                for v in col.iter().take(4) {
                    for w in cols[j].iter().take(4) {
                        let mut row = base.clone();
                        row[i] = *v;
                        row[j] = *w;
                        rows.push(row);
                    }
                }
            }
        }
        let mut genesis = base.clone();
        for (i, ((source, id), d)) in c.bindings.iter().zip(&c.input_domains).enumerate() {
            if *source == Source::State {
                if let Some((_, a)) = c.genesis.iter().find(|(field, _)| Some(*field) == *id) {
                    genesis[i] = match a {
                        crate::contract::graph::Atom::Bool(b) => Atom::Bool(*b),
                        crate::contract::graph::Atom::I128(n) => Atom::I128(*n),
                        crate::contract::graph::Atom::U128(n) => Atom::U128(*n),
                        crate::contract::graph::Atom::Sum { variant, .. } => {
                            Atom::I128((*variant).into())
                        }
                        _ => random(d, &mut seed),
                    };
                }
            }
        }
        rows.push(genesis);
        for _ in 0..RANDOM {
            let mut row: Vec<_> = c
                .input_domains
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    if matches!(unsigned::input_leaf(c, i), InputLeaf::Sum { .. }) {
                        cols[i][(next(&mut seed) % cols[i].len() as u128) as usize]
                    } else {
                        random(d, &mut seed)
                    }
                })
                .collect();
            for (i, v) in &command {
                row[*i] = *v;
            }
            rows.push(row);
        }
    }
    if rows.len() > 65_536 {
        return Err(error("encoding guard exceeds 65536 samples"));
    }
    Ok(rows)
}
