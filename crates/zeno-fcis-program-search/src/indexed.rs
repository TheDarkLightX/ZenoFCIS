//! Exact feasibility indexes for depth-one demands under a depth-two split.
//! Private child of topdown: reuses its metered arena and owned pool emitter.
//! Expanded cost bounds only this fragment; the archive judges emitted DAGs.

use super::*;

const COMPONENT_COST: usize = 6;
const TREE_COST: usize = 96;

fn expanded(search: &TopDown<'_>, id: usize) -> usize {
    search.arena[id].expanded_cost
}

fn kind(search: &TopDown<'_>, id: usize) -> Kind {
    match search.arena[id].node {
        Node::Input(i) => domain_kind(search.original.inputs[usize::from(i)]).unwrap_or(Kind::Int),
        Node::Bool(_)
        | Node::Eq(..)
        | Node::Lt(..)
        | Node::And(..)
        | Node::Not(_)
        | Node::ProductsGe(..) => Kind::Bool,
        Node::Select(_, a, _) => kind(search, a),
        _ => Kind::Int,
    }
}

/// Add bounded source components and pair selectors, without inspecting targets.
fn prepare(search: &mut TopDown<'_>, meter: &mut Meter) -> Result<Vec<usize>, Stop> {
    let mut ids: Vec<Option<usize>> = Vec::new();
    let mut costs: Vec<usize> = Vec::new();
    for node in &search.original.nodes.clone() {
        meter.charge(1)?;
        let (args, count) = node.operands();
        let cost = args[..count].iter().fold(1usize, |sum, arg| {
            sum.saturating_add(costs.get(*arg).copied().unwrap_or(usize::MAX))
        });
        costs.push(cost);
        if cost > COMPONENT_COST || args[..count].iter().any(|i| ids[*i].is_none()) {
            ids.push(None);
        } else {
            let renamed = node.map(|i| ids[i].unwrap_or(usize::MAX));
            ids.push(search.make(renamed, meter)?);
        }
    }
    let inputs: Vec<_> = search
        .atoms_int
        .iter()
        .copied()
        .filter(|id| matches!(search.arena[*id].node, Node::Input(_)))
        .collect();
    for (i, a) in inputs.iter().enumerate() {
        for b in &inputs[i + 1..] {
            let Some(p) = search.make(Node::Lt(*a, *b), meter)? else {
                continue;
            };
            search.make(Node::Select(p, *a, *b), meter)?;
            search.make(Node::Select(p, *b, *a), meter)?;
        }
    }
    let ints: Vec<_> = (0..search.arena.len())
        .filter(|id| kind(search, *id) == Kind::Int && expanded(search, *id) <= COMPONENT_COST)
        .collect();
    // The growable predicate-ID list can hold up to twice its final bound.
    let predicate_bound = search.atoms_bool.len() as u128
        + (ints.len() as u128) * (ints.len().saturating_sub(1) as u128) * 3 / 2;
    reserve(
        search,
        predicate_bound * 2 * core::mem::size_of::<usize>() as u128,
        meter,
    )?;
    let mut predicates = search.atoms_bool.clone();
    for (i, a) in ints.iter().enumerate() {
        for (j, b) in ints.iter().enumerate() {
            if i == j {
                continue;
            }
            meter.charge(1)?;
            if let Some(p) = search.make(Node::Lt(*a, *b), meter)? {
                predicates.push(p);
            }
            if i < j
                && let Some(p) = search.make(Node::Eq(*a, *b), meter)?
            {
                predicates.push(p);
            }
        }
    }
    predicates.sort_unstable();
    predicates.dedup();
    meter.charge(predicates.len().saturating_mul(search.size))?;
    predicates.retain(|id| {
        let values = &search.arena[*id].value;
        // Two positive-cost leaves and Select cost at least three. A guard
        // above this cap cannot participate in the declared tree fragment.
        expanded(search, *id) <= TREE_COST - 3 && values.contains(&0) && values.contains(&1)
    });
    predicates.sort_by_key(|id| (expanded(search, *id), *id));
    Ok(predicates)
}

fn words(bits: usize) -> usize {
    bits.div_ceil(64)
}

fn full(size: usize) -> Vec<u64> {
    let mut mask = vec![0; words(size)];
    for (word, value) in mask.iter_mut().enumerate() {
        let remaining = size.saturating_sub(word * 64).min(64);
        *value = if remaining == 64 {
            u64::MAX
        } else {
            (1u64 << remaining) - 1
        };
    }
    mask
}

fn ones(mask: &[u64]) -> impl Iterator<Item = usize> + '_ {
    mask.iter().enumerate().flat_map(|(word, value)| {
        let mut remaining = *value;
        core::iter::from_fn(move || {
            if remaining == 0 {
                return None;
            }
            let bit = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            Some(word * 64 + bit)
        })
    })
}

/// Every retained word, descriptor and the largest concurrent scratch is
/// reserved before allocation. Cells round bytes upward to the existing i128
/// unit. Reservations are monotone, including interrupted/refused requests.
fn reserve(search: &mut TopDown<'_>, bytes: u128, meter: &mut Meter) -> Result<(), Stop> {
    meter.charge(1)?;
    search.cells = search.cells.saturating_add(bytes.div_ceil(16));
    if search.cells > u128::from(search.max_cells) {
        return Err(Stop::CellsOverBudget {
            cells: search.cells,
            limit: search.max_cells,
        });
    }
    Ok(())
}

struct Index {
    leaves: Vec<usize>,
    costs: Vec<usize>,
    predicates: Vec<usize>,
    cover: Vec<Vec<u64>>,
    pair_cover: Vec<u64>,
    guard_true: Vec<u64>,
    pair_words: usize,
    guard_words: usize,
}

#[derive(Clone, Copy, Debug)]
struct Branch {
    guard: Option<usize>,
    yes: usize,
    no: usize,
    cost: usize,
}

impl Index {
    fn new(
        search: &mut TopDown<'_>,
        target: &[i128],
        output: Kind,
        predicates: &[usize],
        meter: &mut Meter,
    ) -> Result<Self, Stop> {
        let mut leaves: Vec<_> = (0..search.arena.len())
            .filter(|id| kind(search, *id) == output && expanded(search, *id) <= COMPONENT_COST)
            .collect();
        leaves.sort_by_key(|id| (expanded(search, *id), *id));
        let n = leaves.len();
        let pair_words = words(n.saturating_mul(n));
        let guard_words = words(predicates.len());
        let case_words = words(target.len());
        let retained_words = (target.len() as u128) * (pair_words as u128 + guard_words as u128)
            + n as u128 * case_words as u128;
        let scratch_words = (case_words as u128) * 3 + pair_words as u128 + guard_words as u128;
        let descriptors =
            (n as u128 * 6 + predicates.len() as u128) * core::mem::size_of::<usize>() as u128;
        reserve(
            search,
            (retained_words + scratch_words) * 8 + descriptors,
            meter,
        )?;
        let mut index = Self {
            leaves,
            costs: Vec::with_capacity(n),
            predicates: predicates.to_vec(),
            cover: vec![vec![0; case_words]; n],
            pair_cover: vec![0; target.len().saturating_mul(pair_words)],
            guard_true: vec![0; target.len().saturating_mul(guard_words)],
            pair_words,
            guard_words,
        };
        for (i, id) in index.leaves.iter().enumerate() {
            index.costs.push(expanded(search, *id));
            meter.charge(target.len())?;
            for (case, want) in target.iter().enumerate() {
                if search.arena[*id].value[case] == *want {
                    index.cover[i][case / 64] |= 1 << (case % 64);
                }
            }
        }
        for case in 0..target.len() {
            meter.charge(n.saturating_mul(n).saturating_add(predicates.len()))?;
            for a in 0..n {
                for b in 0..n {
                    if (index.cover[a][case / 64] | index.cover[b][case / 64]) & (1 << (case % 64))
                        != 0
                    {
                        let pair = a * n + b;
                        index.pair_cover[case * pair_words + pair / 64] |= 1 << (pair % 64);
                    }
                }
            }
            for (p, id) in predicates.iter().enumerate() {
                if search.arena[*id].value[case] == 1 {
                    index.guard_true[case * guard_words + p / 64] |= 1 << (p % 64);
                }
            }
        }
        Ok(index)
    }

    fn branch(
        &self,
        search: &TopDown<'_>,
        mask: &[u64],
        ceiling: usize,
        meter: &mut Meter,
    ) -> Result<Option<Branch>, Stop> {
        let mut best = None;
        for (i, id) in self.leaves.iter().enumerate() {
            meter.charge(mask.len())?;
            if self.costs[i] <= ceiling && mask.iter().zip(&self.cover[i]).all(|(s, c)| s & !c == 0)
            {
                best = Some(Branch {
                    guard: None,
                    yes: *id,
                    no: *id,
                    cost: self.costs[i],
                });
                break;
            }
        }
        if self.predicates.is_empty() {
            return Ok(best);
        }
        // Condition 1: S is covered by the union of the two leaves.
        meter.charge(self.pair_words)?;
        let mut pairs = full(self.leaves.len().saturating_mul(self.leaves.len()));
        for case in ones(mask) {
            meter.charge(self.pair_words)?;
            for (w, possible) in pairs.iter_mut().enumerate() {
                *possible &= self.pair_cover[case * self.pair_words + w];
            }
            if pairs.iter().all(|v| *v == 0) {
                return Ok(best);
            }
        }
        for pair in ones(&pairs) {
            meter.charge(1)?;
            let (a, b) = (pair / self.leaves.len(), pair % self.leaves.len());
            let lower_bound =
                1 + self.costs[a] + self.costs[b] + expanded(search, self.predicates[0]);
            let ceiling = best.map_or(ceiling, |b: Branch| ceiling.min(b.cost));
            if ceiling < lower_bound {
                continue;
            }
            meter.charge(self.guard_words)?;
            let mut guards = full(self.predicates.len());
            for case in ones(mask) {
                let bit = 1 << (case % 64);
                let must_true = self.cover[b][case / 64] & bit == 0;
                let must_false = self.cover[a][case / 64] & bit == 0;
                if !must_true && !must_false {
                    continue;
                }
                meter.charge(self.guard_words)?;
                for (w, possible) in guards.iter_mut().enumerate() {
                    let truth = self.guard_true[case * self.guard_words + w];
                    if must_true {
                        *possible &= truth;
                    } // condition 2
                    if must_false {
                        *possible &= !truth;
                    } // condition 3
                }
                if guards.iter().all(|v| *v == 0) {
                    break;
                }
            }
            if let Some(p) = ones(&guards).next() {
                let guard = self.predicates[p];
                let cost = 1 + self.costs[a] + self.costs[b] + expanded(search, guard);
                if cost <= ceiling && best.is_none_or(|b| cost < b.cost) {
                    best = Some(Branch {
                        guard: Some(guard),
                        yes: self.leaves[a],
                        no: self.leaves[b],
                        cost,
                    });
                }
            }
        }
        Ok(best)
    }
}

fn put(search: &TopDown<'_>, pool: &mut Pool, branch: Branch) -> usize {
    let a = search.put(pool, branch.yes);
    if let Some(p) = branch.guard {
        let b = search.put(pool, branch.no);
        let p = search.put(pool, p);
        pool.add(Node::Select(p, a, b))
    } else {
        a
    }
}

pub(super) fn program_for(
    search: &mut TopDown<'_>,
    targets: &[(Vec<i128>, Kind)],
    meter: &mut Meter,
) -> Result<Option<Graph>, Stop> {
    let predicates = prepare(search, meter)?;
    let mut pool = Pool::new(&search.original.inputs);
    let original = pool.load(search.original);
    let mut roots = Vec::new();
    let mut found = false;
    for ((target, output), old) in targets.iter().zip(&search.original.roots.clone()) {
        let index = Index::new(search, target, *output, &predicates, meter)?;
        let all = full(target.len());
        let mut answer = index
            .branch(search, &all, TREE_COST, meter)?
            .map(|branch| (None, branch, branch, branch.cost));
        for guard in &predicates {
            let pred_cost = expanded(search, *guard);
            let ceiling = answer.map_or(TREE_COST, |a| a.3);
            if index.leaves.is_empty() || ceiling < 1 + pred_cost + 2 * index.costs[0] {
                continue;
            }
            meter.charge(target.len())?;
            let mut on = vec![0; all.len()];
            for (case, value) in search.arena[*guard].value.iter().enumerate() {
                if *value == 1 {
                    on[case / 64] |= 1 << (case % 64);
                }
            }
            let off: Vec<_> = all.iter().zip(&on).map(|(a, b)| a & !b).collect();
            let Some(yes) =
                index.branch(search, &on, ceiling - 1 - pred_cost - index.costs[0], meter)?
            else {
                continue;
            };
            let Some(no) = index.branch(search, &off, ceiling - 1 - pred_cost - yes.cost, meter)?
            else {
                continue;
            };
            let cost = 1 + pred_cost + yes.cost + no.cost;
            if answer.is_none_or(|a| cost < a.3) {
                answer = Some((Some(*guard), yes, no, cost));
            }
        }
        roots.push(if let Some((guard, yes, no, _)) = answer {
            found = true;
            let a = put(search, &mut pool, yes);
            if let Some(guard) = guard {
                let b = put(search, &mut pool, no);
                let p = search.put(&mut pool, guard);
                pool.add(Node::Select(p, a, b))
            } else {
                a
            }
        } else {
            original[*old]
        });
    }
    Ok(found.then(|| pool.emit(&search.original.outputs, &roots)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::space::{Reference, Space};
    use crate::{Archive, Settings};
    use zeno_fcis_synthesis::finite::{Domain, Op, Program};

    fn exercise(f: impl FnOnce(&mut TopDown<'_>, &Env<'_>, &mut Meter)) {
        let p = Program::try_new(
            vec![Domain::Int { min: 0, max: 2 }; 2],
            vec![Domain::Int { min: 0, max: 2 }],
            vec![
                Op::Input(0),
                Op::Input(1),
                Op::Lt(0, 1),
                Op::Select(2, 1, 0),
            ],
            vec![3],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let graph = Graph::from_program(&p).unwrap_or_else(|e| panic!("{e:?}"));
        let settings = Settings::default();
        let space = Space::new(p.inputs(), 100).unwrap_or_else(|e| panic!("{e:?}"));
        let mut meter = Meter::new(u64::MAX);
        let reference = Reference::new(&p, &space, settings.budget.max_cells, &mut meter)
            .unwrap_or_else(|e| panic!("{e:?}"));
        let env = Env {
            reference: &reference,
            space: &space,
            settings: &settings,
        };
        let mut search = TopDown::new(&graph, reference.live(), 2, &env, &mut meter)
            .unwrap_or_else(|e| panic!("{e:?}"));
        f(&mut search, &env, &mut meter);
    }

    fn target(search: &TopDown<'_>) -> Vec<i128> {
        search.columns[0]
            .iter()
            .zip(&search.columns[1])
            .map(|(a, b)| (*a).max(*b))
            .collect()
    }

    fn proposal(search: &TopDown<'_>, branch: Branch) -> Graph {
        let mut pool = Pool::new(&search.original.inputs);
        let root = put(search, &mut pool, branch);
        pool.emit(&search.original.outputs, &[root])
    }

    fn exact_branch() {
        exercise(|search, _, meter| {
            let wanted = target(search);
            let index = Index::new(
                search,
                &wanted,
                Kind::Int,
                &search.atoms_bool.clone(),
                meter,
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            let branch = index
                .branch(search, &full(9), 96, meter)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .unwrap_or_else(|| panic!("no branch"));
            let graph = proposal(search, branch)
                .to_program()
                .unwrap_or_else(|| panic!("program not admitted"));
            for a in 0..=2 {
                for b in 0..=2 {
                    assert_eq!(
                        graph.evaluate(&[a, b]).unwrap_or_else(|e| panic!("{e:?}")),
                        vec![a.max(b)]
                    );
                }
            }
        });
    }

    #[test]
    fn every_demand_mask_matches_an_independent_scan() {
        exercise(|search, _, meter| {
            for minimum in [false, true] {
                let wanted: Vec<_> = search.columns[0]
                    .iter()
                    .zip(&search.columns[1])
                    .map(|(a, b)| if minimum { (*a).min(*b) } else { (*a).max(*b) })
                    .collect();
                let index = Index::new(
                    search,
                    &wanted,
                    Kind::Int,
                    &search.atoms_bool.clone(),
                    meter,
                )
                .unwrap_or_else(|e| panic!("{e:?}"));
                for mask in 0u64..512 {
                    let demanded: Vec<_> = (0..9).filter(|case| mask & (1 << case) != 0).collect();
                    let mut brute = None;
                    for (a, id) in index.leaves.iter().enumerate() {
                        if demanded
                            .iter()
                            .all(|case| search.arena[*id].value[*case] == wanted[*case])
                        {
                            brute =
                                Some(brute.map_or(index.costs[a], |cost: usize| {
                                    cost.min(index.costs[a])
                                }));
                        }
                        for (b, other) in index.leaves.iter().enumerate() {
                            for guard in &index.predicates {
                                if demanded.iter().all(|case| {
                                    let chosen = if search.arena[*guard].value[*case] == 1 {
                                        *id
                                    } else {
                                        *other
                                    };
                                    search.arena[chosen].value[*case] == wanted[*case]
                                }) {
                                    let cost = 1
                                        + expanded(search, *guard)
                                        + index.costs[a]
                                        + index.costs[b];
                                    brute = Some(brute.map_or(cost, |old| old.min(cost)));
                                }
                            }
                        }
                    }
                    let found = index
                        .branch(search, &[mask], 96, meter)
                        .unwrap_or_else(|e| panic!("{e:?}"));
                    assert_eq!(
                        found.map(|branch| branch.cost),
                        brute,
                        "mask={mask} minimum={minimum}"
                    );
                }
            }
        });
    }

    #[test]
    fn condition_two_is_required() {
        exact_branch();
    }
    #[test]
    fn condition_three_is_required() {
        exact_branch();
    }

    #[test]
    fn exact_cost_boundary_is_inclusive() {
        exercise(|search, _, meter| {
            let wanted = target(search);
            let index = Index::new(
                search,
                &wanted,
                Kind::Int,
                &search.atoms_bool.clone(),
                meter,
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            let branch = index
                .branch(search, &full(9), 6, meter)
                .unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(branch.map(|b| b.cost), Some(6));
            assert!(
                index
                    .branch(search, &full(9), 5, meter)
                    .unwrap_or_else(|e| panic!("{e:?}"))
                    .is_none()
            );
        });
    }

    #[test]
    fn tuple_mask_includes_last_case_and_excludes_padding() {
        for size in [0, 1, 2, 63, 64, 65, 127, 128, 129] {
            assert_eq!(
                ones(&full(size)).collect::<Vec<_>>(),
                (0..size).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn index_memory_is_charged_before_allocation() {
        exercise(|search, _, meter| {
            let wanted = target(search);
            search.max_cells = u64::try_from(search.cells).unwrap_or_else(|e| panic!("{e:?}"));
            let before = meter.used();
            assert!(matches!(
                Index::new(
                    search,
                    &wanted,
                    Kind::Int,
                    &search.atoms_bool.clone(),
                    meter
                ),
                Err(Stop::CellsOverBudget { .. })
            ));
            assert!(meter.used() > before);
            assert!(search.cells > u128::from(search.max_cells));
        });
    }

    #[test]
    fn interrupted_index_keeps_charges_and_retries_deterministically() {
        exercise(|search, _, _| {
            let wanted = target(search);
            let before = search.cells;
            let mut meter = Meter::new(2);
            assert!(matches!(
                Index::new(
                    search,
                    &wanted,
                    Kind::Int,
                    &search.atoms_bool.clone(),
                    &mut meter
                ),
                Err(Stop::WorkBudget { .. })
            ));
            assert!(search.cells > before);
            assert!(meter.used() > 2);
            let after = search.cells;
            let mut retry = Meter::new(0);
            assert!(matches!(
                Index::new(
                    search,
                    &wanted,
                    Kind::Int,
                    &search.atoms_bool.clone(),
                    &mut retry
                ),
                Err(Stop::WorkBudget { .. })
            ));
            assert_eq!(search.cells, after);
        });
    }

    /// The defect runner uses this passing test to demonstrate that a relaxed
    /// guard's wrong candidate reaches the actual archive comparison.
    #[test]
    fn branch_candidates_cross_full_domain_comparison() {
        exercise(|search, env, meter| {
            let wanted = target(search);
            let index = Index::new(
                search,
                &wanted,
                Kind::Int,
                &search.atoms_bool.clone(),
                meter,
            )
            .unwrap_or_else(|e| panic!("{e:?}"));
            let branch = index
                .branch(search, &full(9), 96, meter)
                .unwrap_or_else(|e| panic!("{e:?}"))
                .unwrap_or_else(|| panic!("no branch"));
            let graph = proposal(search, branch);
            let p = search
                .original
                .to_program()
                .unwrap_or_else(|| panic!("program not admitted"));
            let bytes = crate::ir::encode(&p).unwrap_or_else(|| panic!("encoding failed"));
            let mut archive = Archive::new(
                search.original.clone(),
                bytes.clone(),
                (p.nodes().len(), bytes.len()),
            );
            archive
                .offer(Some(graph.clone()), "indexed-control", 0, env, meter)
                .unwrap_or_else(|e| panic!("{e:?}"));
            let candidate = graph
                .to_program()
                .unwrap_or_else(|| panic!("program not admitted"));
            let wrong = (0..=2).any(|a| {
                (0..=2).any(|b| {
                    candidate
                        .evaluate(&[a, b])
                        .unwrap_or_else(|e| panic!("{e:?}"))
                        != vec![a.max(b)]
                })
            });
            assert_eq!(archive.offers["indexed-control"].refused, u32::from(wrong));
            if wrong {
                assert_eq!(archive.entries.len(), 1);
                assert!(matches!(
                    archive.refused[0].refusal,
                    crate::Refusal::Differs { .. }
                ));
                println!("full-domain refused: {:?}", archive.refused[0].refusal);
            }
        });
    }
}
