//! LLM-generated requirements-only finite search. Counts, partitions and
//! conflicts are proposals; this module has no acceptance authority. It reads
//! no existing core. The CLI checks a singleton with existing exact checkers.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Op as Instruction, Program};

use crate::Refusal;
use crate::expr::{self, Env, Expr, Fill, Hole, Kind, Op, Vocabulary};
use crate::lower::lower_outputs;
use crate::subject::{columns, domain_size, tuples};

/// Maximum requirements, including confirmed answers.
pub const MAX_REQUIREMENTS: usize = 64;

/// A law on every input/output row, or one complete example.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Requirement {
    /// Universal Boolean expression over input names and outN.
    Law {
        /// Unique requirement identifier.
        id: String,
        /// Boolean formula on every input/output row.
        formula: Expr,
    },
    /// Complete input and output tuple.
    Example {
        /// Unique requirement identifier.
        id: String,
        /// Complete declared input tuple.
        input: Vec<i128>,
        /// Complete declared output tuple.
        output: Vec<i128>,
    },
}

impl Requirement {
    /// Stable owner-facing identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Law { id, .. } | Self::Example { id, .. } => id,
        }
    }
}

/// Owned declaration; no original core or target table.
#[derive(Clone, Debug)]
pub struct Specification {
    /// Names in product enumeration order.
    pub names: Vec<String>,
    /// Finite input domains.
    pub inputs: Vec<Domain>,
    /// Finite output domains.
    pub outputs: Vec<Domain>,
    /// One sketch expression per output.
    pub sketch: Vec<Expr>,
    /// Universal laws and examples.
    pub requirements: Vec<Requirement>,
}

/// Deterministic cumulative limits; the contained CLI also imposes time limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Domain creation, visits, expression cells, filtering and checking.
    pub work: u64,
    /// Cartesian input tuples.
    pub domain: u64,
    /// Distinct complete tables retained before filtering.
    pub meanings: usize,
    /// Cumulative intermediate and retained table cells.
    pub cells: u64,
    /// Subsets examined for a minimum conflict.
    pub conflict_subsets: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            work: 2_000_000,
            domain: 4096,
            meanings: 4096,
            cells: 1_000_000,
            conflict_subsets: 10_000,
        }
    }
}

/// Typed stop; a search prefix cannot justify an exact count.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Stop {
    /// Invalid declaration or representation.
    Refused(String),
    /// Incomplete enumeration or later phase.
    Inconclusive(String),
}
impl From<Refusal> for Stop {
    fn from(value: Refusal) -> Self {
        Self::Refused(value.0)
    }
}

/// Owned, invocation-local accounting; retained even on refusal or retry.
#[derive(Clone, Debug)]
pub struct Budget {
    limits: Limits,
    work: u64,
    cells: u64,
    subsets: u64,
}
impl Budget {
    /// Immutable policy copied at invocation start.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.limits
    }
    /// Work successfully charged, including on a later refusal.
    #[must_use]
    pub fn work(&self) -> u64 {
        self.work
    }
    /// Cumulative cells successfully charged.
    #[must_use]
    pub fn cells(&self) -> u64 {
        self.cells
    }
    /// Conflict subsets examined.
    #[must_use]
    pub fn subsets(&self) -> u64 {
        self.subsets
    }
    /// Start a fresh search; hard caps cannot be disabled by a caller.
    pub fn new(limits: Limits) -> Result<Self, Stop> {
        if limits.work > 100_000_000
            || limits.domain > 65_536
            || limits.meanings > 16_384
            || limits.cells > 8_000_000
            || limits.conflict_subsets > 1_000_000
        {
            return Err(Stop::Refused("version-space-budget-above-hard-cap".into()));
        }
        Ok(Self {
            limits,
            work: 0,
            cells: 0,
            subsets: 0,
        })
    }
    /// Charge before the operation; a refusal retains previous charges.
    pub fn spend(&mut self, n: u64) -> Result<(), Stop> {
        let next = self
            .work
            .checked_add(n)
            .filter(|n| *n <= self.limits.work)
            .ok_or_else(|| Stop::Inconclusive("work-budget".into()))?;
        self.work = next;
        Ok(())
    }
    /// Conservative cumulative storage charge.
    pub fn retain(&mut self, n: u64) -> Result<(), Stop> {
        let next = self
            .cells
            .checked_add(n)
            .filter(|n| *n <= self.limits.cells)
            .ok_or_else(|| Stop::Inconclusive("table-cell-budget".into()))?;
        self.cells = next;
        Ok(())
    }
}

fn identifier(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_alphabetic() || b[0] == b'_')
        && b.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

fn typed(node: &Expr, want: Kind, vocab: &Vocabulary, holes: bool) -> Result<(), Stop> {
    match node {
        Expr::Int(v) if want != Kind::Bool || (0..=1).contains(v) => Ok(()),
        Expr::Var(n) if vocab.kind(n)? == want => Ok(()),
        Expr::Hole(_, h) if holes => match h {
            Hole::Choice(choices) => {
                for choice in choices {
                    typed(choice, want, vocab, false)?;
                }
                Ok(())
            }
            Hole::Const(Some((lo, hi)))
                if lo > hi || (want == Kind::Bool && (*lo < 0 || *hi > 1)) =>
            {
                Err(Stop::Refused("version-space-hole-range".into()))
            }
            _ => Ok(()),
        },
        Expr::Not(a) if want == Kind::Bool => typed(a, Kind::Bool, vocab, holes),
        Expr::If(c, a, b) => {
            typed(c, Kind::Bool, vocab, holes)?;
            typed(a, want, vocab, holes)?;
            typed(b, want, vocab, holes)
        }
        Expr::Call(function, args) => {
            if !function.accepts(args.len()) || function.kind() != want {
                return Err(Stop::Refused("version-space-call-kind-or-arity".into()));
            }
            for arg in args {
                typed(arg, Kind::Int, vocab, holes)?;
            }
            Ok(())
        }
        Expr::Bin(op, a, b) => {
            let (side, result) = match op {
                Op::Add | Op::Sub | Op::Mul => (Kind::Int, Kind::Int),
                Op::And | Op::Or | Op::Implies => (Kind::Bool, Kind::Bool),
                Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                    let side = expr::compare_side(a, b, vocab)?;
                    if side == Kind::Bool && !matches!(op, Op::Eq | Op::Ne) {
                        return Err(Stop::Refused("version-space-boolean-order".into()));
                    }
                    (side, Kind::Bool)
                }
            };
            if want != result {
                return Err(Stop::Refused("version-space-kind".into()));
            }
            typed(a, side, vocab, holes)?;
            typed(b, side, vocab, holes)
        }
        _ => Err(Stop::Refused("version-space-name-or-kind".into())),
    }
}

/// Only an ABI carrier for lowering. Its constants never filter meanings and
/// are never a reference for intended behavior.
pub fn carrier(inputs: &[Domain], outputs: &[Domain]) -> Result<Program, Stop> {
    let nodes: Vec<_> = outputs
        .iter()
        .map(|d| match d {
            Domain::Bool => Instruction::Bool(false),
            _ => Instruction::Int(d.bounds().0),
        })
        .collect();
    let roots = (0..outputs.len())
        .map(|k| u16::try_from(k).map_err(|_| Stop::Refused("version-space-output-count".into())))
        .collect::<Result<Vec<_>, _>>()?;
    Program::try_new(inputs.to_vec(), outputs.to_vec(), nodes, roots)
        .map_err(|e| Stop::Refused(format!("version-space-abi:{e:?}")))
}

impl Specification {
    /// Input vocabulary, without variants or implicit core names.
    pub fn vocabulary(&self) -> Result<Vocabulary, Stop> {
        Ok(Vocabulary::new(
            self.names.clone(),
            self.inputs.clone(),
            self.outputs.clone(),
            &BTreeMap::new(),
        )?)
    }
    /// Private input aliases used to lower outN observations without
    /// changing the existing expression vocabulary's input-name rules.
    #[must_use]
    pub fn output_aliases(&self) -> Vec<String> {
        let mut names = self.names.clone();
        let mut aliases = Vec::new();
        let mut k = 0usize;
        for _ in &self.outputs {
            loop {
                let name = format!("result_{k}");
                k += 1;
                if !names.contains(&name) {
                    names.push(name.clone());
                    aliases.push(name);
                    break;
                }
            }
        }
        aliases
    }
    /// Bind output observations to separate immutable input fields.
    pub fn law_formula(&self, node: &Expr) -> Result<Expr, Stop> {
        let aliases = self.output_aliases();
        Ok(expr::substitute(node, &mut |n| {
            if let Expr::Var(name) = n {
                for (j, alias) in aliases.iter().enumerate() {
                    if *name == format!("out{j}") {
                        return Ok(Some(Expr::Var(alias.clone())));
                    }
                }
            }
            Ok(None)
        })?)
    }
    /// Complete law vocabulary: inputs followed by bound output observations.
    pub fn law_vocabulary(&self) -> Result<Vocabulary, Stop> {
        let names = self
            .names
            .iter()
            .cloned()
            .chain(self.output_aliases())
            .collect();
        let domains = self.inputs.iter().chain(&self.outputs).copied().collect();
        Ok(Vocabulary::new(
            names,
            domains,
            vec![Domain::Bool],
            &BTreeMap::new(),
        )?)
    }
    /// Refuse malformed declarations before search. This does not prove
    /// satisfiability or acceptance of a representative.
    pub fn validate(&self) -> Result<(), Stop> {
        if self.names.len() != self.inputs.len()
            || self.inputs.len() + self.outputs.len() > 32
            || self.outputs.is_empty()
            || self.outputs.len() > 16
            || self.sketch.len() != self.outputs.len()
            || self.requirements.len() > MAX_REQUIREMENTS
        {
            return Err(Stop::Refused("version-space-shape".into()));
        }
        if self.names.iter().any(|n| {
            !identifier(n)
                || expr::RESERVED.contains(&n.as_str())
                || n.strip_prefix("out")
                    .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        }) || self.names.iter().collect::<BTreeSet<_>>().len() != self.names.len()
            || self
                .inputs
                .iter()
                .chain(&self.outputs)
                .any(|d| domain_size(*d) == 0)
        {
            return Err(Stop::Refused("version-space-inputs".into()));
        }
        let vocab = self.vocabulary()?;
        let laws = self.law_vocabulary()?;
        let mut ids = BTreeSet::new();
        let mut hole_ids = BTreeSet::new();
        let mut nodes = 0usize;
        for (n, d) in self.sketch.iter().zip(&self.outputs) {
            nodes += expr::size(n);
            vocab.check_names(n)?;
            typed(
                n,
                Kind::of(*d).ok_or_else(|| Stop::Refused("version-space-domain-kind".into()))?,
                &vocab,
                true,
            )?;
            for h in expr::holes_of(n) {
                if let Expr::Hole(id, _) = h
                    && !hole_ids.insert(*id)
                {
                    return Err(Stop::Refused("version-space-shared-hole".into()));
                }
            }
        }
        if hole_ids.len() > 16 || 4 * nodes + self.inputs.len() > 256 {
            return Err(Stop::Refused("version-space-lowering-size".into()));
        }
        for r in &self.requirements {
            if !identifier(r.id()) || !ids.insert(r.id()) {
                return Err(Stop::Refused("version-space-requirement-ids".into()));
            }
            match r {
                Requirement::Law { formula, .. } => {
                    let permitted: BTreeSet<String> = self
                        .names
                        .iter()
                        .cloned()
                        .chain((0..self.outputs.len()).map(|j| format!("out{j}")))
                        .collect();
                    if expr::names_of(formula)
                        .iter()
                        .any(|n| !permitted.contains(n))
                    {
                        return Err(Stop::Refused("version-space-law-name".into()));
                    }
                    let bound = self.law_formula(formula)?;
                    laws.check_names(&bound)?;
                    typed(&bound, Kind::Bool, &laws, false)?;
                    if 4 * expr::size(formula) + self.inputs.len() + self.outputs.len() > 256 {
                        return Err(Stop::Refused("version-space-law-lowering-size".into()));
                    }
                }
                Requirement::Example { input, output, .. } => {
                    self.point(input, &self.inputs)?;
                    self.point(output, &self.outputs)?;
                }
            }
        }
        Ok(())
    }
    /// Validate a complete tuple with exact domain membership.
    pub fn point(&self, values: &[i128], domains: &[Domain]) -> Result<(), Stop> {
        if values.len() != domains.len() || values.iter().zip(domains).any(|(v, d)| !d.contains(*v))
        {
            Err(Stop::Refused("version-space-value-outside-domain".into()))
        } else {
            Ok(())
        }
    }
    /// Requirement predicate on one input/output row. Examples compile to
    /// implications, so unasked inputs remain unconstrained.
    #[must_use]
    pub fn predicate(&self, requirement: &Requirement) -> Expr {
        match requirement {
            Requirement::Law { formula, .. } => formula.clone(),
            Requirement::Example { input, output, .. } => {
                let mut at = Expr::Int(1);
                let mut expected = Expr::Int(1);
                for (n, v) in self.names.iter().zip(input) {
                    at = Expr::bin(
                        Op::And,
                        at,
                        Expr::bin(Op::Eq, Expr::Var(n.clone()), Expr::Int(*v)),
                    );
                }
                for (k, v) in output.iter().enumerate() {
                    expected = Expr::bin(
                        Op::And,
                        expected,
                        Expr::bin(Op::Eq, Expr::Var(format!("out{k}")), Expr::Int(*v)),
                    );
                }
                Expr::bin(Op::Implies, at, expected)
            }
        }
    }
    /// Lower each requirement separately, for the final existing checker.
    pub fn requirement_programs(&self) -> Result<Vec<Program>, Stop> {
        let vocab = self.law_vocabulary()?;
        let template = carrier(vocab.inputs(), &[Domain::Bool])?;
        self.requirements
            .iter()
            .map(|r| {
                Ok(lower_outputs(
                    &[self.law_formula(&self.predicate(r))?],
                    &vocab,
                    &template,
                )?)
            })
            .collect()
    }
}

/// One complete meaning, compared by its table values rather than a hash.
#[derive(Clone, Debug)]
pub struct Meaning {
    /// Representative expressions.
    pub expressions: Vec<Expr>,
    /// Input-major complete output tuples.
    pub table: Vec<Vec<i128>>,
}
/// An offered output tuple and its exact proposed survivors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Answer {
    /// Complete offered output tuple.
    pub output: Vec<i128>,
    /// Exact proposed survivor count for this tuple.
    pub remaining: usize,
}
/// Deterministic one-step minimax partition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Question {
    /// Declared input at which meanings differ.
    pub input: Vec<i128>,
    /// Product ordinal, last input fastest.
    pub ordinal: usize,
    /// Lexically ordered complete answer partition.
    pub answers: Vec<Answer>,
    /// Largest surviving bucket.
    pub worst_remaining: usize,
    /// Deterministic second split objective.
    pub sum_squared_buckets: u64,
}
/// Proposal status. `Singleton` is never an accepted result from this crate.
#[derive(Clone, Debug)]
pub enum Proposal {
    /// Several full-domain meanings remain.
    Question {
        /// Exact proposed count, after complete enumeration and filtering.
        count: usize,
        /// Minimax question over those meanings.
        question: Question,
    },
    /// One representative remains; not yet checked for acceptance.
    Singleton(Meaning),
    /// No meaning fits this sketch and its requirements.
    Empty {
        /// Minimum-cardinality relative conflict, lexical ID tie-break.
        conflict: Vec<String>,
    },
}

fn enumerate(
    spec: &Specification,
    nodes: Vec<Expr>,
    env: &Env<'_>,
    budget: &mut Budget,
    seen: &mut BTreeSet<Vec<Vec<i128>>>,
    meanings: &mut Vec<Meaning>,
    kinds: &BTreeMap<usize, Kind>,
) -> Result<(), Stop> {
    budget.spend(1)?;
    for (j, node) in nodes.iter().enumerate() {
        if let Some(Expr::Hole(k, hole)) = expr::holes_of(node).first().copied() {
            let (lo, hi, choice) = match hole {
                Hole::Choice(alternatives) => {
                    (0, i128::try_from(alternatives.len()).unwrap_or(0) - 1, true)
                }
                Hole::Const(Some((lo, hi))) => (*lo, *hi, false),
                Hole::Const(None) if kinds.get(k) == Some(&Kind::Bool) => (0, 1, false),
                _ => return Err(Stop::Inconclusive("finite-hole-space-required".into())),
            };
            for value in lo..=hi {
                let mut one = BTreeMap::new();
                one.insert(
                    *k,
                    if choice {
                        Fill::Choice(
                            usize::try_from(value)
                                .map_err(|_| Stop::Refused("choice-index".into()))?,
                        )
                    } else {
                        Fill::Const(value)
                    },
                );
                let mut next = nodes.clone();
                next[j] = expr::fill(node, &one)?;
                enumerate(spec, next, env, budget, seen, meanings, kinds)?;
            }
            return Ok(());
        }
    }
    let mut columns = Vec::new();
    for node in &nodes {
        budget.spend((env.len as u64).saturating_mul(expr::size(node) as u64))?;
        let Some(values) = expr::skip_overflow(expr::evaluate(node, env, &BTreeMap::new()))? else {
            return Ok(());
        };
        columns.push(values);
    }
    if columns
        .iter()
        .zip(&spec.outputs)
        .any(|(c, d)| c.iter().any(|v| !d.contains(*v)))
    {
        return Ok(());
    }
    budget.retain((env.len as u64).saturating_mul(spec.outputs.len() as u64))?;
    let table: Vec<Vec<i128>> = (0..env.len)
        .map(|k| columns.iter().map(|c| c[k]).collect())
        .collect();
    if !seen.contains(&table) {
        if meanings.len() >= budget.limits.meanings {
            return Err(Stop::Inconclusive("meaning-budget".into()));
        }
        budget.retain((env.len as u64).saturating_mul(spec.outputs.len() as u64))?;
        seen.insert(table.clone());
        meanings.push(Meaning {
            expressions: nodes,
            table,
        });
    }
    Ok(())
}

fn minimum_conflict(
    spec: &Specification,
    masks: &[Vec<bool>],
    count: usize,
    budget: &mut Budget,
) -> Result<Vec<String>, Stop> {
    let mut order: Vec<usize> = (0..spec.requirements.len()).collect();
    order.sort_by_key(|k| spec.requirements[*k].id());
    for size in 0..=order.len() {
        let mut choice: Vec<usize> = (0..size).collect();
        loop {
            if budget.subsets >= budget.limits.conflict_subsets {
                return Err(Stop::Inconclusive("conflict-subset-budget".into()));
            }
            budget.spend(1 + (size as u64).saturating_mul(count as u64))?;
            budget.subsets += 1;
            if !(0..count).any(|k| choice.iter().all(|i| masks[order[*i]][k])) {
                return Ok(choice
                    .iter()
                    .map(|i| spec.requirements[order[*i]].id().to_string())
                    .collect());
            }
            let next = (0..size)
                .rev()
                .find(|i| choice[*i] < order.len() - size + *i);
            let Some(i) = next else {
                break;
            };
            choice[i] += 1;
            for j in i + 1..size {
                choice[j] = choice[j - 1] + 1;
            }
        }
    }
    Err(Stop::Refused("version-space-conflict-internal".into()))
}

/// Exhaustive proposal computation. Every phase shares `budget`; an error
/// returns no count/partition/candidate. The caller retains usage on errors.
pub fn analyze(spec: &Specification, budget: &mut Budget) -> Result<Proposal, Stop> {
    spec.validate()?;
    let domain = tuples(&spec.inputs);
    if domain > u128::from(budget.limits.domain) {
        return Err(Stop::Inconclusive("domain-budget".into()));
    }
    let domain = usize::try_from(domain).map_err(|_| Stop::Inconclusive("domain-budget".into()))?;
    budget.spend((domain as u64).saturating_mul(spec.inputs.len().max(1) as u64))?;
    budget.retain(
        (domain as u64)
            .saturating_mul(spec.inputs.len().max(1) as u64)
            .saturating_mul(2),
    )?;
    let cols = columns(&spec.inputs, budget.limits.domain)?;
    let vocab = spec.vocabulary()?;
    let env = Env {
        vocab: &vocab,
        columns: &cols,
        keep: None,
        len: domain,
    };
    let mut kinds = BTreeMap::new();
    for (n, d) in spec.sketch.iter().zip(&spec.outputs) {
        expr::hole_kinds(
            n,
            Kind::of(*d).ok_or_else(|| Stop::Refused("version-space-domain-kind".into()))?,
            &vocab,
            &mut kinds,
        )?;
    }
    // Validate every space before enumeration, including unreachable branches.
    for n in &spec.sketch {
        for h in expr::holes_of(n) {
            if let Expr::Hole(k, Hole::Expr | Hole::Const(None)) = h
                && (matches!(h, Expr::Hole(_, Hole::Expr)) || kinds.get(k) != Some(&Kind::Bool))
            {
                return Err(Stop::Inconclusive("finite-hole-space-required".into()));
            }
        }
    }
    let mut meanings = Vec::new();
    let mut seen = BTreeSet::new();
    enumerate(
        spec,
        spec.sketch.clone(),
        &env,
        budget,
        &mut seen,
        &mut meanings,
        &kinds,
    )?;
    let law_vocab = spec.law_vocabulary()?;
    budget.retain((meanings.len() as u64).saturating_mul(spec.requirements.len() as u64))?;
    let mut masks = Vec::new();
    for requirement in &spec.requirements {
        let mut mask = Vec::new();
        for meaning in &meanings {
            let ok = match requirement {
                Requirement::Example { input, output, .. } => {
                    budget.spend(spec.outputs.len() as u64)?;
                    let k = input.iter().zip(&spec.inputs).fold(0u128, |n, (v, d)| {
                        n * domain_size(*d)
                            + u128::try_from(i128::from(*v) - i128::from(d.bounds().0)).unwrap_or(0)
                    });
                    meaning.table.get(usize::try_from(k).unwrap_or(usize::MAX)) == Some(output)
                }
                Requirement::Law { formula, .. } => {
                    budget.spend((domain as u64).saturating_mul(expr::size(formula) as u64))?;
                    budget.retain((domain as u64).saturating_mul(spec.outputs.len() as u64))?;
                    let mut lc = cols.clone();
                    for j in 0..spec.outputs.len() {
                        lc.push(meaning.table.iter().map(|r| r[j]).collect());
                    }
                    let le = Env {
                        vocab: &law_vocab,
                        columns: &lc,
                        keep: None,
                        len: domain,
                    };
                    expr::skip_overflow(expr::evaluate(
                        &spec.law_formula(formula)?,
                        &le,
                        &BTreeMap::new(),
                    ))?
                    .is_some_and(|v| v.iter().all(|x| *x == 1))
                }
            };
            mask.push(ok);
        }
        masks.push(mask);
    }
    budget.spend((meanings.len() as u64).saturating_mul(masks.len().max(1) as u64))?;
    let survivors: Vec<usize> = (0..meanings.len())
        .filter(|k| masks.iter().all(|m| m[*k]))
        .collect();
    match survivors.as_slice() {
        [] => Ok(Proposal::Empty {
            conflict: minimum_conflict(spec, &masks, meanings.len(), budget)?,
        }),
        [k] => Ok(Proposal::Singleton(meanings[*k].clone())),
        _ => {
            let mut best: Option<((usize, u64, usize), Question)> = None;
            for ordinal in 0..domain {
                budget.spend((survivors.len() as u64).saturating_mul(spec.outputs.len() as u64))?;
                let mut buckets = BTreeMap::<Vec<i128>, usize>::new();
                for k in &survivors {
                    *buckets
                        .entry(meanings[*k].table[ordinal].clone())
                        .or_default() += 1;
                }
                if buckets.len() < 2 {
                    continue;
                }
                let worst = buckets.values().copied().max().unwrap_or(0);
                let squared = buckets.values().map(|n| (*n as u64) * (*n as u64)).sum();
                let key = (worst, squared, ordinal);
                if best.as_ref().is_none_or(|(old, _)| key < *old) {
                    best = Some((
                        key,
                        Question {
                            input: cols.iter().map(|c| c[ordinal]).collect(),
                            ordinal,
                            answers: buckets
                                .into_iter()
                                .map(|(output, remaining)| Answer { output, remaining })
                                .collect(),
                            worst_remaining: worst,
                            sum_squared_buckets: squared,
                        },
                    ));
                }
            }
            best.map(|(_, question)| Proposal::Question {
                count: survivors.len(),
                question,
            })
            .ok_or_else(|| Stop::Refused("version-space-distinct-tables-have-no-split".into()))
        }
    }
}
