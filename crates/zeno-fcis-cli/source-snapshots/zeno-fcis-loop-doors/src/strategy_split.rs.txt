//! Case composition over a learn-door corpus; no second learner or judge.
//! LLM-generated. Tables/trees/questions are proposals. The caller must check
//! the assembled program against its fixed original and replay the receipt.
use crate::strategy_algebra::{Step, Stop, Work};
use crate::strategy_questions::Limits;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
/// Narrow owned learn-door handoff. Tables are deliberately not an input.
#[derive(Clone, Debug)]
pub struct LearnTermsV1 {
    /// Full-domain integer terms, cheapest meanings first.
    pub integers: Vec<Program>,
    /// Nontrapping finite Boolean predicates.
    pub predicates: Vec<Program>,
    /// True only when producer reports its enumeration complete.
    pub complete: bool,
}
/// Bounded tree/committee/query configuration.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Initial evenly spaced samples.
    pub initial: usize,
    /// Maximum tree depth.
    pub depth: u8,
    /// Committee size.
    pub committee: usize,
    /// Splits explored at a region.
    pub alternatives: usize,
    /// Next explaining terms at each leaf.
    pub leaf_alternatives: usize,
    /// Consecutive agreeing spot checks.
    pub spot_checks: usize,
    /// Total oracle query cap.
    pub queries: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            initial: 8,
            depth: 2,
            committee: 16,
            alternatives: 6,
            leaf_alternatives: 8,
            spot_checks: 8,
            queries: 64,
        }
    }
}
/// Immutable owned finite case tree.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Tree {
    Leaf(usize),
    Case {
        predicate: usize,
        left: alloc::boxed::Box<Tree>,
        right: alloc::boxed::Box<Tree>,
    },
}
/// A proposed assembly and measured search evidence, not a qualified program.
#[derive(Clone, Debug)]
pub struct SplitReport {
    /// Candidate sent to the existing checker by caller.
    pub candidate: Program,
    /// Declared steps actually performed.
    pub trace: Vec<Step>,
    /// Queried tuple ordinals.
    pub queried: Vec<usize>,
    /// Committee questions.
    pub committee_queries: Vec<usize>,
    /// Region-guided spot checks.
    pub spot_queries: Vec<usize>,
    /// Final nonempty minority cells (all visited before a candidate returns).
    pub region_cells: usize,
    /// Charged proposer work.
    pub work: u64,
}
fn table(p: &Program, pts: &[Vec<i128>], w: &mut Work) -> Result<Vec<i128>, Stop> {
    let mut out = Vec::new();
    for point in pts {
        w.charge(p.nodes().len())?;
        let row = p
            .evaluate(point)
            .map_err(|_| w.stop("strategy-term-fails", false))?;
        out.push(row[0]);
    }
    Ok(out)
}
fn cover(m: &[Vec<bool>], cols: &[usize], w: &mut Work) -> Result<Option<Vec<usize>>, Stop> {
    let mut unseen = cols.to_vec();
    let mut chosen = Vec::new();
    while !unseen.is_empty() {
        w.charge(m.len() * unseen.len())?;
        let best = (0..m.len()).max_by_key(|i| {
            (
                unseen.iter().filter(|j| m[*i][**j]).count(),
                core::cmp::Reverse(*i),
            )
        });
        let Some(best) = best else {
            return Ok(None);
        };
        if !unseen.iter().any(|j| m[best][*j]) {
            return Ok(None);
        }
        chosen.push(best);
        unseen.retain(|j| !m[best][*j]);
    }
    Ok(Some(chosen))
}
fn trees(
    m: &[Vec<bool>],
    pred: &[Vec<i128>],
    sizes: &[usize],
    cols: &[usize],
    depth: u8,
    c: Config,
    w: &mut Work,
) -> Result<Vec<Tree>, Stop> {
    w.charge(m.len() * cols.len() + 1)?;
    let leaves = (0..m.len())
        .filter(|i| cols.iter().all(|j| m[*i][*j]))
        .take(c.committee)
        .map(Tree::Leaf)
        .collect::<Vec<_>>();
    if !leaves.is_empty() || depth == 0 {
        return Ok(leaves);
    }
    let Some(chosen) = cover(m, cols, w)? else {
        return Ok(vec![]);
    };
    let mut order = Vec::new();
    for (i, p) in pred.iter().enumerate() {
        w.charge(cols.len() * chosen.len())?;
        let a = cols
            .iter()
            .copied()
            .filter(|j| p[*j] == 1)
            .collect::<Vec<_>>();
        let b = cols
            .iter()
            .copied()
            .filter(|j| p[*j] == 0)
            .collect::<Vec<_>>();
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let score = chosen
            .iter()
            .map(|t| a.iter().filter(|j| m[*t][**j]).count())
            .max()
            .unwrap_or(0)
            + chosen
                .iter()
                .map(|t| b.iter().filter(|j| m[*t][**j]).count())
                .max()
                .unwrap_or(0);
        order.push((core::cmp::Reverse(score), sizes[i], i, a, b));
    }
    order.sort();
    let mut out = Vec::new();
    for (_, _, i, a, b) in order.into_iter().take(c.alternatives) {
        let left = trees(m, pred, sizes, &a, depth - 1, c, w)?;
        let right = trees(m, pred, sizes, &b, depth - 1, c, w)?;
        if let (Some(a), Some(b)) = (left.first(), right.first()) {
            for l in left.iter().take(2) {
                out.push(Tree::Case {
                    predicate: i,
                    left: alloc::boxed::Box::new(l.clone()),
                    right: alloc::boxed::Box::new(b.clone()),
                });
            }
            for r in right.iter().skip(1).take(1) {
                out.push(Tree::Case {
                    predicate: i,
                    left: alloc::boxed::Box::new(a.clone()),
                    right: alloc::boxed::Box::new(r.clone()),
                });
            }
        }
        if out.len() >= c.committee {
            break;
        }
    }
    out.truncate(c.committee);
    Ok(out)
}
fn meaning(
    t: &Tree,
    terms: &[Vec<i128>],
    pred: &[Vec<i128>],
    w: &mut Work,
) -> Result<Vec<i128>, Stop> {
    match t {
        Tree::Leaf(i) => {
            w.charge(terms[*i].len())?;
            Ok(terms[*i].clone())
        }
        Tree::Case {
            predicate,
            left,
            right,
        } => {
            let a = meaning(left, terms, pred, w)?;
            let b = meaning(right, terms, pred, w)?;
            w.charge(a.len())?;
            Ok((0..a.len())
                .map(|j| if pred[*predicate][j] == 1 { a[j] } else { b[j] })
                .collect())
        }
    }
}
fn leaf_variants(
    t: &Tree,
    m: &[Vec<bool>],
    pred: &[Vec<i128>],
    cols: &[usize],
    k: usize,
    w: &mut Work,
) -> Result<Vec<Tree>, Stop> {
    w.charge(m.len() * cols.len() + 1)?;
    match t {
        Tree::Leaf(current) => Ok((0..m.len())
            .filter(|i| cols.iter().all(|j| m[*i][*j]))
            .take(k + 1)
            .filter(|i| i != current)
            .map(Tree::Leaf)
            .collect()),
        Tree::Case {
            predicate,
            left,
            right,
        } => {
            let a = cols
                .iter()
                .copied()
                .filter(|j| pred[*predicate][*j] == 1)
                .collect::<Vec<_>>();
            let b = cols
                .iter()
                .copied()
                .filter(|j| pred[*predicate][*j] == 0)
                .collect::<Vec<_>>();
            let mut out = Vec::new();
            for replacement in leaf_variants(left, m, pred, &a, k, w)? {
                out.push(Tree::Case {
                    predicate: *predicate,
                    left: alloc::boxed::Box::new(replacement),
                    right: right.clone(),
                });
            }
            for replacement in leaf_variants(right, m, pred, &b, k, w)? {
                out.push(Tree::Case {
                    predicate: *predicate,
                    left: left.clone(),
                    right: alloc::boxed::Box::new(replacement),
                });
            }
            Ok(out)
        }
    }
}
fn regions(
    t: &Tree,
    mask: Vec<bool>,
    pred: &[Vec<i128>],
    out: &mut Vec<Vec<bool>>,
    w: &mut Work,
) -> Result<(), Stop> {
    w.charge(mask.len())?;
    match t {
        Tree::Leaf(_) => out.push(mask),
        Tree::Case {
            predicate,
            left,
            right,
        } => {
            let a = mask
                .iter()
                .enumerate()
                .map(|(j, m)| *m && pred[*predicate][j] == 1)
                .collect();
            let b = mask
                .iter()
                .enumerate()
                .map(|(j, m)| *m && pred[*predicate][j] == 0)
                .collect();
            regions(left, a, pred, out, w)?;
            regions(right, b, pred, out, w)?;
        }
    }
    Ok(())
}
fn append(p: &Program, nodes: &mut Vec<Op>, w: &mut Work) -> Result<u16, Stop> {
    w.charge(p.nodes().len())?;
    let base = nodes.len();
    if base + p.nodes().len() > u16::MAX as usize {
        return Err(w.stop("strategy-assembly-size", true));
    }
    let at = |i: u16| base as u16 + i;
    for node in p.nodes() {
        nodes.push(match *node {
            Op::Input(i) => Op::Input(i),
            Op::Int(v) => Op::Int(v),
            Op::Bool(v) => Op::Bool(v),
            Op::Add(a, b) => Op::Add(at(a), at(b)),
            Op::Sub(a, b) => Op::Sub(at(a), at(b)),
            Op::Eq(a, b) => Op::Eq(at(a), at(b)),
            Op::Lt(a, b) => Op::Lt(at(a), at(b)),
            Op::And(a, b) => Op::And(at(a), at(b)),
            Op::Not(a) => Op::Not(at(a)),
            Op::Select(a, b, c) => Op::Select(at(a), at(b), at(c)),
            _ => return Err(w.stop("strategy-term-operation", false)),
        });
    }
    Ok(at(p.roots()[0]))
}
fn assemble(
    t: &Tree,
    terms: &[Program],
    pred: &[Program],
    nodes: &mut Vec<Op>,
    w: &mut Work,
) -> Result<u16, Stop> {
    match t {
        Tree::Leaf(i) => append(&terms[*i], nodes, w),
        Tree::Case {
            predicate,
            left,
            right,
        } => {
            let c = append(&pred[*predicate], nodes, w)?;
            let a = assemble(left, terms, pred, nodes, w)?;
            let b = assemble(right, terms, pred, nodes, w)?;
            let root =
                u16::try_from(nodes.len()).map_err(|_| w.stop("strategy-assembly-size", true))?;
            nodes.push(Op::Select(c, a, b));
            Ok(root)
        }
    }
}
/// Greedy cover, case-tree alternatives, committee questions and minority-side
/// checks within every leaf region. Deterministic tie/fallback order replaces
/// research randomness; no sampled-fit success is called equivalence here.
pub fn split(
    original: &Program,
    corpus: &LearnTermsV1,
    c: Config,
    l: Limits,
) -> Result<SplitReport, Stop> {
    let mut w = Work::new(l.work);
    if !corpus.complete {
        return Err(w.stop("strategy-learn-incomplete", true));
    }
    if original.outputs().len() != 1
        || !matches!(original.outputs()[0], Domain::Int { .. })
        || corpus.integers.is_empty()
        || c.depth > 3
        || c.committee == 0
        || c.committee > 16
        || c.alternatives == 0
        || c.alternatives > 8
        || c.leaf_alternatives > 8
        || c.initial == 0
        || c.initial > 64
        || c.spot_checks == 0
        || c.spot_checks > 64
        || c.queries > 64
    {
        return Err(w.stop("strategy-split-shape", false));
    }
    let mut count = 1u128;
    for d in original.inputs() {
        let (a, b) = d.bounds();
        count = count.saturating_mul(
            b.checked_sub(a)
                .and_then(|span| span.checked_add(1))
                .map_or(u128::MAX, |width| width as u128),
        );
    }
    let items = corpus.integers.len() + corpus.predicates.len();
    let leaves = 1usize << c.depth;
    let meanings = c.committee + leaves * c.leaf_alternatives;
    // Bound scalar scratch before allocation, including all minority masks and
    // temporary recursive tables. The conservative reservation is not CPU work.
    let rows = (original.inputs().len() as u128) * 2
        + (items as u128) * 2
        + (meanings as u128) * 3
        + (leaves as u128) * (corpus.predicates.len() as u128 + 4)
        + 16;
    if count > u128::from(l.worlds) {
        return Err(w.stop("strategy-split-tuples", true));
    }
    if items > l.items {
        return Err(w.stop("strategy-split-items", true));
    }
    if count.saturating_mul(rows) > u128::from(l.cells) {
        return Err(w.stop("strategy-split-cells", true));
    }
    if c.initial.min(usize::try_from(count).unwrap_or(usize::MAX)) > c.queries {
        return Err(w.stop("strategy-query-budget", true));
    }
    let columns = crate::subject::columns(original.inputs(), l.worlds)
        .map_err(|_| w.stop("strategy-split-domain", false))?;
    let size = usize::try_from(count).map_err(|_| w.stop("strategy-split-domain", true))?;
    let pts = (0..size)
        .map(|j| columns.iter().map(|col| col[j]).collect())
        .collect::<Vec<Vec<i128>>>();
    let truth = table(original, &pts, &mut w)?;
    let mut terms = corpus.integers.clone();
    terms.sort_by_key(|p| p.nodes().len());
    let mut kept = Vec::new();
    let mut term_rows = Vec::new();
    for term in terms {
        if term.inputs() != original.inputs()
            || term.outputs()
                != [Domain::Int {
                    min: i128::MIN,
                    max: i128::MAX,
                }]
        {
            return Err(w.stop("strategy-learn-term-abi", false));
        }
        zeno_fcis_program_search::concrete::validate_term(&term, original.inputs())
            .map_err(|_| w.stop("strategy-learn-term-live", false))?;
        let row = table(&term, &pts, &mut w)?;
        w.charge(term_rows.len() * row.len())?;
        if !term_rows.contains(&row) {
            kept.push(term);
            term_rows.push(row);
        }
    }
    let mut predicates = Vec::new();
    let mut pred_rows = Vec::new();
    for p in &corpus.predicates {
        if p.inputs() != original.inputs() || p.outputs() != [Domain::Bool] {
            return Err(w.stop("strategy-learn-predicate-abi", false));
        }
        let row = table(p, &pts, &mut w)?;
        w.charge(pred_rows.len() * row.len())?;
        if row.contains(&0) && row.contains(&1) && !pred_rows.contains(&row) {
            predicates.push(p.clone());
            pred_rows.push(row);
        }
    }
    let sizes = predicates
        .iter()
        .map(|p| p.nodes().len())
        .collect::<Vec<_>>();
    let mut queried = BTreeSet::new();
    for i in 0..c.initial.min(size) {
        queried.insert(i * size / c.initial.min(size));
    }
    w.record(Step::Sample)?;
    let mut committee_queries = Vec::new();
    let mut spot_queries = Vec::new();
    let mut streak = 0;
    loop {
        if queried.len() > c.queries {
            return Err(w.stop("strategy-query-budget", true));
        }
        let cols = queried.iter().copied().collect::<Vec<_>>();
        w.record(Step::LearnTerms)?;
        w.charge(kept.len() * size)?;
        let matches = term_rows
            .iter()
            .map(|row| (0..size).map(|j| row[j] == truth[j]).collect())
            .collect::<Vec<Vec<bool>>>();
        w.record(Step::Cover)?;
        let selected = cover(&matches, &cols, &mut w)?;
        if selected.is_none() {
            return Err(w.stop("strategy-no-cover-proposed", false));
        }
        w.record(Step::LearnSplit)?;
        let mut candidates = trees(&matches, &pred_rows, &sizes, &cols, c.depth, c, &mut w)?;
        let Some(tree) = candidates.first().cloned() else {
            return Err(w.stop("strategy-no-case-tree-proposed", false));
        };
        w.record(Step::Assemble)?;
        candidates.extend(leaf_variants(
            &tree,
            &matches,
            &pred_rows,
            &cols,
            c.leaf_alternatives,
            &mut w,
        )?);
        let mut meanings = BTreeSet::new();
        for t in &candidates {
            meanings.insert(meaning(t, &term_rows, &pred_rows, &mut w)?);
        }
        let mut best = None;
        for j in 0..size {
            if queried.contains(&j) {
                continue;
            }
            w.charge(meanings.len())?;
            let mut buckets = BTreeMap::<i128, usize>::new();
            for row in &meanings {
                *buckets.entry(row[j]).or_default() += 1;
            }
            let key = (
                buckets.values().copied().max().unwrap_or(0),
                buckets.values().map(|n| n * n).sum::<usize>(),
                j,
            );
            if best.is_none_or(|old| key < old) {
                best = Some(key);
            }
        }
        if let Some((worst, _, j)) = best.filter(|(worst, _, _)| *worst < meanings.len()) {
            let _ = worst;
            if queried.len() >= c.queries {
                return Err(w.stop("strategy-query-budget", true));
            }
            queried.insert(j);
            committee_queries.push(j);
            w.record(Step::CommitteeQuestion)?;
            streak = 0;
            continue;
        }
        let mut leaves = Vec::new();
        regions(&tree, vec![true; size], &pred_rows, &mut leaves, &mut w)?;
        let mut cells = Vec::new();
        for region in leaves {
            for p in &pred_rows {
                w.charge(size)?;
                let inside = (0..size)
                    .map(|j| region[j] && p[j] == 1)
                    .collect::<Vec<_>>();
                let outside = (0..size)
                    .map(|j| region[j] && p[j] == 0)
                    .collect::<Vec<_>>();
                let side = if inside.iter().filter(|v| **v).count()
                    <= outside.iter().filter(|v| **v).count()
                {
                    inside
                } else {
                    outside
                };
                if side.iter().any(|v| *v) {
                    cells.push(side);
                }
            }
        }
        if cells.len() as u128 * size as u128 > u128::from(l.cells) {
            return Err(w.stop("strategy-region-cells", true));
        }
        let untested = cells
            .iter()
            .filter(|cell| !queried.iter().any(|j| cell[*j]))
            .collect::<Vec<_>>();
        let free = (0..size)
            .filter(|j| !queried.contains(j))
            .collect::<Vec<_>>();
        let next = free.iter().copied().max_by_key(|j| {
            (
                untested.iter().filter(|side| side[*j]).count(),
                core::cmp::Reverse(*j),
            )
        });
        if let Some(j) = next {
            if queried.len() >= c.queries {
                return Err(w.stop("strategy-query-budget", true));
            }
            let predicted = meaning(&tree, &term_rows, &pred_rows, &mut w)?[j];
            queried.insert(j);
            spot_queries.push(j);
            w.record(Step::SpotCheck)?;
            if predicted == truth[j] {
                streak += 1;
            } else {
                streak = 0;
                continue;
            }
        }
        let covered = cells.iter().all(|cell| queried.iter().any(|j| cell[*j]));
        if next.is_none() || (streak >= c.spot_checks && covered) {
            if next.is_none() {
                w.record(Step::SpotCheck)?;
            }
            let mut nodes = Vec::new();
            let root = assemble(&tree, &kept, &predicates, &mut nodes, &mut w)?;
            let candidate = Program::try_new(
                original.inputs().to_vec(),
                original.outputs().to_vec(),
                nodes,
                vec![root],
            )
            .map_err(|_| w.stop("strategy-assembled-program-admission", false))?;
            return Ok(SplitReport {
                candidate,
                trace: w.trace().to_vec(),
                queried: queried.into_iter().collect(),
                committee_queries,
                spot_queries,
                region_cells: cells.len(),
                work: w.used(),
            });
        }
    }
}
