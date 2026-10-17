//! The search's working view of a finite program: plain node indices, kinds
//! and the conversions to and from the library's admitted `Program`.
//!
//! Nothing here evaluates a program. Admission and canonical bytes always come
//! from the library (`Program::try_new`, `Program::value`).

use alloc::vec::Vec;
use zeno_fcis_codec::CanonicalEncode;
use zeno_fcis_synthesis::finite::{Domain, Op, Program};

/// The two scalar kinds of the finite IR.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Kind {
    Int,
    Bool,
}

/// The kind of a declared domain; `None` for a domain this crate does not know.
pub(crate) fn domain_kind(domain: Domain) -> Option<Kind> {
    match domain {
        Domain::Bool => Some(Kind::Bool),
        Domain::Int { .. } => Some(Kind::Int),
        _ => None,
    }
}

/// One instruction; operands are indices of earlier nodes (or pool terms).
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Node {
    Input(u16),
    Int(i128),
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

impl Node {
    /// Operands in order, padded; the second value is how many are used.
    pub(crate) fn operands(self) -> ([usize; 6], usize) {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => ([0; 6], 0),
            Self::Not(a) => ([a, 0, 0, 0, 0, 0], 1),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::DivFloor(a, b)
            | Self::DivCeil(a, b)
            | Self::Eq(a, b)
            | Self::Lt(a, b)
            | Self::And(a, b) => ([a, b, 0, 0, 0, 0], 2),
            Self::Select(c, a, b) | Self::MulDivFloor(c, a, b) | Self::MulDivCeil(c, a, b) => {
                ([c, a, b, 0, 0, 0], 3)
            }
            Self::ProductsGe(a, b) => ([a[0], a[1], a[2], b[0], b[1], b[2]], 6),
        }
    }

    /// True for inputs and constants.
    pub(crate) fn is_leaf(self) -> bool {
        matches!(self, Self::Input(_) | Self::Int(_) | Self::Bool(_))
    }

    /// The same instruction with every operand renamed.
    pub(crate) fn map(self, mut rename: impl FnMut(usize) -> usize) -> Self {
        match self {
            Self::Input(_) | Self::Int(_) | Self::Bool(_) => self,
            Self::Add(a, b) => Self::Add(rename(a), rename(b)),
            Self::Sub(a, b) => Self::Sub(rename(a), rename(b)),
            Self::Mul(a, b) => Self::Mul(rename(a), rename(b)),
            Self::DivFloor(a, b) => Self::DivFloor(rename(a), rename(b)),
            Self::DivCeil(a, b) => Self::DivCeil(rename(a), rename(b)),
            Self::MulDivFloor(a, b, d) => Self::MulDivFloor(rename(a), rename(b), rename(d)),
            Self::MulDivCeil(a, b, d) => Self::MulDivCeil(rename(a), rename(b), rename(d)),
            Self::ProductsGe(a, b) => Self::ProductsGe(a.map(&mut rename), b.map(&mut rename)),
            Self::Eq(a, b) => Self::Eq(rename(a), rename(b)),
            Self::Lt(a, b) => Self::Lt(rename(a), rename(b)),
            Self::And(a, b) => Self::And(rename(a), rename(b)),
            Self::Not(a) => Self::Not(rename(a)),
            Self::Select(c, a, b) => Self::Select(rename(c), rename(a), rename(b)),
        }
    }

    /// Converts indices only. Admission and evaluation remain library work.
    pub(crate) fn library(self) -> Option<Op> {
        let id = |value: usize| u16::try_from(value).ok();
        Some(match self {
            Self::Input(a) => Op::Input(a),
            Self::Int(v) => Op::Int(v),
            Self::Bool(v) => Op::Bool(v),
            Self::Add(a, b) => Op::Add(id(a)?, id(b)?),
            Self::Sub(a, b) => Op::Sub(id(a)?, id(b)?),
            Self::Mul(a, b) => Op::Mul(id(a)?, id(b)?),
            Self::DivFloor(a, b) => Op::DivFloor(id(a)?, id(b)?),
            Self::DivCeil(a, b) => Op::DivCeil(id(a)?, id(b)?),
            Self::MulDivFloor(a, b, d) => Op::MulDivFloor(id(a)?, id(b)?, id(d)?),
            Self::MulDivCeil(a, b, d) => Op::MulDivCeil(id(a)?, id(b)?, id(d)?),
            Self::ProductsGe(a, b) => Op::ProductsGe(
                [id(a[0])?, id(a[1])?, id(a[2])?],
                [id(b[0])?, id(b[1])?, id(b[2])?],
            ),
            Self::Eq(a, b) => Op::Eq(id(a)?, id(b)?),
            Self::Lt(a, b) => Op::Lt(id(a)?, id(b)?),
            Self::And(a, b) => Op::And(id(a)?, id(b)?),
            Self::Not(a) => Op::Not(id(a)?),
            Self::Select(c, a, b) => Op::Select(id(c)?, id(a)?, id(b)?),
        })
    }
}

/// A topologically ordered program in the search's own index space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Graph {
    pub(crate) inputs: Vec<Domain>,
    pub(crate) outputs: Vec<Domain>,
    pub(crate) nodes: Vec<Node>,
    pub(crate) roots: Vec<usize>,
}

impl Graph {
    /// Reads an admitted library program; refuses an instruction or domain
    /// this crate does not know.
    pub(crate) fn from_program(program: &Program) -> Result<Self, &'static str> {
        for domain in program.inputs().iter().chain(program.outputs()) {
            domain_kind(*domain).ok_or("unsupported-domain")?;
        }
        let nodes = program
            .nodes()
            .iter()
            .map(|op| {
                let id = usize::from;
                Ok(match *op {
                    Op::Input(a) => Node::Input(a),
                    Op::Int(value) => Node::Int(value),
                    Op::Bool(value) => Node::Bool(value),
                    Op::Add(a, b) => Node::Add(id(a), id(b)),
                    Op::Sub(a, b) => Node::Sub(id(a), id(b)),
                    Op::Mul(a, b) => Node::Mul(id(a), id(b)),
                    Op::DivFloor(a, b) => Node::DivFloor(id(a), id(b)),
                    Op::DivCeil(a, b) => Node::DivCeil(id(a), id(b)),
                    Op::MulDivFloor(a, b, d) => Node::MulDivFloor(id(a), id(b), id(d)),
                    Op::MulDivCeil(a, b, d) => Node::MulDivCeil(id(a), id(b), id(d)),
                    Op::ProductsGe(a, b) => Node::ProductsGe(a.map(id), b.map(id)),
                    Op::Eq(a, b) => Node::Eq(id(a), id(b)),
                    Op::Lt(a, b) => Node::Lt(id(a), id(b)),
                    Op::And(a, b) => Node::And(id(a), id(b)),
                    Op::Not(a) => Node::Not(id(a)),
                    Op::Select(c, a, b) => Node::Select(id(c), id(a), id(b)),
                    _ => return Err("unsupported-instruction"),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            inputs: program.inputs().to_vec(),
            outputs: program.outputs().to_vec(),
            nodes,
            roots: program
                .roots()
                .iter()
                .map(|root| usize::from(*root))
                .collect(),
        })
    }

    /// Kind of every node. Operands must refer to earlier nodes, which every
    /// graph built by this crate guarantees; an out-of-range reference reads
    /// as `Int` and the library refuses the graph at admission.
    pub(crate) fn kinds(&self) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let kind = match *node {
                Node::Input(k) => self
                    .inputs
                    .get(usize::from(k))
                    .and_then(|domain| domain_kind(*domain))
                    .unwrap_or(Kind::Int),
                Node::Int(_)
                | Node::Add(..)
                | Node::Sub(..)
                | Node::Mul(..)
                | Node::DivFloor(..)
                | Node::DivCeil(..)
                | Node::MulDivFloor(..)
                | Node::MulDivCeil(..) => Kind::Int,
                Node::Bool(_)
                | Node::Eq(..)
                | Node::Lt(..)
                | Node::And(..)
                | Node::Not(_)
                | Node::ProductsGe(..) => Kind::Bool,
                Node::Select(_, a, _) => kinds.get(a).copied().unwrap_or(Kind::Int),
            };
            kinds.push(kind);
        }
        kinds
    }

    fn library_nodes(&self) -> Option<Vec<Op>> {
        self.nodes.iter().map(|node| node.library()).collect()
    }

    /// The library's profile-1 admission of this graph, or `None` when the
    /// library refuses it.
    pub(crate) fn to_program(&self) -> Option<Program> {
        let roots = self
            .roots
            .iter()
            .map(|root| u16::try_from(*root).ok())
            .collect::<Option<Vec<_>>>()?;
        Program::try_new(
            self.inputs.clone(),
            self.outputs.clone(),
            self.library_nodes()?,
            roots,
        )
        .ok()
    }

    /// A probe: the same instructions with `chunk` as the results, every
    /// result over its kind's full range. Evaluating it reads those nodes'
    /// values with the library's eager semantics, trap included.
    pub(crate) fn probe(&self, chunk: &[usize], kinds: &[Kind]) -> Option<Program> {
        let outputs = chunk
            .iter()
            .map(|node| match kinds.get(*node)? {
                Kind::Bool => Some(Domain::Bool),
                Kind::Int => Some(Domain::Int {
                    min: i128::MIN,
                    max: i128::MAX,
                }),
            })
            .collect::<Option<Vec<_>>>()?;
        let roots = chunk
            .iter()
            .map(|node| u16::try_from(*node).ok())
            .collect::<Option<Vec<_>>>()?;
        Program::try_new_extended(self.inputs.clone(), outputs, self.library_nodes()?, roots).ok()
    }
}

/// Canonical program bytes from the library encoder.
pub(crate) fn encode(program: &Program) -> Option<Vec<u8>> {
    program
        .value()
        .ok()
        .and_then(|value| value.canonical_bytes().ok())
}

/// Selection order: instructions, then canonical bytes (the first two keys
/// of `optimize`'s order).
pub(crate) type Cost = (usize, usize);
