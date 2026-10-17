//! Lowering complete expressions to a finite program with the original's
//! exact input and output domains.
//!
//! Terms are hash-consed in one pool (commutative `Add`, `Eq` and `And`
//! operands are ordered), the original program's nodes are loaded first so
//! `keep` and shared structure cost nothing, and the result is emitted
//! dead-code-free in one fixed depth-first order, then admitted by the
//! library (`Program::try_new`). A lowering error is a refusal; a wrong
//! lowering cannot pass the checker, which judges every lowered program.
//!
//! Operators lower as the contract compiler lowers them: `a || b` is
//! `Select(a, a, b)`, `a -> b` is `Select(a, b, true)`, `a != b` is
//! `Not(Eq(a, b))`, `a > b` is `Lt(b, a)`, `a <= b` is `Not(Lt(b, a))`,
//! `a >= b` is `Not(Lt(a, b))`, and `0 - k` for a literal `k` is the literal
//! `-k`. `*` is refused.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use zeno_fcis_synthesis::finite::{Domain, Error, Op as LibOp, Program};

use crate::expr::{Expr, Function, Kind, Op, Vocabulary, compare_side};
use crate::{Refusal, Result};

/// One pool term; operands are pool ids.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Term {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(usize, usize),
    Sub(usize, usize),
    Mul(usize, usize),
    DivFloor(usize, usize),
    DivCeil(usize, usize),
    MulDivFloor(usize, usize, usize),
    MulDivCeil(usize, usize, usize),
    ProductsGe([usize; 3], [usize; 3]),
    Eq(usize, usize),
    Lt(usize, usize),
    And(usize, usize),
    Not(usize),
    Select(usize, usize, usize),
}

impl Term {
    fn operands(self) -> Vec<usize> {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => Vec::new(),
            Self::Not(a) => vec![a],
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::DivFloor(a, b)
            | Self::DivCeil(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::And(a, b) => {
                vec![a, b]
            }
            Self::Select(c, a, b) | Self::MulDivFloor(c, a, b) | Self::MulDivCeil(c, a, b) => {
                vec![c, a, b]
            }
            Self::ProductsGe(a, b) => a.into_iter().chain(b).collect(),
        }
    }

    fn map(self, f: &impl Fn(usize) -> usize) -> Self {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => self,
            Self::Add(a, b) => Self::Add(f(a), f(b)),
            Self::Sub(a, b) => Self::Sub(f(a), f(b)),
            Self::Mul(a, b) => Self::Mul(f(a), f(b)),
            Self::DivFloor(a, b) => Self::DivFloor(f(a), f(b)),
            Self::DivCeil(a, b) => Self::DivCeil(f(a), f(b)),
            Self::MulDivFloor(a, b, d) => Self::MulDivFloor(f(a), f(b), f(d)),
            Self::MulDivCeil(a, b, d) => Self::MulDivCeil(f(a), f(b), f(d)),
            Self::ProductsGe(a, b) => Self::ProductsGe(a.map(f), b.map(f)),
            Self::Eq(a, b) => Self::Eq(f(a), f(b)),
            Self::Lt(a, b) => Self::Lt(f(a), f(b)),
            Self::And(a, b) => Self::And(f(a), f(b)),
            Self::Not(a) => Self::Not(f(a)),
            Self::Select(c, a, b) => Self::Select(f(c), f(a), f(b)),
        }
    }
}

#[derive(Default)]
struct Pool {
    terms: Vec<Term>,
    index: BTreeMap<Term, usize>,
}

impl Pool {
    fn add(&mut self, term: Term) -> usize {
        let term = match term {
            Term::Add(a, b) if a > b => Term::Add(b, a),
            Term::Eq(a, b) if a > b => Term::Eq(b, a),
            Term::And(a, b) if a > b => Term::And(b, a),
            other => other,
        };
        if let Some(id) = self.index.get(&term) {
            return *id;
        }
        let id = self.terms.len();
        self.terms.push(term);
        self.index.insert(term, id);
        id
    }

    fn load(&mut self, program: &Program) -> Result<Vec<usize>> {
        let mut ids: Vec<usize> = Vec::with_capacity(program.nodes().len());
        for op in program.nodes() {
            let at = |k: u16| ids.get(usize::from(k)).copied().unwrap_or(usize::MAX);
            let term = match *op {
                LibOp::Input(k) => Term::Input(k),
                LibOp::Int(value) => Term::Int(value),
                LibOp::Bool(value) => Term::Bool(value),
                LibOp::Add(a, b) => Term::Add(at(a), at(b)),
                LibOp::Sub(a, b) => Term::Sub(at(a), at(b)),
                LibOp::Mul(a, b) => Term::Mul(at(a), at(b)),
                LibOp::DivFloor(a, b) => Term::DivFloor(at(a), at(b)),
                LibOp::DivCeil(a, b) => Term::DivCeil(at(a), at(b)),
                LibOp::MulDivFloor(a, b, d) => Term::MulDivFloor(at(a), at(b), at(d)),
                LibOp::MulDivCeil(a, b, d) => Term::MulDivCeil(at(a), at(b), at(d)),
                LibOp::ProductsGe(a, b) => Term::ProductsGe(a.map(at), b.map(at)),
                LibOp::Eq(a, b) => Term::Eq(at(a), at(b)),
                LibOp::Lt(a, b) => Term::Lt(at(a), at(b)),
                LibOp::And(a, b) => Term::And(at(a), at(b)),
                LibOp::Not(a) => Term::Not(at(a)),
                LibOp::Select(c, a, b) => Term::Select(at(c), at(a), at(b)),
                _ => return Err(Refusal::new("original-instruction-unsupported")),
            };
            ids.push(self.add(term));
        }
        Ok(ids)
    }

    /// The terms reachable from `roots`, operands first, in a fixed
    /// iterative depth-first post-order; with each root's position.
    fn emit(&self, roots: &[usize]) -> (Vec<Term>, Vec<usize>) {
        let mut order = Vec::new();
        let mut seen = vec![false; self.terms.len()];
        for &root in roots {
            let mut stack = vec![(root, false)];
            while let Some((term, done)) = stack.pop() {
                let Some(node) = self.terms.get(term) else {
                    continue;
                };
                if done {
                    if !seen[term] {
                        seen[term] = true;
                        order.push(term);
                    }
                    continue;
                }
                if seen[term] {
                    continue;
                }
                stack.push((term, true));
                for operand in node.operands().into_iter().rev() {
                    if seen.get(operand) == Some(&false) {
                        stack.push((operand, false));
                    }
                }
            }
        }
        let mut position = vec![usize::MAX; self.terms.len()];
        for (index, term) in order.iter().enumerate() {
            position[*term] = index;
        }
        let at = |term: usize| position.get(term).copied().unwrap_or(usize::MAX);
        let nodes = order
            .iter()
            .map(|term| self.terms[*term].map(&at))
            .collect();
        (nodes, roots.iter().map(|root| at(*root)).collect())
    }
}

fn library(nodes: &[Term]) -> Result<Vec<LibOp>> {
    let id = |value: usize| u16::try_from(value).map_err(|_| Refusal::new("program-too-large"));
    nodes
        .iter()
        .map(|term| {
            Ok(match *term {
                Term::Input(k) => LibOp::Input(k),
                Term::Int(value) => LibOp::Int(value),
                Term::Bool(value) => LibOp::Bool(value),
                Term::Add(a, b) => LibOp::Add(id(a)?, id(b)?),
                Term::Sub(a, b) => LibOp::Sub(id(a)?, id(b)?),
                Term::Mul(a, b) => LibOp::Mul(id(a)?, id(b)?),
                Term::DivFloor(a, b) => LibOp::DivFloor(id(a)?, id(b)?),
                Term::DivCeil(a, b) => LibOp::DivCeil(id(a)?, id(b)?),
                Term::MulDivFloor(a, b, d) => LibOp::MulDivFloor(id(a)?, id(b)?, id(d)?),
                Term::MulDivCeil(a, b, d) => LibOp::MulDivCeil(id(a)?, id(b)?, id(d)?),
                Term::ProductsGe(a, b) => LibOp::ProductsGe(
                    [id(a[0])?, id(a[1])?, id(a[2])?],
                    [id(b[0])?, id(b[1])?, id(b[2])?],
                ),
                Term::Eq(a, b) => LibOp::Eq(id(a)?, id(b)?),
                Term::Lt(a, b) => LibOp::Lt(id(a)?, id(b)?),
                Term::And(a, b) => LibOp::And(id(a)?, id(b)?),
                Term::Not(a) => LibOp::Not(id(a)?),
                Term::Select(c, a, b) => LibOp::Select(id(c)?, id(a)?, id(b)?),
            })
        })
        .collect()
}

fn not_admitted(error: &Error) -> Refusal {
    let code = match *error {
        Error::Invalid(code) | Error::Limit(code) => code,
        _ => "unclassified",
    };
    Refusal::new(format!("lowering-not-admitted:{code}"))
}

struct Lowerer<'a> {
    vocab: &'a Vocabulary,
    pool: Pool,
    original: Vec<usize>,
    roots: &'a [u16],
}

impl Lowerer<'_> {
    fn lower(&mut self, node: &Expr, want: Kind) -> Result<usize> {
        let vocab = self.vocab;
        Ok(match node {
            Expr::Keep => return Err(Refusal::new("keep-only-as-a-whole-output")),
            Expr::Int(value) => {
                if want == Kind::Bool {
                    if *value != 0 && *value != 1 {
                        return Err(Refusal::new("proposal-boolean-literal"));
                    }
                    self.pool.add(Term::Bool(*value == 1))
                } else {
                    self.pool.add(Term::Int(*value))
                }
            }
            Expr::Var(name) => {
                let Some(k) = vocab.position(name) else {
                    if let Some(value) = vocab.variants().get(name) {
                        return self.lower(&Expr::Int(*value), want);
                    }
                    return Err(Refusal::new(format!("proposal-unknown-name:{name}")));
                };
                let kind = vocab.input_kinds()[k];
                if want == Kind::Bool && kind != Kind::Bool {
                    return Err(Refusal::new(format!("proposal-kind:{name}")));
                }
                let k = u16::try_from(k).map_err(|_| Refusal::new("program-too-large"))?;
                let input = self.pool.add(Term::Input(k));
                if kind == Kind::Bool && want == Kind::Int {
                    let one = self.pool.add(Term::Int(1));
                    let zero = self.pool.add(Term::Int(0));
                    self.pool.add(Term::Select(input, one, zero))
                } else {
                    input
                }
            }
            Expr::Hole(..) | Expr::Param(_) => {
                return Err(Refusal::new("lowering-needs-a-complete-expression"));
            }
            Expr::Not(a) => {
                let a = self.lower(a, Kind::Bool)?;
                self.pool.add(Term::Not(a))
            }
            Expr::If(c, a, b) => {
                let c = self.lower(c, Kind::Bool)?;
                let a = self.lower(a, want)?;
                let b = self.lower(b, want)?;
                self.pool.add(Term::Select(c, a, b))
            }
            Expr::Bin(op, a, b) => self.binary(*op, a, b, want)?,
            Expr::Call(function, args) => {
                if !function.accepts(args.len()) {
                    return Err(Refusal::new("expression-call-arity"));
                }
                if function.kind() != want {
                    return Err(Refusal::new("proposal-kind:call"));
                }
                let args = args
                    .iter()
                    .map(|a| self.lower(a, Kind::Int))
                    .collect::<Result<Vec<_>>>()?;
                let term = match function {
                    Function::DivFloor => Term::DivFloor(args[0], args[1]),
                    Function::DivCeil => Term::DivCeil(args[0], args[1]),
                    Function::MulDivFloor => Term::MulDivFloor(args[0], args[1], args[2]),
                    Function::MulDivCeil => Term::MulDivCeil(args[0], args[1], args[2]),
                    Function::ProductsGe if args.len() == 4 => {
                        let one = self.pool.add(Term::Int(1));
                        Term::ProductsGe([args[0], args[1], one], [args[2], args[3], one])
                    }
                    Function::ProductsGe => {
                        Term::ProductsGe([args[0], args[1], args[2]], [args[3], args[4], args[5]])
                    }
                };
                self.pool.add(term)
            }
        })
    }

    fn binary(&mut self, op: Op, a: &Expr, b: &Expr, want: Kind) -> Result<usize> {
        match op {
            Op::Add | Op::Sub | Op::Mul => {
                if want != Kind::Int {
                    return Err(Refusal::new("proposal-kind:arithmetic"));
                }
                if op == Op::Sub
                    && *a == Expr::Int(0)
                    && let Expr::Int(k) = b
                {
                    let negated = k
                        .checked_neg()
                        .ok_or_else(|| Refusal::new("expression-integer-range"))?;
                    return Ok(self.pool.add(Term::Int(negated)));
                }
                let x = self.lower(a, Kind::Int)?;
                let y = self.lower(b, Kind::Int)?;
                Ok(self.pool.add(match op {
                    Op::Add => Term::Add(x, y),
                    Op::Mul => Term::Mul(x, y),
                    _ => Term::Sub(x, y),
                }))
            }
            _ if want != Kind::Bool => Err(Refusal::new(format!("proposal-kind:{}", op.symbol()))),
            Op::And => {
                let x = self.lower(a, Kind::Bool)?;
                let y = self.lower(b, Kind::Bool)?;
                Ok(self.pool.add(Term::And(x, y)))
            }
            Op::Or => {
                let x = self.lower(a, Kind::Bool)?;
                let y = self.lower(b, Kind::Bool)?;
                Ok(self.pool.add(Term::Select(x, x, y)))
            }
            Op::Implies => {
                let x = self.lower(a, Kind::Bool)?;
                let y = self.lower(b, Kind::Bool)?;
                let t = self.pool.add(Term::Bool(true));
                Ok(self.pool.add(Term::Select(x, y, t)))
            }
            _ => {
                let side = compare_side(a, b, self.vocab)?;
                let x = self.lower(a, side)?;
                let y = self.lower(b, side)?;
                if matches!(op, Op::Eq | Op::Ne) {
                    let eq = self.pool.add(Term::Eq(x, y));
                    return Ok(if op == Op::Eq {
                        eq
                    } else {
                        self.pool.add(Term::Not(eq))
                    });
                }
                if side != Kind::Int {
                    return Err(Refusal::new("proposal-kind:order"));
                }
                Ok(match op {
                    Op::Lt => self.pool.add(Term::Lt(x, y)),
                    Op::Gt => self.pool.add(Term::Lt(y, x)),
                    Op::Le => {
                        let lt = self.pool.add(Term::Lt(y, x));
                        self.pool.add(Term::Not(lt))
                    }
                    _ => {
                        let lt = self.pool.add(Term::Lt(x, y));
                        self.pool.add(Term::Not(lt))
                    }
                })
            }
        }
    }

    fn root(&mut self, node: &Expr, j: usize) -> Result<usize> {
        if *node == Expr::Keep {
            let root = self
                .roots
                .get(j)
                .ok_or_else(|| Refusal::new("proposal-output-count"))?;
            return self
                .original
                .get(usize::from(*root))
                .copied()
                .ok_or_else(|| Refusal::new("keep-without-original"));
        }
        let want = self
            .vocab
            .output_kinds()
            .get(j)
            .copied()
            .ok_or_else(|| Refusal::new("proposal-output-count"))?;
        self.lower(node, want)
    }
}

/// One complete (hole-free, parameter-free) expression per output, lowered
/// to a canonical program with `original`'s input and output domains.
/// Variant names are resolved first.
pub fn lower_outputs(nodes: &[Expr], vocab: &Vocabulary, original: &Program) -> Result<Program> {
    if nodes.len() != vocab.outputs().len() || original.outputs() != vocab.outputs() {
        return Err(Refusal::new("proposal-output-count"));
    }
    if original.inputs() != vocab.inputs() {
        return Err(Refusal::new("vocabulary-abi"));
    }
    let mut lowerer = Lowerer {
        vocab,
        pool: Pool::default(),
        original: Vec::new(),
        roots: original.roots(),
    };
    lowerer.original = lowerer.pool.load(original)?;
    let mut roots = Vec::with_capacity(nodes.len());
    for (j, node) in nodes.iter().enumerate() {
        let node = vocab.resolve_variants(node)?;
        roots.push(lowerer.root(&node, j)?);
    }
    let (nodes, roots) = lowerer.pool.emit(&roots);
    // Hash-cons again over the emitted order, as the prototype did.
    let mut again = Pool::default();
    let mut ids: Vec<usize> = Vec::with_capacity(nodes.len());
    for term in &nodes {
        let renamed = term.map(&|operand| ids.get(operand).copied().unwrap_or(usize::MAX));
        ids.push(again.add(renamed));
    }
    let roots: Vec<usize> = roots
        .iter()
        .map(|root| ids.get(*root).copied().unwrap_or(usize::MAX))
        .collect();
    let (nodes, roots) = again.emit(&roots);
    let roots = roots
        .iter()
        .map(|root| u16::try_from(*root).map_err(|_| Refusal::new("program-too-large")))
        .collect::<Result<Vec<_>>>()?;
    Program::try_new(
        original.inputs().to_vec(),
        original.outputs().to_vec(),
        library(&nodes)?,
        roots,
    )
    .map_err(|error| not_admitted(&error))
}

/// Fixed-width size of a program's canonical encoding: a leaf or `Not` is
/// 39 bytes, a binary instruction 56 and a select 73, plus 57 bytes of
/// framing, 40 per input and 57 per output. Used only to rank candidates;
/// reports use the encoder's actual bytes.
#[must_use]
pub fn estimated_bytes(program: &Program) -> usize {
    let nodes: usize = program
        .nodes()
        .iter()
        .map(|op| match op {
            LibOp::Add(..)
            | LibOp::Sub(..)
            | LibOp::Mul(..)
            | LibOp::DivFloor(..)
            | LibOp::DivCeil(..)
            | LibOp::Eq(..)
            | LibOp::Lt(..)
            | LibOp::And(..) => 56,
            LibOp::Select(..) | LibOp::MulDivFloor(..) | LibOp::MulDivCeil(..) => 73,
            LibOp::ProductsGe(..) => 124,
            _ => 39,
        })
        .sum();
    57 + 40 * program.inputs().len() + 57 * program.outputs().len() + nodes
}

/// Selection order: instructions, then estimated bytes.
#[must_use]
pub fn cost(program: &Program) -> (usize, usize) {
    (program.nodes().len(), estimated_bytes(program))
}

/// Each output of a program as one expression over the input names (shared
/// nodes are inlined; refused when an inlined output would exceed
/// `max_size` nodes).
pub fn from_program(program: &Program, vocab: &Vocabulary, max_size: usize) -> Result<Vec<Expr>> {
    let mut memo: Vec<(Expr, usize)> = Vec::with_capacity(program.nodes().len());
    for op in program.nodes() {
        let at = |k: &u16| memo.get(usize::from(*k)).cloned();
        let missing = || Refusal::new("program-reference");
        let (expr, size) = match op {
            LibOp::Input(k) => (
                Expr::Var(
                    vocab
                        .names()
                        .get(usize::from(*k))
                        .cloned()
                        .ok_or_else(missing)?,
                ),
                1,
            ),
            LibOp::Int(value) => (Expr::Int(*value), 1),
            LibOp::Bool(value) => (Expr::Int(i64::from(*value)), 1),
            LibOp::Not(a) => {
                let (a, s) = at(a).ok_or_else(missing)?;
                (Expr::negation(a), 1 + s)
            }
            LibOp::Select(c, a, b) => {
                let (c, sc) = at(c).ok_or_else(missing)?;
                let (a, sa) = at(a).ok_or_else(missing)?;
                let (b, sb) = at(b).ok_or_else(missing)?;
                (Expr::select(c, a, b), 1 + sc + sa + sb)
            }
            LibOp::MulDivFloor(a, b, d) | LibOp::MulDivCeil(a, b, d) => {
                let function = if matches!(op, LibOp::MulDivFloor(..)) {
                    Function::MulDivFloor
                } else {
                    Function::MulDivCeil
                };
                let (a, sa) = at(a).ok_or_else(missing)?;
                let (b, sb) = at(b).ok_or_else(missing)?;
                let (d, sd) = at(d).ok_or_else(missing)?;
                (Expr::Call(function, vec![a, b, d]), 1 + sa + sb + sd)
            }
            LibOp::ProductsGe(a, b) => {
                let args = a
                    .iter()
                    .chain(b)
                    .map(|k| at(k).ok_or_else(missing))
                    .collect::<Result<Vec<_>>>()?;
                let size = 1 + args.iter().map(|(_, s)| *s).sum::<usize>();
                (
                    Expr::Call(
                        Function::ProductsGe,
                        args.into_iter().map(|(e, _)| e).collect(),
                    ),
                    size,
                )
            }
            LibOp::DivFloor(a, b) | LibOp::DivCeil(a, b) => {
                let function = if matches!(op, LibOp::DivFloor(..)) {
                    Function::DivFloor
                } else {
                    Function::DivCeil
                };
                let (a, sa) = at(a).ok_or_else(missing)?;
                let (b, sb) = at(b).ok_or_else(missing)?;
                (Expr::Call(function, vec![a, b]), 1 + sa + sb)
            }
            LibOp::Add(a, b)
            | LibOp::Sub(a, b)
            | LibOp::Mul(a, b)
            | LibOp::Eq(a, b)
            | LibOp::Lt(a, b)
            | LibOp::And(a, b) => {
                let symbol = match op {
                    LibOp::Add(..) => Op::Add,
                    LibOp::Sub(..) => Op::Sub,
                    LibOp::Mul(..) => Op::Mul,
                    LibOp::Eq(..) => Op::Eq,
                    LibOp::Lt(..) => Op::Lt,
                    _ => Op::And,
                };
                let (a, sa) = at(a).ok_or_else(missing)?;
                let (b, sb) = at(b).ok_or_else(missing)?;
                (Expr::bin(symbol, a, b), 1 + sa + sb)
            }
            _ => return Err(Refusal::new("original-instruction-unsupported")),
        };
        if size > max_size {
            return Err(Refusal::new("program-too-shared-to-inline"));
        }
        memo.push((expr, size));
    }
    program
        .roots()
        .iter()
        .map(|root| {
            memo.get(usize::from(*root))
                .map(|(expr, _)| expr.clone())
                .ok_or_else(|| Refusal::new("program-reference"))
        })
        .collect()
}

/// Output domains with full ranges: the same instructions with every output
/// over its kind's whole range, so per-output values can be read where the
/// original refuses an output outside its declared domain.
pub fn widened(program: &Program) -> Result<Program> {
    let outputs = program
        .outputs()
        .iter()
        .map(|domain| match Kind::of(*domain) {
            Some(Kind::Bool) => Ok(Domain::Bool),
            Some(Kind::Int) => Ok(Domain::Int {
                min: i64::MIN,
                max: i64::MAX,
            }),
            None => Err(Refusal::new("unsupported-domain")),
        })
        .collect::<Result<Vec<_>>>()?;
    let nodes = program
        .nodes()
        .iter()
        .map(|op| {
            Ok(match *op {
                LibOp::Input(k) => LibOp::Input(k),
                LibOp::Int(value) => LibOp::Int(value),
                LibOp::Bool(value) => LibOp::Bool(value),
                LibOp::Add(a, b) => LibOp::Add(a, b),
                LibOp::Sub(a, b) => LibOp::Sub(a, b),
                LibOp::Mul(a, b) => LibOp::Mul(a, b),
                LibOp::DivFloor(a, b) => LibOp::DivFloor(a, b),
                LibOp::DivCeil(a, b) => LibOp::DivCeil(a, b),
                LibOp::MulDivFloor(a, b, d) => LibOp::MulDivFloor(a, b, d),
                LibOp::MulDivCeil(a, b, d) => LibOp::MulDivCeil(a, b, d),
                LibOp::ProductsGe(a, b) => LibOp::ProductsGe(a, b),
                LibOp::Eq(a, b) => LibOp::Eq(a, b),
                LibOp::Lt(a, b) => LibOp::Lt(a, b),
                LibOp::And(a, b) => LibOp::And(a, b),
                LibOp::Not(a) => LibOp::Not(a),
                LibOp::Select(c, a, b) => LibOp::Select(c, a, b),
                _ => return Err(Refusal::new("original-instruction-unsupported")),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Program::try_new(
        program.inputs().to_vec(),
        outputs,
        nodes,
        program.roots().to_vec(),
    )
    .map_err(|error| not_admitted(&error))
}
