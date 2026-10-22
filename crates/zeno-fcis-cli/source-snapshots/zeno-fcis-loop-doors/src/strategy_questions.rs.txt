//! Finite diagnosis and reading proposals. LLM-generated; no semantic judge.
//! All tables use the existing library evaluator. Counts and questions remain
//! conditional proposals; neither this module nor a singleton grants authority.
use crate::strategy_algebra::{Step, Stop, Work};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};
/// Bounds shared by one question-strategy invocation.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Full domain cap.
    pub worlds: u64,
    /// Items or retained rows cap.
    pub items: usize,
    /// Retained scalar cell cap.
    pub cells: u64,
    /// Charged proposer work cap.
    pub work: u64,
}
fn points(inputs: &[Domain], l: Limits, w: &mut Work) -> Result<Vec<Vec<i128>>, Stop> {
    let mut size = 1u128;
    for d in inputs {
        let (a, b) = d.bounds();
        size = size.saturating_mul(
            b.checked_sub(a)
                .and_then(|span| span.checked_add(1))
                .map_or(u128::MAX, |width| width as u128),
        );
    }
    if size > u128::from(l.worlds)
        || size.saturating_mul(inputs.len() as u128 * 2) > u128::from(l.cells)
    {
        return Err(w.stop("strategy-world-budget", true));
    }
    w.charge(usize::try_from(size).unwrap_or(usize::MAX))?;
    let columns = crate::subject::columns(inputs, l.worlds)
        .map_err(|_| w.stop("strategy-world-shape", false))?;
    Ok((0..usize::try_from(size).unwrap_or(0))
        .map(|i| columns.iter().map(|c| c[i]).collect())
        .collect())
}
/// Exact Boolean table used only as proposer data.
pub(crate) fn bool_table(p: &Program, pts: &[Vec<i128>], w: &mut Work) -> Result<Vec<bool>, Stop> {
    if p.outputs() != [Domain::Bool] {
        return Err(w.stop("strategy-formula-output", false));
    }
    let mut out = Vec::new();
    for point in pts {
        w.charge(p.nodes().len())?;
        let v = p
            .evaluate(point)
            .map_err(|_| w.stop("strategy-formula-fails", false))?;
        out.push(v[0] == 1);
    }
    Ok(out)
}
/// Finite Boolean circuit operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum GateKind {
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Not,
}
/// One gate's immutable caller-owned definition.
#[derive(Clone, Debug)]
pub struct Gate {
    /// Fixed kind, not caller executable code.
    pub kind: GateKind,
    /// Prior wire positions.
    pub wires: Vec<usize>,
}
/// A circuit with outputs and Boolean primary input domains.
#[derive(Clone, Debug)]
pub struct Circuit {
    /// Number of primary inputs.
    pub inputs: usize,
    /// Operand-first gates.
    pub gates: Vec<Gate>,
    /// Measured output wire positions.
    pub outputs: Vec<usize>,
}
fn circuit_program(c: &Circuit, forced: &BTreeMap<usize, bool>, w: &Work) -> Result<Program, Stop> {
    if c.inputs == 0 || c.inputs > 8 || c.gates.len() > 16 || c.outputs.is_empty() {
        return Err(w.stop("strategy-circuit-shape", false));
    }
    let mut nodes = Vec::new();
    let mut roots = Vec::<u16>::new();
    fn push(nodes: &mut Vec<Op>, op: Op) -> u16 {
        let i = nodes.len() as u16;
        nodes.push(op);
        i
    }
    for i in 0..c.inputs {
        roots.push(push(&mut nodes, Op::Input(i as u16)));
    }
    for (g, gate) in c.gates.iter().enumerate() {
        let arity = if gate.kind == GateKind::Not { 1 } else { 2 };
        if gate.wires.len() != arity || gate.wires.iter().any(|v| *v >= roots.len()) {
            return Err(w.stop("strategy-gate-wires", false));
        }
        let a = roots[gate.wires[0]];
        let b = *gate.wires.get(1).map(|i| &roots[*i]).unwrap_or(&a);
        let id = if let Some(value) = forced.get(&g) {
            push(&mut nodes, Op::Bool(*value))
        } else {
            match gate.kind {
                GateKind::And => push(&mut nodes, Op::And(a, b)),
                GateKind::Not => push(&mut nodes, Op::Not(a)),
                GateKind::Xor => {
                    let eq = push(&mut nodes, Op::Eq(a, b));
                    push(&mut nodes, Op::Not(eq))
                }
                GateKind::Nand => {
                    let and = push(&mut nodes, Op::And(a, b));
                    push(&mut nodes, Op::Not(and))
                }
                GateKind::Or | GateKind::Nor => {
                    let na = push(&mut nodes, Op::Not(a));
                    let nb = push(&mut nodes, Op::Not(b));
                    let both = push(&mut nodes, Op::And(na, nb));
                    if gate.kind == GateKind::Or {
                        push(&mut nodes, Op::Not(both))
                    } else {
                        both
                    }
                }
            }
        };
        roots.push(id);
    }
    if c.outputs.iter().any(|o| *o >= roots.len()) {
        return Err(w.stop("strategy-circuit-output", false));
    }
    Program::try_new(
        vec![Domain::Bool; c.inputs],
        vec![Domain::Bool; roots.len()],
        nodes,
        roots,
    )
    .map_err(|_| w.stop("strategy-circuit-admission", false))
}
/// One proposed inclusion-minimal explanation and all allowed wire vectors.
#[derive(Clone, Debug)]
pub struct Diagnosis {
    /// Gate indices, in order.
    pub gates: Vec<usize>,
    /// Every allowed forced-output world.
    pub worlds: Vec<Vec<i128>>,
}
/// Proposed split, including overlapping prediction buckets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Probe {
    /// Internal wire position.
    pub wire: usize,
    /// Number of candidates that can yield zero.
    pub zero: usize,
    /// Number that can yield one.
    pub one: usize,
}
/// Diagnostics conditioned on supplied measurements; no actual fault claim.
#[derive(Clone, Debug)]
pub struct DiagnosisReport {
    /// Proposed explanations.
    pub diagnoses: Vec<Diagnosis>,
    /// Next proposed measurement.
    pub probe: Option<Probe>,
    /// Declared macro-steps actually performed.
    pub trace: Vec<Step>,
    /// Charged work.
    pub work: u64,
}
/// Enumerate weak-fault explanations then choose the minimax internal probe.
pub fn diagnose(
    c: &Circuit,
    vector: &[i128],
    observed: &BTreeMap<usize, i128>,
    max_faults: usize,
    l: Limits,
) -> Result<DiagnosisReport, Stop> {
    let mut w = Work::new(l.work);
    if vector.len() != c.inputs || vector.iter().any(|v| *v != 0 && *v != 1) || max_faults > 2 {
        return Err(w.stop("strategy-diagnosis-input", false));
    }
    let healthy = circuit_program(c, &BTreeMap::new(), &w)?;
    if observed
        .iter()
        .any(|(wire, value)| *wire >= c.inputs + c.gates.len() || (*value != 0 && *value != 1))
    {
        return Err(w.stop("strategy-observation", false));
    }
    w.record(Step::Observe)?;
    w.record(Step::Explain)?;
    let mut diagnoses: Vec<Diagnosis> = Vec::new();
    let all = 1usize << c.gates.len();
    let mut examined = 0u64;
    let mut retained = 0u128;
    for size in 0..=max_faults {
        for bits in 0..all {
            w.charge(1)?;
            if bits.count_ones() as usize != size {
                continue;
            }
            let gates = (0..c.gates.len())
                .filter(|g| bits & (1 << g) != 0)
                .collect::<Vec<_>>();
            if diagnoses
                .iter()
                .any(|d| d.gates.iter().all(|g| gates.contains(g)))
            {
                continue;
            }
            let mut worlds = Vec::new();
            for choices in 0..(1usize << size) {
                w.charge(healthy.nodes().len() + observed.len())?;
                examined = examined.saturating_add(1);
                if examined > l.worlds {
                    return Err(w.stop("strategy-diagnosis-worlds", true));
                }
                let next_cells =
                    retained + (worlds.len() as u128 + 1) * (c.inputs + c.gates.len()) as u128;
                if next_cells > u128::from(l.cells) {
                    return Err(w.stop("strategy-diagnosis-cells", true));
                }
                let forced = gates
                    .iter()
                    .enumerate()
                    .map(|(i, g)| (*g, choices & (1 << i) != 0))
                    .collect();
                let program = circuit_program(c, &forced, &w)?;
                let values = program
                    .evaluate(vector)
                    .map_err(|_| w.stop("strategy-circuit-evaluation", false))?;
                if observed.iter().all(|(wire, value)| values[*wire] == *value) {
                    worlds.push(values);
                }
            }
            worlds.sort();
            if !worlds.is_empty() {
                if diagnoses.len() >= l.items {
                    return Err(w.stop("strategy-diagnosis-items", true));
                }
                retained += worlds.len() as u128 * (c.inputs + c.gates.len()) as u128;
                diagnoses.push(Diagnosis { gates, worlds });
            }
        }
    }
    let mut best: Option<((usize, usize, usize), Probe)> = None;
    if diagnoses.len() > 1 {
        for wire in c.inputs..c.inputs + c.gates.len() {
            if c.outputs.contains(&wire) || observed.contains_key(&wire) {
                continue;
            }
            w.charge(diagnoses.len())?;
            let zero = diagnoses
                .iter()
                .filter(|d| d.worlds.iter().any(|row| row[wire] == 0))
                .count();
            let one = diagnoses
                .iter()
                .filter(|d| d.worlds.iter().any(|row| row[wire] == 1))
                .count();
            let key = (zero.max(one), zero * zero + one * one, wire);
            if best.as_ref().is_none_or(|(old, _)| key < *old) {
                best = Some((key, Probe { wire, zero, one }));
            }
        }
    }
    let probe = best
        .filter(|(key, _)| key.0 < diagnoses.len())
        .map(|(_, p)| p);
    if probe.is_some() {
        w.record(Step::ChooseProbe)?;
    } else {
        w.record(Step::Isolate)?;
    }
    Ok(DiagnosisReport {
        diagnoses,
        probe,
        trace: w.trace().to_vec(),
        work: w.used(),
    })
}
/// A model's listed alternatives for one statement.
#[derive(Clone, Debug)]
pub struct Reading {
    /// Statement identifier.
    pub id: alloc::string::String,
    /// Finite Boolean formulas, preferred first.
    pub alternatives: Vec<Program>,
}
/// Finite question kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(missing_docs)]
pub enum Query {
    Count,
    Must,
    Possible,
}
/// A separating world to pass through the existing owner confirmation gate.
#[derive(Clone, Debug)]
pub struct ReadingQuestion {
    /// Statement index (including query for must/possible).
    pub statement: usize,
    /// First separating tuple ordinal.
    pub ordinal: usize,
    /// Input world.
    pub world: Vec<i128>,
    /// Surviving alternative indices for a true answer.
    pub holds: Vec<usize>,
    /// Surviving indices for a false answer.
    pub does_not_hold: Vec<usize>,
}
/// Conditional answers and an optional owner question, never owner intent.
#[derive(Clone, Debug)]
pub struct ReadingsReport {
    /// Deduplicated indices in each offered list.
    pub distinct: Vec<Vec<usize>>,
    /// Answers under every surviving combination.
    pub answers: Vec<(Vec<usize>, usize)>,
    /// Ask only when answers differ.
    pub question: Option<ReadingQuestion>,
    /// Macro-step trace.
    pub trace: Vec<Step>,
    /// Charged work.
    pub work: u64,
}
/// Enumerate all reading combinations over finite declared worlds.
/// No owner answers are accepted by this pure API; the shell filters the lists
/// only after a fresh question is answered on the existing owner channel.
pub fn readings(
    background: &Program,
    groups: &[Reading],
    query: Query,
    l: Limits,
) -> Result<ReadingsReport, Stop> {
    let mut w = Work::new(l.work);
    let pts = points(background.inputs(), l, &mut w)?;
    if groups.is_empty()
        || groups.len() > 16
        || groups.iter().any(|g| g.alternatives.is_empty())
        || groups.iter().map(|g| &g.id).collect::<BTreeSet<_>>().len() != groups.len()
    {
        return Err(w.stop("strategy-reading-shape", false));
    }
    let count = groups.iter().map(|g| g.alternatives.len()).sum::<usize>();
    let table_cells =
        (count as u128 * 2 + background.inputs().len() as u128 * 2 + 3) * pts.len() as u128;
    if count > l.items || table_cells > u128::from(l.cells) {
        return Err(w.stop("strategy-reading-cells", true));
    }
    w.record(Step::Formalize)?;
    let bg = bool_table(background, &pts, &mut w)?;
    w.record(Step::Distinct)?;
    let mut tables = Vec::new();
    let mut distinct = Vec::new();
    for group in groups {
        let mut masks: Vec<Vec<bool>> = Vec::new();
        let mut kept = Vec::new();
        for (index, p) in group.alternatives.iter().enumerate() {
            if p.inputs() != background.inputs() {
                return Err(w.stop("strategy-reading-abi", false));
            }
            let mask = bool_table(p, &pts, &mut w)?
                .into_iter()
                .zip(&bg)
                .map(|(a, b)| a && *b)
                .collect::<Vec<_>>();
            w.charge(masks.len() * pts.len())?;
            if !masks.contains(&mask) {
                masks.push(mask);
                kept.push(index);
            }
        }
        tables.push(masks);
        distinct.push(kept);
    }
    w.record(Step::SolveAll)?;
    let combinations = distinct
        .iter()
        .fold(1usize, |n, g| n.saturating_mul(g.len()));
    if combinations > 4096
        || combinations > l.items.saturating_mul(l.items)
        || table_cells + combinations as u128 * (groups.len() + 1) as u128 > u128::from(l.cells)
    {
        return Err(w.stop("strategy-reading-combinations", true));
    }
    let mut answers = Vec::new();
    for ordinal in 0..combinations {
        let mut n = ordinal;
        let mut combo = vec![0; groups.len()];
        for i in (0..groups.len()).rev() {
            combo[i] = n % distinct[i].len();
            n /= distinct[i].len();
        }
        w.charge(pts.len() * groups.len())?;
        let statements = if query == Query::Count {
            groups.len()
        } else {
            groups.len() - 1
        };
        let mut models = bg.clone();
        for i in 0..statements {
            for (j, m) in models.iter_mut().enumerate() {
                *m &= tables[i][combo[i]][j];
            }
        }
        let answer = match query {
            Query::Count => models.iter().filter(|m| **m).count(),
            Query::Must => usize::from(
                models.iter().any(|m| *m)
                    && models
                        .iter()
                        .enumerate()
                        .all(|(j, m)| !*m || tables[groups.len() - 1][combo[groups.len() - 1]][j]),
            ),
            Query::Possible => usize::from(
                models
                    .iter()
                    .enumerate()
                    .any(|(j, m)| *m && tables[groups.len() - 1][combo[groups.len() - 1]][j]),
            ),
        };
        answers.push((combo, answer));
    }
    let spread = answers.iter().map(|(_, a)| *a).collect::<BTreeSet<_>>();
    let mut question = None;
    if spread.len() > 1 {
        let mut best: Option<(usize, usize)> = None;
        for i in 0..groups.len() {
            w.charge(answers.len() * distinct[i].len())?;
            let sets = (0..distinct[i].len())
                .map(|k| {
                    answers
                        .iter()
                        .filter(|(c, _)| c[i] == k)
                        .map(|(_, a)| *a)
                        .collect::<BTreeSet<_>>()
                })
                .collect::<BTreeSet<_>>();
            if sets.len() > 1 && best.is_none_or(|(old, _)| sets.len() > old) {
                best = Some((sets.len(), i));
            }
        }
        if let Some((_, i)) = best {
            let mut separating = None;
            for a in 0..tables[i].len() {
                for b in a + 1..tables[i].len() {
                    w.charge(pts.len())?;
                    if let Some(j) = tables[i][a]
                        .iter()
                        .zip(&tables[i][b])
                        .position(|(a, b)| a != b)
                    {
                        separating = Some(j);
                        break;
                    }
                }
                if separating.is_some() {
                    break;
                }
            }
            if let Some(j) = separating {
                let holds = (0..tables[i].len())
                    .filter(|k| tables[i][*k][j])
                    .map(|k| distinct[i][k])
                    .collect();
                let does_not_hold = (0..tables[i].len())
                    .filter(|k| !tables[i][*k][j])
                    .map(|k| distinct[i][k])
                    .collect();
                question = Some(ReadingQuestion {
                    statement: i,
                    ordinal: j,
                    world: pts[j].clone(),
                    holds,
                    does_not_hold,
                });
            }
        }
    } else {
        w.record(Step::Decide)?;
    }
    Ok(ReadingsReport {
        distinct,
        answers,
        question,
        trace: w.trace().to_vec(),
        work: w.used(),
    })
}
