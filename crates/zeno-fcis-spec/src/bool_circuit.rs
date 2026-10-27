//! Owned, acyclic Boolean formulas with explicit sharing.

use alloc::sync::Arc;
use alloc::vec::Vec;

/// One Boolean operation. References always name an earlier node.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BoolNode<F> {
    /// An ordinary, independently checked formula.
    Formula(F),
    /// A constant.
    Literal(bool),
    /// Boolean complement.
    Not(u32),
    /// Conjunction.
    And(u32, u32),
    /// Disjunction.
    Or(u32, u32),
    /// An eager guarded value.
    Select(u32, u32, u32),
}

impl<F> BoolNode<F> {
    /// Maps only the ordinary formula leaves, preserving references.
    pub fn try_map<G, E>(&self, mut map: impl FnMut(&F) -> Result<G, E>) -> Result<BoolNode<G>, E> {
        Ok(match self {
            Self::Formula(f) => BoolNode::Formula(map(f)?),
            Self::Literal(v) => BoolNode::Literal(*v),
            Self::Not(a) => BoolNode::Not(*a),
            Self::And(a, b) => BoolNode::And(*a, *b),
            Self::Or(a, b) => BoolNode::Or(*a, *b),
            Self::Select(c, a, b) => BoolNode::Select(*c, *a, *b),
        })
    }
}

/// An immutable bounded circuit topology with invocation-local evaluation scratch.
/// Formula leaves retain the ownership contract of `F`; authoritative instantiations
/// use transitively immutable formulas. This topology is not a publication capability.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoolCircuit<F> {
    nodes: Arc<[BoolNode<F>]>,
    output: u32,
}

impl<F> BoolCircuit<F> {
    /// Checks the size, output and every backward reference before construction.
    pub fn try_new(nodes: Vec<BoolNode<F>>, output: u32) -> Option<Self> {
        if nodes.len() > crate::MAX_FORMULA_NODES || output as usize >= nodes.len() {
            return None;
        }
        for (i, node) in nodes.iter().enumerate() {
            let earlier = |n: &u32| (*n as usize) < i;
            if !match node {
                BoolNode::Formula(_) | BoolNode::Literal(_) => true,
                BoolNode::Not(a) => earlier(a),
                BoolNode::And(a, b) | BoolNode::Or(a, b) => earlier(a) && earlier(b),
                BoolNode::Select(c, a, b) => earlier(c) && earlier(a) && earlier(b),
            } {
                return None;
            }
        }
        Some(Self {
            nodes: nodes.into(),
            output,
        })
    }

    /// Operations in evaluation order. No mutable alias is exposed.
    pub fn nodes(&self) -> &[BoolNode<F>] {
        &self.nodes
    }

    /// Ordinary formula leaves in operation order.
    pub fn formulas(&self) -> impl Iterator<Item = &F> {
        self.nodes.iter().filter_map(|node| {
            if let BoolNode::Formula(f) = node {
                Some(f)
            } else {
                None
            }
        })
    }

    /// Maps formula leaves with a total function.
    pub fn map<G>(&self, mut map: impl FnMut(&F) -> G) -> BoolCircuit<G> {
        match self.try_map(|f| Ok::<G, core::convert::Infallible>(map(f))) {
            Ok(c) => c,
            Err(e) => match e {},
        }
    }

    /// The result's node index.
    pub const fn output(&self) -> u32 {
        self.output
    }

    /// Selects another checked result while sharing only immutable nodes.
    pub fn at_output(&self, output: u32) -> Option<Self> {
        ((output as usize) < self.nodes.len()).then(|| Self {
            nodes: self.nodes.clone(),
            output,
        })
    }

    /// Maps formula leaves without changing the checked graph topology.
    pub fn try_map<G, E>(
        &self,
        mut map: impl FnMut(&F) -> Result<G, E>,
    ) -> Result<BoolCircuit<G>, E> {
        Ok(BoolCircuit {
            nodes: self
                .nodes
                .iter()
                .map(|node| node.try_map(&mut map))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            output: self.output,
        })
    }

    /// Eager callback evaluation with exclusively owned invocation scratch.
    /// The caller supplies each lower formula's semantics; this diagnostic utility
    /// grants no decision, measured-work or publication authority.
    /// Runtime authorities compile these operations into their metered graph instead.
    pub fn evaluate(&self, formula: impl FnMut(&F) -> Option<bool>) -> Option<bool> {
        self.evaluate_all(formula)?
            .get(self.output as usize)
            .copied()
    }

    /// Owned invocation scratch; one eager pass, with no retained aliases.
    pub(crate) fn evaluate_all(
        &self,
        mut formula: impl FnMut(&F) -> Option<bool>,
    ) -> Option<Vec<bool>> {
        let mut values: Vec<bool> = Vec::with_capacity(self.nodes.len());
        for node in self.nodes.iter() {
            let read = |n: u32| values.get(n as usize).copied();
            let value = match node {
                BoolNode::Formula(f) => formula(f)?,
                BoolNode::Literal(v) => *v,
                BoolNode::Not(a) => !read(*a)?,
                BoolNode::And(a, b) => read(*a)? & read(*b)?,
                BoolNode::Or(a, b) => read(*a)? | read(*b)?,
                BoolNode::Select(c, a, b) => {
                    if read(*c)? {
                        read(*a)?
                    } else {
                        read(*b)?
                    }
                }
            };
            values.push(value);
        }
        Some(values)
    }
}
