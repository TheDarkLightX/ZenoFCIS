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

use crate::expr::{Expr, Kind, Op, Vocabulary, compare_side};
use crate::{Refusal, Result};

/// One pool term; operands are pool ids.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Term {
    Input(u16),
    Int(i64),
    Bool(bool),
    Add(usize, usize),
    Sub(usize, usize),
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
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::And(a, b) => {
                vec![a, b]
            }
            Self::Select(c, a, b) => vec![c, a, b],
        }
    }

    fn map(self, f: &impl Fn(usize) -> usize) -> Self {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => self,
            Self::Add(a, b) => Self::Add(f(a), f(b)),
            Self::Sub(a, b) => Self::Sub(f(a), f(b)),
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
        })
    }

    fn binary(&mut self, op: Op, a: &Expr, b: &Expr, want: Kind) -> Result<usize> {
        match op {
            Op::Mul => Err(Refusal::new("proposal-operator-unsupported:*")),
            Op::Add | Op::Sub => {
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
                Ok(self.pool.add(if op == Op::Add {
                    Term::Add(x, y)
                } else {
                    Term::Sub(x, y)
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
            LibOp::Add(..) | LibOp::Sub(..) | LibOp::Eq(..) | LibOp::Lt(..) | LibOp::And(..) => 56,
            LibOp::Select(..) => 73,
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
            LibOp::Add(a, b)
            | LibOp::Sub(a, b)
            | LibOp::Eq(a, b)
            | LibOp::Lt(a, b)
            | LibOp::And(a, b) => {
                let symbol = match op {
                    LibOp::Add(..) => Op::Add,
                    LibOp::Sub(..) => Op::Sub,
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
