//! LLM-generated, bounded vocabulary enumeration from examples.
//!
//! Counts, uniqueness, conflicts and partitions are proposals. Only the
//! existing library evaluator can confirm the lowered program's examples.
//! Pools are exclusively owned invocation scratch; no cache or alias escapes.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Error, Op as Instruction, Program};

use crate::Refusal;
use crate::expr::{self, Expr, Kind, Op, Vocabulary};
use crate::lower::lower_outputs;
use crate::subject::{columns, domain_size, tuples};

/// Fixed, checked operations. No user-supplied primitive or multiplication.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Operation {
    /// Checked integer addition.
    Add,
    /// Checked integer subtraction.
    Sub,
    /// Integer less than.
    Lt,
    /// Integer less than or equal.
    Le,
    /// Integer equality.
    Eq,
    /// Integer inequality.
    Ne,
    /// Boolean conjunction.
    And,
    /// Boolean disjunction.
    Or,
    /// Boolean negation.
    Not,
    /// Boolean condition and integer branches, all eager.
    Select,
    /// Standard minimum macro, cost one plus its arguments.
    Min,
    /// Standard maximum macro, cost one plus its arguments.
    Max,
    /// Standard absolute-value macro; its subtraction is eager.
    Abs,
}

impl Operation {
    fn arguments(self) -> &'static [Kind] {
        use Kind::{Bool, Int};
        match self {
            Self::Not => &[Bool],
            Self::Abs => &[Int],
            Self::And | Self::Or => &[Bool, Bool],
            Self::Select => &[Bool, Int, Int],
            _ => &[Int, Int],
        }
    }

    fn kind(self) -> Kind {
        match self {
            Self::Lt | Self::Le | Self::Eq | Self::Ne | Self::And | Self::Or | Self::Not => {
                Kind::Bool
            }
            _ => Kind::Int,
        }
    }

    fn commutative(self) -> bool {
        matches!(
            self,
            Self::Add | Self::Eq | Self::Ne | Self::And | Self::Or | Self::Min | Self::Max
        )
    }

    fn probe(self) -> Result<Program, Stop> {
        let inputs = self.arguments().iter().map(|k| full_domain(*k)).collect();
        let (nodes, root) = match self {
            Self::Not => (vec![Instruction::Input(0), Instruction::Not(0)], 1),
            Self::Abs => (
                vec![
                    Instruction::Input(0),
                    Instruction::Int(0),
                    Instruction::Lt(0, 1),
                    Instruction::Sub(1, 0),
                    Instruction::Select(2, 3, 0),
                ],
                4,
            ),
            Self::Select => (
                vec![
                    Instruction::Input(0),
                    Instruction::Input(1),
                    Instruction::Input(2),
                    Instruction::Select(0, 1, 2),
                ],
                3,
            ),
            other => {
                let mut nodes = vec![Instruction::Input(0), Instruction::Input(1)];
                match other {
                    Self::Add => nodes.push(Instruction::Add(0, 1)),
                    Self::Sub => nodes.push(Instruction::Sub(0, 1)),
                    Self::Lt => nodes.push(Instruction::Lt(0, 1)),
                    Self::Eq => nodes.push(Instruction::Eq(0, 1)),
                    Self::And => nodes.push(Instruction::And(0, 1)),
                    Self::Or => nodes.push(Instruction::Select(0, 0, 1)),
                    Self::Le => {
                        nodes.push(Instruction::Lt(1, 0));
                        nodes.push(Instruction::Not(2));
                    }
                    Self::Ne => {
                        nodes.push(Instruction::Eq(0, 1));
                        nodes.push(Instruction::Not(2));
                    }
                    Self::Min | Self::Max => {
                        nodes.push(Instruction::Lt(0, 1));
                        nodes.push(if other == Self::Min {
                            Instruction::Select(2, 0, 1)
                        } else {
                            Instruction::Select(2, 1, 0)
                        });
                    }
                    _ => return Err(Stop::Refused("learn-operation".into())),
                }
                let root = u16::try_from(nodes.len() - 1)
                    .map_err(|_| Stop::Refused("learn-probe".into()))?;
                (nodes, root)
            }
        };
        Program::try_new(inputs, vec![full_domain(self.kind())], nodes, vec![root])
            .map_err(|e| Stop::Refused(format!("learn-probe:{e}")))
    }
}

fn full_domain(kind: Kind) -> Domain {
    match kind {
        Kind::Bool => Domain::Bool,
        Kind::Int => Domain::Int {
            min: i64::MIN,
            max: i64::MAX,
        },
    }
}

/// One exact observation. Failure tags cannot collide with integer values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Observation {
    /// Successful scalar result.
    Value(i64),
    /// Eager checked-arithmetic failure, including an unselected argument.
    Arithmetic,
    /// A root value outside this output's declared domain.
    OutputDomain,
}

/// An owner-declared input/output example, in program order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Example {
    /// Complete input tuple.
    pub input: Vec<i64>,
    /// Complete successful output tuple.
    pub output: Vec<i64>,
}

/// Complete owned declaration, independent of an original or hidden core.
#[derive(Clone, Debug)]
pub struct Specification {
    /// Input names, in product order.
    pub names: Vec<String>,
    /// Finite input domains.
    pub inputs: Vec<Domain>,
    /// Output domains.
    pub outputs: Vec<Domain>,
    /// Declared integer leaves; Boolean leaves come from Boolean inputs.
    pub constants: Vec<i64>,
    /// Declared operation order breaks equal-size ties deterministically.
    pub operations: Vec<Operation>,
    /// Maximum vocabulary expression size, including leaves.
    pub max_size: usize,
    /// Every example is required; noisy acceptance is not supported.
    pub examples: Vec<Example>,
}

impl Specification {
    /// Validate shape before allocating any domain table.
    pub fn vocabulary(&self) -> Result<Vocabulary, Stop> {
        if !(1..=8).contains(&self.inputs.len())
            || !(1..=8).contains(&self.outputs.len())
            || self.names.len() != self.inputs.len()
            || !(1..=12).contains(&self.max_size)
            || self.constants.len() > 32
            || self.examples.len() > 64
            || self.operations.len() > 14
        {
            return Err(Stop::Refused("learn-declaration-bounds".into()));
        }
        if self.operations.iter().collect::<BTreeSet<_>>().len() != self.operations.len()
            || self.constants.iter().collect::<BTreeSet<_>>().len() != self.constants.len()
        {
            return Err(Stop::Refused("learn-vocabulary-duplicate".into()));
        }
        for name in &self.names {
            if name.len() > 32
                || !name.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || ["min", "max", "abs"].contains(&name.as_str())
            {
                return Err(Stop::Refused("learn-input-name".into()));
            }
        }
        for d in self.inputs.iter().chain(&self.outputs) {
            if Kind::of(*d).is_none() || d.bounds().0 > d.bounds().1 {
                return Err(Stop::Refused("learn-domain".into()));
            }
        }
        let mut points = BTreeSet::new();
        for e in &self.examples {
            if e.input.len() != self.inputs.len()
                || e.output.len() != self.outputs.len()
                || !e
                    .input
                    .iter()
                    .zip(&self.inputs)
                    .all(|(v, d)| d.contains(*v))
                || !e
                    .output
                    .iter()
                    .zip(&self.outputs)
                    .all(|(v, d)| d.contains(*v))
            {
                return Err(Stop::Refused("learn-example-domain".into()));
            }
            if !points.insert(&e.input) {
                return Err(Stop::Refused("learn-example-duplicate".into()));
            }
        }
        Vocabulary::new(
            self.names.clone(),
            self.inputs.clone(),
            self.outputs.clone(),
            &BTreeMap::new(),
        )
        .map_err(Stop::from)
    }
}

/// Deterministic cumulative caps. The CLI also bounds each contained stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Candidate instruction/cell visits and later comparisons.
    pub work: u64,
    /// Cartesian input domain cap.
    pub tuples: u64,
    /// Raw meanings per kind, before example filtering.
    pub meanings: usize,
    /// Cumulative allocated table cells, including scratch and index copies.
    pub cells: u64,
    /// Exact conflict subsets visited.
    pub conflict_subsets: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            work: 2_000_000,
            tuples: 4096,
            meanings: 4096,
            cells: 1_000_000,
            conflict_subsets: 10_000,
        }
    }
}

/// No prefix result survives either stop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stop {
    /// Invalid declaration, unsupported type, or failed lowering consistency.
    Refused(String),
    /// At least one search or checking phase did not finish.
    Inconclusive(String),
}
impl From<Refusal> for Stop {
    fn from(e: Refusal) -> Self {
        Self::Refused(e.0)
    }
}

/// Work already charged, including on a later stop.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    /// Charged instruction visits, cells and comparisons.
    pub work: u64,
    /// Charged cumulative table cells.
    pub cells: u64,
    /// Charged conflict subset visits.
    pub conflict_subsets: u64,
}

/// Caller-owned accounting; invocation scratch only, never a decision handle.
#[derive(Clone, Debug)]
pub struct Budget {
    limits: Limits,
    usage: Usage,
}
impl Budget {
    /// Copies immutable policy; subsequent changes to the caller's copy have no effect.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            usage: Usage::default(),
        }
    }
    /// Retained measured work on success, refusal or retry.
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.usage
    }
    /// Copied invocation policy for bounded-search diagnostics. No alias escapes.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }
    fn spend(&mut self, work: u64) -> Result<(), Stop> {
        let next = self
            .usage
            .work
            .checked_add(work)
            .ok_or_else(|| Stop::Inconclusive("learn-work-budget".into()))?;
        if next > self.limits.work {
            return Err(Stop::Inconclusive("learn-work-budget".into()));
        }
        self.usage.work = next;
        Ok(())
    }
    fn allocate(&mut self, cells: u64) -> Result<(), Stop> {
        let next = self
            .usage
            .cells
            .checked_add(cells)
            .ok_or_else(|| Stop::Inconclusive("learn-cell-budget".into()))?;
        if next > self.limits.cells {
            return Err(Stop::Inconclusive("learn-cell-budget".into()));
        }
        self.usage.cells = next;
        Ok(())
    }
}

#[derive(Clone, Debug)]
enum Term {
    Input(usize),
    Constant(i64),
    Apply(Operation, Vec<usize>),
}
struct Meaning {
    term: Term,
    size: usize,
    table: Vec<Observation>,
}
struct Pool {
    entries: Vec<Meaning>,
    seen: [BTreeMap<Vec<Observation>, usize>; 2],
    sizes: [Vec<Vec<usize>>; 2],
}
fn kind_index(k: Kind) -> usize {
    if k == Kind::Int { 0 } else { 1 }
}

impl Pool {
    fn new(max_size: usize) -> Self {
        Self {
            entries: Vec::new(),
            seen: [BTreeMap::new(), BTreeMap::new()],
            sizes: [
                vec![Vec::new(); max_size + 1],
                vec![Vec::new(); max_size + 1],
            ],
        }
    }
    fn add(
        &mut self,
        kind: Kind,
        size: usize,
        term: Term,
        table: Vec<Observation>,
        budget: &mut Budget,
    ) -> Result<(), Stop> {
        let k = kind_index(kind);
        budget.spend(table.len() as u64)?;
        if self.seen[k].contains_key(&table) {
            return Ok(());
        }
        if self.seen[k].len() >= budget.limits.meanings {
            return Err(Stop::Inconclusive("learn-meaning-budget".into()));
        }
        budget.allocate((table.len() as u64).saturating_mul(2))?;
        let id = self.entries.len();
        self.seen[k].insert(table.clone(), id);
        self.sizes[k][size].push(id);
        self.entries.push(Meaning { term, size, table });
        Ok(())
    }

    fn emit(
        &mut self,
        op: Operation,
        sizes: &[usize],
        size: usize,
        cells: usize,
        budget: &mut Budget,
    ) -> Result<(), Stop> {
        let choices: Vec<Vec<usize>> = op
            .arguments()
            .iter()
            .zip(sizes)
            .map(|(k, s)| self.sizes[kind_index(*k)][*s].clone())
            .collect();
        if choices.iter().any(Vec::is_empty) {
            return Ok(());
        }
        let probe = op.probe()?;
        let mut positions = vec![0; choices.len()];
        loop {
            budget.spend(1)?;
            let args: Vec<usize> = choices.iter().zip(&positions).map(|(c, p)| c[*p]).collect();
            if !(op.commutative() && args.len() == 2 && sizes[0] == sizes[1] && args[0] > args[1]) {
                budget.spend((cells as u64).saturating_mul(probe.nodes().len() as u64))?;
                budget.allocate(cells as u64)?;
                let mut table = Vec::with_capacity(cells);
                for j in 0..cells {
                    let values: Option<Vec<i64>> = args
                        .iter()
                        .map(|id| match self.entries[*id].table[j] {
                            Observation::Value(v) => Some(v),
                            _ => None,
                        })
                        .collect();
                    let observation = if let Some(values) = values {
                        match probe.evaluate(&values) {
                            Ok(v) if v.len() == 1 => Observation::Value(v[0]),
                            Err(Error::Arithmetic) => Observation::Arithmetic,
                            other => {
                                return Err(Stop::Refused(format!(
                                    "learn-probe-evaluation:{other:?}"
                                )));
                            }
                        }
                    } else {
                        Observation::Arithmetic
                    };
                    table.push(observation);
                }
                self.add(op.kind(), size, Term::Apply(op, args), table, budget)?;
            }
            let Some(k) = (0..positions.len())
                .rev()
                .find(|k| positions[*k] + 1 < choices[*k].len())
            else {
                break;
            };
            positions[k] += 1;
            for p in &mut positions[k + 1..] {
                *p = 0;
            }
        }
        Ok(())
    }

    fn expression(
        &self,
        id: usize,
        names: &[String],
        budget: &mut Budget,
    ) -> Result<(Expr, String), Stop> {
        budget.spend(1)?;
        let (node, text) = match &self.entries[id].term {
            Term::Input(k) => (Expr::Var(names[*k].clone()), names[*k].clone()),
            Term::Constant(v) => (Expr::Int(*v), v.to_string()),
            Term::Apply(op, args) => {
                let parts = args
                    .iter()
                    .map(|k| self.expression(*k, names, budget))
                    .collect::<Result<Vec<_>, _>>()?;
                let a = parts[0].0.clone();
                let show_a = &parts[0].1;
                match op {
                    Operation::Not => (Expr::negation(a), format!("!{show_a}")),
                    Operation::Abs => (
                        Expr::select(
                            Expr::bin(Op::Lt, a.clone(), Expr::Int(0)),
                            Expr::bin(Op::Sub, Expr::Int(0), a.clone()),
                            a,
                        ),
                        format!("abs({show_a})"),
                    ),
                    Operation::Select => (
                        Expr::select(a, parts[1].0.clone(), parts[2].0.clone()),
                        format!("({show_a} ? {} : {})", parts[1].1, parts[2].1),
                    ),
                    Operation::Min | Operation::Max => {
                        let b = parts[1].0.clone();
                        let c = Expr::bin(Op::Lt, a.clone(), b.clone());
                        let node = if *op == Operation::Min {
                            Expr::select(c, a, b)
                        } else {
                            Expr::select(c, b, a)
                        };
                        (
                            node,
                            format!(
                                "{}({show_a}, {})",
                                if *op == Operation::Min { "min" } else { "max" },
                                parts[1].1
                            ),
                        )
                    }
                    _ => {
                        let symbol = match op {
                            Operation::Add => Op::Add,
                            Operation::Sub => Op::Sub,
                            Operation::Lt => Op::Lt,
                            Operation::Le => Op::Le,
                            Operation::Eq => Op::Eq,
                            Operation::Ne => Op::Ne,
                            Operation::And => Op::And,
                            Operation::Or => Op::Or,
                            _ => return Err(Stop::Refused("learn-operation".into())),
                        };
                        (
                            Expr::bin(symbol, a, parts[1].0.clone()),
                            format!("({show_a} {} {})", symbol.symbol(), parts[1].1),
                        )
                    }
                }
            }
        };
        if expr::size(&node) > expr::MAX_NODES || expr::depth(&node) > expr::MAX_DEPTH {
            return Err(Stop::Inconclusive(
                "learn-expanded-expression-budget".into(),
            ));
        }
        Ok((node, text))
    }
}

/// Proposal outcome; none establishes uniqueness outside the bounded vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    /// One root meaning per output fits every example.
    Determined,
    /// Several meanings remain and an unanswered input distinguishes them.
    Question,
    /// No program in this bounded vocabulary fits every example.
    OutsideVocabulary,
    /// Several meanings remain but no unanswered input distinguishes them.
    Ambiguous,
}

/// One output's bucket. Failures are observations, never owner answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bucket {
    /// Value or exact failure category.
    pub observation: Observation,
    /// Proposed remaining meanings for this output.
    pub remaining: usize,
}
/// Minimax question without materializing a Cartesian product of answers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Question {
    /// Complete input tuple in domain order.
    pub input: Vec<i64>,
    /// Input ordinal, last input fastest.
    pub ordinal: usize,
    /// Ordered buckets per output; the joint partition is their product.
    pub buckets: Vec<Vec<Bucket>>,
    /// Largest joint bucket, computed by checked integer arithmetic.
    pub worst: u128,
    /// Sum of squared joint buckets.
    pub squares: u128,
}
/// A smallest representative under the declared vocabulary cost.
#[derive(Clone, Debug)]
pub struct Representative {
    /// Vocabulary size, not emitted instruction count.
    pub size: usize,
    /// Compact text with standard macro calls.
    pub text: String,
    /// Expanded, checked-IR expression, still a proposal.
    pub expression: Expr,
}
/// Fully owned result. No table pool, mutable alias or authority handle escapes.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// Proposed status, subject to final exact example checking.
    pub status: Status,
    /// Distinct root meanings per output after example filtering.
    pub counts: Vec<usize>,
    /// Distinct root meanings per output before filtering.
    pub before: Vec<usize>,
    /// Number of new raw integer and Boolean meanings at each vocabulary size.
    pub by_size: Vec<[usize; 2]>,
    /// Proposed minimax input.
    pub question: Option<Question>,
    /// Minimum-cardinality conflicting example indices, in ordinal order.
    pub core: Option<Vec<usize>>,
    /// Smallest fitting representative per output, when every output has a fit.
    pub representatives: Vec<Representative>,
    /// Lowered representative, not yet example-checked by the CLI.
    pub candidate: Option<Program>,
}

fn ordinal(point: &[i64], domains: &[Domain]) -> Result<usize, Stop> {
    let mut n = 0u128;
    for (v, d) in point.iter().zip(domains) {
        n = n
            .checked_mul(domain_size(*d))
            .and_then(|n| {
                n.checked_add(u128::try_from(i128::from(*v) - i128::from(d.bounds().0)).ok()?)
            })
            .ok_or_else(|| Stop::Refused("learn-example-ordinal".into()))?;
    }
    usize::try_from(n).map_err(|_| Stop::Inconclusive("learn-domain-budget".into()))
}

fn minimum_core(
    spec: &Specification,
    tables: &[Vec<Vec<Observation>>],
    budget: &mut Budget,
) -> Result<Vec<usize>, Stop> {
    let mut order = (0..spec.examples.len())
        .map(|k| Ok((ordinal(&spec.examples[k].input, &spec.inputs)?, k)))
        .collect::<Result<Vec<_>, Stop>>()?;
    order.sort();
    for size in 0..=order.len() {
        let mut choice: Vec<usize> = (0..size).collect();
        loop {
            if budget.usage.conflict_subsets >= budget.limits.conflict_subsets {
                return Err(Stop::Inconclusive("learn-conflict-subset-budget".into()));
            }
            budget.spend(1)?;
            budget.usage.conflict_subsets += 1;
            let mut excluded = false;
            for (output, table) in tables.iter().enumerate() {
                let mut fits = false;
                for row in table {
                    budget.spend(size as u64 + 1)?;
                    if choice.iter().all(|i| {
                        row[order[*i].0]
                            == Observation::Value(spec.examples[order[*i].1].output[output])
                    }) {
                        fits = true;
                        break;
                    }
                }
                if !fits {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                return Ok(choice.iter().map(|i| order[*i].1).collect());
            }
            let Some(k) = (0..size)
                .rev()
                .find(|k| choice[*k] < order.len() - size + *k)
            else {
                break;
            };
            choice[k] += 1;
            for j in k + 1..size {
                choice[j] = choice[j - 1] + 1;
            }
        }
    }
    Err(Stop::Refused("learn-conflict-internal".into()))
}

// BEGIN typed strategy corpus producer interface (merge with strategy consumer).
/// Owned full-domain terms for typed case composition; no checker authority.
pub struct StrategyTermsV1 {
    /// Integer terms with full i64 output, in declared size/tie order.
    pub integers: Vec<Program>,
    /// Nontrapping Boolean predicates, in declared size/tie order.
    pub predicates: Vec<Program>,
}

// Invocation-owned enumeration data; consumers move the pool and columns.
struct Enumeration {
    vocab: Vocabulary,
    cells: usize,
    pool: Pool,
    columns: Vec<Vec<i64>>,
}

fn enumerate_for_analysis(spec: &Specification, budget: &mut Budget) -> Result<Enumeration, Stop> {
    budget.spend(
        1 + spec.names.len() as u64
            + spec.inputs.len() as u64
            + spec.outputs.len() as u64
            + spec.constants.len() as u64
            + spec.operations.len() as u64
            + spec
                .examples
                .iter()
                .map(|e| (e.input.len() + e.output.len()) as u64 + 1)
                .sum::<u64>(),
    )?;
    let vocab = spec.vocabulary()?;
    let total = tuples(&spec.inputs);
    if total > u128::from(budget.limits.tuples) {
        return Err(Stop::Inconclusive("learn-domain-budget".into()));
    }
    let cells =
        usize::try_from(total).map_err(|_| Stop::Inconclusive("learn-domain-budget".into()))?;
    budget.spend((cells as u64).saturating_mul(spec.inputs.len() as u64))?;
    budget.allocate((cells as u64).saturating_mul(spec.inputs.len() as u64))?;
    let columns = columns(&spec.inputs, budget.limits.tuples)?;
    let mut pool = Pool::new(spec.max_size);
    for (k, d) in spec.inputs.iter().enumerate() {
        budget.allocate(cells as u64)?;
        pool.add(
            Kind::of(*d).ok_or_else(|| Stop::Refused("learn-domain".into()))?,
            1,
            Term::Input(k),
            columns[k].iter().copied().map(Observation::Value).collect(),
            budget,
        )?;
    }
    for c in &spec.constants {
        budget.allocate(cells as u64)?;
        pool.add(
            Kind::Int,
            1,
            Term::Constant(*c),
            vec![Observation::Value(*c); cells],
            budget,
        )?;
    }
    for size in 2..=spec.max_size {
        for op in &spec.operations {
            budget.spend(1)?;
            match op.arguments().len() {
                1 => pool.emit(*op, &[size - 1], size, cells, budget)?,
                2 => {
                    for a in 1..size - 1 {
                        let b = size - 1 - a;
                        if !op.commutative() || a <= b {
                            pool.emit(*op, &[a, b], size, cells, budget)?;
                        }
                    }
                }
                3 => {
                    for c in 1..size.saturating_sub(2) {
                        for a in 1..size - 1 - c {
                            let b = size - 1 - c - a;
                            pool.emit(*op, &[c, a, b], size, cells, budget)?;
                        }
                    }
                }
                _ => return Err(Stop::Refused("learn-operation-arity".into())),
            }
        }
    }
    Ok(Enumeration {
        vocab,
        cells,
        pool,
        columns,
    })
}

/// Export the same complete bounded meaning pool used by analyze.
/// Stops return no corpus; caller retains Budget::usage(). Programs own their
/// data. Expanded IR node slots are conservatively reserved before lowering.
pub fn strategy_terms_v1(
    spec: &Specification,
    budget: &mut Budget,
) -> Result<StrategyTermsV1, Stop> {
    let pool = enumerate_for_analysis(spec, budget)?.pool;
    let mut integers = Vec::new();
    let mut predicates = Vec::new();
    for kind in [Kind::Int, Kind::Bool] {
        let output = full_domain(kind);
        let constant = if kind == Kind::Bool {
            Instruction::Bool(false)
        } else {
            Instruction::Int(0)
        };
        let base = Program::try_new(spec.inputs.clone(), vec![output], vec![constant], vec![0])
            .map_err(|_| Stop::Refused("learn-strategy-base".into()))?;
        let vocabulary = Vocabulary::new(
            spec.names.clone(),
            spec.inputs.clone(),
            vec![output],
            &BTreeMap::new(),
        )?;
        for size in 1..=spec.max_size {
            for id in &pool.sizes[kind_index(kind)][size] {
                budget.spend(pool.entries[*id].table.len() as u64)?;
                if !pool.entries[*id]
                    .table
                    .iter()
                    .all(|v| matches!(v, Observation::Value(_)))
                {
                    continue;
                }
                budget.allocate(
                    (spec.max_size as u64)
                        .saturating_mul(5)
                        .saturating_add(spec.inputs.len() as u64 * 2 + 3),
                )?;
                let (expression, _) = pool.expression(*id, &spec.names, budget)?;
                let program = lower_outputs(&[expression], &vocabulary, &base)?;
                if kind == Kind::Int {
                    integers.push(program);
                } else {
                    predicates.push(program);
                }
            }
        }
    }
    Ok(StrategyTermsV1 {
        integers,
        predicates,
    })
}
/// Consume the named producer handoff without serialized tables or aliases.
impl From<StrategyTermsV1> for crate::strategy_split::LearnTermsV1 {
    fn from(terms: StrategyTermsV1) -> Self {
        Self {
            integers: terms.integers,
            predicates: terms.predicates,
            complete: true,
        }
    }
}
// END typed strategy corpus producer interface.

/// Complete enumeration, filtering, conflict minimization and lowering share one
/// budget. Errors return no partial count, program or conflict. Uses the common
/// product enumerator and existing evaluator; it does not enumerate sketch holes.
pub fn analyze(spec: &Specification, budget: &mut Budget) -> Result<Analysis, Stop> {
    let Enumeration {
        vocab,
        cells,
        pool,
        columns,
    } = enumerate_for_analysis(spec, budget)?;
    let mut tables = Vec::new();
    let mut representatives = Vec::new();
    let mut fits = Vec::new();
    let mut counts = Vec::new();
    let mut before = Vec::new();
    for (output, d) in spec.outputs.iter().enumerate() {
        let kind = kind_index(Kind::of(*d).ok_or_else(|| Stop::Refused("learn-domain".into()))?);
        let mut seen = BTreeSet::new();
        let mut table = Vec::new();
        let mut ids = Vec::new();
        let mut fit = Vec::new();
        for size in 1..=spec.max_size {
            for id in &pool.sizes[kind][size] {
                budget.spend(cells as u64)?;
                budget.allocate(cells as u64)?;
                let row: Vec<Observation> = pool.entries[*id]
                    .table
                    .iter()
                    .map(|v| match v {
                        Observation::Value(v) if !d.contains(*v) => Observation::OutputDomain,
                        other => *other,
                    })
                    .collect();
                if seen.contains(&row) {
                    continue;
                }
                budget.allocate((cells as u64).saturating_mul(2))?;
                seen.insert(row.clone());
                budget.spend(spec.examples.len() as u64 + 1)?;
                let mut matches = true;
                for e in &spec.examples {
                    if row[ordinal(&e.input, &spec.inputs)?] != Observation::Value(e.output[output])
                    {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    fit.push(table.len());
                }
                table.push(row);
                ids.push(*id);
            }
        }
        before.push(table.len());
        counts.push(fit.len());
        if let Some(k) = fit.first() {
            representatives.push(ids[*k]);
        }
        tables.push(table);
        fits.push(fit);
    }
    let by_size = (1..=spec.max_size)
        .map(|s| [pool.sizes[0][s].len(), pool.sizes[1][s].len()])
        .collect();
    if counts.contains(&0) {
        let core = minimum_core(spec, &tables, budget)?;
        return Ok(Analysis {
            status: Status::OutsideVocabulary,
            counts,
            before,
            by_size,
            question: None,
            core: Some(core),
            representatives: Vec::new(),
            candidate: None,
        });
    }
    let answered: BTreeSet<usize> = spec
        .examples
        .iter()
        .map(|e| ordinal(&e.input, &spec.inputs))
        .collect::<Result<_, _>>()?;
    let mut best: Option<Question> = None;
    if counts.iter().any(|n| *n != 1) {
        for j in 0..cells {
            if answered.contains(&j) {
                continue;
            }
            let mut buckets = Vec::new();
            let (mut worst, mut squares) = (1u128, 1u128);
            let mut split = false;
            for (table, fit) in tables.iter().zip(&fits) {
                budget.spend(fit.len() as u64)?;
                let mut bucket = BTreeMap::<Observation, usize>::new();
                for k in fit {
                    *bucket.entry(table[*k][j]).or_default() += 1;
                }
                split |= bucket.len() > 1;
                let w = bucket.values().copied().max().unwrap_or(0) as u128;
                let s = bucket
                    .values()
                    .map(|n| (*n as u128) * (*n as u128))
                    .sum::<u128>();
                worst = worst
                    .checked_mul(w)
                    .ok_or_else(|| Stop::Inconclusive("learn-question-score-budget".into()))?;
                squares = squares
                    .checked_mul(s)
                    .ok_or_else(|| Stop::Inconclusive("learn-question-score-budget".into()))?;
                buckets.push(
                    bucket
                        .into_iter()
                        .map(|(observation, remaining)| Bucket {
                            observation,
                            remaining,
                        })
                        .collect(),
                );
            }
            if split
                && best
                    .as_ref()
                    .is_none_or(|q| (worst, squares, j) < (q.worst, q.squares, q.ordinal))
            {
                best = Some(Question {
                    input: columns.iter().map(|c| c[j]).collect(),
                    ordinal: j,
                    buckets,
                    worst,
                    squares,
                });
            }
        }
    }
    let status = if counts.iter().all(|n| *n == 1) {
        Status::Determined
    } else if best.is_some() {
        Status::Question
    } else {
        Status::Ambiguous
    };
    let representatives = representatives
        .iter()
        .map(|id| {
            let (expression, text) = pool.expression(*id, &spec.names, budget)?;
            Ok(Representative {
                size: pool.entries[*id].size,
                text,
                expression,
            })
        })
        .collect::<Result<Vec<_>, Stop>>()?;
    let expanded_nodes: usize = representatives
        .iter()
        .map(|r| expr::size(&r.expression))
        .sum();
    if expanded_nodes > zeno_fcis_synthesis::finite::MAX_NODES {
        return Err(Stop::Inconclusive(
            "learn-lowered-program-size-budget".into(),
        ));
    }
    budget.spend(expanded_nodes as u64)?;
    // ABI-only seed, never a target/reference. `keep` is absent from this grammar.
    let seed_nodes: Vec<Instruction> = spec
        .outputs
        .iter()
        .map(|d| match d {
            Domain::Bool => Instruction::Bool(false),
            _ => Instruction::Int(d.bounds().0),
        })
        .collect();
    let seed_roots = (0..spec.outputs.len())
        .map(|k| {
            u16::try_from(k).map_err(|_| Stop::Inconclusive("learn-output-size-budget".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let seed = Program::try_new(
        spec.inputs.clone(),
        spec.outputs.clone(),
        seed_nodes,
        seed_roots,
    )
    .map_err(|e| Stop::Refused(format!("learn-abi:{e}")))?;
    let candidate = lower_outputs(
        &representatives
            .iter()
            .map(|r| r.expression.clone())
            .collect::<Vec<_>>(),
        &vocab,
        &seed,
    )?;
    Ok(Analysis {
        status,
        counts,
        before,
        by_size,
        question: best,
        core: None,
        representatives,
        candidate: Some(candidate),
    })
}

/// Result of checking the LOWERED joint program at every required example.
/// Enumeration tables never supply this verdict. No mutable result alias escapes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExampleCheck {
    /// Every provided example reproduced exactly; zero examples is explicitly vacuous.
    Reproduced {
        /// Number actually checked.
        checked: usize,
    },
    /// First mismatch or eager failure, after checking earlier examples.
    Mismatch {
        /// Failing example index.
        example: usize,
        /// Number evaluated, including the failing example.
        checked: usize,
        /// Successful values, or no values on library failure.
        actual: Option<Vec<i64>>,
    },
}

/// Existing exact evaluator decides example agreement with measured work. This
/// checked wrapper creates no replacement interpreter, acceptance or publication
/// authority. Program ABI must match before any example is evaluated.
pub fn check_examples(
    spec: &Specification,
    candidate: &Program,
    budget: &mut Budget,
) -> Result<ExampleCheck, Stop> {
    budget.spend(
        1 + spec.inputs.len() as u64
            + spec.outputs.len() as u64
            + spec
                .examples
                .iter()
                .map(|e| (e.input.len() + e.output.len()) as u64 + 1)
                .sum::<u64>(),
    )?;
    let _ = spec.vocabulary()?;
    if candidate.inputs() != spec.inputs || candidate.outputs() != spec.outputs {
        return Err(Stop::Refused("learn-candidate-abi".into()));
    }
    for (k, e) in spec.examples.iter().enumerate() {
        budget.spend(candidate.nodes().len() as u64 + candidate.outputs().len() as u64 + 1)?;
        let actual = candidate.evaluate(&e.input).ok();
        if actual.as_ref() != Some(&e.output) {
            return Ok(ExampleCheck::Mismatch {
                example: k,
                checked: k + 1,
                actual,
            });
        }
    }
    Ok(ExampleCheck::Reproduced {
        checked: spec.examples.len(),
    })
}

#[cfg(test)]
#[path = "learn_tests.rs"]
mod tests;

// BEGIN strategy corpus producer regressions (merge dependency).
#[cfg(test)]
mod strategy_corpus_tests {
    use super::*;
    fn declaration() -> Specification {
        Specification {
            names: vec!["x".into()],
            inputs: vec![Domain::Int { min: 0, max: 3 }],
            outputs: vec![Domain::Int {
                min: i64::MIN,
                max: i64::MAX,
            }],
            constants: vec![0, 2, 7],
            operations: vec![Operation::Lt, Operation::Eq],
            max_size: 3,
            examples: vec![],
        }
    }
    #[test]
    fn learn_strategy_export_owns_nontrapping_full_domain_programs() {
        let mut spec = declaration();
        let mut budget = Budget::new(Limits::default());
        let corpus = strategy_terms_v1(&spec, &mut budget).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(!corpus.integers.is_empty());
        assert!(!corpus.predicates.is_empty());
        for p in corpus.integers.iter().chain(&corpus.predicates) {
            assert_eq!(p.inputs(), spec.inputs);
            for x in 0..=3 {
                assert!(p.evaluate(&[x]).is_ok());
            }
        }
        spec.inputs.clear();
        spec.constants.clear();
        assert_eq!(
            corpus.integers[0].inputs(),
            [Domain::Int { min: 0, max: 3 }]
        );
        assert!(budget.usage().work > 0);
        assert!(budget.usage().cells > 0);
    }
    #[test]
    fn learn_strategy_export_zero_or_late_budget_returns_no_corpus() {
        let spec = declaration();
        let mut stopped = Budget::new(Limits {
            work: 0,
            ..Limits::default()
        });
        assert!(matches!(
            strategy_terms_v1(&spec, &mut stopped),
            Err(Stop::Inconclusive(_))
        ));
        let mut prefix = Budget::new(Limits::default());
        let _ = enumerate_for_analysis(&spec, &mut prefix).unwrap_or_else(|e| panic!("{e:?}"));
        let mut stopped = Budget::new(Limits {
            work: prefix.usage().work,
            ..Limits::default()
        });
        assert!(matches!(
            strategy_terms_v1(&spec, &mut stopped),
            Err(Stop::Inconclusive(_))
        ));
        assert_eq!(stopped.usage().work, prefix.usage().work);
    }
    #[test]
    fn learn_strategy_real_producer_supplies_the_case_consumer() {
        let spec = declaration();
        let terms = strategy_terms_v1(&spec, &mut Budget::new(Limits::default()))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let corpus: crate::strategy_split::LearnTermsV1 = terms.into();
        let original = Program::try_new(
            spec.inputs.clone(),
            spec.outputs.clone(),
            vec![
                Instruction::Input(0),
                Instruction::Int(2),
                Instruction::Lt(0, 1),
                Instruction::Int(7),
                Instruction::Select(2, 3, 0),
            ],
            vec![4],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        let run = crate::strategy_split::split(
            &original,
            &corpus,
            crate::strategy_split::Config {
                initial: 2,
                spot_checks: 1,
                ..crate::strategy_split::Config::default()
            },
            crate::strategy_questions::Limits {
                worlds: 4096,
                items: 128,
                cells: 1_000_000,
                work: 10_000_000,
            },
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        for x in 0..=3 {
            assert_eq!(original.evaluate(&[x]), run.candidate.evaluate(&[x]));
        }
    }
    #[test]
    fn learn_strategy_export_reuses_the_analysis_pool_without_filtering_examples() {
        let spec = declaration();
        let corpus = strategy_terms_v1(&spec, &mut Budget::new(Limits::default()))
            .unwrap_or_else(|e| panic!("{e:?}"));
        let analysis =
            analyze(&spec, &mut Budget::new(Limits::default())).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(corpus.integers.len(), analysis.before[0]);
    }
}
// END strategy corpus producer regressions.
